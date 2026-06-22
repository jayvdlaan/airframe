use airframe_data::backend::fs::FsBackend;
use airframe_data::cache::{BackendByteCache, ByteCache};
use airframe_data::codec::{Codec, JsonCodec};
use airframe_data::key::Key;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a filesystem backend and adapt it to a ByteCache
    let tmp = tempfile::tempdir()?;
    let fs = FsBackend::new(tmp.path(), JsonCodec.file_extension())?;
    let bytes = BackendByteCache::new(fs);

    let k = Key::new("raw:blob")?;
    let payload = b"hello raw bytes".to_vec();

    bytes.put_bytes(&k, &payload)?;
    let got = bytes.get_bytes(&k)?.unwrap();
    assert_eq!(got, payload);

    // write another and list keys
    let k2 = Key::new("raw:other")?;
    bytes.put_bytes(&k2, b"second")?;
    let mut keys = bytes.list()?;
    keys.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    println!(
        "bytecache_fs ok: keys={:?}",
        keys.iter().map(|k| k.as_str()).collect::<Vec<_>>()
    );
    Ok(())
}
