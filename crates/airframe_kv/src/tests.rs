use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;

use airframe_core::app::AppBuilder;
use airframe_core::bus::EventBus;

use crate::acl::AclMode;
use crate::inmemory::InMemoryKvStore;
use crate::module::KvModule;
use crate::store::{
    DeleteResult, KvEvent, KvStore, KvStoreExt, PutOptions, PutResult, ServiceRegistryKvExt,
};
use crate::watch::{kv_watch_prefix_t, kv_watch_prefix_t_with_deletes, PrefixEvent};

#[tokio::test]
async fn helper_put_if_absent_and_touch() {
    let kv = Arc::new(InMemoryKvStore::new()) as Arc<dyn KvStore>;
    let key = "helpers/test";
    // put_if_absent should insert when missing
    let ins1 = KvStoreExt::put_if_absent(&*kv, key, b"v1", Some(Duration::from_millis(50)))
        .await
        .unwrap();
    assert!(ins1);
    // second call should return false (exists)
    let ins2 = KvStoreExt::put_if_absent(&*kv, key, b"v2", Some(Duration::from_millis(50)))
        .await
        .unwrap();
    assert!(!ins2);
    // value should still be v1
    let (v, _m) = kv.get(key).await.unwrap().unwrap();
    assert_eq!(v, b"v1");
    // Extend TTL before expiry
    tokio::time::sleep(Duration::from_millis(30)).await;
    let touched = KvStoreExt::touch(&*kv, key, Duration::from_millis(100))
        .await
        .unwrap();
    assert!(touched);
    // After additional 50ms, key should still exist due to extended TTL
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(kv.get(key).await.unwrap().is_some());
    // After total >130ms more, it should expire
    tokio::time::sleep(Duration::from_millis(90)).await;
    // Allow janitor to run
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(kv.get(key).await.unwrap().is_none());
}

#[tokio::test]
async fn kv_trait_object_registered_and_typed_helpers_work() {
    let app = AppBuilder::new()
        .with(KvModule::new())
        .start()
        .await
        .unwrap();
    // Retrieve via extension helper and as trait object
    let kv_dyn_via_ext = ServiceRegistryKvExt::kv(&app.services).expect("kv via ext");
    let kv_dyn = app
        .services
        .get::<dyn KvStore>()
        .expect("dyn KvStore present");
    assert!(Arc::ptr_eq(&kv_dyn_via_ext, &kv_dyn));
    // Typed put/get via KvStoreExt
    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq, Debug)]
    struct Foo {
        a: u32,
    }
    KvStoreExt::put_t(
        &*kv_dyn,
        "typed/foo",
        &Foo { a: 5 },
        PutOptions {
            ttl: None,
            if_match: None,
        },
    )
    .await
    .unwrap();
    let got = KvStoreExt::get_t::<Foo>(&*kv_dyn, "typed/foo")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got.0, Foo { a: 5 });
    // Also check concrete accessor
    let kv_conc = ServiceRegistryKvExt::kv_inmemory(&app.services).expect("inmem kv via ext");
    let got2 = kv_conc.get("typed/foo").await.unwrap().unwrap();
    assert_eq!(got2.0, serde_json::to_vec(&Foo { a: 5 }).unwrap());
}

#[tokio::test]
async fn kv_events_forwarded_to_eventbus() {
    // Build app with KvModule; it forwards KvEvent to the EventBus
    let app = AppBuilder::new()
        .with(KvModule::new())
        .start()
        .await
        .unwrap();
    let events = app.events.clone();
    let mut stream = events.subscribe::<KvEvent>().unwrap();
    let kv = app.services.get::<InMemoryKvStore>().expect("kv present");
    // Trigger a KV put and expect to receive a forwarded KvEvent via EventBus
    kv.put(
        "bus/test",
        b"ok",
        PutOptions {
            ttl: None,
            if_match: None,
        },
    )
    .await
    .unwrap();
    let got = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
        .await
        .expect("no timeout")
        .expect("some event");
    match got {
        KvEvent::Put { key, .. } => assert_eq!(key, "bus/test"),
        other => panic!("unexpected event: {:?}", other),
    }
}

#[tokio::test]
async fn kv_put_get_and_watch() {
    // Start app with KvModule so store is registered
    let app = AppBuilder::new()
        .with(KvModule::new())
        .start()
        .await
        .unwrap();
    let kv = app.services.get::<InMemoryKvStore>().expect("kv present");
    // Watch prefix
    let mut w = kv.watch_prefix("foo/").unwrap();
    // Put
    let res = kv
        .put(
            "foo/x",
            b"1",
            PutOptions {
                ttl: None,
                if_match: None,
            },
        )
        .await
        .unwrap();
    match res {
        PutResult::Created { etag } => assert!(etag > 0),
        _ => panic!("expected created"),
    }
    // Get
    let got = kv.get("foo/x").await.unwrap().unwrap();
    assert_eq!(got.0, b"1");
    // Expect event
    let evt = w.next().await.unwrap();
    match evt {
        KvEvent::Put { key, .. } => assert_eq!(key, "foo/x"),
        _ => panic!("expected put"),
    }
}

