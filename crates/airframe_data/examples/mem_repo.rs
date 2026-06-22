use airframe_data::backend::mem::MemBackend;
use airframe_data::codec::JsonCodec;
use airframe_data::key::Key;
use airframe_data::repo::{Repo, RepoBuilder};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Profile {
    name: String,
    age: u8,
}

fn main() {
    let backend = MemBackend::new();
    let codec = JsonCodec;
    let repo: Repo<_, _> = RepoBuilder::new()
        .backend(backend)
        .codec(codec)
        .build()
        .unwrap();

    let key = Key::new("user:alice").unwrap();
    let value = Profile {
        name: "Alice".into(),
        age: 30,
    };

    repo.put(&key, &value).expect("put");
    println!("contains? {}", repo.contains(&key).unwrap());

    let loaded: Profile = repo.get(&key).unwrap().unwrap();
    assert_eq!(loaded, value);
    println!("loaded profile: {:?}", loaded);

    let keys = repo.list().unwrap();
    println!(
        "stored keys: {:?}",
        keys.iter().map(|k| k.as_str()).collect::<Vec<_>>()
    );

    repo.remove(&key).expect("remove");
    println!("after remove contains? {}", repo.contains(&key).unwrap());
}
