use airframe_data::cache::ByteCache;
use airframe_data::key::Key;
use airframe_winreg::{HiveKind, WinRegByteCache};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // On non-Windows, this example will likely fail at runtime (InvalidState).
    let cache = WinRegByteCache::new(HiveKind::CurrentUser, r"Software\\Airframe\\ExampleCache");

    let k = Key::new("greeting")?;
    cache.put_bytes(&k, b"hello")?;
    assert!(cache.contains(&k)?);
    let out = cache.get_bytes(&k)?.unwrap();
    assert_eq!(out, b"hello");
    cache.remove(&k)?;

    println!("winreg bytecache example ok");
    Ok(())
}
