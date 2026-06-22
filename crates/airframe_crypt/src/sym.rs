use crate::error::AirframeCryptError;
use openssl::aes::{unwrap_key, wrap_key, AesKey};
use openssl::symm::{Cipher, Crypter, Mode};

pub trait KeyWrapper {
    /// Wrap (encrypt) the given plaintext key data.
    /// - `plaintext`: must be ≥16 bytes and a multiple of 8 bytes for RFC 3394.
    fn wrap_key(&self, plaintext: &[u8]) -> Result<Vec<u8>, AirframeCryptError>;

    /// Unwrap (decrypt) the given wrapped key data.
    /// - `wrapped`: must be ≥24 bytes and a multiple of 8 bytes for RFC 3394.
    fn unwrap_key(&self, wrapped: &[u8]) -> Result<Vec<u8>, AirframeCryptError>;
}

#[derive(zeroize::ZeroizeOnDrop)]
pub struct OpenSSLKeyWrapper {
    kek: Vec<u8>,
}

impl OpenSSLKeyWrapper {
    /// Create a new OpenSSLKeyWrapper with the given Key Encryption Key (KEK)
    /// The KEK must be a valid AES key (16, 24, or 32 bytes)
    pub fn new(kek: Vec<u8>) -> Result<Self, AirframeCryptError> {
        // Validate KEK length (must be 16, 24, or 32 bytes for AES-128, AES-192, or AES-256)
        if kek.len() != 16 && kek.len() != 24 && kek.len() != 32 {
            return Err(AirframeCryptError::InvalidLength(format!(
                "KEK length must be 16, 24, or 32 bytes, got {}",
                kek.len()
            )));
        }
        Ok(Self { kek })
    }
}

impl KeyWrapper for OpenSSLKeyWrapper {
    fn wrap_key(&self, plaintext: &[u8]) -> Result<Vec<u8>, AirframeCryptError> {
        if plaintext.len() < 16 || !plaintext.len().is_multiple_of(8) {
            return Err(AirframeCryptError::InvalidLength(format!(
                "Plaintext length must be ≥16 and multiple of 8 bytes, got {}",
                plaintext.len()
            )));
        }
        // Prepare AES key schedule for encryption
        let aes_key = AesKey::new_encrypt(&self.kek)
            .map_err(|_| AirframeCryptError::OpenSSLError(openssl::error::ErrorStack::get()))?;
        // Output buffer: wrapped length = plaintext.len() + 8
        let mut out = vec![0u8; plaintext.len() + 8];
        // Use default IV (None) => default of 0xA6A6A6A6A6A6A6A6 per RFC 3394
        let written = wrap_key(&aes_key, None, &mut out, plaintext)
            .map_err(|_| AirframeCryptError::OpenSSLError(openssl::error::ErrorStack::get()))?;
        out.truncate(written);
        Ok(out)
    }

    fn unwrap_key(&self, wrapped: &[u8]) -> Result<Vec<u8>, AirframeCryptError> {
        // RFC 3394: wrapped length >=24 (i.e., plaintext≥16 yields wrapped≥24) and multiple of 8
        if wrapped.len() < 24 || !wrapped.len().is_multiple_of(8) {
            return Err(AirframeCryptError::InvalidLength(format!(
                "Wrapped length must be ≥24 and multiple of 8 bytes, got {}",
                wrapped.len()
            )));
        }
        // Prepare AES key schedule for decryption
        let aes_key = AesKey::new_decrypt(&self.kek)
            .map_err(|_| AirframeCryptError::OpenSSLError(openssl::error::ErrorStack::get()))?;
        // Output buffer size = wrapped.len() - 8
        let mut out = vec![0u8; wrapped.len() - 8];
        let written = unwrap_key(&aes_key, None, &mut out, wrapped)
            .map_err(|_| AirframeCryptError::OpenSSLError(openssl::error::ErrorStack::get()))?;
        out.truncate(written);
        Ok(out)
    }
}

pub trait SymmetricCrypter {
    /// Encrypts the given plaintext in one shot, returning ciphertext (including padding or tag).
    /// The implementor has been constructed/configured with a specific key and parameters.
    /// For AEAD modes, optional AAD can be provided and will be authenticated.
    fn encrypt(&self, plaintext: &[u8], aad: Option<&[u8]>) -> Result<Vec<u8>, AirframeCryptError>;

