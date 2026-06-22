//! Error type for `airframe_db`, generated via the shared
//! [`airframe_macros::airframe_error!`] macro.

airframe_macros::airframe_error! {
    /// Errors produced by `airframe_db`.
    pub enum AirframeDbError => Db;
    unit: {
        Timeout = 2 => "Timeout",
        RetryExhausted = 3 => "Retry exhausted",
        InvalidState = 8 => "Invalid state",
    }
    data: {
        Connection(String) = 1 => "Connection error",
        TxBegin(String) = 4 => "Transaction begin failed",
        TxCommit(String) = 5 => "Transaction commit failed",
        TxRollback(String) = 6 => "Transaction rollback failed",
        Migration(String) = 7 => "Migration error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airframe_core::error::{AirframeError, ErrorRange};

    #[test]
    fn codes_map_into_db_range_or_core() {
        let db_base = ErrorRange::Db.base();
        let core_base = ErrorRange::Core.base();
        assert_eq!(AirframeDbError::Success.to_int(), db_base);
        assert_eq!(
            AirframeDbError::Connection("x".into()).to_int(),
            db_base + 1
        );
        assert_eq!(AirframeDbError::Timeout.to_int(), db_base + 2);
        assert_eq!(AirframeDbError::RetryExhausted.to_int(), db_base + 3);
        assert_eq!(AirframeDbError::TxBegin("x".into()).to_int(), db_base + 4);
        assert_eq!(AirframeDbError::TxCommit("x".into()).to_int(), db_base + 5);
        assert_eq!(
            AirframeDbError::TxRollback("x".into()).to_int(),
            db_base + 6
        );
        assert_eq!(AirframeDbError::Migration("x".into()).to_int(), db_base + 7);
        assert_eq!(AirframeDbError::InvalidState.to_int(), db_base + 8);
        assert_eq!(
            AirframeDbError::CoreError(AirframeError::Success).to_int(),
            core_base
        );
    }

    #[test]
    fn display_matches_legacy_format() {
        assert_eq!(AirframeDbError::Timeout.to_string(), "Timeout");
        assert_eq!(
            AirframeDbError::Connection("boom".into()).to_string(),
            "Connection error: boom"
        );
        assert_eq!(
            AirframeDbError::TxBegin("x".into()).to_string(),
            "Transaction begin failed: x"
        );
    }
}
