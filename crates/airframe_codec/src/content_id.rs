use airframe_crypt::hash::{openssl_digest, DigestAlgorithm};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContentId(#[serde(with = "serde_bytes")] pub Vec<u8>);

impl ContentId {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
    pub fn to_hex(&self) -> String {
        crate::basexx::base16_encode(&self.0)
    }
}

pub fn content_id_sha256(bytes: &[u8]) -> ContentId {
    let digest = openssl_digest(DigestAlgorithm::Sha256, bytes)
        .expect("sha256 hashing to be infallible with openssl");
    ContentId(digest)
}
