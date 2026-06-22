use crate::error::AirframeCryptError;
use crate::suite::CipherSuite;
use crate::sym::SymmetricAlgorithm;
use crate::AlgorithmId;
use base64::Engine;
use core::marker::PhantomData;
use secrecy::{ExposeSecret, SecretBox, SecretSlice, SecretString};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// Recommended nonce lengths for supported algorithms.
fn recommended_nonce_len(alg: SymmetricAlgorithm) -> usize {
    match alg {
        // AEAD algorithms typically use 96-bit (12-byte) nonces
        SymmetricAlgorithm::AesGcm | SymmetricAlgorithm::ChaCha20Poly1305 => 12,
        // CBC and others often use 16 bytes IV
        SymmetricAlgorithm::AesCbc | SymmetricAlgorithm::CamelliaCbc => 16,
        // XTS uses a 16-byte tweak
        SymmetricAlgorithm::AesXts => 16,
    }
}

/// Helper type for JSON serialization of EnvelopeBytes.
#[derive(Serialize, Deserialize)]
struct EnvelopeBytesJson {
    alg: AlgorithmId,
    nonce: String,
    ciphertext: String,
}

/// An encrypted blob stored in secrecy types to reduce exposure in memory dumps.
/// Holds algorithm, nonce/IV, and ciphertext. Ciphertext for AEAD includes the tag (as produced by sym::openssl_sym_encrypt).
#[derive(Debug)]
pub struct EnvelopeBytes {
    pub alg: SymmetricAlgorithm,
    pub nonce: SecretBox<Vec<u8>>,
    pub ciphertext: SecretBox<Vec<u8>>,
}

impl EnvelopeBytes {
    /// Encrypts the given plaintext using the provided suite, algorithm, and key.
    /// - key and plaintext are accepted as secrecy types to avoid accidental exposure.
    /// - aad is optional additional authenticated data for AEAD modes.
    pub fn encrypt_with_suite<S: CipherSuite>(
        suite: &S,
        alg: SymmetricAlgorithm,
        key: &SecretSlice<u8>,
        plaintext: &SecretSlice<u8>,
        aad: Option<&[u8]>,
    ) -> Result<Self, AirframeCryptError> {
        let nonce_len = recommended_nonce_len(alg);
        let nonce = suite.random_bytes(nonce_len)?;
        let ct = suite.sym_encrypt(
            alg,
            key.expose_secret(),
            &nonce,
            plaintext.expose_secret(),
            aad,
        )?;
        Ok(EnvelopeBytes {
            alg,
            nonce: SecretBox::new(Box::new(nonce)),
            ciphertext: SecretBox::new(Box::new(ct)),
        })
    }

    /// Decrypts and returns the plaintext as `SecretVec<u8>`.
    pub fn decrypt_with_suite<S: CipherSuite>(
        &self,
        suite: &S,
        key: &SecretSlice<u8>,
        aad: Option<&[u8]>,
    ) -> Result<SecretBox<Vec<u8>>, AirframeCryptError> {
        let pt = suite.sym_decrypt(
            self.alg,
            key.expose_secret(),
            self.nonce.expose_secret(),
            self.ciphertext.expose_secret(),
            aad,
        )?;
        Ok(SecretBox::new(Box::new(pt)))
    }

    /// Expose the nonce/IV bytes for storage or transport.
    /// Note: exposing raw bytes may leak in logs; handle carefully.
    pub fn nonce_bytes(&self) -> &[u8] {
        self.nonce.expose_secret().as_slice()
    }

    /// Expose the ciphertext bytes for storage or transport.
    /// For AEAD ciphers, this includes the authentication tag as produced by sym_encrypt.
    pub fn ciphertext_bytes(&self) -> &[u8] {
        self.ciphertext.expose_secret().as_slice()
    }

    /// Serialize this envelope to a JSON string with base64-encoded nonce and ciphertext.
    pub fn to_json_string(&self) -> Result<String, AirframeCryptError> {
        let j = EnvelopeBytesJson {
            alg: AlgorithmId::from(self.alg),
            nonce: base64::engine::general_purpose::STANDARD.encode(self.nonce_bytes()),
            ciphertext: base64::engine::general_purpose::STANDARD.encode(self.ciphertext_bytes()),
        };
        serde_json::to_string(&j).map_err(|e| {
            AirframeCryptError::InvalidParameters(format!("json serialization failed: {}", e))
        })
    }

