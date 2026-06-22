use crate::error::AirframeCryptError;
use openssl::encrypt::{Decrypter, Encrypter};
use openssl::hash::MessageDigest;
use openssl::nid::Nid;
use openssl::pkey::{Id, PKey, Private, Public};
use openssl::rsa::{Padding as RsaPadding, Rsa};
use openssl::sign::{Signer, Verifier};
use openssl::{
    derive::Deriver,
    ec::{EcGroup, EcKey},
};

// Algorithm enums following the design used in other modules (hash, sym, otp)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsymEncryptAlgorithm {
    RsaOaepSha256,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsymSignAlgorithm {
    RsaPssSha256,
    RsaPkcs1v15Sha256,
    Ed25519,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsymKexAlgorithm {
    X25519,
}

/// Helper: map to MessageDigest limited to SHA-2 family for RSA
fn sha2_md_sha256() -> MessageDigest {
    MessageDigest::sha256()
}

/// RSA key generation
pub fn openssl_rsa_generate(bits: u32) -> Result<PKey<Private>, AirframeCryptError> {
    // Commonly 2048 or 3072+; enforce a sane minimum (>= 2048)
    if bits < 2048 {
        return Err(AirframeCryptError::InvalidParameters(format!(
            "RSA bits must be >= 2048, got {}",
            bits
        )));
    }
    let rsa = Rsa::generate(bits)?;
    let pkey = PKey::from_rsa(rsa)?;
    Ok(pkey)
}

/// RSA-OAEP encrypt (SHA-256 + MGF1(SHA-256))
pub fn openssl_rsa_oaep_encrypt(
    public_key: &PKey<Public>,
    plaintext: &[u8],
) -> Result<Vec<u8>, AirframeCryptError> {
    if public_key.id() != Id::RSA {
        return Err(AirframeCryptError::InvalidParameters(
            "Public key is not RSA".into(),
        ));
    }
    let mut enc = Encrypter::new(public_key)?;
    enc.set_rsa_padding(RsaPadding::PKCS1_OAEP)?;
    enc.set_rsa_mgf1_md(sha2_md_sha256())?;
    enc.set_rsa_oaep_md(sha2_md_sha256())?;
    // No OAEP label
    let mut out = vec![0u8; enc.encrypt_len(plaintext)?];
    let n = enc.encrypt(plaintext, &mut out)?;
    out.truncate(n);
    Ok(out)
}

/// RSA-OAEP decrypt (SHA-256 + MGF1(SHA-256))
pub fn openssl_rsa_oaep_decrypt(
    private_key: &PKey<Private>,
    ciphertext: &[u8],
) -> Result<Vec<u8>, AirframeCryptError> {
    if private_key.id() != Id::RSA {
        return Err(AirframeCryptError::InvalidParameters(
            "Private key is not RSA".into(),
        ));
    }
    let mut dec = Decrypter::new(private_key)?;
    dec.set_rsa_padding(RsaPadding::PKCS1_OAEP)?;
    dec.set_rsa_mgf1_md(sha2_md_sha256())?;
    dec.set_rsa_oaep_md(sha2_md_sha256())?;
    let mut out = vec![0u8; dec.decrypt_len(ciphertext)?];
    let n = dec.decrypt(ciphertext, &mut out)?;
    out.truncate(n);
    Ok(out)
}

/// RSA-PSS sign (SHA-256, saltlen=auto)
pub fn openssl_rsa_pss_sign(
    private_key: &PKey<Private>,
    msg: &[u8],
) -> Result<Vec<u8>, AirframeCryptError> {
    if private_key.id() != Id::RSA {
        return Err(AirframeCryptError::InvalidParameters(
            "Private key is not RSA".into(),
        ));
    }
    let mut signer = Signer::new(sha2_md_sha256(), private_key)?;
    signer.set_rsa_padding(RsaPadding::PKCS1_PSS)?;
    signer.set_rsa_mgf1_md(sha2_md_sha256())?;
    signer.update(msg)?;
    Ok(signer.sign_to_vec()?)
}

/// RSA-PSS verify (SHA-256, saltlen=auto)
pub fn openssl_rsa_pss_verify(
    public_key: &PKey<Public>,
    msg: &[u8],
    sig: &[u8],
) -> Result<bool, AirframeCryptError> {
    if public_key.id() != Id::RSA {
        return Err(AirframeCryptError::InvalidParameters(
            "Public key is not RSA".into(),
        ));
    }
    let mut verifier = Verifier::new(sha2_md_sha256(), public_key)?;
    verifier.set_rsa_padding(RsaPadding::PKCS1_PSS)?;
    verifier.set_rsa_mgf1_md(sha2_md_sha256())?;
    verifier.update(msg)?;
    Ok(verifier.verify(sig)?)
}

/// PKCS#1 v1.5 sign/verify over SHA-256 (optional convenience)
pub fn openssl_rsa_pkcs1_v15_sign(
    private_key: &PKey<Private>,
    msg: &[u8],
) -> Result<Vec<u8>, AirframeCryptError> {
    if private_key.id() != Id::RSA {
        return Err(AirframeCryptError::InvalidParameters(
            "Private key is not RSA".into(),
        ));
    }
    let mut signer = Signer::new(sha2_md_sha256(), private_key)?;
    signer.update(msg)?;
    Ok(signer.sign_to_vec()?)
}

pub fn openssl_rsa_pkcs1_v15_verify(
    public_key: &PKey<Public>,
    msg: &[u8],
    sig: &[u8],
) -> Result<bool, AirframeCryptError> {
    if public_key.id() != Id::RSA {
        return Err(AirframeCryptError::InvalidParameters(
            "Public key is not RSA".into(),
        ));
    }
    let mut verifier = Verifier::new(sha2_md_sha256(), public_key)?;
    verifier.update(msg)?;
    Ok(verifier.verify(sig)?)
}

/// Ed25519 deterministic signatures
pub fn openssl_ed25519_generate() -> Result<PKey<Private>, AirframeCryptError> {
    Ok(PKey::generate_ed25519()?)
}

pub fn openssl_ed25519_public(pk: &PKey<Private>) -> Result<PKey<Public>, AirframeCryptError> {
    Ok(PKey::public_key_from_pem(&pk.public_key_to_pem()?)?)
}

pub fn openssl_ed25519_sign(
    private_key: &PKey<Private>,
    msg: &[u8],
) -> Result<Vec<u8>, AirframeCryptError> {
    if private_key.id() != Id::ED25519 {
        return Err(AirframeCryptError::InvalidParameters(
            "Private key is not Ed25519".into(),
        ));
    }
    // For Ed25519, use one-shot signing with no digest.
    let mut signer = Signer::new_without_digest(private_key)?;
    Ok(signer.sign_oneshot_to_vec(msg)?)
}

pub fn openssl_ed25519_verify(
    public_key: &PKey<Public>,
    msg: &[u8],
    sig: &[u8],
) -> Result<bool, AirframeCryptError> {
    if public_key.id() != Id::ED25519 {
        return Err(AirframeCryptError::InvalidParameters(
            "Public key is not Ed25519".into(),
        ));
    }
    let mut verifier = Verifier::new_without_digest(public_key)?;
    Ok(verifier.verify_oneshot(sig, msg)?)
}

/// X25519 key agreement
pub fn openssl_x25519_generate() -> Result<PKey<Private>, AirframeCryptError> {
    Ok(PKey::generate_x25519()?)
}

pub fn openssl_x25519_derive(
    my_private: &PKey<Private>,
    peer_public: &PKey<Public>,
) -> Result<Vec<u8>, AirframeCryptError> {
    if my_private.id() != Id::X25519 {
        return Err(AirframeCryptError::InvalidParameters(
            "Private key is not X25519".into(),
        ));
    }
    if peer_public.id() != Id::X25519 {
        return Err(AirframeCryptError::InvalidParameters(
            "Peer public key is not X25519".into(),
        ));
    }
    let mut deriver = Deriver::new(my_private)?;
    deriver.set_peer(peer_public)?;
    let secret = deriver.derive_to_vec()?;
    // Reject the all-zero shared secret produced by low-order peer public keys
    // (RFC 7748 §6.1). Protocols built on this (e.g. the Noise handshake in
    // airframe_channel) require a contributory DH, so an all-zero result must be
    // treated as an invalid peer key rather than a usable secret. Constant-time
    // OR-fold so the check does not branch on secret bytes.
    let nonzero = secret.iter().fold(0u8, |acc, &b| acc | b);
    if nonzero == 0 {
        return Err(AirframeCryptError::InvalidParameters(
            "X25519 shared secret is all zero (low-order peer public key)".into(),
        ));
    }
    Ok(secret)
}

/// ECDSA over NIST P-256 (example; can be extended to P-384/P-521)
pub fn openssl_ecdsa_p256_generate() -> Result<PKey<Private>, AirframeCryptError> {
    let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)?;
    let ec_key = EcKey::generate(&group)?;
    Ok(PKey::from_ec_key(ec_key)?)
}

