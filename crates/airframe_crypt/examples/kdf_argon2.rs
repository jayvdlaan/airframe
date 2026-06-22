use airframe_crypt::kdf::{Argon2Params, Argon2Variant};
use airframe_crypt::suite::{CipherSuite, SoftwareCipherSuite};

// Build with: cargo run -p airframe_crypt --features argon2 --example kdf_argon2
fn main() {
    // Prefer Argon2id (RFC 9106). Defaults are interactive-friendly.
    let params = Argon2Params {
        variant: Argon2Variant::Id,
        ..Default::default()
    };

    let suite = SoftwareCipherSuite::new();

    let password = b"hunter2"; // for demo only
    let salt = b"0123456789ABCDEF"; // 16+ bytes required; use suite.random_bytes(16+) in real apps

    let key = suite
        .argon2(password, salt, params, 32)
        .expect("argon2 derive");
    println!("Derived key ({} bytes): {:02x?}", key.len(), key);
}