    /// Parse an envelope from a JSON string produced by to_json_string.
    pub fn from_json_str(s: &str) -> Result<Self, AirframeCryptError> {
        let parsed: EnvelopeBytesJson = serde_json::from_str(s).map_err(|e| {
            AirframeCryptError::InvalidParameters(format!("json parse failed: {}", e))
        })?;
        let aid = parsed.alg;
        let alg = <SymmetricAlgorithm as core::convert::TryFrom<AlgorithmId>>::try_from(aid)
            .map_err(|_| {
                AirframeCryptError::InvalidParameters("algorithm is not a symmetric cipher".into())
            })?;
        let nonce = base64::engine::general_purpose::STANDARD
            .decode(parsed.nonce)
            .map_err(|e| {
                AirframeCryptError::InvalidParameters(format!("bad base64 nonce: {}", e))
            })?;
        let ciphertext = base64::engine::general_purpose::STANDARD
            .decode(parsed.ciphertext)
            .map_err(|e| {
                AirframeCryptError::InvalidParameters(format!("bad base64 ciphertext: {}", e))
            })?;
        Ok(EnvelopeBytes {
            alg,
            nonce: SecretBox::new(Box::new(nonce)),
            ciphertext: SecretBox::new(Box::new(ciphertext)),
        })
    }
}

/// A convenience wrapper for storing an encrypted UTF-8 string.
#[derive(Debug)]
pub struct EnvelopeString {
    inner: EnvelopeBytes,
}

impl EnvelopeString {
    /// Encrypts a SecretString and returns an EnvelopeString.
    pub fn encrypt_with_suite<S: CipherSuite>(
        suite: &S,
        alg: SymmetricAlgorithm,
        key: &SecretSlice<u8>,
        plaintext: &SecretString,
        aad: Option<&[u8]>,
    ) -> Result<Self, AirframeCryptError> {
        let bytes_box: Box<[u8]> = plaintext
            .expose_secret()
            .as_bytes()
            .to_vec()
            .into_boxed_slice();
        let bytes = SecretSlice::new(bytes_box);
        let inner = EnvelopeBytes::encrypt_with_suite(suite, alg, key, &bytes, aad)?;
        Ok(EnvelopeString { inner })
    }

    /// Decrypts into a SecretString (valid UTF-8 expected).
    pub fn decrypt_with_suite<S: CipherSuite>(
        &self,
        suite: &S,
        key: &SecretSlice<u8>,
        aad: Option<&[u8]>,
    ) -> Result<SecretString, AirframeCryptError> {
        let bytes = self.inner.decrypt_with_suite(suite, key, aad)?;
        match String::from_utf8(bytes.expose_secret().clone()) {
            Ok(s) => Ok(SecretString::new(s.into())),
            Err(_) => Err(AirframeCryptError::InvalidParameters(
                "Decrypted bytes are not valid UTF-8".into(),
            )),
        }
    }

    pub fn alg(&self) -> SymmetricAlgorithm {
        self.inner.alg
    }
    pub fn nonce(&self) -> &SecretBox<Vec<u8>> {
        &self.inner.nonce
    }
    pub fn ciphertext(&self) -> &SecretBox<Vec<u8>> {
        &self.inner.ciphertext
    }

    /// Convenience: expose raw nonce bytes.
    pub fn nonce_bytes(&self) -> &[u8] {
        self.inner.nonce_bytes()
    }

    /// Convenience: expose raw ciphertext bytes for writing to disk.
    pub fn ciphertext_bytes(&self) -> &[u8] {
        self.inner.ciphertext_bytes()
    }

    /// Serialize to JSON (delegates to inner).
    pub fn to_json_string(&self) -> Result<String, AirframeCryptError> {
        self.inner.to_json_string()
    }

    /// Parse from JSON (delegates to EnvelopeBytes::from_json_str).
    pub fn from_json_str(s: &str) -> Result<Self, AirframeCryptError> {
        Ok(EnvelopeString {
            inner: EnvelopeBytes::from_json_str(s)?,
        })
    }

    pub fn into_inner(self) -> EnvelopeBytes {
        self.inner
    }
}

/// Generic, serde-powered envelope storage for any serializable type.
/// Codec abstraction to encode/decode values into bytes without exposing plaintext more than necessary.
pub trait Codec<T> {
    type Error: core::fmt::Display;
    fn encode(value: &T) -> Result<Vec<u8>, Self::Error>;
    fn decode(bytes: &[u8]) -> Result<T, Self::Error>;
}

pub struct VarintCodec;
impl Codec<u64> for VarintCodec {
    type Error = &'static str;
    fn encode(v: &u64) -> Result<Vec<u8>, Self::Error> {
        Ok(encode_varint(*v))
    }
    fn decode(bytes: &[u8]) -> Result<u64, Self::Error> {
        decode_varint(bytes).ok_or("bad varint")
    }
}