pub fn openssl_ecdsa_sign_sha256(
    private_key: &PKey<Private>,
    msg: &[u8],
) -> Result<Vec<u8>, AirframeCryptError> {
    if private_key.id() != Id::EC {
        return Err(AirframeCryptError::InvalidParameters(
            "Private key is not EC".into(),
        ));
    }
    let mut signer = Signer::new(MessageDigest::sha256(), private_key)?;
    signer.update(msg)?;
    Ok(signer.sign_to_vec()?)
}

pub fn openssl_ecdsa_verify_sha256(
    public_key: &PKey<Public>,
    msg: &[u8],
    sig: &[u8],
) -> Result<bool, AirframeCryptError> {
    if public_key.id() != Id::EC {
        return Err(AirframeCryptError::InvalidParameters(
            "Public key is not EC".into(),
        ));
    }
    let mut verifier = Verifier::new(MessageDigest::sha256(), public_key)?;
    verifier.update(msg)?;
    Ok(verifier.verify(sig)?)
}

// Stateful contexts for RSA-PSS streaming sign/verify
pub struct RsaPssSigner<'a> {
    signer: Signer<'a>,
}

impl<'a> RsaPssSigner<'a> {
    pub fn update(&mut self, data: &[u8]) -> Result<(), AirframeCryptError> {
        self.signer.update(data)?;
        Ok(())
    }
    pub fn sign_to_vec(self) -> Result<Vec<u8>, AirframeCryptError> {
        Ok(self.signer.sign_to_vec()?)
    }
}