#[tokio::test]
async fn kv_watch_prefix_typed() {
    // Start app with KvModule so store is registered
    let app = AppBuilder::new()
        .with(KvModule::new())
        .start()
        .await
        .unwrap();
    let kv = app.services.get::<InMemoryKvStore>().expect("kv present");
    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
    struct Item {
        n: u32,
    }
    // Start typed watcher
    let mut w = kv.watch_prefix_t::<Item>("typed/").unwrap();
    // Put a typed value
    kv.put_t(
        "typed/one",
        &Item { n: 7 },
        PutOptions {
            ttl: None,
            if_match: None,
        },
    )
    .await
    .unwrap();
    // Expect typed tuple (key, Item, meta)
    let (k, item, meta) = w.next().await.unwrap();
    assert_eq!(k, "typed/one");
    assert_eq!(item, Item { n: 7 });
    assert!(meta.etag > 0);
}

#[tokio::test]
async fn kv_watch_prefix_typed_via_dyn() {
    let app = AppBuilder::new()
        .with(KvModule::new())
        .start()
        .await
        .unwrap();
    let kv_dyn = app.services.get::<dyn KvStore>().expect("dyn kv");
    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
    struct Item {
        n: u32,
    }
    let mut w = kv_watch_prefix_t::<Item>(kv_dyn.clone(), "typed_dyn/").unwrap();
    KvStoreExt::put_t(
        &*kv_dyn,
        "typed_dyn/one",
        &Item { n: 9 },
        PutOptions {
            ttl: None,
            if_match: None,
        },
    )
    .await
    .unwrap();
    let (k, item, _meta) = w.next().await.unwrap();
    assert_eq!(k, "typed_dyn/one");
    assert_eq!(item, Item { n: 9 });
}

#[tokio::test]
async fn kv_acl_enforce_blocks_disallowed_prefix() {
    let app = AppBuilder::new()
        .with(KvModule::new().with_allow_prefixes(vec!["allowed/".to_string()], AclMode::Enforce))
        .start()
        .await
        .unwrap();
    let kv_dyn = app.services.get::<dyn KvStore>().expect("dyn kv");
    // Allowed write succeeds
    KvStoreExt::put_t(
        &*kv_dyn,
        "allowed/x",
        &1u32,
        PutOptions {
            ttl: None,
            if_match: None,
        },
    )
    .await
    .unwrap();
    // Disallowed write fails
    let err = KvStoreExt::put_t(
        &*kv_dyn,
        "denied/x",
        &2u32,
        PutOptions {
            ttl: None,
            if_match: None,
        },
    )
    .await
    .err()
    .unwrap();
    assert!(err.to_string().contains("disallowed"));
    // Ensure denied key not present
    let got = kv_dyn.get("denied/x").await.unwrap();
    assert!(got.is_none());
}

#[tokio::test]
async fn kv_acl_warn_allows_but_logs() {
    let app = AppBuilder::new()
        .with(KvModule::new().with_allow_prefixes(vec!["allowed/".to_string()], AclMode::Warn))
        .start()
        .await
        .unwrap();
    let kv_dyn = app.services.get::<dyn KvStore>().expect("dyn kv");
    // Both writes succeed under Warn
    KvStoreExt::put_t(
        &*kv_dyn,
        "allowed/x",
        &1u32,
        PutOptions {
            ttl: None,
            if_match: None,
        },
    )
    .await
    .unwrap();
    KvStoreExt::put_t(
        &*kv_dyn,
        "denied/x",
        &2u32,
        PutOptions {
            ttl: None,
            if_match: None,
        },
    )
    .await
    .unwrap();
    // Check they are stored
    assert!(kv_dyn.get("allowed/x").await.unwrap().is_some());
    assert!(kv_dyn.get("denied/x").await.unwrap().is_some());
}

#[tokio::test]
async fn kv_cas_mismatch() {
    let app = AppBuilder::new()
        .with(KvModule::new())
        .start()
        .await
        .unwrap();
    let kv = app.services.get::<InMemoryKvStore>().unwrap();
    let _ = kv
        .put(
            "a",
            b"x",
            PutOptions {
                ttl: None,
                if_match: None,
            },
        )
        .await
        .unwrap();
    let err = kv
        .put(
            "a",
            b"y",
            PutOptions {
                ttl: None,
                if_match: Some(999),
            },
        )
        .await
        .err()
        .unwrap();
    assert!(err.to_string().contains("etag mismatch"));
}