/// Unsigned LEB128-style varint encoding for u64.
/// Encodes in 7-bit groups with MSB as continuation bit.
fn encode_varint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(10);
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
    out
}

/// Decodes a u64 from LEB128-style varint bytes.
/// Returns None if the input is not a well-formed single varint (e.g., overflow,
/// missing termination, or extra trailing bytes beyond one varint).
fn decode_varint(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() {
        return None;
    }

    let mut result: u64 = 0;
    let mut shift: u32 = 0;

    for (i, &b) in bytes.iter().enumerate() {
        let payload = (b & 0x7F) as u64;
        if shift >= 64 {
            return None;
        }
        if payload != 0 && (shift >= 64 || (payload << shift) >> shift != payload) {
            return None;
        }
        result |= payload << shift;
        if (b & 0x80) == 0 {
            if i != bytes.len() - 1 {
                return None;
            }
            return Some(result);
        }
        shift += 7;
        if i >= 9 && (b & 0x80) != 0 {
            return None;
        }
    }
    None
}

/// Bincode-based codec for any serde-serializable type.
pub struct BincodeCodec;

impl<T> Codec<T> for BincodeCodec
where
    T: Serialize + DeserializeOwned,
{
    type Error = bincode::Error;

    fn encode(value: &T) -> Result<Vec<u8>, Self::Error> {
        bincode::serialize(value)
    }

    fn decode(bytes: &[u8]) -> Result<T, Self::Error> {
        bincode::deserialize(bytes)
    }
}

/// The generic envelope storage type parameterized by the plaintext type `T` and the serialization `Codec`.
pub struct Envelope<T, C> {
    inner: EnvelopeBytes,
    _marker: PhantomData<(T, C)>,
}

impl<T, C> Envelope<T, C> {
    pub fn alg(&self) -> SymmetricAlgorithm {
        self.inner.alg
    }
    pub fn nonce(&self) -> &SecretBox<Vec<u8>> {
        &self.inner.nonce
    }
    pub fn ciphertext(&self) -> &SecretBox<Vec<u8>> {
        &self.inner.ciphertext
    }

    /// Convenience: expose raw nonce bytes.
    pub fn nonce_bytes(&self) -> &[u8] {
        self.inner.nonce_bytes()
    }

    /// Convenience: expose raw ciphertext bytes for writing to disk.
    pub fn ciphertext_bytes(&self) -> &[u8] {
        self.inner.ciphertext_bytes()
    }

    /// Serialize to JSON (delegates to inner).
    pub fn to_json_string(&self) -> Result<String, AirframeCryptError> {
        self.inner.to_json_string()
    }

    /// Parse from JSON (delegates to EnvelopeBytes::from_json_str) and rebuild the typed envelope.
    pub fn from_json_str(s: &str) -> Result<Self, AirframeCryptError> {
        Ok(Envelope {
            inner: EnvelopeBytes::from_json_str(s)?,
            _marker: PhantomData,
        })
    }

    pub fn into_inner(self) -> EnvelopeBytes {
        self.inner
    }
}

impl<T, C> Envelope<T, C>
where
    C: Codec<T>,
{
    /// Encrypt any value using the provided codec to convert to bytes first.
    pub fn encrypt_with_suite<S: CipherSuite>(
        suite: &S,
        alg: SymmetricAlgorithm,
        key: &SecretSlice<u8>,
        value: &T,
        aad: Option<&[u8]>,
    ) -> Result<Self, AirframeCryptError> {
        let v = C::encode(value).map_err(|e| {
            AirframeCryptError::InvalidParameters(format!("serialization failed: {}", e))
        })?;
        let boxed: Box<[u8]> = v.into_boxed_slice();
        let secret = SecretSlice::new(boxed);
        let inner = EnvelopeBytes::encrypt_with_suite(suite, alg, key, &secret, aad)?;
        Ok(Envelope {
            inner,
            _marker: PhantomData,
        })
    }
}

impl<T, C> Envelope<T, C>
where
    C: Codec<T>,
{
    /// Decrypt and deserialize using the selected codec.
    pub fn decrypt_with_suite<S: CipherSuite>(
        &self,
        suite: &S,
        key: &SecretSlice<u8>,
        aad: Option<&[u8]>,
    ) -> Result<T, AirframeCryptError> {
        let bytes = self.inner.decrypt_with_suite(suite, key, aad)?;
        let value: T = C::decode(bytes.expose_secret().as_slice()).map_err(|e| {
            AirframeCryptError::InvalidParameters(format!("deserialization failed: {}", e))
        })?;
        Ok(value)
    }
}