pub struct RsaPssVerifier<'a> {
    verifier: Verifier<'a>,
}

impl<'a> RsaPssVerifier<'a> {
    pub fn update(&mut self, data: &[u8]) -> Result<(), AirframeCryptError> {
        self.verifier.update(data)?;
        Ok(())
    }
    pub fn verify(self, sig: &[u8]) -> Result<bool, AirframeCryptError> {
        Ok(self.verifier.verify(sig)?)
    }
}

pub fn openssl_rsa_pss_signer<'a>(
    private_key: &'a PKey<Private>,
) -> Result<RsaPssSigner<'a>, AirframeCryptError> {
    if private_key.id() != Id::RSA {
        return Err(AirframeCryptError::InvalidParameters(
            "Private key is not RSA".into(),
        ));
    }
    let mut signer = Signer::new(sha2_md_sha256(), private_key)?;
    signer.set_rsa_padding(RsaPadding::PKCS1_PSS)?;
    signer.set_rsa_mgf1_md(sha2_md_sha256())?;
    Ok(RsaPssSigner { signer })
}

pub fn openssl_rsa_pss_verifier<'a>(
    public_key: &'a PKey<Public>,
) -> Result<RsaPssVerifier<'a>, AirframeCryptError> {
    if public_key.id() != Id::RSA {
        return Err(AirframeCryptError::InvalidParameters(
            "Public key is not RSA".into(),
        ));
    }
    let mut verifier = Verifier::new(sha2_md_sha256(), public_key)?;
    verifier.set_rsa_padding(RsaPadding::PKCS1_PSS)?;
    verifier.set_rsa_mgf1_md(sha2_md_sha256())?;
    Ok(RsaPssVerifier { verifier })
}

// Enum-dispatch helpers mirroring other modules' design
pub fn openssl_asym_encrypt(
    alg: AsymEncryptAlgorithm,
    public_key: &PKey<Public>,
    plaintext: &[u8],
) -> Result<Vec<u8>, AirframeCryptError> {
    match alg {
        AsymEncryptAlgorithm::RsaOaepSha256 => openssl_rsa_oaep_encrypt(public_key, plaintext),
    }
}

