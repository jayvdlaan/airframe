use thiserror::Error;

/// Defines the error code ranges for different crates in the Airframe SDK.
/// This helps prevent overlapping error codes between crates.
#[derive(Debug, Clone, Copy)]
pub enum ErrorRange {
    /// Core errors: 0-99
    Core = 0,
    /// Cryptography errors: 100-199
    Crypt = 100,
    /// Storage errors: 200-299
    Storage = 200,
    /// Application errors: 300-399
    App = 300,
    /// Reserved for future use: 400-499
    Reserved2 = 400,
    /// Attestation errors: 500-599
    Attest = 500,
    /// Database errors: 600-699
    Db = 600,
    /// TLS errors: 700-799
    Tls = 700,
    /// SQLite errors: 800-899
    Sqlite = 800,
    /// X.509 errors: 900-999
    X509 = 900,
    /// TPM errors: 1000-1099
    Tpm = 1000,
    /// Resource errors: 1100-1199
    Resource = 1100,
    /// Service errors: 1200-1299
    Service = 1200,
    /// Log errors: 1300-1399
    Log = 1300,
    /// Metrics errors: 1400-1499
    Metrics = 1400,
    /// Time errors: 1500-1599
    Time = 1500,
    /// Trace errors: 1600-1699
    Trace = 1600,
    /// TUI errors: 1700-1799
    Tui = 1700,
    /// Update errors: 1800-1899
    Update = 1800,
    /// Winreg errors: 1900-1999
    Winreg = 1900,
    /// YubiKey errors: 2000-2099
    Yk = 2000,
    /// Audit errors: 2100-2199
    Audit = 2100,
    /// Codec errors: 2200-2299
    Codec = 2200,
    /// Data errors: 2300-2399
    Data = 2300,
    /// Event errors: 2400-2499
    Event = 2400,
    /// Harden errors: 2500-2599
    Harden = 2500,
    /// HSM errors: 2600-2699
    Hsm = 2600,
    /// PData errors: 2700-2799
    Pdata = 2700,
    /// Plugin errors: 2800-2899
    Plugin = 2800,
    /// Policy errors: 2900-2999
    Policy = 2900,
    /// Redis errors: 3000-3099
    Redis = 3000,
    /// SC errors: 3100-3199
    Sc = 3100,
    /// SData errors: 3200-3299
    Sdata = 3200,
    /// Secrets errors: 3300-3399
    Secrets = 3300,
}

impl ErrorRange {
    pub fn base(&self) -> u32 {
        *self as u32
    }

    pub fn max(&self) -> u32 {
        self.base() + 99
    }

    pub fn contains(&self, code: u32) -> bool {
        code >= self.base() && code <= self.max()
    }
}

#[derive(Debug, Error)]
pub enum AirframeError {
    #[error("Success")]
    Success,
    #[error("Invalid argument")]
    InvalidArgument,
    #[error("Invalid session")]
    InvalidSession,
    #[error("Invalid operation")]
    InvalidOperation,
    #[error("Invalid state")]
    InvalidState,
    #[error("Unknown command")]
    UnknownCommand,
    #[error("Not implemented")]
    NotImplemented,
    #[error("Already exists")]
    AlreadyExists,
    #[error("Server error")]
    ServerError,
    #[error("Unknown error code: {0}")]
    Other(u32),
}

impl AirframeError {
    pub fn to_int(&self) -> u32 {
        let base = ErrorRange::Core.base();
        match self {
            AirframeError::Success => base,
            AirframeError::InvalidArgument => base + 1,
            AirframeError::InvalidSession => base + 2,
            AirframeError::InvalidOperation => base + 3,
            AirframeError::InvalidState => base + 4,
            AirframeError::UnknownCommand => base + 5,
            AirframeError::NotImplemented => base + 6,
            AirframeError::AlreadyExists => base + 7,
            AirframeError::ServerError => base + 8,
            AirframeError::Other(code) => *code,
        }
    }

