use crate::error::Result;
use crate::secret::SecretBytes;

/// Resolves a symmetric encryption key, optionally by key id.
///
/// Minimal trait to allow SecretBlob/SecretCache to obtain keys without callers
/// having to pass raw key material everywhere.
pub trait KeyResolver {
    /// Resolve a key optionally using a key_id hint stored alongside the secret.
    /// Implementations may ignore key_id when a single key is used.
    fn resolve(&self, key_id: Option<&[u8]>) -> Result<SecretBytes>;
}