    /// Decrypts the given ciphertext in one shot, returning plaintext (with padding removed).
    /// For AEAD modes, the same AAD used during encryption must be provided.
    fn decrypt(&self, ciphertext: &[u8], aad: Option<&[u8]>)
        -> Result<Vec<u8>, AirframeCryptError>;
}

/// One-shot symmetric algorithm selector for convenience APIs.
#[derive(Debug, Clone, Copy)]
pub enum SymmetricAlgorithm {
    AesGcm,
    AesCbc,
    ChaCha20Poly1305,
    AesXts,
    CamelliaCbc,
}

/// One-shot encrypt function akin to hash.rs convenience functions.
/// For AEAD algorithms (AES-GCM, ChaCha20-Poly1305), the returned ciphertext has the 16-byte tag appended.
pub fn openssl_sym_encrypt(
    alg: SymmetricAlgorithm,
    key: &[u8],
    iv_or_nonce: &[u8],
    plaintext: &[u8],
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, AirframeCryptError> {
    match alg {
        SymmetricAlgorithm::AesGcm => {
            OpenSSLAesGcm::new(key.to_vec(), iv_or_nonce.to_vec())?.encrypt(plaintext, aad)
        }
        SymmetricAlgorithm::AesCbc => {
            OpenSSLAesCbc::new(key.to_vec(), iv_or_nonce.to_vec()).encrypt(plaintext, None)
        }
        SymmetricAlgorithm::ChaCha20Poly1305 => {
            OpenSSLChaCha20Poly1305::new(key.to_vec(), iv_or_nonce.to_vec())?
                .encrypt(plaintext, aad)
        }
        SymmetricAlgorithm::AesXts => {
            OpenSSLAesXts::new(key.to_vec(), iv_or_nonce.to_vec())?.encrypt(plaintext, None)
        }
        SymmetricAlgorithm::CamelliaCbc => {
            OpenSSLCamelliaCbc::new(key.to_vec(), iv_or_nonce.to_vec())?.encrypt(plaintext, None)
        }
    }
}

/// One-shot decrypt function akin to hash.rs convenience functions.
/// For AEAD algorithms, the input ciphertext must include the appended 16-byte tag; pass the same AAD used at encryption.
pub fn openssl_sym_decrypt(
    alg: SymmetricAlgorithm,
    key: &[u8],
    iv_or_nonce: &[u8],
    ciphertext: &[u8],
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, AirframeCryptError> {
    match alg {
        SymmetricAlgorithm::AesGcm => {
            OpenSSLAesGcm::new(key.to_vec(), iv_or_nonce.to_vec())?.decrypt(ciphertext, aad)
        }
        SymmetricAlgorithm::AesCbc => {
            OpenSSLAesCbc::new(key.to_vec(), iv_or_nonce.to_vec()).decrypt(ciphertext, None)
        }
        SymmetricAlgorithm::ChaCha20Poly1305 => {
            OpenSSLChaCha20Poly1305::new(key.to_vec(), iv_or_nonce.to_vec())?
                .decrypt(ciphertext, aad)
        }
        SymmetricAlgorithm::AesXts => {
            OpenSSLAesXts::new(key.to_vec(), iv_or_nonce.to_vec())?.decrypt(ciphertext, None)
        }
        SymmetricAlgorithm::CamelliaCbc => {
            OpenSSLCamelliaCbc::new(key.to_vec(), iv_or_nonce.to_vec())?.decrypt(ciphertext, None)
        }
    }
}

#[derive(zeroize::ZeroizeOnDrop)]
pub struct OpenSSLAesGcm {
    key: Vec<u8>,
    iv: Vec<u8>,
}

impl OpenSSLAesGcm {
    pub fn new(key: Vec<u8>, iv: Vec<u8>) -> Result<Self, AirframeCryptError> {
        if !(key.len() == 16 || key.len() == 24 || key.len() == 32) {
            return Err(AirframeCryptError::InvalidLength(format!(
                "AES-GCM key length must be 16, 24, or 32 bytes, got {}",
                key.len()
            )));
        }
        if iv.is_empty() {
            return Err(AirframeCryptError::InvalidLength(
                "AES-GCM IV/nonce must not be empty".into(),
            ));
        }
        Ok(Self { key, iv })
    }

    fn _determine_cipher(&self) -> Result<Cipher, AirframeCryptError> {
        match self.key.len() {
            16 => Ok(Cipher::aes_128_gcm()),
            24 => Ok(Cipher::aes_192_gcm()),
            32 => Ok(Cipher::aes_256_gcm()),
            _ => Err(AirframeCryptError::InvalidLength(format!(
                "AES key length must be 16, 24, or 32 bytes, got {}",
                self.key.len()
            ))),
        }
    }
}

impl SymmetricCrypter for OpenSSLAesGcm {
    fn encrypt(&self, plaintext: &[u8], aad: Option<&[u8]>) -> Result<Vec<u8>, AirframeCryptError> {
        // Determine the cipher based on key length
        let cipher = self._determine_cipher()?;

        // Create a crypter instance
        let mut crypter = Crypter::new(cipher, Mode::Encrypt, &self.key, Some(&self.iv))?;

        // Supply AAD if provided
        if let Some(a) = aad {
            crypter.aad_update(a)?;
        }

        // Allocate buffer for the ciphertext
        // For GCM, the output size is the same as the input size plus the tag size (16 bytes)
        let mut ciphertext = vec![0; plaintext.len() + cipher.block_size()];

        // Encrypt the plaintext
        let count = crypter.update(plaintext, &mut ciphertext)?;

        // Finalize the encryption
        let rest = crypter.finalize(&mut ciphertext[count..])?;

        // Get the tag (authentication tag)
        let mut tag = vec![0; 16]; // GCM tag is 16 bytes
        crypter.get_tag(&mut tag)?;

        // Resize the ciphertext to the actual size
        ciphertext.truncate(count + rest);

        // Append the tag to the ciphertext
        ciphertext.extend_from_slice(&tag);

        Ok(ciphertext)
    }

    fn decrypt(
        &self,
        ciphertext: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        // GCM tag is 16 bytes, so ciphertext must be at least that long
        if ciphertext.len() < 16 {
            return Err(AirframeCryptError::InvalidLength(format!(
                "Ciphertext too short for GCM mode, must be at least 16 bytes, got {}",
                ciphertext.len()
            )));
        }

        // Determine the cipher based on key length
        let cipher = self._determine_cipher()?;

        // Split the ciphertext and tag
        let tag_start = ciphertext.len() - 16;
        let actual_ciphertext = &ciphertext[..tag_start];
        let tag = &ciphertext[tag_start..];

        // Create a crypter instance
        let mut crypter = Crypter::new(cipher, Mode::Decrypt, &self.key, Some(&self.iv))?;

        // Supply AAD if provided
        if let Some(a) = aad {
            crypter.aad_update(a)?;
        }

        // Set the tag for verification
        crypter.set_tag(tag)?;

        // Allocate buffer for the plaintext
        let mut plaintext = vec![0; actual_ciphertext.len() + cipher.block_size()];

        // Decrypt the ciphertext
        let count = crypter.update(actual_ciphertext, &mut plaintext)?;

        // Finalize the decryption (this will verify the tag)
        let rest = crypter.finalize(&mut plaintext[count..])?;

        // Resize the plaintext to the actual size
        plaintext.truncate(count + rest);

        Ok(plaintext)
    }
}

#[derive(zeroize::ZeroizeOnDrop)]
pub struct OpenSSLAesCbc {
    key: Vec<u8>,
    iv: Vec<u8>,
}

impl OpenSSLAesCbc {
    pub fn new(key: Vec<u8>, iv: Vec<u8>) -> Self {
        Self { key, iv }
    }
    fn cipher(&self) -> Result<Cipher, AirframeCryptError> {
        match self.key.len() {
            16 => Ok(Cipher::aes_128_cbc()),
            24 => Ok(Cipher::aes_192_cbc()),
            32 => Ok(Cipher::aes_256_cbc()),
            _ => Err(AirframeCryptError::InvalidLength(format!(
                "AES CBC key length must be 16, 24, or 32 bytes, got {}",
                self.key.len()
            ))),
        }
    }
}

impl SymmetricCrypter for OpenSSLAesCbc {
    fn encrypt(
        &self,
        plaintext: &[u8],
        _aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        let cipher = self.cipher()?;
        let mut crypter = Crypter::new(cipher, Mode::Encrypt, &self.key, Some(&self.iv))?;
        let mut out = vec![0u8; plaintext.len() + cipher.block_size()];
        let n = crypter.update(plaintext, &mut out)?;
        let f = crypter.finalize(&mut out[n..])?;
        out.truncate(n + f);
        Ok(out)
    }
    fn decrypt(
        &self,
        ciphertext: &[u8],
        _aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        let cipher = self.cipher()?;
        let mut crypter = Crypter::new(cipher, Mode::Decrypt, &self.key, Some(&self.iv))?;
        let mut out = vec![0u8; ciphertext.len() + cipher.block_size()];
        let n = crypter.update(ciphertext, &mut out)?;
        let f = crypter.finalize(&mut out[n..])?;
        out.truncate(n + f);
        Ok(out)
    }
}

#[derive(zeroize::ZeroizeOnDrop)]
pub struct OpenSSLChaCha20Poly1305 {
    key: Vec<u8>,
    iv: Vec<u8>,
}

impl OpenSSLChaCha20Poly1305 {
    pub fn new(key: Vec<u8>, iv: Vec<u8>) -> Result<Self, AirframeCryptError> {
        if key.len() != 32 {
            return Err(AirframeCryptError::InvalidLength(format!(
                "ChaCha20-Poly1305 key must be 32 bytes, got {}",
                key.len()
            )));
        }
        // Nonce is typically 12 bytes
        Ok(Self { key, iv })
    }
}

impl SymmetricCrypter for OpenSSLChaCha20Poly1305 {
    fn encrypt(&self, plaintext: &[u8], aad: Option<&[u8]>) -> Result<Vec<u8>, AirframeCryptError> {
        let cipher = Cipher::chacha20_poly1305();
        let mut crypter = Crypter::new(cipher, Mode::Encrypt, &self.key, Some(&self.iv))?;
        if let Some(a) = aad {
            crypter.aad_update(a)?;
        }
        let mut out = vec![0u8; plaintext.len() + cipher.block_size()];
        let n = crypter.update(plaintext, &mut out)?;
        let f = crypter.finalize(&mut out[n..])?;
        out.truncate(n + f);
        let mut tag = [0u8; 16];
        crypter.get_tag(&mut tag)?;
        out.extend_from_slice(&tag);
        Ok(out)
    }
    fn decrypt(
        &self,
        ciphertext: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        if ciphertext.len() < 16 {
            return Err(AirframeCryptError::InvalidLength(
                "Ciphertext too short for ChaCha20-Poly1305".into(),
            ));
        }
        let cipher = Cipher::chacha20_poly1305();
        let tag_pos = ciphertext.len() - 16;
        let ct = &ciphertext[..tag_pos];
        let tag = &ciphertext[tag_pos..];
        let mut crypter = Crypter::new(cipher, Mode::Decrypt, &self.key, Some(&self.iv))?;
        if let Some(a) = aad {
            crypter.aad_update(a)?;
        }
        crypter.set_tag(tag)?;
        let mut out = vec![0u8; ct.len() + cipher.block_size()];
        let n = crypter.update(ct, &mut out)?;
        let f = crypter.finalize(&mut out[n..])?;
        out.truncate(n + f);
        Ok(out)
    }
}

#[derive(zeroize::ZeroizeOnDrop)]
pub struct OpenSSLAesXts {
    key: Vec<u8>,
    iv: Vec<u8>, // XTS tweak, must be 16 bytes
}

impl OpenSSLAesXts {
    pub fn new(key: Vec<u8>, iv: Vec<u8>) -> Result<Self, AirframeCryptError> {
        if !(key.len() == 32 || key.len() == 64) {
            return Err(AirframeCryptError::InvalidLength(format!(
                "AES-XTS key must be 32 or 64 bytes, got {}",
                key.len()
            )));
        }
        if iv.len() != 16 {
            return Err(AirframeCryptError::InvalidLength(format!(
                "AES-XTS IV/tweak must be 16 bytes, got {}",
                iv.len()
            )));
        }
        Ok(Self { key, iv })
    }
    fn cipher(&self) -> Cipher {
        match self.key.len() {
            32 => Cipher::aes_128_xts(),
            _ => Cipher::aes_256_xts(),
        }
    }
}

impl SymmetricCrypter for OpenSSLAesXts {
    fn encrypt(
        &self,
        plaintext: &[u8],
        _aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        let cipher = self.cipher();
        let mut crypter = Crypter::new(cipher, Mode::Encrypt, &self.key, Some(&self.iv))?;
        let mut out = vec![0u8; plaintext.len() + cipher.block_size()];
        let n = crypter.update(plaintext, &mut out)?;
        let f = crypter.finalize(&mut out[n..])?;
        out.truncate(n + f);
        Ok(out)
    }
    fn decrypt(
        &self,
        ciphertext: &[u8],
        _aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        let cipher = self.cipher();
        let mut crypter = Crypter::new(cipher, Mode::Decrypt, &self.key, Some(&self.iv))?;
        let mut out = vec![0u8; ciphertext.len() + cipher.block_size()];
        let n = crypter.update(ciphertext, &mut out)?;
        let f = crypter.finalize(&mut out[n..])?;
        out.truncate(n + f);
        Ok(out)
    }
}

#[derive(zeroize::ZeroizeOnDrop)]
pub struct OpenSSLCamelliaCbc {
    key: Vec<u8>,
    iv: Vec<u8>,
}

impl OpenSSLCamelliaCbc {
    pub fn new(key: Vec<u8>, iv: Vec<u8>) -> Result<Self, AirframeCryptError> {
        // Camellia common key sizes: 16 or 32 via rust-openssl convenience
        match key.len() {
            16 | 32 => Ok(Self { key, iv }),
            _ => Err(AirframeCryptError::InvalidLength(format!(
                "Camellia CBC key must be 16 or 32 bytes, got {}",
                key.len()
            ))),
        }
    }

    #[cfg(not(target_os = "android"))]
    fn cipher(&self) -> Cipher {
        match self.key.len() {
            16 => Cipher::camellia_128_cbc(),
            _ => Cipher::camellia_256_cbc(),
        }
    }

    #[cfg(target_os = "android")]
    fn cipher(&self) -> Cipher {
        // Some Android/vendored OpenSSL configurations do not expose Camellia.
        // Fall back to AES-CBC so the crate remains buildable for Android.
        match self.key.len() {
            16 => Cipher::aes_128_cbc(),
            _ => Cipher::aes_256_cbc(),
        }
    }
}

impl SymmetricCrypter for OpenSSLCamelliaCbc {
    fn encrypt(
        &self,
        plaintext: &[u8],
        _aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        let cipher = self.cipher();
        let mut crypter = Crypter::new(cipher, Mode::Encrypt, &self.key, Some(&self.iv))?;
        let mut out = vec![0u8; plaintext.len() + cipher.block_size()];
        let n = crypter.update(plaintext, &mut out)?;
        let f = crypter.finalize(&mut out[n..])?;
        out.truncate(n + f);
        Ok(out)
    }
    fn decrypt(
        &self,
        ciphertext: &[u8],
        _aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AirframeCryptError> {
        let cipher = self.cipher();
        let mut crypter = Crypter::new(cipher, Mode::Decrypt, &self.key, Some(&self.iv))?;
        let mut out = vec![0u8; ciphertext.len() + cipher.block_size()];
        let n = crypter.update(ciphertext, &mut out)?;
        let f = crypter.finalize(&mut out[n..])?;
        out.truncate(n + f);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to create a test key for AES-GCM
    fn create_test_aes_key(size: usize) -> Vec<u8> {
        let mut key = Vec::with_capacity(size);
        for i in 0..size {
            key.push(i as u8);
        }
        key
    }

    // Helper function to create a test IV for AES-GCM
    fn create_test_iv(size: usize) -> Vec<u8> {
        let mut iv = Vec::with_capacity(size);
        for i in 0..size {
            iv.push((i + 100) as u8);
        }
        iv
    }

    #[test]
    fn test_aes_gcm_encrypt_decrypt_basic() {
        // Test basic encryption and decryption
        let key = create_test_aes_key(16); // AES-128
        let iv = create_test_iv(12); // GCM typically uses 12-byte IV
        let plaintext = b"Hello, world!";

        let crypter = OpenSSLAesGcm::new(key, iv).unwrap();
        let ciphertext = crypter.encrypt(plaintext, None).unwrap();

        // Ciphertext should be different from plaintext
        assert_ne!(&ciphertext[..plaintext.len()], plaintext);

        // Ciphertext should be longer than plaintext (includes tag)
        assert!(ciphertext.len() > plaintext.len());

        // Decrypt and verify
        let decrypted = crypter.decrypt(&ciphertext, None).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_aes_gcm_different_key_sizes() {
        // Test with different key sizes
        let iv = create_test_iv(12);
        let plaintext = b"Testing different key sizes";

        // Test AES-128 (16 bytes)
        let key16 = create_test_aes_key(16);
        let crypter16 = OpenSSLAesGcm::new(key16, iv.clone()).unwrap();
        let ciphertext16 = crypter16.encrypt(plaintext, None).unwrap();
        let decrypted16 = crypter16.decrypt(&ciphertext16, None).unwrap();
        assert_eq!(decrypted16, plaintext);

        // Test AES-192 (24 bytes)
        let key24 = create_test_aes_key(24);
        let crypter24 = OpenSSLAesGcm::new(key24, iv.clone()).unwrap();
        let ciphertext24 = crypter24.encrypt(plaintext, None).unwrap();
        let decrypted24 = crypter24.decrypt(&ciphertext24, None).unwrap();
        assert_eq!(decrypted24, plaintext);

        // Test AES-256 (32 bytes)
        let key32 = create_test_aes_key(32);
        let crypter32 = OpenSSLAesGcm::new(key32, iv.clone()).unwrap();
        let ciphertext32 = crypter32.encrypt(plaintext, None).unwrap();
        let decrypted32 = crypter32.decrypt(&ciphertext32, None).unwrap();
        assert_eq!(decrypted32, plaintext);

        // Different key sizes should produce different ciphertexts
        assert_ne!(ciphertext16, ciphertext24);
        assert_ne!(ciphertext16, ciphertext32);
        assert_ne!(ciphertext24, ciphertext32);
    }

    #[test]
    fn test_aes_gcm_invalid_key_size() {
        // Test with invalid key size
        let key = create_test_aes_key(20); // Not a valid AES key size
        let iv = create_test_iv(12);
        let plaintext = b"This should fail";

        let crypter = OpenSSLAesGcm::new(key, iv);
        // Still an error at construction or encrypt; unwrap construction only when needed
        let result = crypter.and_then(|c| c.encrypt(plaintext, None));

        // Encryption should fail with InvalidLength error
        assert!(result.is_err());
        if let Err(err) = result {
            match err {
                AirframeCryptError::InvalidLength(_) => {} // Expected error type
                _ => panic!("Expected InvalidLength error, got {:?}", err),
            }
        }
    }

    #[test]
    fn test_aes_gcm_tampered_ciphertext() {
        // Test that tampering with the ciphertext causes decryption to fail
        let key = create_test_aes_key(16);
        let iv = create_test_iv(12);
        let plaintext = b"This message will be tampered with";

        let crypter = OpenSSLAesGcm::new(key, iv).unwrap();
        let mut ciphertext = crypter.encrypt(plaintext, None).unwrap();

        // Tamper with the ciphertext (not the tag)
        ciphertext[0] ^= 0xFF;

        // Decryption should fail
        let result = crypter.decrypt(&ciphertext, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_aes_gcm_tampered_tag() {
        // Test that tampering with the tag causes decryption to fail
        let key = create_test_aes_key(16);
        let iv = create_test_iv(12);
        let plaintext = b"This message has a tampered tag";

        let crypter = OpenSSLAesGcm::new(key, iv).unwrap();
        let mut ciphertext = crypter.encrypt(plaintext, None).unwrap();

        // Tamper with the tag (last 16 bytes)
        let tag_start = ciphertext.len() - 16;
        ciphertext[tag_start] ^= 0xFF;

        // Decryption should fail
        let result = crypter.decrypt(&ciphertext, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_aes_gcm_empty_plaintext() {
        // Test encryption and decryption of empty plaintext
        let key = create_test_aes_key(16);
        let iv = create_test_iv(12);
        let plaintext = b"";

        let crypter = OpenSSLAesGcm::new(key, iv).unwrap();
        let ciphertext = crypter.encrypt(plaintext, None).unwrap();

        // Ciphertext should just be the tag (16 bytes)
        assert_eq!(ciphertext.len(), 16);

        // Decrypt and verify
        let decrypted = crypter.decrypt(&ciphertext, None).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    // Helper function to create a valid KEK
    fn create_test_kek(size: usize) -> Vec<u8> {
        let mut kek = Vec::with_capacity(size);
        for i in 0..size {
            kek.push(i as u8);
        }
        kek
    }

    // Helper function to create a valid plaintext key
    fn create_test_plaintext(size: usize) -> Vec<u8> {
        // Size must be ≥16 and a multiple of 8
        assert!(size >= 16 && size.is_multiple_of(8));
        let mut plaintext = Vec::with_capacity(size);
        for i in 0..size {
            plaintext.push((i + 10) as u8);
        }
        plaintext
    }

    #[test]
    fn test_new_valid_kek_lengths() {
        // Test valid KEK lengths (16, 24, 32 bytes)
        let kek16 = create_test_kek(16);
        let kek24 = create_test_kek(24);
        let kek32 = create_test_kek(32);

        assert!(OpenSSLKeyWrapper::new(kek16).is_ok());
        assert!(OpenSSLKeyWrapper::new(kek24).is_ok());
        assert!(OpenSSLKeyWrapper::new(kek32).is_ok());
    }

    #[test]
    fn test_new_invalid_kek_lengths() {
        // Test invalid KEK lengths
        let kek8 = create_test_kek(8);
        let kek20 = create_test_kek(20);
        let kek40 = create_test_kek(40);

        assert!(OpenSSLKeyWrapper::new(kek8).is_err());
        assert!(OpenSSLKeyWrapper::new(kek20).is_err());
        assert!(OpenSSLKeyWrapper::new(kek40).is_err());
    }

    #[test]
    fn test_wrap_key_basic() {
        // Test basic key wrapping
        let kek = create_test_kek(16);
        let plaintext = create_test_plaintext(16);

        let wrapper = OpenSSLKeyWrapper::new(kek).unwrap();
        let wrapped = wrapper.wrap_key(&plaintext).unwrap();

        // Wrapped key should be 8 bytes longer than plaintext
        assert_eq!(wrapped.len(), plaintext.len() + 8);

        // Wrapped key should be different from plaintext
        assert_ne!(wrapped[8..], plaintext);
    }

    #[test]
    fn test_wrap_key_invalid_lengths() {
        // Test invalid plaintext lengths
        let kek = create_test_kek(16);
        let wrapper = OpenSSLKeyWrapper::new(kek).unwrap();

        // Too short
        let plaintext_short = create_test_kek(8);
        assert!(wrapper.wrap_key(&plaintext_short).is_err());

        // Not a multiple of 8
        let plaintext_odd = create_test_kek(20);
        assert!(wrapper.wrap_key(&plaintext_odd).is_err());
    }

    #[test]
    fn test_unwrap_key_basic() {
        // Test basic key unwrapping
        let kek = create_test_kek(16);
        let plaintext = create_test_plaintext(16);

        let wrapper = OpenSSLKeyWrapper::new(kek).unwrap();
        let wrapped = wrapper.wrap_key(&plaintext).unwrap();
        let unwrapped = wrapper.unwrap_key(&wrapped).unwrap();

        // Unwrapped key should match original plaintext
        assert_eq!(unwrapped, plaintext);
    }

    #[test]
    fn test_unwrap_key_invalid_lengths() {
        // Test invalid wrapped key lengths
        let kek = create_test_kek(16);
        let wrapper = OpenSSLKeyWrapper::new(kek).unwrap();

        // Too short
        let wrapped_short = create_test_kek(16);
        assert!(wrapper.unwrap_key(&wrapped_short).is_err());

        // Not a multiple of 8
        let wrapped_odd = create_test_kek(25);
        assert!(wrapper.unwrap_key(&wrapped_odd).is_err());
    }

    #[test]
    fn test_unwrap_key_wrong_kek() {
        // Test unwrapping with wrong KEK
        let kek1 = create_test_kek(16);
        let kek2 = vec![0xFF; 16]; // Different KEK
        let plaintext = create_test_plaintext(16);

        let wrapper1 = OpenSSLKeyWrapper::new(kek1).unwrap();
        let wrapper2 = OpenSSLKeyWrapper::new(kek2).unwrap();

        let wrapped = wrapper1.wrap_key(&plaintext).unwrap();

        // Unwrapping with wrong KEK should fail
        assert!(wrapper2.unwrap_key(&wrapped).is_err());
    }

    #[test]
    fn test_roundtrip_different_key_sizes() {
        // Test roundtrip with different key sizes
        let kek = create_test_kek(16);
        let wrapper = OpenSSLKeyWrapper::new(kek).unwrap();

        // Test with different plaintext sizes (all ≥16 and multiples of 8)
        let sizes = [16, 24, 32, 40, 64, 128];

        for size in sizes.iter() {
            let plaintext = create_test_plaintext(*size);
            let wrapped = wrapper.wrap_key(&plaintext).unwrap();
            let unwrapped = wrapper.unwrap_key(&wrapped).unwrap();

            // Unwrapped key should match original plaintext
            assert_eq!(unwrapped, plaintext);
        }
    }

    #[test]
    fn test_roundtrip_different_kek_sizes() {
        // Test roundtrip with different KEK sizes
        let kek_sizes = [16, 24, 32];
        let plaintext = create_test_plaintext(32);

        for kek_size in kek_sizes.iter() {
            let kek = create_test_kek(*kek_size);
            let wrapper = OpenSSLKeyWrapper::new(kek).unwrap();

            let wrapped = wrapper.wrap_key(&plaintext).unwrap();
            let unwrapped = wrapper.unwrap_key(&wrapped).unwrap();

            // Unwrapped key should match original plaintext
            assert_eq!(unwrapped, plaintext);
        }
    }

    #[test]
    fn test_tampered_wrapped_key() {
        // Test that tampering with the wrapped key causes unwrapping to fail
        let kek = create_test_kek(16);
        let plaintext = create_test_plaintext(16);

        let wrapper = OpenSSLKeyWrapper::new(kek).unwrap();
        let mut wrapped = wrapper.wrap_key(&plaintext).unwrap();

        // Tamper with the wrapped key
        wrapped[0] ^= 0xFF;

        // Unwrapping should fail
        assert!(wrapper.unwrap_key(&wrapped).is_err());
    }

    #[test]
    fn test_aes_cbc_roundtrip() {
        let key = create_test_aes_key(32);
        let iv = create_test_iv(16);
        let plaintext = b"CBC mode test message";
        let crypter = OpenSSLAesCbc::new(key, iv);
        let ct = crypter.encrypt(plaintext, None).unwrap();
        assert_ne!(&ct[..plaintext.len()], plaintext);
        let pt = crypter.decrypt(&ct, None).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn test_chacha20poly1305_roundtrip_and_tamper() {
        let key = vec![0x11; 32];
        let iv = create_test_iv(12);
        let crypter = OpenSSLChaCha20Poly1305::new(key, iv).unwrap();
        let msg = b"ChaCha20-Poly1305 message";
        let mut ct = crypter.encrypt(msg, None).unwrap();
        let pt = crypter.decrypt(&ct, None).unwrap();
        assert_eq!(pt, msg);
        // Tamper with tag
        let tag_start = ct.len() - 16;
        ct[tag_start] ^= 1;
        assert!(crypter.decrypt(&ct, None).is_err());
    }

    #[test]
    fn test_aes_xts_constraints_and_roundtrip() {
        // 32-byte key => aes_128_xts
        let mut key32 = vec![0x22; 32];
        for byte in &mut key32[16..32] {
            *byte = 0x33;
        }
        let iv = create_test_iv(16);
        let xts = OpenSSLAesXts::new(key32, iv.clone()).unwrap();
        let msg = b"XTS Mode message";
        let ct = xts.encrypt(msg, None).unwrap();
        let pt = xts.decrypt(&ct, None).unwrap();
        assert_eq!(pt, msg);
        // Wrong iv length should error
        assert!(OpenSSLAesXts::new(vec![0x33; 64], vec![0u8; 12]).is_err());
    }

    #[test]
    fn test_camellia_cbc_roundtrip() {
        let key = vec![0x44; 16];
        let iv = create_test_iv(16);
        let cam = OpenSSLCamelliaCbc::new(key, iv).unwrap();
        let msg = b"Camellia CBC message";
        let ct = cam.encrypt(msg, None).unwrap();
        let pt = cam.decrypt(&ct, None).unwrap();
        assert_eq!(pt, msg);
    }
}
