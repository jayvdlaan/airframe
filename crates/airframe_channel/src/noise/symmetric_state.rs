use crate::error::ChannelError;
use crate::hkdf::hkdf_sha256;
use crate::noise::cipher_state::CipherState;
use airframe_crypt::hash::{openssl_digest, DigestAlgorithm};

/// SymmetricState per Noise spec Section 5.2.
///
/// Manages the chaining key `ck`, handshake hash `h`, and an inner `CipherState`.
pub struct SymmetricState {
    ck: Vec<u8>,
    h: Vec<u8>,
    cipher: CipherState,
}

impl SymmetricState {
    /// Initialize with a protocol name.
    ///
    /// Per Noise spec: if `protocol_name.len() <= 32`, pad with zeros to 32 bytes.
    /// Otherwise, hash it with SHA-256.
    pub fn initialize(protocol_name: &[u8]) -> Self {
        let h = if protocol_name.len() <= 32 {
            let mut buf = [0u8; 32];
            buf[..protocol_name.len()].copy_from_slice(protocol_name);
            buf.to_vec()
        } else {
            openssl_digest(DigestAlgorithm::Sha256, protocol_name)
                .expect("SHA-256 digest should not fail")
        };
        let ck = h.clone();
        Self {
            ck,
            h,
            cipher: CipherState::empty(),
        }
    }

    /// MixKey: HKDF(ck, input_key_material) -> (new_ck, temp_k)
    /// Then initialize the CipherState with temp_k, truncated to 32 bytes.
    pub fn mix_key(&mut self, input_key_material: &[u8]) -> Result<(), ChannelError> {
        let outputs = hkdf_sha256(&self.ck, input_key_material, 2)?;
        self.ck = outputs[0].clone();
        let mut temp_k = [0u8; 32];
        temp_k.copy_from_slice(&outputs[1][..32]);
        self.cipher = CipherState::initialize_key(Some(temp_k));
        Ok(())
    }

    /// MixHash: h = SHA-256(h || data)
    pub fn mix_hash(&mut self, data: &[u8]) -> Result<(), ChannelError> {
        let mut buf = Vec::with_capacity(self.h.len() + data.len());
        buf.extend_from_slice(&self.h);
        buf.extend_from_slice(data);
        self.h = openssl_digest(DigestAlgorithm::Sha256, &buf)?;
        Ok(())
    }

    /// EncryptAndHash: if key is set, AEAD encrypt with h as AD.
    /// Then mix_hash the ciphertext.
    pub fn encrypt_and_hash(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, ChannelError> {
        let ciphertext = self.cipher.encrypt_with_ad(&self.h, plaintext)?;
        self.mix_hash(&ciphertext)?;
        Ok(ciphertext)
    }

    /// DecryptAndHash: if key is set, AEAD decrypt with h as AD.
    /// Then mix_hash the ciphertext (the original input).
    pub fn decrypt_and_hash(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, ChannelError> {
        let plaintext = self.cipher.decrypt_with_ad(&self.h, ciphertext)?;
        self.mix_hash(ciphertext)?;
        Ok(plaintext)
    }

    /// Split: returns two CipherState objects for bidirectional transport.
    /// HKDF(ck, zerolen) -> (temp_k1, temp_k2)
    pub fn split(&self) -> Result<(CipherState, CipherState), ChannelError> {
        let outputs = hkdf_sha256(&self.ck, &[], 2)?;
        let mut k1 = [0u8; 32];
        let mut k2 = [0u8; 32];
        k1.copy_from_slice(&outputs[0][..32]);
        k2.copy_from_slice(&outputs[1][..32]);
        Ok((
            CipherState::initialize_key(Some(k1)),
            CipherState::initialize_key(Some(k2)),
        ))
    }

    /// Get the current handshake hash (used as channel binding after handshake).
    pub fn handshake_hash(&self) -> &[u8] {
        &self.h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_short_name() {
        let name = b"Noise_XX_25519_ChaChaPoly_SHA256";
        let ss = SymmetricState::initialize(name);
        // Name is exactly 32 bytes, so h should be the name itself (zero-padded)
        assert_eq!(ss.h.len(), 32);
        assert_eq!(&ss.h, name.as_slice());
        assert_eq!(ss.ck, ss.h);
    }

    #[test]
    fn test_initialize_long_name() {
        let name = b"Noise_XX_25519_ChaChaPoly_SHA256_this_is_longer_than_32_bytes!!!";
        let ss = SymmetricState::initialize(name);
        assert_eq!(ss.h.len(), 32);
        // Should be SHA-256 of the name
        let expected = openssl_digest(DigestAlgorithm::Sha256, name).unwrap();
        assert_eq!(ss.h, expected);
    }

    #[test]
    fn test_mix_hash() {
        let mut ss = SymmetricState::initialize(b"test");
        let h_before = ss.h.clone();
        ss.mix_hash(b"data").unwrap();
        assert_ne!(ss.h, h_before);
        assert_eq!(ss.h.len(), 32);
    }

    #[test]
    fn test_mix_key_sets_cipher() {
        let mut ss = SymmetricState::initialize(b"test");
        assert!(!ss.cipher.has_key());
        ss.mix_key(&[0x42; 32]).unwrap();
        assert!(ss.cipher.has_key());
    }

    #[test]
    fn test_encrypt_and_hash_without_key() {
        let mut ss = SymmetricState::initialize(b"test");
        // Without a key, encrypt_and_hash returns plaintext
        let ct = ss.encrypt_and_hash(b"hello").unwrap();
        assert_eq!(ct, b"hello");
    }

    #[test]
    fn test_encrypt_and_hash_with_key() {
        let mut ss = SymmetricState::initialize(b"test");
        ss.mix_key(&[0x42; 32]).unwrap();
        let ct = ss.encrypt_and_hash(b"hello").unwrap();
        // With a key, ciphertext should differ from plaintext
        assert_ne!(ct.as_slice(), b"hello");
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        // Two symmetric states initialized identically
        let mut enc_ss = SymmetricState::initialize(b"test");
        let mut dec_ss = SymmetricState::initialize(b"test");

        // Mix same key material
        enc_ss.mix_key(&[0x42; 32]).unwrap();
        dec_ss.mix_key(&[0x42; 32]).unwrap();

        let ct = enc_ss.encrypt_and_hash(b"secret").unwrap();
        let pt = dec_ss.decrypt_and_hash(&ct).unwrap();
        assert_eq!(pt, b"secret");
    }

    #[test]
    fn test_split_produces_two_cipher_states() {
        let mut ss = SymmetricState::initialize(b"test");
        ss.mix_key(&[0x42; 32]).unwrap();
        let (cs1, cs2) = ss.split().unwrap();
        assert!(cs1.has_key());
        assert!(cs2.has_key());
    }
}