    pub fn from_int(val: u32) -> Option<Self> {
        if ErrorRange::Core.contains(val) {
            let code = val - ErrorRange::Core.base();
            match code {
                0 => Some(AirframeError::Success),
                1 => Some(AirframeError::InvalidArgument),
                2 => Some(AirframeError::InvalidSession),
                3 => Some(AirframeError::InvalidOperation),
                4 => Some(AirframeError::InvalidState),
                5 => Some(AirframeError::UnknownCommand),
                6 => Some(AirframeError::NotImplemented),
                7 => Some(AirframeError::AlreadyExists),
                8 => Some(AirframeError::ServerError),
                _ => Some(AirframeError::Other(val)),
            }
        } else {
            Some(AirframeError::Other(val))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_range() {
        // Test ErrorRange base values
        assert_eq!(ErrorRange::Core.base(), 0);
        assert_eq!(ErrorRange::Crypt.base(), 100);
        assert_eq!(ErrorRange::Storage.base(), 200);
        assert_eq!(ErrorRange::App.base(), 300);
        assert_eq!(ErrorRange::Reserved2.base(), 400);

        // Test ErrorRange max values
        assert_eq!(ErrorRange::Core.max(), 99);
        assert_eq!(ErrorRange::Crypt.max(), 199);
        assert_eq!(ErrorRange::Storage.max(), 299);

        // Test ErrorRange contains method
        assert!(ErrorRange::Core.contains(0));
        assert!(ErrorRange::Core.contains(50));
        assert!(ErrorRange::Core.contains(99));
        assert!(!ErrorRange::Core.contains(100));

        assert!(ErrorRange::Crypt.contains(100));
        assert!(ErrorRange::Crypt.contains(150));
        assert!(ErrorRange::Crypt.contains(199));
        assert!(!ErrorRange::Crypt.contains(200));

        assert!(ErrorRange::Storage.contains(200));
        assert!(ErrorRange::Storage.contains(250));
        assert!(ErrorRange::Storage.contains(299));
        assert!(!ErrorRange::Storage.contains(300));
    }

    #[test]
    fn test_display() {
        // Test display implementation for all variants
        assert_eq!(format!("{}", AirframeError::Success), "Success");
        assert_eq!(
            format!("{}", AirframeError::InvalidArgument),
            "Invalid argument"
        );
        assert_eq!(
            format!("{}", AirframeError::InvalidSession),
            "Invalid session"
        );
        assert_eq!(
            format!("{}", AirframeError::InvalidOperation),
            "Invalid operation"
        );
        assert_eq!(format!("{}", AirframeError::InvalidState), "Invalid state");
        assert_eq!(
            format!("{}", AirframeError::UnknownCommand),
            "Unknown command"
        );
        assert_eq!(
            format!("{}", AirframeError::NotImplemented),
            "Not implemented"
        );
        assert_eq!(
            format!("{}", AirframeError::AlreadyExists),
            "Already exists"
        );
        assert_eq!(format!("{}", AirframeError::ServerError), "Server error");
        assert_eq!(
            format!("{}", AirframeError::Other(42)),
            "Unknown error code: 42"
        );
    }

    #[test]
    fn test_to_int() {
        let core_base = ErrorRange::Core.base();

        // Test to_int() for all variants
        assert_eq!(AirframeError::Success.to_int(), core_base);
        assert_eq!(AirframeError::InvalidArgument.to_int(), core_base + 1);
        assert_eq!(AirframeError::InvalidSession.to_int(), core_base + 2);
        assert_eq!(AirframeError::InvalidOperation.to_int(), core_base + 3);
        assert_eq!(AirframeError::InvalidState.to_int(), core_base + 4);
        assert_eq!(AirframeError::UnknownCommand.to_int(), core_base + 5);
        assert_eq!(AirframeError::NotImplemented.to_int(), core_base + 6);
        assert_eq!(AirframeError::AlreadyExists.to_int(), core_base + 7);
        assert_eq!(AirframeError::ServerError.to_int(), core_base + 8);
        assert_eq!(AirframeError::Other(42).to_int(), 42);
    }

    #[test]
    fn test_from_int() {
        let core_base = ErrorRange::Core.base();

        // Test from_int() for all standard variants
        assert!(matches!(
            AirframeError::from_int(core_base),
            Some(AirframeError::Success)
        ));
        assert!(matches!(
            AirframeError::from_int(core_base + 1),
            Some(AirframeError::InvalidArgument)
        ));
        assert!(matches!(
            AirframeError::from_int(core_base + 2),
            Some(AirframeError::InvalidSession)
        ));
        assert!(matches!(
            AirframeError::from_int(core_base + 3),
            Some(AirframeError::InvalidOperation)
        ));
        assert!(matches!(
            AirframeError::from_int(core_base + 4),
            Some(AirframeError::InvalidState)
        ));
        assert!(matches!(
            AirframeError::from_int(core_base + 5),
            Some(AirframeError::UnknownCommand)
        ));
        assert!(matches!(
            AirframeError::from_int(core_base + 6),
            Some(AirframeError::NotImplemented)
        ));
        assert!(matches!(
            AirframeError::from_int(core_base + 7),
            Some(AirframeError::AlreadyExists)
        ));
        assert!(matches!(
            AirframeError::from_int(core_base + 8),
            Some(AirframeError::ServerError)
        ));

        // Test Other variant
        if let Some(AirframeError::Other(code)) = AirframeError::from_int(42) {
            assert_eq!(code, 42);
        } else {
            panic!("Expected Other variant");
        }

        // Test values outside the Core range
        if let Some(AirframeError::Other(code)) = AirframeError::from_int(150) {
            assert_eq!(code, 150);
        } else {
            panic!("Expected Other variant for out-of-range value");
        }
    }

    #[test]
    fn test_roundtrip_conversion() {
        // Test roundtrip conversion (to_int -> from_int)
        let errors = [
            AirframeError::Success,
            AirframeError::InvalidArgument,
            AirframeError::InvalidSession,
            AirframeError::InvalidOperation,
            AirframeError::InvalidState,
            AirframeError::UnknownCommand,
            AirframeError::NotImplemented,
            AirframeError::AlreadyExists,
            AirframeError::ServerError,
            AirframeError::Other(42),
        ];

        for error in &errors {
            let code = error.to_int();
            let roundtrip = AirframeError::from_int(code).unwrap();

            // Compare string representations since we can't directly compare enum variants with data
            assert_eq!(format!("{:?}", error), format!("{:?}", roundtrip));
        }
    }
}