pub fn openssl_asym_decrypt(
    alg: AsymEncryptAlgorithm,
    private_key: &PKey<Private>,
    ciphertext: &[u8],
) -> Result<Vec<u8>, AirframeCryptError> {
    match alg {
        AsymEncryptAlgorithm::RsaOaepSha256 => openssl_rsa_oaep_decrypt(private_key, ciphertext),
    }
}

pub fn openssl_asym_sign(
    alg: AsymSignAlgorithm,
    private_key: &PKey<Private>,
    msg: &[u8],
) -> Result<Vec<u8>, AirframeCryptError> {
    match alg {
        AsymSignAlgorithm::RsaPssSha256 => openssl_rsa_pss_sign(private_key, msg),
        AsymSignAlgorithm::RsaPkcs1v15Sha256 => openssl_rsa_pkcs1_v15_sign(private_key, msg),
        AsymSignAlgorithm::Ed25519 => openssl_ed25519_sign(private_key, msg),
    }
}

pub fn openssl_asym_verify(
    alg: AsymSignAlgorithm,
    public_key: &PKey<Public>,
    msg: &[u8],
    sig: &[u8],
) -> Result<bool, AirframeCryptError> {
    match alg {
        AsymSignAlgorithm::RsaPssSha256 => openssl_rsa_pss_verify(public_key, msg, sig),
        AsymSignAlgorithm::RsaPkcs1v15Sha256 => openssl_rsa_pkcs1_v15_verify(public_key, msg, sig),
        AsymSignAlgorithm::Ed25519 => openssl_ed25519_verify(public_key, msg, sig),
    }
}

