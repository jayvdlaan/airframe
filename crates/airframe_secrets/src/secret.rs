// Prevent enabling two backends at once (supporting both new and legacy feature names)
#[cfg(all(
    any(feature = "backend-secrecy", feature = "secret-backend-secrecy"),
    any(feature = "backend-secrets", feature = "secret-backend-secrets"),
))]
compile_error!("Enable only one of backend-secrecy (legacy: secret-backend-secrecy) or backend-secrets (legacy: secret-backend-secrets)");

pub struct SecretBytes {
    inner: Inner,
}

enum Inner {
    #[cfg(any(feature = "backend-secrecy", feature = "secret-backend-secrecy"))]
    Secrecy(secrecy::SecretBox<[u8]>),
    #[cfg(any(feature = "backend-secrets", feature = "secret-backend-secrets"))]
    Secrets(secrets::Secret<Vec<u8>>), // adjust to actual API of the secrets crate if needed
    // Fallback when no backend is enabled: store bytes plainly but still zeroize on drop
    Plain(Vec<u8>),
}

impl core::fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SecretBytes(..redacted..)")
    }
}

impl SecretBytes {
    pub fn from_vec(v: Vec<u8>) -> Self {
        #[cfg(any(feature = "backend-secrecy", feature = "secret-backend-secrecy"))]
        {
            use secrecy::SecretBox;
            return Self {
                inner: Inner::Secrecy(SecretBox::new(v.into_boxed_slice())),
            };
        }
        #[cfg(any(feature = "backend-secrets", feature = "secret-backend-secrets"))]
        {
            return Self {
                inner: Inner::Secrets(secrets::Secret::new(v)),
            };
        }
        // Fallback
        Self {
            inner: Inner::Plain(v),
        }
    }

    pub fn len(&self) -> usize {
        match &self.inner {
            #[cfg(any(feature = "backend-secrecy", feature = "secret-backend-secrecy"))]
            Inner::Secrecy(b) => {
                use secrecy::ExposeSecret;
                b.expose_secret().len()
            }
            #[cfg(any(feature = "backend-secrets", feature = "secret-backend-secrets"))]
            Inner::Secrets(s) => s.expose().len(),
            Inner::Plain(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    // Closure-based exposure to avoid leaking references publicly
    pub fn expose<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        match &self.inner {
            #[cfg(any(feature = "backend-secrecy", feature = "secret-backend-secrecy"))]
            Inner::Secrecy(b) => {
                use secrecy::ExposeSecret;
                let slice: &[u8] = b.expose_secret();
                f(slice)
            }
            #[cfg(any(feature = "backend-secrets", feature = "secret-backend-secrets"))]
            Inner::Secrets(s) => {
                let guard = s.expose();
                f(&guard)
            }
            Inner::Plain(v) => f(v.as_slice()),
        }
    }

    // Dangerous: makes a copy of the secret data
    pub fn to_vec(&self) -> Vec<u8> {
        self.expose(|b| b.to_vec())
    }
}

// Helper to adapt SecretBytes into a temporary secrecy::SecretSlice<u8> for calling airframe_crypt
impl SecretBytes {
    pub fn with_secrecy_slice<T>(&self, f: impl FnOnce(&secrecy::SecretSlice<u8>) -> T) -> T {
        self.expose(|bytes| {
            // Create a temporary SecretSlice owned value and pass a reference to it
            let tmp = secrecy::SecretSlice::new(bytes.to_vec().into_boxed_slice());
            let out = f(&tmp);
            drop(tmp);
            out
        })
    }
}