#[tokio::test]
async fn kv_ttl_expiry_emits_event_and_removes_key() {
    let app = AppBuilder::new()
        .with(KvModule::new())
        .start()
        .await
        .unwrap();
    let kv = app.services.get::<InMemoryKvStore>().unwrap();
    // watch all keys
    let mut w = kv.watch_prefix("").unwrap();
    // put with small TTL
    let _ = kv
        .put(
            "ttl/key",
            b"v",
            PutOptions {
                ttl: Some(Duration::from_millis(50)),
                if_match: None,
            },
        )
        .await
        .unwrap();
    // expect an Expire event eventually
    loop {
        if let Some(evt) = w.next().await {
            match evt {
                KvEvent::Expire { key } if key == "ttl/key" => break,
                _ => {}
            }
        } else {
            panic!("watch stream ended unexpectedly");
        }
    }
    // ensure key is gone
    let got = kv.get("ttl/key").await.unwrap();
    assert!(got.is_none());
}

#[tokio::test]
async fn pagination_roundtrip_small_and_large_sets() {
    let app = AppBuilder::new()
        .with(KvModule::new())
        .start()
        .await
        .unwrap();
    let kv = app.services.get::<InMemoryKvStore>().unwrap();
    // small set
    for i in 0..3 {
        let _ = kv
            .put(
                &format!("pg/small/{}", i),
                format!("v{}", i).as_bytes(),
                PutOptions {
                    ttl: None,
                    if_match: None,
                },
            )
            .await
            .unwrap();
    }
    let page1 = kv.list_prefix_paged("pg/small/", 10, None).unwrap();
    assert_eq!(page1.items.len(), 3);
    assert!(page1.next_cursor.is_none());
    // large set
    for i in 0..25 {
        let _ = kv
            .put(
                &format!("pg/large/{:02}", i),
                b"x",
                PutOptions {
                    ttl: None,
                    if_match: None,
                },
            )
            .await
            .unwrap();
    }
    let mut all: Vec<String> = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let p = kv
            .list_prefix_paged("pg/large/", 10, cursor.clone())
            .unwrap();
        for (k, _v, _m) in &p.items {
            all.push(k.clone());
        }
        if p.next_cursor.is_none() {
            break;
        }
        cursor = p.next_cursor.clone();
    }
    assert_eq!(all.len(), 25);
    // ensure order is lexicographic by key
    let mut sorted = all.clone();
    sorted.sort();
    assert_eq!(all, sorted);
}

#[tokio::test]
async fn typed_watch_includes_delete_and_expire_with_optional_meta() {
    use futures::StreamExt;
    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
    struct Item {
        n: u32,
    }
    let app = AppBuilder::new()
        .with(KvModule::new())
        .start()
        .await
        .unwrap();
    let kv_dyn = app.services.get::<dyn KvStore>().unwrap();
    let mut w = kv_watch_prefix_t_with_deletes::<Item>(kv_dyn.clone(), "tw/").unwrap();
    // Put an item
    KvStoreExt::put_t(
        &*kv_dyn,
        "tw/key",
        &Item { n: 1 },
        PutOptions {
            ttl: None,
            if_match: None,
        },
    )
    .await
    .unwrap();
    // Expect Put
    let evt1 = tokio::time::timeout(Duration::from_secs(1), w.next())
        .await
        .unwrap()
        .unwrap();
    match evt1 {
        PrefixEvent::Put { key, value, meta } => {
            assert_eq!(key, "tw/key");
            assert_eq!(value, Item { n: 1 });
            assert!(meta.etag > 0);
        }
        _ => panic!("expected Put"),
    }
    // Delete the key and expect Delete with meta Some
    let del_res = kv_dyn.delete("tw/key", None).await.unwrap();
    assert!(matches!(del_res, DeleteResult::Deleted));
    let evt2 = tokio::time::timeout(Duration::from_secs(1), w.next())
        .await
        .unwrap()
        .unwrap();
    match evt2 {
        PrefixEvent::Delete { key, meta } => {
            assert_eq!(key, "tw/key");
            assert!(meta.is_some());
        }
        _ => panic!("expected Delete"),
    }
    // Put with TTL to cause Expire
    KvStoreExt::put_t(
        &*kv_dyn,
        "tw/key",
        &Item { n: 2 },
        PutOptions {
            ttl: Some(Duration::from_millis(50)),
            if_match: None,
        },
    )
    .await
    .unwrap();
    // Wait for expire event through watcher with cached meta
    loop {
        if let Ok(Some(evt)) = tokio::time::timeout(Duration::from_secs(2), w.next()).await {
            match evt {
                PrefixEvent::Expire { key, meta } if key == "tw/key" => {
                    assert!(meta.is_some());
                    break;
                }
                _ => {}
            }
        } else {
            panic!("timeout waiting for expire");
        }
    }
}