pub fn openssl_asym_derive(
    alg: AsymKexAlgorithm,
    my_private: &PKey<Private>,
    peer_public: &PKey<Public>,
) -> Result<Vec<u8>, AirframeCryptError> {
    match alg {
        AsymKexAlgorithm::X25519 => openssl_x25519_derive(my_private, peer_public),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rsa_oaep_roundtrip() {
        let sk = openssl_rsa_generate(2048).unwrap();
        let pk = PKey::public_key_from_pem(&sk.public_key_to_pem().unwrap()).unwrap();
        let msg = b"RSA OAEP test message";
        let ct = openssl_rsa_oaep_encrypt(&pk, msg).unwrap();
        let pt = openssl_rsa_oaep_decrypt(&sk, &ct).unwrap();
        assert_eq!(pt, msg);
    }

    #[test]
    fn test_rsa_pss_sign_verify() {
        let sk = openssl_rsa_generate(2048).unwrap();
        let pk = PKey::public_key_from_pem(&sk.public_key_to_pem().unwrap()).unwrap();
        let msg = b"message for PSS";
        let sig = openssl_rsa_pss_sign(&sk, msg).unwrap();
        assert!(openssl_rsa_pss_verify(&pk, msg, &sig).unwrap());
        // Negative: wrong message
        assert!(!openssl_rsa_pss_verify(&pk, b"other", &sig).unwrap());
    }

    #[test]
    fn test_ed25519_sign_verify() {
        let sk = openssl_ed25519_generate().unwrap();
        let pk = PKey::public_key_from_pem(&sk.public_key_to_pem().unwrap()).unwrap();
        let msg = b"Ed25519 message";
        let sig = openssl_ed25519_sign(&sk, msg).unwrap();
        assert!(openssl_ed25519_verify(&pk, msg, &sig).unwrap());
        assert!(!openssl_ed25519_verify(&pk, b"bad", &sig).unwrap());
    }

    #[test]
    fn test_x25519_agree() {
        let a_sk = openssl_x25519_generate().unwrap();
        let a_pk = PKey::public_key_from_pem(&a_sk.public_key_to_pem().unwrap()).unwrap();
        let b_sk = openssl_x25519_generate().unwrap();
        let b_pk = PKey::public_key_from_pem(&b_sk.public_key_to_pem().unwrap()).unwrap();
        let a_ss = openssl_x25519_derive(&a_sk, &b_pk).unwrap();
        let b_ss = openssl_x25519_derive(&b_sk, &a_pk).unwrap();
        assert_eq!(a_ss, b_ss);
        assert!(!a_ss.is_empty());
    }

    #[test]
    fn test_ecdsa_p256_sign_verify() {
        let sk = openssl_ecdsa_p256_generate().unwrap();
        let pk = PKey::public_key_from_pem(&sk.public_key_to_pem().unwrap()).unwrap();
        let msg = b"ECDSA P-256";
        let sig = openssl_ecdsa_sign_sha256(&sk, msg).unwrap();
        assert!(openssl_ecdsa_verify_sha256(&pk, msg, &sig).unwrap());
        assert!(!openssl_ecdsa_verify_sha256(&pk, b"wrong", &sig).unwrap());
    }

    #[test]
    fn test_rsa_pss_streaming_sign_verify() {
        let sk = openssl_rsa_generate(2048).unwrap();
        let pk = PKey::public_key_from_pem(&sk.public_key_to_pem().unwrap()).unwrap();
        let mut signer = openssl_rsa_pss_signer(&sk).unwrap();
        signer.update(b"The quick brown ").unwrap();
        signer.update(b"fox jumps over ").unwrap();
        signer.update(b"the lazy dog").unwrap();
        let sig = signer.sign_to_vec().unwrap();

        let mut verifier = openssl_rsa_pss_verifier(&pk).unwrap();
        verifier.update(b"The quick brown ").unwrap();
        verifier.update(b"fox jumps over ").unwrap();
        verifier.update(b"the lazy dog").unwrap();
        assert!(verifier.verify(&sig).unwrap());

        let mut bad = openssl_rsa_pss_verifier(&pk).unwrap();
        bad.update(b"The quick brown ").unwrap();
        bad.update(b"fox jumps over ").unwrap();
        bad.update(b"the lazy cat").unwrap();
        assert!(!bad.verify(&sig).unwrap());
    }

    // Enum-dispatch helper tests
    #[test]
    fn test_enum_encrypt_decrypt_rsa_oaep() {
        let sk = openssl_rsa_generate(2048).unwrap();
        let pk = PKey::public_key_from_pem(&sk.public_key_to_pem().unwrap()).unwrap();
        let msg = b"hello enum oaep";
        let ct = openssl_asym_encrypt(AsymEncryptAlgorithm::RsaOaepSha256, &pk, msg).unwrap();
        let pt = openssl_asym_decrypt(AsymEncryptAlgorithm::RsaOaepSha256, &sk, &ct).unwrap();
        assert_eq!(pt, msg);
    }

    #[test]
    fn test_enum_sign_verify_rsa_variants() {
        let sk = openssl_rsa_generate(2048).unwrap();
        let pk = PKey::public_key_from_pem(&sk.public_key_to_pem().unwrap()).unwrap();
        let msg = b"enum sign";
        let sig_pss = openssl_asym_sign(AsymSignAlgorithm::RsaPssSha256, &sk, msg).unwrap();
        assert!(openssl_asym_verify(AsymSignAlgorithm::RsaPssSha256, &pk, msg, &sig_pss).unwrap());
        let sig_pk = openssl_asym_sign(AsymSignAlgorithm::RsaPkcs1v15Sha256, &sk, msg).unwrap();
        assert!(
            openssl_asym_verify(AsymSignAlgorithm::RsaPkcs1v15Sha256, &pk, msg, &sig_pk).unwrap()
        );
    }

    #[test]
    fn test_enum_sign_verify_ed25519() {
        let sk = openssl_ed25519_generate().unwrap();
        let pk = PKey::public_key_from_pem(&sk.public_key_to_pem().unwrap()).unwrap();
        let msg = b"enum ed";
        let sig = openssl_asym_sign(AsymSignAlgorithm::Ed25519, &sk, msg).unwrap();
        assert!(openssl_asym_verify(AsymSignAlgorithm::Ed25519, &pk, msg, &sig).unwrap());
    }

    #[test]
    fn test_enum_derive_x25519() {
        let a_sk = openssl_x25519_generate().unwrap();
        let a_pk = PKey::public_key_from_pem(&a_sk.public_key_to_pem().unwrap()).unwrap();
        let b_sk = openssl_x25519_generate().unwrap();
        let b_pk = PKey::public_key_from_pem(&b_sk.public_key_to_pem().unwrap()).unwrap();
        let a_ss = openssl_asym_derive(AsymKexAlgorithm::X25519, &a_sk, &b_pk).unwrap();
        let b_ss = openssl_asym_derive(AsymKexAlgorithm::X25519, &b_sk, &a_pk).unwrap();
        assert_eq!(a_ss, b_ss);
    }
}
