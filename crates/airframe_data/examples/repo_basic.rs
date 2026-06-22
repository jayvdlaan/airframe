use airframe_data::{
    backend::{fs::FsBackend, mem::MemBackend},
    codec::{Codec, JsonCodec},
    key::Key,
    repo::RepoBuilder,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Profile {
    name: String,
    age: u8,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // In-memory repository
    let repo_mem = RepoBuilder::new()
        .backend(MemBackend::new())
        .codec(JsonCodec)
        .build()?;

    let k = Key::new("user:alice")?;
    let v = Profile {
        name: "Alice".into(),
        age: 30,
    };
    repo_mem.put(&k, &v)?;
    let out: Profile = repo_mem.get(&k)?.unwrap();
    assert_eq!(out, v);

    // Filesystem repository
    let tmp = tempfile::tempdir()?;
    let fs = FsBackend::new(tmp.path(), JsonCodec.file_extension())?;
    let repo_fs = RepoBuilder::new().backend(fs).codec(JsonCodec).build()?;
    repo_fs.put(&k, &v)?;

    println!("Repo examples completed successfully");
    Ok(())
}
