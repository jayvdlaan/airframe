use std::fmt;

/// Platform identifier used for module compatibility checks.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum Platform {
    Android,
    Ios,
    Windows,
    Macos,
    Linux,
    Unknown,
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Platform::Android => "android",
            Platform::Ios => "ios",
            Platform::Windows => "windows",
            Platform::Macos => "macos",
            Platform::Linux => "linux",
            Platform::Unknown => "unknown",
        };
        f.write_str(s)
    }
}

/// Returns the platform for the current compilation target.
#[inline]
pub fn current_platform() -> Platform {
    if cfg!(target_os = "android") {
        Platform::Android
    } else if cfg!(target_os = "ios") {
        Platform::Ios
    } else if cfg!(target_os = "windows") {
        Platform::Windows
    } else if cfg!(target_os = "macos") {
        Platform::Macos
    } else if cfg!(target_os = "linux") {
        Platform::Linux
    } else {
        Platform::Unknown
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct PlatformSupport {
    mask: u32,
    reason: Option<&'static str>,
}

impl PlatformSupport {
    const ANDROID: u32 = 1 << 0;
    const IOS: u32 = 1 << 1;
    const WINDOWS: u32 = 1 << 2;
    const MACOS: u32 = 1 << 3;
    const LINUX: u32 = 1 << 4;
    const UNKNOWN: u32 = 1 << 31;
    const ALL: u32 =
        Self::ANDROID | Self::IOS | Self::WINDOWS | Self::MACOS | Self::LINUX | Self::UNKNOWN;

    #[inline]
    pub const fn all() -> Self {
        Self {
            mask: Self::ALL,
            reason: None,
        }
    }

    #[inline]
    pub const fn none(reason: &'static str) -> Self {
        Self {
            mask: 0,
            reason: Some(reason),
        }
    }

    #[inline]
    pub const fn windows_only(reason: &'static str) -> Self {
        Self {
            mask: Self::WINDOWS,
            reason: Some(reason),
        }
    }

    #[inline]
    pub const fn desktop_only(reason: &'static str) -> Self {
        Self {
            mask: Self::WINDOWS | Self::MACOS | Self::LINUX,
            reason: Some(reason),
        }
    }

    #[inline]
    pub const fn mobile_only(reason: &'static str) -> Self {
        Self {
            mask: Self::ANDROID | Self::IOS,
            reason: Some(reason),
        }
    }

    #[inline]
    pub const fn reason(&self) -> Option<&'static str> {
        self.reason
    }

    #[inline]
    pub fn allows(&self, platform: Platform) -> bool {
        let bit = match platform {
            Platform::Android => Self::ANDROID,
            Platform::Ios => Self::IOS,
            Platform::Windows => Self::WINDOWS,
            Platform::Macos => Self::MACOS,
            Platform::Linux => Self::LINUX,
            Platform::Unknown => Self::UNKNOWN,
        };
        (self.mask & bit) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_display() {
        assert_eq!(format!("{}", Platform::Android), "android");
        assert_eq!(format!("{}", Platform::Ios), "ios");
        assert_eq!(format!("{}", Platform::Windows), "windows");
        assert_eq!(format!("{}", Platform::Macos), "macos");
        assert_eq!(format!("{}", Platform::Linux), "linux");
        assert_eq!(format!("{}", Platform::Unknown), "unknown");
    }

    #[test]
    fn current_platform_returns_valid() {
        let p = current_platform();
        // Should be one of the known platforms
        assert!(matches!(
            p,
            Platform::Android
                | Platform::Ios
                | Platform::Windows
                | Platform::Macos
                | Platform::Linux
                | Platform::Unknown
        ));
    }

    #[test]
    fn platform_support_all_allows_everything() {
        let support = PlatformSupport::all();
        assert!(support.allows(Platform::Android));
        assert!(support.allows(Platform::Ios));
        assert!(support.allows(Platform::Windows));
        assert!(support.allows(Platform::Macos));
        assert!(support.allows(Platform::Linux));
        assert!(support.allows(Platform::Unknown));
        assert!(support.reason().is_none());
    }

    #[test]
    fn platform_support_none_allows_nothing() {
        let support = PlatformSupport::none("not supported");
        assert!(!support.allows(Platform::Android));
        assert!(!support.allows(Platform::Ios));
        assert!(!support.allows(Platform::Windows));
        assert!(!support.allows(Platform::Macos));
        assert!(!support.allows(Platform::Linux));
        assert!(!support.allows(Platform::Unknown));
        assert_eq!(support.reason(), Some("not supported"));
    }

    #[test]
    fn platform_support_windows_only() {
        let support = PlatformSupport::windows_only("windows feature");
        assert!(!support.allows(Platform::Android));
        assert!(!support.allows(Platform::Ios));
        assert!(support.allows(Platform::Windows));
        assert!(!support.allows(Platform::Macos));
        assert!(!support.allows(Platform::Linux));
        assert_eq!(support.reason(), Some("windows feature"));
    }

    #[test]
    fn platform_support_desktop_only() {
        let support = PlatformSupport::desktop_only("desktop feature");
        assert!(!support.allows(Platform::Android));
        assert!(!support.allows(Platform::Ios));
        assert!(support.allows(Platform::Windows));
        assert!(support.allows(Platform::Macos));
        assert!(support.allows(Platform::Linux));
        assert_eq!(support.reason(), Some("desktop feature"));
    }

    #[test]
    fn platform_support_mobile_only() {
        let support = PlatformSupport::mobile_only("mobile feature");
        assert!(support.allows(Platform::Android));
        assert!(support.allows(Platform::Ios));
        assert!(!support.allows(Platform::Windows));
        assert!(!support.allows(Platform::Macos));
        assert!(!support.allows(Platform::Linux));
        assert_eq!(support.reason(), Some("mobile feature"));
    }

    #[test]
    fn platform_equality() {
        assert_eq!(Platform::Linux, Platform::Linux);
        assert_ne!(Platform::Linux, Platform::Windows);
    }

    #[test]
    fn platform_clone_and_copy() {
        let p1 = Platform::Macos;
        let p2 = p1; // Copy
        let p3 = p1;
        assert_eq!(p1, p2);
        assert_eq!(p1, p3);
    }
}
