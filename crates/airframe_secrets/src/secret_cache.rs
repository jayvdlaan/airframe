use airframe_crypt::envelope::{BincodeCodec, Envelope, EnvelopeBytes, EnvelopeValue};
use airframe_crypt::suite::CipherSuite;
use airframe_crypt::sym::SymmetricAlgorithm;
use airframe_data::cache::ByteCache;
use airframe_data::key::Key;
use secrecy::ExposeSecret; // used only to expose decrypted payloads from airframe_crypt
use serde::{de::DeserializeOwned, Serialize};

use crate::error::{AirframeSecretsError, Result};
use crate::resolver::KeyResolver;
use crate::secret::SecretBytes;

/// SecretCache persists encrypted envelopes in a ByteCache backend (FS/mem/etc.).
/// It handles JSON encoding of EnvelopeBytes and typed envelope round-trips.
#[derive(Clone)]
pub struct SecretCache<BC: ByteCache> {
    inner: BC,
}

impl<BC: ByteCache> SecretCache<BC> {
    pub fn new(inner: BC) -> Self {
        Self { inner }
    }
    pub fn inner(&self) -> &BC {
        &self.inner
    }
}

impl<BC: ByteCache> SecretCache<BC> {
    pub fn put_encrypted_bytes<S: CipherSuite>(
        &self,
        key: &Key,
        suite: &S,
        alg: SymmetricAlgorithm,
        enc_key: &SecretBytes,
        plaintext: &SecretBytes,
        aad: Option<&[u8]>,
    ) -> Result<()> {
        let env = enc_key
            .with_secrecy_slice(|k| {
                plaintext.with_secrecy_slice(|p| {
                    EnvelopeBytes::encrypt_with_suite(suite, alg, k, p, aad)
                })
            })
            .map_err(|_| AirframeSecretsError::InvalidState)?;
        let json = env
            .to_json_string()
            .map_err(|_| AirframeSecretsError::InvalidState)?;
        self.inner
            .put_bytes(key, json.as_bytes())
            .map_err(|_| AirframeSecretsError::InvalidState)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn put_encrypted_bytes_resolved<S: CipherSuite>(
        &self,
        key: &Key,
        suite: &S,
        alg: SymmetricAlgorithm,
        resolver: &dyn KeyResolver,
        key_id: Option<&[u8]>,
        plaintext: &SecretBytes,
        aad: Option<&[u8]>,
    ) -> Result<()> {
        let enc_key = resolver.resolve(key_id)?;
        self.put_encrypted_bytes(key, suite, alg, &enc_key, plaintext, aad)
    }

    pub fn get_decrypted_bytes<S: CipherSuite>(
        &self,
        key: &Key,
        suite: &S,
        enc_key: &SecretBytes,
        aad: Option<&[u8]>,
    ) -> Result<Option<Vec<u8>>> {
        let Some(b) = self
            .inner
            .get_bytes(key)
            .map_err(|_| AirframeSecretsError::InvalidState)?
        else {
            return Ok(None);
        };
        let s = String::from_utf8(b).map_err(|_| AirframeSecretsError::InvalidState)?;
        let env =
            EnvelopeBytes::from_json_str(&s).map_err(|_| AirframeSecretsError::InvalidState)?;
        let pt = enc_key
            .with_secrecy_slice(|k| env.decrypt_with_suite(suite, k, aad))
            .map_err(|_| AirframeSecretsError::InvalidState)?;
        Ok(Some(pt.expose_secret().clone()))
    }

    pub fn get_decrypted_bytes_resolved<S: CipherSuite>(
        &self,
        key: &Key,
        suite: &S,
        resolver: &dyn KeyResolver,
        key_id: Option<&[u8]>,
        aad: Option<&[u8]>,
    ) -> Result<Option<Vec<u8>>> {
        let enc_key = resolver.resolve(key_id)?;
        self.get_decrypted_bytes(key, suite, &enc_key, aad)
    }

    pub fn put_value<S, T>(
        &self,
        key: &Key,
        suite: &S,
        alg: SymmetricAlgorithm,
        enc_key: &SecretBytes,
        value: &T,
        aad: Option<&[u8]>,
    ) -> Result<()>
    where
        S: CipherSuite,
        T: Serialize + DeserializeOwned,
    {
        let env: Envelope<T, BincodeCodec> = enc_key
            .with_secrecy_slice(|k| EnvelopeValue::encrypt_with_suite(suite, alg, k, value, aad))
            .map_err(|_| AirframeSecretsError::InvalidState)?;
        let env_bytes = env.into_inner();
        let json = env_bytes
            .to_json_string()
            .map_err(|_| AirframeSecretsError::InvalidState)?;
        self.inner
            .put_bytes(key, json.as_bytes())
            .map_err(|_| AirframeSecretsError::InvalidState)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn put_value_resolved<S, T>(
        &self,
        key: &Key,
        suite: &S,
        alg: SymmetricAlgorithm,
        resolver: &dyn KeyResolver,
        key_id: Option<&[u8]>,
        value: &T,
        aad: Option<&[u8]>,
    ) -> Result<()>
    where
        S: CipherSuite,
        T: Serialize + DeserializeOwned,
    {
        let enc_key = resolver.resolve(key_id)?;
        self.put_value(key, suite, alg, &enc_key, value, aad)
    }

    pub fn get_value<S, T>(
        &self,
        key: &Key,
        suite: &S,
        enc_key: &SecretBytes,
        aad: Option<&[u8]>,
    ) -> Result<Option<T>>
    where
        S: CipherSuite,
        T: Serialize + DeserializeOwned,
    {
        let Some(b) = self
            .inner
            .get_bytes(key)
            .map_err(|_| AirframeSecretsError::InvalidState)?
        else {
            return Ok(None);
        };
        let s = String::from_utf8(b).map_err(|_| AirframeSecretsError::InvalidState)?;
        let env: Envelope<T, BincodeCodec> =
            Envelope::from_json_str(&s).map_err(|_| AirframeSecretsError::InvalidState)?;
        let v: T = enc_key
            .with_secrecy_slice(|k| env.decrypt_with_suite(suite, k, aad))
            .map_err(|_| AirframeSecretsError::InvalidState)?;
        Ok(Some(v))
    }

    pub fn get_value_resolved<S, T>(
        &self,
        key: &Key,
        suite: &S,
        resolver: &dyn KeyResolver,
        key_id: Option<&[u8]>,
        aad: Option<&[u8]>,
    ) -> Result<Option<T>>
    where
        S: CipherSuite,
        T: Serialize + DeserializeOwned,
    {
        let enc_key = resolver.resolve(key_id)?;
        self.get_value(key, suite, &enc_key, aad)
    }

    pub fn remove(&self, key: &Key) -> Result<()> {
        self.inner
            .remove(key)
            .map_err(|_| AirframeSecretsError::InvalidState)
    }
    pub fn contains(&self, key: &Key) -> Result<bool> {
        self.inner
            .contains(key)
            .map_err(|_| AirframeSecretsError::InvalidState)
    }
    pub fn list(&self) -> Result<Vec<Key>> {
        self.inner
            .list()
            .map_err(|_| AirframeSecretsError::InvalidState)
    }
}