mod conformance {
    use super::*;
    use std::sync::Arc;
    use tokio::time::{sleep, Duration};

    async fn run_basic_conformance(kv: Arc<dyn KvStore>) -> anyhow::Result<()> {
        // clean slate: ensure key not present
        let _ = kv.delete("conf/ns/key", None).await?;
        // cas-create idiom: `if_match: Some(0)` on an ABSENT key means
        // "create if absent" (etag 0 = expect-absent — etags start at 1).
        // Must create, not return "etag mismatch": the ceremony runner's
        // cas_put-create relies on this, and it was broken on real backends.
        let _ = kv.delete("conf/ns/cascreate", None).await?;
        let cas_create = kv
            .put(
                "conf/ns/cascreate",
                b"c",
                PutOptions {
                    ttl: None,
                    if_match: Some(0),
                },
            )
            .await?;
        assert!(
            matches!(cas_create, PutResult::Created { .. }),
            "if_match Some(0) on an absent key must create"
        );
        // ...and Some(0) on an EXISTING key must still fail (0 never matches a real etag).
        let cas_conflict = kv
            .put(
                "conf/ns/cascreate",
                b"c2",
                PutOptions {
                    ttl: None,
                    if_match: Some(0),
                },
            )
            .await;
        assert!(
            cas_conflict.is_err(),
            "if_match Some(0) on an existing key must fail (not overwrite)"
        );
        let _ = kv.delete("conf/ns/cascreate", None).await?;
        // create
        let r = kv
            .put(
                "conf/ns/key",
                b"v1",
                PutOptions {
                    ttl: None,
                    if_match: None,
                },
            )
            .await?;
        let etag = match r {
            PutResult::Created { etag } => etag,
            _ => 0,
        };
        assert!(etag > 0);
        // get
        let (v, m) = kv.get("conf/ns/key").await?.expect("value present");
        assert_eq!(v, b"v1");
        assert_eq!(m.etag, etag);
        // CAS mismatch
        let err = kv
            .put(
                "conf/ns/key",
                b"v2",
                PutOptions {
                    ttl: None,
                    if_match: Some(etag + 1),
                },
            )
            .await;
        assert!(err.is_err(), "expected etag mismatch");
        // CAS update
        let r2 = kv
            .put(
                "conf/ns/key",
                b"v2",
                PutOptions {
                    ttl: None,
                    if_match: Some(etag),
                },
            )
            .await?;
        let etag2 = match r2 {
            PutResult::Updated { etag } => etag,
            _ => 0,
        };
        assert!(etag2 > etag);
        let (v2, m2) = kv.get("conf/ns/key").await?.expect("present");
        assert_eq!(v2, b"v2");
        assert_eq!(m2.etag, etag2);
        // TTL
        let _ = kv
            .put(
                "conf/ttl",
                b"t",
                PutOptions {
                    ttl: Some(Duration::from_millis(200)),
                    if_match: None,
                },
            )
            .await?;
        sleep(Duration::from_millis(450)).await;
        let got = kv.get("conf/ttl").await?;
        assert!(got.is_none(), "ttl key should expire");
        // Listing and pagination
        let _ = kv
            .put(
                "conf/list/a",
                b"1",
                PutOptions {
                    ttl: None,
                    if_match: None,
                },
            )
            .await?;
        let _ = kv
            .put(
                "conf/list/b",
                b"2",
                PutOptions {
                    ttl: None,
                    if_match: None,
                },
            )
            .await?;
        let _ = kv
            .put(
                "conf/list/c",
                b"3",
                PutOptions {
                    ttl: None,
                    if_match: None,
                },
            )
            .await?;
        let page1 = kv.list_prefix_paged("conf/list/", 2, None)?;
        assert_eq!(page1.items.len(), 2);
        let page2 = kv.list_prefix_paged("conf/list/", 2, page1.next_cursor.clone())?;
        assert!(page1.items.len() + page2.items.len() >= 3);
        // Delete
        let d = kv.delete("conf/ns/key", Some(etag2)).await?;
        assert!(matches!(d, DeleteResult::Deleted));
        Ok(())
    }

    #[tokio::test]
    async fn conformance_inmemory() -> anyhow::Result<()> {
        let kv = Arc::new(InMemoryKvStore::new()) as Arc<dyn KvStore>;
        run_basic_conformance(kv).await
    }

    #[cfg(feature = "kv-fs")]
    #[tokio::test]
    async fn conformance_filesystem() -> anyhow::Result<()> {
        use crate::filesystem::FilesystemKvStore;
        let dir = tempfile::tempdir().unwrap();
        let kv = FilesystemKvStore::open(dir.path()).await?;
        run_basic_conformance(kv as Arc<dyn KvStore>).await
    }
}
