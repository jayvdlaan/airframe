use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::error::{AirframeSecretsError, Result};
use crate::resolver::KeyResolver;
use crate::secret::SecretBytes;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FactorKind {
    Password,
    Data,
}

#[derive(Clone, Debug)]
pub struct FactorInput {
    pub kind: FactorKind,
    pub value: SecretString,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KdfSpec {
    /// Algorithm identifier (canonical: "pbkdf2-sha256" or "pbkdf2-sha512")
    pub alg: airframe_crypt::AlgorithmId,
    pub iters: u32,
    pub salt: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FactorPolicy {
    pub kdf: KdfSpec,
    pub min_factors: u8,
    pub domain: Option<String>,
}

pub struct FactorsKeyResolver {
    policy: FactorPolicy,
    inputs: Vec<FactorInput>,
}

impl FactorsKeyResolver {
    pub fn new(policy: FactorPolicy, inputs: Vec<FactorInput>) -> Self {
        Self { policy, inputs }
    }

    fn derive_key(&self, key_id: Option<&[u8]>) -> Result<[u8; 32]> {
        if (self.inputs.len() as u8) < self.policy.min_factors {
            return Err(AirframeSecretsError::InvalidState);
        }
        // Stable concatenation of factor bytes in given order
        let mut concat: Vec<u8> = Vec::new();
        for f in &self.inputs {
            match f.kind {
                FactorKind::Password | FactorKind::Data => {
                    concat.extend_from_slice(f.value.expose_secret().as_bytes());
                    concat.push(0x00); // delimiter for stability
                }
            }
        }
        // Build salt: domain || | || key_id || | || optional policy salt
        let mut salt: Vec<u8> = Vec::new();
        if let Some(d) = &self.policy.domain {
            salt.extend_from_slice(d.as_bytes());
        }
        salt.push(b'|');
        if let Some(kid) = key_id {
            salt.extend_from_slice(kid);
        }
        salt.push(b'|');
        if let Some(s) = &self.policy.kdf.salt {
            salt.extend_from_slice(s);
        }

        // Perform PBKDF2 using centralized algorithm parsing
        use airframe_crypt::kdf::{self, Pbkdf2Digest};
        let mut out = [0u8; 32];
        let aid = self.policy.kdf.alg;
        // Reject unknown/unsupported KDF algorithms instead of silently downgrading
        // to SHA-256 — a silent downgrade could mask a misconfigured stronger digest.
        let digest = Pbkdf2Digest::try_from(aid).map_err(|_| AirframeSecretsError::InvalidState)?;
        // Enforce a minimum PBKDF2 iteration count so a misconfigured low value
        // cannot silently weaken key derivation below a safe floor.
        const MIN_PBKDF2_ITERS: u32 = 100_000;
        if self.policy.kdf.iters < MIN_PBKDF2_ITERS {
            return Err(AirframeSecretsError::InvalidState);
        }
        kdf::openssl_derive_pbkdf2(
            &concat,
            &salt,
            self.policy.kdf.iters as usize,
            digest,
            &mut out,
        )
        .map_err(|_| AirframeSecretsError::InvalidState)?;

        // Zeroize sensitive buffers
        concat.zeroize();
        salt.zeroize();
        Ok(out)
    }
}

impl KeyResolver for FactorsKeyResolver {
    fn resolve(&self, key_id: Option<&[u8]>) -> Result<SecretBytes> {
        let k = self.derive_key(key_id)?;
        Ok(SecretBytes::from_vec(k.to_vec()))
    }
}
