//! Error type for `airframe_sqlite`, generated via the shared
//! [`airframe_macros::airframe_error!`] macro.

airframe_macros::airframe_error! {
    /// Errors produced by `airframe_sqlite`.
    pub enum AirframeSqliteError => Sqlite;
    unit: { InvalidState = 1 => "Invalid state" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airframe_core::error::{AirframeError, ErrorRange};

    #[test]
    fn to_int_mappings() {
        let base = ErrorRange::Sqlite.base();
        assert_eq!(AirframeSqliteError::Success.to_int(), base);
        assert_eq!(AirframeSqliteError::InvalidState.to_int(), base + 1);
        assert_eq!(AirframeSqliteError::Other(777).to_int(), 777);
    }

    #[test]
    fn to_int_core_passthrough() {
        let core = AirframeError::InvalidState;
        let code = core.to_int();
        assert_eq!(AirframeSqliteError::CoreError(core).to_int(), code);
    }

    #[test]
    fn from_int_variants_and_other() {
        let base = ErrorRange::Sqlite.base();
        assert!(matches!(
            AirframeSqliteError::from_int(base),
            Some(AirframeSqliteError::Success)
        ));
        assert!(matches!(
            AirframeSqliteError::from_int(base + 1),
            Some(AirframeSqliteError::InvalidState)
        ));

        // Core code maps to CoreError
        let core_code = AirframeError::InvalidArgument.to_int();
        match AirframeSqliteError::from_int(core_code).unwrap() {
            AirframeSqliteError::CoreError(inner) => {
                assert!(matches!(inner, AirframeError::InvalidArgument))
            }
            _ => panic!("expected core error"),
        }

        // Unknown outside any range becomes Other(val)
        let unknown = ErrorRange::Yk.max() + 7;
        assert!(
            matches!(AirframeSqliteError::from_int(unknown), Some(AirframeSqliteError::Other(v)) if v == unknown)
        );
    }
}
