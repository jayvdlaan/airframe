use airframe_crypt::envelope::EnvelopeBytes;
use airframe_crypt::error::AirframeCryptError;
use airframe_crypt::suite::CipherSuite;
use secrecy::ExposeSecret;

use crate::error::{AirframeSecretsError, Result};
use crate::resolver::KeyResolver;
use crate::secret::SecretBytes;

/// SecretBlob stores an encrypted byte payload and provides closure-based
/// just-in-time decryption so plaintext never escapes the scope.
pub struct SecretBlob {
    inner: EnvelopeBytes,
    key_id: Option<Vec<u8>>, // optional provenance to help resolvers
}

impl core::fmt::Debug for SecretBlob {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SecretBlob(..redacted..)")
    }
}

impl SecretBlob {
    pub fn new(inner: EnvelopeBytes, key_id: Option<Vec<u8>>) -> Self {
        Self { inner, key_id }
    }

    pub fn envelope(&self) -> &EnvelopeBytes {
        &self.inner
    }
    pub fn key_id(&self) -> Option<&[u8]> {
        self.key_id.as_deref()
    }

    /// Decrypt and pass plaintext slice into the provided closure. The buffer is
    /// zeroized on drop.
    pub fn with_plaintext<S, F, R>(
        &self,
        suite: &S,
        key: &SecretBytes,
        aad: Option<&[u8]>,
        f: F,
    ) -> Result<R>
    where
        S: CipherSuite,
        F: FnOnce(&[u8]) -> R,
    {
        let pt = key
            .with_secrecy_slice(|k| self.inner.decrypt_with_suite(suite, k, aad))
            .map_err(map_crypt_err)?;
        // Expose for closure execution; secrecy wrapper ensures it's not accidentally formatted,
        // and the inner buffer will be dropped after this scope.
        let res = f(pt.expose_secret().as_slice());
        drop(pt);
        Ok(res)
    }

    /// Variant that resolves the key via a KeyResolver
    pub fn with_plaintext_resolved<S, F, R>(
        &self,
        suite: &S,
        resolver: &dyn KeyResolver,
        aad: Option<&[u8]>,
        f: F,
    ) -> Result<R>
    where
        S: CipherSuite,
        F: FnOnce(&[u8]) -> R,
    {
        let key = resolver.resolve(self.key_id())?;
        self.with_plaintext(suite, &key, aad, f)
    }

    pub fn into_inner(self) -> EnvelopeBytes {
        self.inner
    }
}

fn map_crypt_err(_e: AirframeCryptError) -> AirframeSecretsError {
    // keep redacted: do not leak crypto internals
    AirframeSecretsError::InvalidState
}
