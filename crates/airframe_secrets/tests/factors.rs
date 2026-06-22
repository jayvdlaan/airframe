use airframe_crypt::AlgorithmId;
use airframe_secrets::factors::{
    FactorInput, FactorKind, FactorPolicy, FactorsKeyResolver, KdfSpec,
};
use airframe_secrets::KeyResolver;
use secrecy::SecretString;

fn inputs(vals: &[&str]) -> Vec<FactorInput> {
    vals.iter()
        .map(|v| FactorInput {
            kind: FactorKind::Password,
            value: SecretString::new((*v).to_string().into()),
        })
        .collect()
}

#[test]
fn deterministic_derivation() {
    let policy = FactorPolicy {
        kdf: KdfSpec {
            alg: AlgorithmId::Pbkdf2Sha256,
            iters: 100_000,
            salt: Some(b"salt".to_vec()),
        },
        min_factors: 1,
        domain: Some("demo".to_string()),
    };
    let r1 = FactorsKeyResolver::new(policy.clone(), inputs(&["alpha", "beta"]));
    let r2 = FactorsKeyResolver::new(policy.clone(), inputs(&["alpha", "beta"]));
    let k1 = r1.resolve(Some(b"kid")).unwrap();
    let k2 = r2.resolve(Some(b"kid")).unwrap();
    assert_eq!(k1.to_vec(), k2.to_vec());
}

#[test]
fn changing_a_factor_changes_key() {
    let policy = FactorPolicy {
        kdf: KdfSpec {
            alg: AlgorithmId::Pbkdf2Sha512,
            iters: 100_000,
            salt: None,
        },
        min_factors: 1,
        domain: Some("demo".to_string()),
    };
    let r1 = FactorsKeyResolver::new(policy.clone(), inputs(&["alpha", "beta"]));
    let r2 = FactorsKeyResolver::new(policy.clone(), inputs(&["alpha", "gamma"]));
    let k1 = r1.resolve(None).unwrap();
    let k2 = r2.resolve(None).unwrap();
    assert_ne!(k1.to_vec(), k2.to_vec());
}

#[test]
fn enforces_min_factors() {
    let policy = FactorPolicy {
        kdf: KdfSpec {
            alg: AlgorithmId::Pbkdf2Sha256,
            iters: 100,
            salt: None,
        },
        min_factors: 2,
        domain: None,
    };
    let r = FactorsKeyResolver::new(policy, inputs(&["onlyone"]));
    assert!(r.resolve(None).is_err());
}
