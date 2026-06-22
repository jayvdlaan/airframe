use crate::context::{KeyResolver, PContext};
use crate::error::{AirframePdataError, Result};
#[cfg(feature = "compress")]
use crate::policy::Compression;
use airframe_crypt::envelope::EnvelopeBytes;
use airframe_data::cache::ByteCache;
use airframe_data::key::Key;
use secrecy::ExposeSecret;

#[derive(Clone)]
pub struct PStoreBytes<BC: ByteCache, R: KeyResolver> {
    inner: BC,
    ctx: PContext<R>,
}

impl<BC: ByteCache, R: KeyResolver> PStoreBytes<BC, R> {
    pub fn new(inner: BC, ctx: PContext<R>) -> Self {
        Self { inner, ctx }
    }

    /// Write bytes using compress-then-encrypt ordering.
    pub fn put_bytes(&self, key: &Key, plaintext: &[u8]) -> Result<()> {
        self.put_bytes_with_meta(key, plaintext, None)
    }

    /// Variant that accepts cleartext meta/index bytes to be bound via AAD policy.
    pub fn put_bytes_with_meta(
        &self,
        key: &Key,
        plaintext: &[u8],
        clear_meta: Option<&[u8]>,
    ) -> Result<()> {
        // optional compress
        #[cfg(feature = "compress")]
        let data: Vec<u8> = match &self.ctx.compression {
            Compression::Disabled => plaintext.to_vec(),
            Compression::Algo(algo) => algo
                .compress(plaintext)
                .map_err(|_| AirframePdataError::InvalidState)?,
        };
        #[cfg(not(feature = "compress"))]
        let data: Vec<u8> = plaintext.to_vec();

        // encrypt
        let aad = self.ctx.aad_for(key, clear_meta);
        let enc_key = self.ctx.resolve_key()?;
        let env = enc_key
            .with_secrecy_slice(|k| {
                let boxed: Box<[u8]> = data.into_boxed_slice();
                let pt = secrecy::SecretSlice::new(boxed);
                EnvelopeBytes::encrypt_with_suite(&self.ctx.suite, self.ctx.alg, k, &pt, Some(&aad))
            })
            .map_err(|_| AirframePdataError::InvalidState)?;

        let json = env
            .to_json_string()
            .map_err(|_| AirframePdataError::InvalidState)?;
        self.inner
            .put_bytes(key, json.as_bytes())
            .map_err(|_| AirframePdataError::InvalidState)
    }

    /// Read bytes using decrypt-then-decompress ordering.
    pub fn get_bytes(&self, key: &Key) -> Result<Option<Vec<u8>>> {
        self.get_bytes_with_meta(key, None)
    }

    /// Variant that accepts cleartext meta/index bytes to be bound via AAD policy.
    pub fn get_bytes_with_meta(
        &self,
        key: &Key,
        clear_meta: Option<&[u8]>,
    ) -> Result<Option<Vec<u8>>> {
        let Some(stored) = self
            .inner
            .get_bytes(key)
            .map_err(|_| AirframePdataError::InvalidState)?
        else {
            return Ok(None);
        };
        let s = String::from_utf8(stored).map_err(|_| AirframePdataError::InvalidState)?;
        let env = EnvelopeBytes::from_json_str(&s).map_err(|_| AirframePdataError::InvalidState)?;

        let aad = self.ctx.aad_for(key, clear_meta);
        let enc_key = self.ctx.resolve_key()?;
        let pt = enc_key
            .with_secrecy_slice(|k| env.decrypt_with_suite(&self.ctx.suite, k, Some(&aad)))
            .map_err(|_| AirframePdataError::InvalidState)?;
        #[cfg_attr(not(feature = "compress"), allow(unused_mut))]
        let mut out = pt.expose_secret().clone();

        // optional decompress
        #[cfg(feature = "compress")]
        {
            match &self.ctx.compression {
                Compression::Disabled => {}
                Compression::Algo(algo) => {
                    out = algo
                        .decompress(&out)
                        .map_err(|_| AirframePdataError::InvalidState)?;
                }
            }
        }

        Ok(Some(out))
    }

    pub fn remove(&self, key: &Key) -> Result<()> {
        self.inner
            .remove(key)
            .map_err(|_| AirframePdataError::InvalidState)
    }
    pub fn contains(&self, key: &Key) -> Result<bool> {
        self.inner
            .contains(key)
            .map_err(|_| AirframePdataError::InvalidState)
    }
    pub fn list(&self) -> Result<Vec<Key>> {
        self.inner
            .list()
            .map_err(|_| AirframePdataError::InvalidState)
    }