/// Preferred alias using bincode for serde types.
pub type EnvelopeValue<T> = Envelope<T, BincodeCodec>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::suite::SoftwareCipherSuite;

    fn key32() -> SecretSlice<u8> {
        SecretSlice::new(vec![42u8; 32].into_boxed_slice())
    }

    #[test]
    fn roundtrip_aes_gcm_envelope_bytes() {
        let suite = SoftwareCipherSuite::new();
        let key = key32();
        let pt = SecretSlice::new(b"hello envelope".to_vec().into_boxed_slice());
        let stored =
            EnvelopeBytes::encrypt_with_suite(&suite, SymmetricAlgorithm::AesGcm, &key, &pt, None)
                .unwrap();
        let decrypted = stored.decrypt_with_suite(&suite, &key, None).unwrap();
        assert_eq!(decrypted.expose_secret(), b"hello envelope");
    }

    #[test]
    fn roundtrip_chacha20poly1305_envelope_string() {
        let suite = SoftwareCipherSuite::new();
        let key = key32();
        let s = SecretString::new("top envelope".to_string().into());
        let stored = EnvelopeString::encrypt_with_suite(
            &suite,
            SymmetricAlgorithm::ChaCha20Poly1305,
            &key,
            &s,
            None,
        )
        .unwrap();
        let out = stored.decrypt_with_suite(&suite, &key, None).unwrap();
        assert_eq!(out.expose_secret(), "top envelope");
    }

    #[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    struct Demo {
        a: u32,
        b: String,
    }

    #[test]
    fn roundtrip_generic_struct_bincode() {
        let suite = SoftwareCipherSuite::new();
        let key = key32();
        let value = Demo {
            a: 7,
            b: "seven".to_string(),
        };
        let stored = EnvelopeValue::encrypt_with_suite(
            &suite,
            SymmetricAlgorithm::AesGcm,
            &key,
            &value,
            None,
        )
        .unwrap();
        let out: Demo = stored.decrypt_with_suite(&suite, &key, None).unwrap();
        assert_eq!(out, value);
    }

    #[test]
    fn roundtrip_generic_primitive() {
        let suite = SoftwareCipherSuite::new();
        let key = key32();
        let value: u64 = 0xDEADBEEFCAFEBABE;
        let stored = EnvelopeValue::encrypt_with_suite(
            &suite,
            SymmetricAlgorithm::AesGcm,
            &key,
            &value,
            None,
        )
        .unwrap();
        let out: u64 = stored.decrypt_with_suite(&suite, &key, None).unwrap();
        assert_eq!(out, value);
    }

    #[test]
    fn varint_encode_decode_roundtrip() {
        let cases: [u64; 8] = [0, 1, 127, 128, 255, 300, 16384, u64::MAX];
        for &v in &cases {
            let enc = encode_varint(v);
            let dec = decode_varint(&enc).expect("decode");
            assert_eq!(dec, v);
        }
        // Ensure decode fails on trailing bytes
        let mut enc = encode_varint(42);
        enc.push(0x00);
        assert!(decode_varint(&enc).is_none());
    }

    #[test]
    fn roundtrip_varint_codec_with_encryption() {
        let suite = SoftwareCipherSuite::new();
        let key = key32();
        let values = [0u64, 1, 127, 128, 255, 300, 16384, u64::MAX];
        for &v in &values {
            let stored: Envelope<u64, VarintCodec> =
                Envelope::encrypt_with_suite(&suite, SymmetricAlgorithm::AesGcm, &key, &v, None)
                    .unwrap();
            let out: u64 = stored.decrypt_with_suite(&suite, &key, None).unwrap();
            assert_eq!(out, v);
        }
    }

    #[test]
    fn json_roundtrip_envelope_bytes() {
        let suite = SoftwareCipherSuite::new();
        let key = key32();
        let pt = SecretSlice::new(b"hello json".to_vec().into_boxed_slice());
        let env =
            EnvelopeBytes::encrypt_with_suite(&suite, SymmetricAlgorithm::AesGcm, &key, &pt, None)
                .unwrap();
        let json = env.to_json_string().unwrap();
        let parsed = EnvelopeBytes::from_json_str(&json).unwrap();
        // decrypt using the parsed envelope
        let out = parsed.decrypt_with_suite(&suite, &key, None).unwrap();
        assert_eq!(out.expose_secret(), b"hello json");
    }
}
