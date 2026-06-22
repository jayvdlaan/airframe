//! Capability helper macros.

/// Convert typed `Cap` values to a slice of capability strings.
///
/// This macro extracts the inner `&'static str` from `Cap` wrapper types,
/// making it easier to work with capability lists in module descriptors.
///
/// # Examples
///
/// ```ignore
/// use airframe_macros::caps;
/// use airframe_core::module::{Cap, CAP_HTTP_SERVER, CAP_CONFIG};
///
/// let provides: &[&str] = caps![CAP_HTTP_SERVER, CAP_CONFIG];
/// assert_eq!(provides, &["cap:http.server", "cap:config"]);
/// ```
#[macro_export]
macro_rules! caps {
    ($($cap:expr),* $(,)?) => {
        &[$($cap.0),*]
    };
}

#[cfg(test)]
mod tests {
    use airframe_core::module::{Cap, CAP_CONFIG, CAP_HTTP_SERVER, CAP_LOGGING};

    #[test]
    fn caps_macro_single() {
        let result: &[&str] = caps![CAP_HTTP_SERVER];
        assert_eq!(result, &["cap:http.server"]);
    }

    #[test]
    fn caps_macro_multiple() {
        let result: &[&str] = caps![CAP_HTTP_SERVER, CAP_CONFIG, CAP_LOGGING];
        assert_eq!(result, &["cap:http.server", "cap:config", "cap:logging"]);
    }

    #[test]
    fn caps_macro_empty() {
        let result: &[&str] = caps![];
        assert!(result.is_empty());
    }

    #[test]
    fn caps_macro_trailing_comma() {
        let result: &[&str] = caps![CAP_HTTP_SERVER, CAP_CONFIG,];
        assert_eq!(result, &["cap:http.server", "cap:config"]);
    }

    #[test]
    fn caps_macro_custom_cap() {
        const MY_CAP: Cap = Cap("cap:custom.service");
        let result: &[&str] = caps![MY_CAP, CAP_CONFIG];
        assert_eq!(result, &["cap:custom.service", "cap:config"]);
    }
}
