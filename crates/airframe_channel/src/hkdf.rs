use crate::error::ChannelError;
use airframe_crypt::hash::{openssl_hmac, DigestAlgorithm};

/// HKDF-Extract (RFC 5869 Section 2.2)
///
/// PRK = HMAC-SHA256(salt, IKM)
pub fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> Result<Vec<u8>, ChannelError> {
    let salt = if salt.is_empty() {
        // If salt is not provided, use a string of HashLen zeros
        &[0u8; 32] as &[u8]
    } else {
        salt
    };
    Ok(openssl_hmac(DigestAlgorithm::Sha256, salt, ikm)?)
}

/// HKDF-Expand (RFC 5869 Section 2.3)
///
/// OKM = T(1) || T(2) || ... || T(N), truncated to `length` bytes
/// where T(i) = HMAC-SHA256(PRK, T(i-1) || info || i)
pub fn hkdf_expand(prk: &[u8], info: &[u8], length: usize) -> Result<Vec<u8>, ChannelError> {
    if length > 255 * 32 {
        return Err(ChannelError::Framing(
            "HKDF-Expand: requested length too large".into(),
        ));
    }
    let n = length.div_ceil(32); // ceil(length / HashLen)
    let mut okm = Vec::with_capacity(n * 32);
    let mut t_prev = Vec::new();

    for i in 1..=n {
        let mut input = Vec::with_capacity(t_prev.len() + info.len() + 1);
        input.extend_from_slice(&t_prev);
        input.extend_from_slice(info);
        input.push(i as u8);
        t_prev = openssl_hmac(DigestAlgorithm::Sha256, prk, &input)?;
        okm.extend_from_slice(&t_prev);
    }

    okm.truncate(length);
    Ok(okm)
}

/// Combined HKDF Extract-then-Expand.
///
/// Used by Noise's MixKey: HKDF(chaining_key, input_key_material) -> (new_ck, temp_key)
pub fn hkdf_sha256(
    chaining_key: &[u8],
    input_key_material: &[u8],
    num_outputs: usize,
) -> Result<Vec<Vec<u8>>, ChannelError> {
    let prk = hkdf_extract(chaining_key, input_key_material)?;
    let mut outputs = Vec::with_capacity(num_outputs);
    let mut t_prev = Vec::new();

    for i in 1..=num_outputs {
        let mut input = Vec::with_capacity(t_prev.len() + 1);
        input.extend_from_slice(&t_prev);
        input.push(i as u8);
        t_prev = openssl_hmac(DigestAlgorithm::Sha256, &prk, &input)?;
        outputs.push(t_prev.clone());
    }

    Ok(outputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    // RFC 5869 Test Case 1
    #[test]
    fn test_rfc5869_case1() {
        let ikm = unhex("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
        let salt = unhex("000102030405060708090a0b0c");
        let info = unhex("f0f1f2f3f4f5f6f7f8f9");
        let expected_prk =
            unhex("077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5");
        let expected_okm = unhex(
            "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865",
        );

        let prk = hkdf_extract(&salt, &ikm).unwrap();
        assert_eq!(hex(&prk), hex(&expected_prk));

        let okm = hkdf_expand(&prk, &info, 42).unwrap();
        assert_eq!(hex(&okm), hex(&expected_okm));
    }

    // RFC 5869 Test Case 2
    #[test]
    fn test_rfc5869_case2() {
        let ikm = unhex(
            "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f\
             202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f\
             404142434445464748494a4b4c4d4e4f",
        );
        let salt = unhex(
            "606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f\
             808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f\
             a0a1a2a3a4a5a6a7a8a9aaabacadaeaf",
        );
        let info = unhex(
            "b0b1b2b3b4b5b6b7b8b9babbbcbdbebfc0c1c2c3c4c5c6c7c8c9cacbcccdcecf\
             d0d1d2d3d4d5d6d7d8d9dadbdcdddedfe0e1e2e3e4e5e6e7e8e9eaebecedeeef\
             f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff",
        );
        let expected_prk =
            unhex("06a6b88c5853361a06104c9ceb35b45cef760014904671014a193f40c15fc244");
        let expected_okm = unhex(
            "b11e398dc80327a1c8e7f78c596a49344f012eda2d4efad8a050cc4c19afa97c\
             59045a99cac7827271cb41c65e590e09da3275600c2f09b8367793a9aca3db71\
             cc30c58179ec3e87c14c01d5c1f3434f1d87",
        );

        let prk = hkdf_extract(&salt, &ikm).unwrap();
        assert_eq!(hex(&prk), hex(&expected_prk));

        let okm = hkdf_expand(&prk, &info, 82).unwrap();
        assert_eq!(hex(&okm), hex(&expected_okm));
    }

    // RFC 5869 Test Case 3 (zero-length salt and info)
    #[test]
    fn test_rfc5869_case3() {
        let ikm = unhex("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
        let salt = vec![];
        let info = vec![];
        let expected_prk =
            unhex("19ef24a32c717b167f33a91d6f648bdf96596776afdb6377ac434c1c293ccb04");
        let expected_okm = unhex(
            "8da4e775a563c18f715f802a063c5a31b8a11f5c5ee1879ec3454e5f3c738d2d9d201395faa4b61a96c8",
        );

        let prk = hkdf_extract(&salt, &ikm).unwrap();
        assert_eq!(hex(&prk), hex(&expected_prk));

        let okm = hkdf_expand(&prk, &info, 42).unwrap();
        assert_eq!(hex(&okm), hex(&expected_okm));
    }

    // Test the Noise-style HKDF function
    #[test]
    fn test_hkdf_sha256_two_outputs() {
        let ck = [0x01u8; 32];
        let ikm = [0x02u8; 32];
        let outputs = hkdf_sha256(&ck, &ikm, 2).unwrap();
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].len(), 32);
        assert_eq!(outputs[1].len(), 32);
        assert_ne!(outputs[0], outputs[1]);
    }

    #[test]
    fn test_hkdf_sha256_three_outputs() {
        let ck = [0x03u8; 32];
        let ikm = [0x04u8; 32];
        let outputs = hkdf_sha256(&ck, &ikm, 3).unwrap();
        assert_eq!(outputs.len(), 3);
        for o in &outputs {
            assert_eq!(o.len(), 32);
        }
    }
}
