//! `airframe_error!` — generate a standard Airframe error enum with stable
//! integer error codes, removing the hand-copied scaffolding that was duplicated
//! across a dozen crates.
//!
//! Each generated enum has:
//! - `Success` (offset 0 within the crate's [`ErrorRange`]),
//! - `CoreError(AirframeError)` with a `From<AirframeError>` conversion,
//! - the `unit:` (fieldless) and `data:` (single-payload) domain variants you
//!   list, each with an explicit offset and `Display` message,
//! - `Other(u32)` for codes outside any known range,
//!
//! plus `Display`/`Error`, `to_int`/`from_int` round-tripping, and a `Result<T>`
//! alias.
//!
//! `from_int` reconstructs `data:` variants with a `Default` payload — the
//! integer code identifies the *variant*, not the original payload value.
//!
//! # Example
//!
//! ```ignore
//! airframe_macros::airframe_error! {
//!     /// Errors for the widget crate.
//!     pub enum AirframeWidgetError => Db;
//!     unit: { Timeout = 2 => "Timeout", InvalidState = 8 => "Invalid state" }
//!     data: { Connection(String) = 1 => "Connection error" }
//! }
//! ```
//!
//! The range token (`Db` above) must be a variant of
//! `airframe_core::error::ErrorRange`. A `data:` message is a *prefix*: the
//! payload is appended as `"<msg>: <payload>"`.

/// Generate a standard Airframe error enum. See the module docs.
#[macro_export]
macro_rules! airframe_error {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident => $range:ident;
        $( unit: { $( $u_variant:ident = $u_off:literal => $u_msg:literal ),* $(,)? } )?
        $( data: { $( $d_variant:ident ( $d_ty:ty ) = $d_off:literal => $d_msg:literal ),* $(,)? } )?
    ) => {
        $(#[$meta])*
        #[derive(Debug)]
        $vis enum $name {
            /// Success / no error.
            Success,
            /// Wraps a core Airframe error (delegates its integer code).
            CoreError($crate::__AirframeError),
            $( $( $u_variant, )* )?
            $( $( $d_variant($d_ty), )* )?
            /// An error code outside any known range.
            Other(u32),
        }

        impl ::core::convert::From<$crate::__AirframeError> for $name {
            fn from(e: $crate::__AirframeError) -> Self {
                $name::CoreError(e)
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    $name::Success => f.write_str("Success"),
                    $name::CoreError(e) => ::core::write!(f, "Core error: {}", e),
                    $( $( $name::$u_variant => f.write_str($u_msg), )* )?
                    $( $( $name::$d_variant(v) => ::core::write!(f, "{}: {}", $d_msg, v), )* )?
                    $name::Other(code) => ::core::write!(f, "Unknown error code: {}", code),
                }
            }
        }

        impl ::std::error::Error for $name {
            fn source(&self) -> ::core::option::Option<&(dyn ::std::error::Error + 'static)> {
                match self {
                    $name::CoreError(e) => ::core::option::Option::Some(e),
                    _ => ::core::option::Option::None,
                }
            }
        }

        impl $name {
            /// Map this error to a stable integer code in its [`ErrorRange`].
            pub fn to_int(&self) -> u32 {
                let base = $crate::__ErrorRange::$range.base();
                match self {
                    $name::Success => base,
                    $name::CoreError(e) => e.to_int(),
                    $( $( $name::$u_variant => base + $u_off, )* )?
                    $( $( $name::$d_variant(_) => base + $d_off, )* )?
                    $name::Other(code) => *code,
                }
            }

            /// Reconstruct an error from a stable integer code (`data:` variants
            /// come back with a `Default` payload).
            pub fn from_int(val: u32) -> ::core::option::Option<Self> {
                if $crate::__ErrorRange::Core.contains(val) {
                    return $crate::__AirframeError::from_int(val).map($name::CoreError);
                }
                if $crate::__ErrorRange::$range.contains(val) {
                    let code = val - $crate::__ErrorRange::$range.base();
                    match code {
                        0 => ::core::option::Option::Some($name::Success),
                        $( $( $u_off => ::core::option::Option::Some($name::$u_variant), )* )?
                        $( $( $d_off => ::core::option::Option::Some(
                            $name::$d_variant(::core::default::Default::default())
                        ), )* )?
                        _ => ::core::option::Option::Some($name::Other(val)),
                    }
                } else {
                    ::core::option::Option::Some($name::Other(val))
                }
            }
        }

        /// Crate-local `Result` alias keyed to this error type.
        $vis type Result<T> = ::core::result::Result<T, $name>;
    };
}
