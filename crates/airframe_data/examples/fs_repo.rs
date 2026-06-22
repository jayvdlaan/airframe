use airframe_data::backend::fs::FsBackend;
use airframe_data::codec::JsonCodec;
use airframe_data::key::Key;
use airframe_data::repo::{Repo, RepoBuilder};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Item {
    id: String,
    qty: u32,
}

fn main() {
    // Use a temporary folder under OS temp dir
    let mut dir = std::env::temp_dir();
    dir.push("airframe_data_example");
    std::fs::create_dir_all(&dir).unwrap();

    let backend = FsBackend::new(&dir, "json").unwrap();
    let codec = JsonCodec;
    let repo: Repo<_, _> = RepoBuilder::new()
        .backend(backend)
        .codec(codec)
        .build()
        .unwrap();

    let key = Key::new("item:widget").unwrap();
    let value = Item {
        id: "widget".into(),
        qty: 5,
    };

    repo.put(&key, &value).expect("put");
    let loaded: Item = repo.get(&key).unwrap().unwrap();
    assert_eq!(loaded, value);
    println!("file written at {:?}", dir);
}
