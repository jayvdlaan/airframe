//! Error type for `airframe_secrets`, generated via the shared
//! [`airframe_macros::airframe_error!`] macro.

airframe_macros::airframe_error! {
    /// Errors produced by `airframe_secrets`.
    pub enum AirframeSecretsError => Secrets;
    unit: { InvalidState = 1 => "Invalid state" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airframe_core::error::{AirframeError, ErrorRange};

    #[test]
    fn to_int_mappings() {
        let base = ErrorRange::Secrets.base();
        assert_eq!(AirframeSecretsError::Success.to_int(), base);
        assert_eq!(AirframeSecretsError::InvalidState.to_int(), base + 1);
        assert_eq!(AirframeSecretsError::Other(42).to_int(), 42);
    }

    #[test]
    fn to_int_core_passthrough() {
        let core = AirframeError::InvalidArgument;
        let code = core.to_int();
        assert_eq!(AirframeSecretsError::CoreError(core).to_int(), code);
    }

    #[test]
    fn from_int_variants_and_other() {
        let base = ErrorRange::Secrets.base();
        assert!(matches!(
            AirframeSecretsError::from_int(base),
            Some(AirframeSecretsError::Success)
        ));
        assert!(matches!(
            AirframeSecretsError::from_int(base + 1),
            Some(AirframeSecretsError::InvalidState)
        ));

        // Core code maps to CoreError
        let core_code = AirframeError::InvalidArgument.to_int();
        match AirframeSecretsError::from_int(core_code).unwrap() {
            AirframeSecretsError::CoreError(inner) => {
                assert!(matches!(inner, AirframeError::InvalidArgument))
            }
            _ => panic!("expected core error"),
        }

        // Unknown outside any range becomes Other(val)
        let unknown = ErrorRange::Yk.max() + 999;
        assert!(
            matches!(AirframeSecretsError::from_int(unknown), Some(AirframeSecretsError::Other(v)) if v == unknown)
        );
    }
}
