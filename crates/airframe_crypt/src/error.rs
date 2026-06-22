use airframe_core::error::ErrorRange;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AirframeCryptError {
    #[error("Success")]
    Success,
    #[error("Airframe error: {0}")]
    CoreError(#[from] airframe_core::error::AirframeError),
    #[error("OpenSSL error: {0}")]
    OpenSSLError(#[from] openssl::error::ErrorStack),
    #[error("CNG error: NTSTATUS 0x{0:X}")]
    Cng(i32),
    #[error("Integrity check failed")]
    IntegrityCheck,
    #[error("Invalid input length: {0}")]
    InvalidLength(String),
    #[error("Unsupported algorithm or parameters")]
    UnsupportedAlgorithm,
    #[error("Required provider/engine is unavailable")]
    ProviderUnavailable,
    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),
    #[error("Unknown error code: {0}")]
    Other(u32),
}

impl AirframeCryptError {
    /// Converts an AirframeCryptError to its integer representation.
    /// Crypt-specific errors are within the ErrorRange::Crypt range (100-199).
    /// Core errors are mapped to their respective codes within the Core range.
    pub fn to_int(&self) -> u32 {
        let crypt_base = ErrorRange::Crypt.base();
        match self {
            AirframeCryptError::Success => crypt_base,
            AirframeCryptError::CoreError(err) => err.to_int(), // Core errors stay in their range
            AirframeCryptError::OpenSSLError(_) => crypt_base + 1,
            AirframeCryptError::Cng(_) => crypt_base + 2,
            AirframeCryptError::IntegrityCheck => crypt_base + 3,
            AirframeCryptError::InvalidLength(_) => crypt_base + 4,
            AirframeCryptError::Other(code) => *code,
            AirframeCryptError::UnsupportedAlgorithm => crypt_base + 5,
            AirframeCryptError::ProviderUnavailable => crypt_base + 6,
            AirframeCryptError::InvalidParameters(_) => crypt_base + 7,
        }
    }

    /// Converts an integer to an AirframeCryptError.
    /// Handles error codes within the ErrorRange::Crypt range (100-199)
    /// and maps Core range errors to AirframeError.
    pub fn from_int(val: u32) -> Option<Self> {
        // Check if it's a core error
        if ErrorRange::Core.contains(val) {
            return airframe_core::error::AirframeError::from_int(val)
                .map(AirframeCryptError::CoreError);
        }

        // Check if it's in the crypt range
        if ErrorRange::Crypt.contains(val) {
            let code = val - ErrorRange::Crypt.base();
            match code {
                0 => Some(AirframeCryptError::Success),
                1 => Some(AirframeCryptError::OpenSSLError(
                    openssl::error::ErrorStack::get(),
                )),
                2 => Some(AirframeCryptError::Cng(0)), // Default NTSTATUS value
                3 => Some(AirframeCryptError::IntegrityCheck),
                4 => Some(AirframeCryptError::InvalidLength(
                    "Unknown length error".to_string(),
                )),
                5 => Some(AirframeCryptError::UnsupportedAlgorithm),
                6 => Some(AirframeCryptError::ProviderUnavailable),
                7 => Some(AirframeCryptError::InvalidParameters(
                    "Unknown parameter error".to_string(),
                )),
                _ => Some(AirframeCryptError::Other(val)),
            }
        } else {
            // For values outside both ranges, use the Other variant
            Some(AirframeCryptError::Other(val))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airframe_core::error::AirframeError;

    #[test]
    fn test_display() {
        // Test display implementation for all variants
        assert_eq!(format!("{}", AirframeCryptError::Success), "Success");

        // Test AirframeError variant
        let core_error = AirframeError::InvalidArgument;
        assert_eq!(
            format!("{}", AirframeCryptError::CoreError(core_error)),
            "Airframe error: Invalid argument"
        );

        // We can't easily test OpenSSLError display since it requires a real error
        // but we can test the Other variant
        assert_eq!(
            format!("{}", AirframeCryptError::Other(42)),
            "Unknown error code: 42"
        );
    }

    #[test]
    fn test_to_int() {
        let crypt_base = ErrorRange::Crypt.base();
        let core_base = ErrorRange::Core.base();

        // Test to_int() for Success and Other variants
        assert_eq!(AirframeCryptError::Success.to_int(), crypt_base);
        assert_eq!(AirframeCryptError::Other(42).to_int(), 42);

        // Test OpenSSLError variant
        assert_eq!(
            AirframeCryptError::OpenSSLError(openssl::error::ErrorStack::get()).to_int(),
            crypt_base + 1
        );

        // Test Cng variant
        assert_eq!(AirframeCryptError::Cng(0x12345678).to_int(), crypt_base + 2);

        // Test IntegrityCheck variant
        assert_eq!(AirframeCryptError::IntegrityCheck.to_int(), crypt_base + 3);

        // Test InvalidLength variant
        assert_eq!(
            AirframeCryptError::InvalidLength("test".to_string()).to_int(),
            crypt_base + 4
        );

        // Test AirframeError variant with different core errors
        assert_eq!(
            AirframeCryptError::CoreError(AirframeError::Success).to_int(),
            core_base
        );
        assert_eq!(
            AirframeCryptError::CoreError(AirframeError::InvalidArgument).to_int(),
            core_base + 1
        );
        assert_eq!(
            AirframeCryptError::CoreError(AirframeError::Other(50)).to_int(),
            50
        );
    }

    #[test]
    fn test_from_int() {
        let crypt_base = ErrorRange::Crypt.base();
        let core_base = ErrorRange::Core.base();

        // Test Success variant
        assert!(matches!(
            AirframeCryptError::from_int(crypt_base),
            Some(AirframeCryptError::Success)
        ));

        // Test Other variant for values outside any range
        if let Some(AirframeCryptError::Other(code)) = AirframeCryptError::from_int(500) {
            assert_eq!(code, 500);
        } else {
            panic!("Expected Other variant");
        }

        // Test AirframeError variants
        if let Some(AirframeCryptError::CoreError(core_error)) =
            AirframeCryptError::from_int(core_base + 1)
        {
            assert!(matches!(core_error, AirframeError::InvalidArgument));
        } else {
            panic!("Expected AirframeError variant with InvalidArgument");
        }

        if let Some(AirframeCryptError::CoreError(core_error)) =
            AirframeCryptError::from_int(core_base + 8)
        {
            assert!(matches!(core_error, AirframeError::ServerError));
        } else {
            panic!("Expected AirframeError variant with ServerError");
        }

        // Test OpenSSLError variant
        if let Some(AirframeCryptError::OpenSSLError(_)) =
            AirframeCryptError::from_int(crypt_base + 1)
        {
            // Success
        } else {
            panic!("Expected OpenSSLError variant");
        }

        // Test Cng variant
        if let Some(AirframeCryptError::Cng(status)) = AirframeCryptError::from_int(crypt_base + 2)
        {
            assert_eq!(status, 0); // Default value
        } else {
            panic!("Expected Cng variant");
        }

        // Test IntegrityCheck variant
        assert!(matches!(
            AirframeCryptError::from_int(crypt_base + 3),
            Some(AirframeCryptError::IntegrityCheck)
        ));

        // Test InvalidLength variant
        if let Some(AirframeCryptError::InvalidLength(msg)) =
            AirframeCryptError::from_int(crypt_base + 4)
        {
            assert_eq!(msg, "Unknown length error");
        } else {
            panic!("Expected InvalidLength variant");
        }
    }

    #[test]
    fn test_from_implementations() {
        // Test From<AirframeCoreError>
        let core_error = AirframeError::InvalidArgument;
        let crypt_error: AirframeCryptError = core_error.into();

        if let AirframeCryptError::CoreError(inner) = crypt_error {
            assert!(matches!(inner, AirframeError::InvalidArgument));
        } else {
            panic!("Expected AirframeError variant");
        }

        // We can't easily test From<ErrorStack> since it requires a real OpenSSL error
    }

    #[test]
    fn test_roundtrip_conversion() {
        // Test roundtrip for Success
        let error = AirframeCryptError::Success;
        let code = error.to_int();
        let roundtrip = AirframeCryptError::from_int(code).unwrap();
        assert!(matches!(roundtrip, AirframeCryptError::Success));

        // Test roundtrip for Other
        let error = AirframeCryptError::Other(500);
        let code = error.to_int();
        let roundtrip = AirframeCryptError::from_int(code).unwrap();
        if let AirframeCryptError::Other(value) = roundtrip {
            assert_eq!(value, 500);
        } else {
            panic!("Expected Other variant");
        }

        // Test roundtrip for AirframeError
        let error = AirframeCryptError::CoreError(AirframeError::InvalidArgument);
        let code = error.to_int();
        let roundtrip = AirframeCryptError::from_int(code).unwrap();
        if let AirframeCryptError::CoreError(core_error) = roundtrip {
            assert!(matches!(core_error, AirframeError::InvalidArgument));
        } else {
            panic!("Expected AirframeError variant");
        }

        // Test roundtrip for Cng
        let error = AirframeCryptError::Cng(0x12345678);
        let code = error.to_int();
        let roundtrip = AirframeCryptError::from_int(code).unwrap();
        assert!(matches!(roundtrip, AirframeCryptError::Cng(_)));

        // Test roundtrip for IntegrityCheck
        let error = AirframeCryptError::IntegrityCheck;
        let code = error.to_int();
        let roundtrip = AirframeCryptError::from_int(code).unwrap();
        assert!(matches!(roundtrip, AirframeCryptError::IntegrityCheck));

        // Test roundtrip for InvalidLength
        let error = AirframeCryptError::InvalidLength("Original message".to_string());
        let code = error.to_int();
        let roundtrip = AirframeCryptError::from_int(code).unwrap();
        assert!(matches!(roundtrip, AirframeCryptError::InvalidLength(_)));
    }

    #[test]
    fn test_error_code_ranges() {
        let crypt_base = ErrorRange::Crypt.base();
        let core_base = ErrorRange::Core.base();

        // Test that error codes are in the expected ranges
        assert_eq!(AirframeCryptError::Success.to_int(), crypt_base);
        assert_eq!(
            AirframeCryptError::OpenSSLError(openssl::error::ErrorStack::get()).to_int(),
            crypt_base + 1
        );
        assert_eq!(AirframeCryptError::Cng(0x12345678).to_int(), crypt_base + 2);
        assert_eq!(AirframeCryptError::IntegrityCheck.to_int(), crypt_base + 3);
        assert_eq!(
            AirframeCryptError::InvalidLength("test".to_string()).to_int(),
            crypt_base + 4
        );

        // AirframeError codes should be in the Core range
        assert_eq!(
            AirframeCryptError::CoreError(AirframeError::Success).to_int(),
            core_base
        );
        assert_eq!(
            AirframeCryptError::CoreError(AirframeError::InvalidArgument).to_int(),
            core_base + 1
        );

        // Other codes should be used as-is
        assert_eq!(AirframeCryptError::Other(500).to_int(), 500);
    }
}
