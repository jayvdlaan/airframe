use openssl::hash::{Hasher as OSSLHasher, MessageDigest};
use openssl::pkey::{PKey, Private};
use openssl::sign::Signer;

use crate::error::AirframeCryptError;

#[derive(Debug, Clone, Copy)]
pub enum DigestAlgorithm {
    Sha1,
    Sha256,
    Sha384,
    Sha512,
    Sha3_256,
    Sha3_384,
    Sha3_512,
    Blake2s256,
    Blake2b512,
}

impl DigestAlgorithm {
    fn to_md(self) -> Result<MessageDigest, AirframeCryptError> {
        use DigestAlgorithm::*;
        Ok(match self {
            Sha1 => MessageDigest::sha1(),
            Sha256 => MessageDigest::sha256(),
            Sha384 => MessageDigest::sha384(),
            Sha512 => MessageDigest::sha512(),
            Sha3_256 => MessageDigest::sha3_256(),
            Sha3_384 => MessageDigest::sha3_384(),
            Sha3_512 => MessageDigest::sha3_512(),
            Blake2s256 => MessageDigest::from_name("BLAKE2s256")
                .ok_or(AirframeCryptError::UnsupportedAlgorithm)?,
            Blake2b512 => MessageDigest::from_name("BLAKE2b512")
                .ok_or(AirframeCryptError::UnsupportedAlgorithm)?,
        })
    }
}

// One-shot helpers
pub fn openssl_digest(alg: DigestAlgorithm, data: &[u8]) -> Result<Vec<u8>, AirframeCryptError> {
    let md = alg.to_md()?;
    let mut hasher = OSSLHasher::new(md)?;
    hasher.update(data)?;
    Ok(hasher.finish()?.to_vec())
}

pub fn openssl_hmac(
    alg: DigestAlgorithm,
    key: &[u8],
    data: &[u8],
) -> Result<Vec<u8>, AirframeCryptError> {
    let md = alg.to_md()?;
    let pkey = PKey::hmac(key)?;
    let mut signer = Signer::new(md, &pkey)?;
    signer.update(data)?;
    Ok(signer.sign_to_vec()?)
}

// Stateful wrappers (streaming)
pub struct OpenSslDigestor {
    hasher: OSSLHasher,
}

impl OpenSslDigestor {
    pub fn new(alg: DigestAlgorithm) -> Result<Self, AirframeCryptError> {
        let md = alg.to_md()?;
        Ok(Self {
            hasher: OSSLHasher::new(md)?,
        })
    }
    pub fn update(&mut self, data: &[u8]) -> Result<(), AirframeCryptError> {
        self.hasher.update(data)?;
        Ok(())
    }
    pub fn finish(mut self) -> Result<Vec<u8>, AirframeCryptError> {
        Ok(self.hasher.finish()?.to_vec())
    }
}

pub struct OpenSslHmacKey {
    pkey: PKey<Private>,
}

impl OpenSslHmacKey {
    pub fn new(key_bytes: &[u8]) -> Result<Self, AirframeCryptError> {
        Ok(Self {
            pkey: PKey::hmac(key_bytes)?,
        })
    }
    pub fn signer<'a>(
        &'a self,
        alg: DigestAlgorithm,
    ) -> Result<OpenSslHmacSigner<'a>, AirframeCryptError> {
        let md = alg.to_md()?;
        let signer = Signer::new(md, &self.pkey)?;
        Ok(OpenSslHmacSigner { signer })
    }
}

pub struct OpenSslHmacSigner<'a> {
    signer: Signer<'a>,
}

impl<'a> OpenSslHmacSigner<'a> {
    pub fn update(&mut self, data: &[u8]) -> Result<(), AirframeCryptError> {
        self.signer.update(data)?;
        Ok(())
    }
    pub fn sign_to_vec(self) -> Result<Vec<u8>, AirframeCryptError> {
        Ok(self.signer.sign_to_vec()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    }

    #[test]
    fn test_sha256_quick_brown_fox() {
        let out = openssl_digest(
            DigestAlgorithm::Sha256,
            b"The quick brown fox jumps over the lazy dog",
        )
        .unwrap();
        assert_eq!(
            hex(&out),
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592"
        );
    }

    #[test]
    fn test_hmac_sha256_vector() {
        // RFC 4231 test case 1: key = 20 x 0x0b, data = "Hi There"
        let key = vec![0x0b; 20];
        let data = b"Hi There";
        let mac = openssl_hmac(DigestAlgorithm::Sha256, &key, data).unwrap();
        assert_eq!(
            hex(&mac),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn test_sha3_256_empty() {
        let out = openssl_digest(DigestAlgorithm::Sha3_256, b"").unwrap();
        // NIST SHA3-256("") digest
        assert_eq!(
            hex(&out),
            "a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a"
        );
    }

    #[test]
    fn test_blake2b512_empty() {
        let out = openssl_digest(DigestAlgorithm::Blake2b512, b"").unwrap();
        assert_eq!(
            hex(&out),
            "786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce"
        );
    }
}

#[cfg(test)]
mod streaming_tests {
    use super::*;
    fn hex(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    }

    #[test]
    fn test_streaming_digest_equals_one_shot() {
        let data1 = b"The quick brown ";
        let data2 = b"fox jumps over the lazy dog";
        let mut d = OpenSslDigestor::new(DigestAlgorithm::Sha256).unwrap();
        d.update(data1).unwrap();
        d.update(data2).unwrap();
        let out_stream = d.finish().unwrap();
        let mut combined = Vec::new();
        combined.extend_from_slice(data1);
        combined.extend_from_slice(data2);
        let out_once = openssl_digest(DigestAlgorithm::Sha256, &combined).unwrap();
        assert_eq!(hex(&out_stream), hex(&out_once));
    }

    #[test]
    fn test_streaming_hmac_equals_one_shot() {
        let key = vec![0x0b; 20];
        let data1 = b"Hi ";
        let data2 = b"There";
        let hkey = OpenSslHmacKey::new(&key).unwrap();
        let mut signer = hkey.signer(DigestAlgorithm::Sha256).unwrap();
        signer.update(data1).unwrap();
        signer.update(data2).unwrap();
        let mac_stream = signer.sign_to_vec().unwrap();
        let mac_once = openssl_hmac(DigestAlgorithm::Sha256, &key, b"Hi There").unwrap();
        assert_eq!(hex(&mac_stream), hex(&mac_once));
    }
}
