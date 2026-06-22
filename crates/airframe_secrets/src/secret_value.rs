use airframe_crypt::envelope::{BincodeCodec, Envelope, EnvelopeValue};
use airframe_crypt::suite::CipherSuite;
use serde::{de::DeserializeOwned, Serialize};

use crate::error::Result;
use crate::resolver::KeyResolver;
use crate::secret::SecretBytes;

/// SecretValue wraps a typed envelope and offers closure-based access to the decrypted value.
pub struct SecretValue<T> {
    inner: Envelope<T, BincodeCodec>,
    key_id: Option<Vec<u8>>,
}

impl<T> SecretValue<T> {
    pub fn new(inner: Envelope<T, BincodeCodec>, key_id: Option<Vec<u8>>) -> Self {
        Self { inner, key_id }
    }
    pub fn envelope(&self) -> &Envelope<T, BincodeCodec> {
        &self.inner
    }
    pub fn key_id(&self) -> Option<&[u8]> {
        self.key_id.as_deref()
    }
}

impl<T> core::fmt::Debug for SecretValue<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SecretValue(..redacted..)")
    }
}

impl<T> SecretValue<T>
where
    T: Serialize + DeserializeOwned,
{
    pub fn encrypt_with_suite<S: CipherSuite>(
        suite: &S,
        alg: airframe_crypt::sym::SymmetricAlgorithm,
        key: &SecretBytes,
        value: &T,
        aad: Option<&[u8]>,
        key_id: Option<Vec<u8>>,
    ) -> Result<Self> {
        let env: Envelope<T, BincodeCodec> = key
            .with_secrecy_slice(|k| EnvelopeValue::encrypt_with_suite(suite, alg, k, value, aad))
            .map_err(|_| crate::error::AirframeSecretsError::InvalidState)?;
        Ok(Self { inner: env, key_id })
    }

    pub fn with_value<S, F, R>(
        &self,
        suite: &S,
        key: &SecretBytes,
        aad: Option<&[u8]>,
        f: F,
    ) -> Result<R>
    where
        S: CipherSuite,
        F: FnOnce(&T) -> R,
    {
        let v: T = key
            .with_secrecy_slice(|k| self.inner.decrypt_with_suite(suite, k, aad))
            .map_err(|_| crate::error::AirframeSecretsError::InvalidState)?;
        let res = f(&v);
        // v is dropped here
        Ok(res)
    }

    pub fn with_value_resolved<S, F, R>(
        &self,
        suite: &S,
        resolver: &dyn KeyResolver,
        aad: Option<&[u8]>,
        f: F,
    ) -> Result<R>
    where
        S: CipherSuite,
        F: FnOnce(&T) -> R,
    {
        let key = resolver.resolve(self.key_id())?;
        self.with_value(suite, &key, aad, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airframe_crypt::suite::SoftwareCipherSuite;
    use airframe_crypt::sym::SymmetricAlgorithm;

    struct StaticResolver(SecretBytes);
    impl crate::resolver::KeyResolver for StaticResolver {
        fn resolve(&self, _key_id: Option<&[u8]>) -> Result<SecretBytes> {
            Ok(SecretBytes::from_vec(self.0.to_vec()))
        }
    }

    #[test]
    fn encrypt_and_decrypt_roundtrip() {
        let suite = SoftwareCipherSuite::new();
        let key = SecretBytes::from_vec(vec![0x11; 32]);
        let alg = SymmetricAlgorithm::AesGcm;
        let value = String::from("top secret");
        let aad = Some(b"ctx".as_ref());
        let key_id = Some(b"k1".to_vec());

        let secret =
            SecretValue::encrypt_with_suite(&suite, alg, &key, &value, aad, key_id.clone())
                .unwrap();
        // key_id is preserved
        assert_eq!(secret.key_id(), key_id.as_deref());

        // Closure-based access
        let recovered = secret.with_value(&suite, &key, aad, |s| s.clone()).unwrap();
        assert_eq!(recovered, value);

        // Resolver-based path
        let resolver = StaticResolver(SecretBytes::from_vec(key.to_vec()));
        let recovered2 = secret
            .with_value_resolved(&suite, &resolver, aad, |s| s.clone())
            .unwrap();
        assert_eq!(recovered2, value);
    }
}
