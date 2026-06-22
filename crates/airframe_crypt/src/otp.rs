use crate::error::AirframeCryptError;

// We use the well-maintained `totp-rs` crate for RFC 6238 compatible TOTP implementation.
use totp_rs::{Algorithm as TotpAlgorithmImpl, TOTP};

#[derive(Debug, Clone, Copy)]
pub enum OtpAlgorithm {
    Sha1,
    Sha256,
    Sha512,
}

impl OtpAlgorithm {
    fn to_impl(self) -> TotpAlgorithmImpl {
        match self {
            OtpAlgorithm::Sha1 => TotpAlgorithmImpl::SHA1,
            OtpAlgorithm::Sha256 => TotpAlgorithmImpl::SHA256,
            OtpAlgorithm::Sha512 => TotpAlgorithmImpl::SHA512,
        }
    }
}

pub struct TotpRsTotp {
    inner: TOTP,
}

impl TotpRsTotp {
    pub fn new(
        alg: OtpAlgorithm,
        digits: u32,
        skew: u8,
        step: u64,
        secret: &[u8],
    ) -> Result<Self, AirframeCryptError> {
        if !(4..=10).contains(&digits) {
            return Err(AirframeCryptError::InvalidParameters(
                "digits must be between 4 and 10".into(),
            ));
        }
        if step == 0 {
            return Err(AirframeCryptError::InvalidParameters(
                "step must be > 0".into(),
            ));
        }
        let inner = TOTP::new(alg.to_impl(), digits as usize, skew, step, secret.to_vec())
            .map_err(|e| {
                AirframeCryptError::InvalidParameters(format!("failed to create TOTP: {e}"))
            })?;
        Ok(Self { inner })
    }

    pub fn generate_current(&self) -> Result<String, AirframeCryptError> {
        self.inner
            .generate_current()
            .map_err(|_e| AirframeCryptError::Other(0xC0DE_0001))
    }

    pub fn verify_current(&self, code: &str) -> Result<bool, AirframeCryptError> {
        self.inner
            .check_current(code)
            .map_err(|_e| AirframeCryptError::Other(0xC0DE_0002))
    }
}

// Convenience one-shot helpers mirroring other modules' style
pub fn totprs_totp_generate_current(
    alg: OtpAlgorithm,
    secret: &[u8],
    digits: u32,
    step: u64,
    skew: u8,
) -> Result<String, AirframeCryptError> {
    let t = TotpRsTotp::new(alg, digits, skew, step, secret)?;
    t.generate_current()
}

pub fn totprs_totp_verify_current(
    alg: OtpAlgorithm,
    secret: &[u8],
    code: &str,
    digits: u32,
    step: u64,
    skew: u8,
) -> Result<bool, AirframeCryptError> {
    let t = TotpRsTotp::new(alg, digits, skew, step, secret)?;
    t.verify_current(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totp_generate_and_verify_current_roundtrip() {
        let secret = b"supersecretkeymaterial";
        let code = totprs_totp_generate_current(OtpAlgorithm::Sha1, secret, 6, 30, 1).unwrap();
        assert!(totprs_totp_verify_current(OtpAlgorithm::Sha1, secret, &code, 6, 30, 1).unwrap());
    }
}
