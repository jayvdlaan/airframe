use crate::error::ChannelError;
use airframe_crypt::sym::{openssl_sym_decrypt, openssl_sym_encrypt, SymmetricAlgorithm};

/// Maximum nonce value before we must rekey or abort.
const MAX_NONCE: u64 = u64::MAX - 1;

/// CipherState per Noise spec Section 5.1.
///
/// Holds an AEAD key `k` and a nonce counter `n`.
/// When `k` is `None`, encryption/decryption are identity operations.
pub struct CipherState {
    k: Option<[u8; 32]>,
    n: u64,
}

impl CipherState {
    /// Create an empty CipherState (no key set).
    pub fn empty() -> Self {
        Self { k: None, n: 0 }
    }

    /// Initialize with a key.
    pub fn initialize_key(key: Option<[u8; 32]>) -> Self {
        Self { k: key, n: 0 }
    }

    /// Returns true if a key has been set.
    pub fn has_key(&self) -> bool {
        self.k.is_some()
    }

    /// Set the nonce value (used during Split).
    pub fn set_nonce(&mut self, n: u64) {
        self.n = n;
    }

    /// Encode nonce as 12-byte value per Noise spec:
    /// 4 zero bytes || 8-byte little-endian nonce
    fn encode_nonce(n: u64) -> [u8; 12] {
        let mut nonce = [0u8; 12];
        nonce[4..12].copy_from_slice(&n.to_le_bytes());
        nonce
    }

    /// Encrypt with associated data. If no key is set, returns plaintext unchanged.
    pub fn encrypt_with_ad(
        &mut self,
        ad: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, ChannelError> {
        match self.k {
            Some(ref k) => {
                if self.n >= MAX_NONCE {
                    return Err(ChannelError::NonceExhausted);
                }
                let nonce = Self::encode_nonce(self.n);
                let ciphertext = openssl_sym_encrypt(
                    SymmetricAlgorithm::ChaCha20Poly1305,
                    k,
                    &nonce,
                    plaintext,
                    Some(ad),
                )?;
                self.n += 1;
                Ok(ciphertext)
            }
            None => Ok(plaintext.to_vec()),
        }
    }

    /// Decrypt with associated data. If no key is set, returns ciphertext unchanged.
    pub fn decrypt_with_ad(
        &mut self,
        ad: &[u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, ChannelError> {
        match self.k {
            Some(ref k) => {
                if self.n >= MAX_NONCE {
                    return Err(ChannelError::NonceExhausted);
                }
                let nonce = Self::encode_nonce(self.n);
                let plaintext = openssl_sym_decrypt(
                    SymmetricAlgorithm::ChaCha20Poly1305,
                    k,
                    &nonce,
                    ciphertext,
                    Some(ad),
                )
                .map_err(|_| ChannelError::DecryptionFailed)?;
                self.n += 1;
                Ok(plaintext)
            }
            None => Ok(ciphertext.to_vec()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_cipher_state_passthrough() {
        let mut cs = CipherState::empty();
        assert!(!cs.has_key());
        let pt = b"hello world";
        let ct = cs.encrypt_with_ad(b"", pt).unwrap();
        assert_eq!(ct, pt);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = [0x42u8; 32];
        let mut enc = CipherState::initialize_key(Some(key));
        let mut dec = CipherState::initialize_key(Some(key));
        let ad = b"associated data";
        let plaintext = b"secret message";

        let ct = enc.encrypt_with_ad(ad, plaintext).unwrap();
        assert_ne!(ct.as_slice(), plaintext.as_slice());

        let pt = dec.decrypt_with_ad(ad, &ct).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn test_nonce_increments() {
        let key = [0x01u8; 32];
        let mut cs = CipherState::initialize_key(Some(key));

        let ct1 = cs.encrypt_with_ad(b"", b"msg1").unwrap();
        let ct2 = cs.encrypt_with_ad(b"", b"msg1").unwrap();
        // Same plaintext, different nonce -> different ciphertext
        assert_ne!(ct1, ct2);
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let key = [0x55u8; 32];
        let mut enc = CipherState::initialize_key(Some(key));
        let mut dec = CipherState::initialize_key(Some(key));

        let mut ct = enc.encrypt_with_ad(b"ad", b"payload").unwrap();
        ct[0] ^= 0xff;

        assert!(dec.decrypt_with_ad(b"ad", &ct).is_err());
    }

    #[test]
    fn test_wrong_ad_fails() {
        let key = [0x77u8; 32];
        let mut enc = CipherState::initialize_key(Some(key));
        let mut dec = CipherState::initialize_key(Some(key));

        let ct = enc.encrypt_with_ad(b"correct", b"payload").unwrap();
        assert!(dec.decrypt_with_ad(b"wrong", &ct).is_err());
    }

    #[test]
    fn test_nonce_encoding() {
        // Nonce 0 -> 4 zero bytes + 8 zero bytes
        let n0 = CipherState::encode_nonce(0);
        assert_eq!(n0, [0; 12]);

        // Nonce 1 -> 4 zero bytes + [1, 0, 0, 0, 0, 0, 0, 0] (LE)
        let n1 = CipherState::encode_nonce(1);
        assert_eq!(n1, [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
    }
}