    /// Re-encrypt data under a new resolver-provided key, without exposing plaintext outside API.
    pub fn rewrap<N: KeyResolver>(&self, key: &Key, new_resolver: &N) -> Result<bool> {
        let Some(stored) = self
            .inner
            .get_bytes(key)
            .map_err(|_| AirframePdataError::InvalidState)?
        else {
            return Ok(false);
        };
        let s = String::from_utf8(stored).map_err(|_| AirframePdataError::InvalidState)?;
        let env = EnvelopeBytes::from_json_str(&s).map_err(|_| AirframePdataError::InvalidState)?;
        let aad = self.ctx.aad_for(key, None);

        // decrypt with old key
        let old_key = self.ctx.resolve_key()?;
        let pt = old_key
            .with_secrecy_slice(|k| env.decrypt_with_suite(&self.ctx.suite, k, Some(&aad)))
            .map_err(|_| AirframePdataError::InvalidState)?;

        // Optional: decompress and re-compress to match policy (keeps consistent), or we can re-encrypt as-is.
        #[cfg_attr(not(feature = "compress"), allow(unused_mut))]
        let mut data = pt.expose_secret().clone();
        #[cfg(feature = "compress")]
        {
            match &self.ctx.compression {
                Compression::Disabled => {}
                Compression::Algo(algo) => {
                    // data currently is decrypted plaintext; recompress again to normalize
                    data = algo
                        .compress(&data)
                        .map_err(|_| AirframePdataError::InvalidState)?;
                }
            }
        }

        // encrypt with new key using same suite/alg/AAD as current context
        let new_key = new_resolver
            .resolve(self.ctx.key_id.as_deref())
            .map_err(|_| AirframePdataError::InvalidState)?;
        let env2 = new_key
            .with_secrecy_slice(|k| {
                let boxed: Box<[u8]> = data.into_boxed_slice();
                let pt = secrecy::SecretSlice::new(boxed);
                EnvelopeBytes::encrypt_with_suite(&self.ctx.suite, self.ctx.alg, k, &pt, Some(&aad))
            })
            .map_err(|_| AirframePdataError::InvalidState)?;
        let json = env2
            .to_json_string()
            .map_err(|_| AirframePdataError::InvalidState)?;
        self.inner
            .put_bytes(key, json.as_bytes())
            .map_err(|_| AirframePdataError::InvalidState)?;
        Ok(true)
    }

    /// Re-encrypt using a new PContext (algorithm/AAD/resolver may differ). Returns Ok(true) if rewrapped, Ok(false) if key missing.
    pub fn rewrap_to<R2: KeyResolver>(&self, key: &Key, new_ctx: &PContext<R2>) -> Result<bool> {
        let Some(stored) = self
            .inner
            .get_bytes(key)
            .map_err(|_| AirframePdataError::InvalidState)?
        else {
            return Ok(false);
        };
        let s = String::from_utf8(stored).map_err(|_| AirframePdataError::InvalidState)?;
        let env = EnvelopeBytes::from_json_str(&s).map_err(|_| AirframePdataError::InvalidState)?;
        let aad_old = self.ctx.aad_for(key, None);

        // decrypt with old key
        let old_key = self.ctx.resolve_key()?;
        let pt = old_key
            .with_secrecy_slice(|k| env.decrypt_with_suite(&self.ctx.suite, k, Some(&aad_old)))
            .map_err(|_| AirframePdataError::InvalidState)?;

        // Optional: decompress and then re-compress according to new context policy
        #[cfg_attr(not(feature = "compress"), allow(unused_mut))]
        let mut data = pt.expose_secret().clone();
        #[cfg(feature = "compress")]
        {
            match &self.ctx.compression {
                Compression::Disabled => {}
                Compression::Algo(algo) => {
                    data = algo
                        .decompress(&data)
                        .map_err(|_| AirframePdataError::InvalidState)?;
                }
            }
        }
        #[cfg(feature = "compress")]
        {
            match &new_ctx.compression {
                Compression::Disabled => {}
                Compression::Algo(algo) => {
                    data = algo
                        .compress(&data)
                        .map_err(|_| AirframePdataError::InvalidState)?;
                }
            }
        }

        // encrypt with new context params
        let aad_new = new_ctx.aad_for(key, None);
        let new_key = new_ctx.resolve_key()?;
        let env2 = new_key
            .with_secrecy_slice(|k| {
                let boxed: Box<[u8]> = data.into_boxed_slice();
                let pt = secrecy::SecretSlice::new(boxed);
                EnvelopeBytes::encrypt_with_suite(
                    &new_ctx.suite,
                    new_ctx.alg,
                    k,
                    &pt,
                    Some(&aad_new),
                )
            })
            .map_err(|_| AirframePdataError::InvalidState)?;
        let json = env2
            .to_json_string()
            .map_err(|_| AirframePdataError::InvalidState)?;
        self.inner
            .put_bytes(key, json.as_bytes())
            .map_err(|_| AirframePdataError::InvalidState)?;
        Ok(true)
    }
}
