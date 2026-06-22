use airframe_core::app::AppBuilder;
use airframe_kv::{KvModule, KvStore, KvStoreExt, PutOptions};
use futures::StreamExt;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = AppBuilder::new().with(KvModule::new()).start().await?;

    if let Some(kv) = app.services.get::<airframe_kv::InMemoryKvStore>() {
        let mut w = kv.watch_prefix("demo/")?;
        kv.put(
            "demo/hello",
            b"world",
            PutOptions {
                ttl: Some(Duration::from_millis(200)),
                if_match: None,
            },
        )
        .await?;
        if let Some((v, _)) = kv.get("demo/hello").await? {
            println!("demo/hello = {}", String::from_utf8_lossy(&v));
        }
        // Observe one event deterministically
        if let Some(evt) = tokio::time::timeout(Duration::from_secs(1), w.next()).await? {
            println!("event: {:?}", evt);
        }
        // Typed helpers via trait-object path
        let kv_dyn = app.services.get::<dyn KvStore>().unwrap();
        KvStoreExt::put_t(
            &*kv_dyn,
            "typed/demo",
            &42u32,
            PutOptions {
                ttl: None,
                if_match: None,
            },
        )
        .await?;
        let (n, _) = KvStoreExt::get_t::<u32>(&*kv_dyn, "typed/demo")
            .await?
            .unwrap();
        println!("typed/demo = {}", n);
    }

    app.cancel.cancel();
    Ok(())
}
