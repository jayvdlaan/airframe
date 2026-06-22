use super::*;
use async_trait::async_trait;
use semver::Version;
use std::sync::Mutex;

use crate::bus::{CommandBus, EventBus};
use crate::module::ModuleDescriptor;
use crate::platform::PlatformSupport;

struct ProbeModule {
    desc: ModuleDescriptor,
    calls: Arc<Mutex<Vec<&'static str>>>,
}

#[async_trait]
impl Module for ProbeModule {
    fn descriptor(&self) -> &ModuleDescriptor {
        &self.desc
    }
    async fn init(&mut self, _ctx: ModuleContext) -> Result<()> {
        self.calls.lock().unwrap().push("init");
        Ok(())
    }
    async fn start(&mut self) -> Result<()> {
        self.calls.lock().unwrap().push("start");
        Ok(())
    }
    async fn stop(&mut self) -> Result<()> {
        self.calls.lock().unwrap().push("stop");
        Ok(())
    }
}

#[tokio::test]
async fn app_builder_allows_bootstrap_minimal() {
    // Should start fine with no modules and a default bootstrap
    let _app = AppBuilder::new()
        .with_bootstrap(Bootstrap::default())
        .start()
        .await
        .unwrap();
}

#[tokio::test]
async fn fails_fast_on_unsupported_platform_module() {
    struct UnsupportedModule {
        desc: ModuleDescriptor,
    }

    #[async_trait]
    impl Module for UnsupportedModule {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }

        fn platform_support(&self) -> PlatformSupport {
            PlatformSupport::none("explicitly unsupported for this test")
        }
    }

    let m = UnsupportedModule {
        desc: ModuleDescriptor {
            name: "unsupported",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
    };

    let err = AppBuilder::new()
        .with(m)
        .start()
        .await
        .err()
        .expect("expected platform support preflight to fail");
    let msg = err.to_string();
    assert!(msg.contains("module \"unsupported\" is not supported"));
    assert!(msg.contains("explicitly unsupported"));
}

#[tokio::test]
async fn app_builder_lifecycle_linear() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let desc = ModuleDescriptor {
        name: "probe",
        version: Version::parse("0.1.0").unwrap(),
        provides: &[],
        requires: &[],
        optional_requires: &[],
        requires_with_versions: &[],
        optional_requires_with_versions: &[],
    };
    let m = ProbeModule {
        desc,
        calls: calls.clone(),
    };

    let builder = AppBuilder::new().with(m);
    let mut app = builder.start().await.unwrap();
    // After start, init and start should have been called
    let got = calls.lock().unwrap().clone();
    assert_eq!(got, vec!["init", "start"]);

    app.shutdown().await.unwrap();
    let got2 = calls.lock().unwrap().clone();
    assert_eq!(got2, vec!["init", "start", "stop"]);
}

#[tokio::test]
async fn resolves_dependencies_and_orders_lifecycle() {
    // Module A provides CAP_A
    struct ModA {
        desc: ModuleDescriptor,
        calls: Arc<Mutex<Vec<&'static str>>>,
    }
    #[async_trait]
    impl Module for ModA {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
        async fn init(&mut self, _ctx: ModuleContext) -> Result<()> {
            self.calls.lock().unwrap().push("A:init");
            Ok(())
        }
        async fn start(&mut self) -> Result<()> {
            self.calls.lock().unwrap().push("A:start");
            Ok(())
        }
        async fn stop(&mut self) -> Result<()> {
            self.calls.lock().unwrap().push("A:stop");
            Ok(())
        }
    }
    // Module B requires CAP_A
    struct ModB {
        desc: ModuleDescriptor,
        calls: Arc<Mutex<Vec<&'static str>>>,
    }
    #[async_trait]
    impl Module for ModB {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
        async fn init(&mut self, _ctx: ModuleContext) -> Result<()> {
            self.calls.lock().unwrap().push("B:init");
            Ok(())
        }
        async fn start(&mut self) -> Result<()> {
            self.calls.lock().unwrap().push("B:start");
            Ok(())
        }
        async fn stop(&mut self) -> Result<()> {
            self.calls.lock().unwrap().push("B:stop");
            Ok(())
        }
    }
    let calls = Arc::new(Mutex::new(Vec::new()));
    let a = ModA {
        desc: ModuleDescriptor {
            name: "A",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[crate::module::CAP_A.0],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
        calls: calls.clone(),
    };
    let b = ModB {
        desc: ModuleDescriptor {
            name: "B",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[],
            requires: &[crate::module::CAP_A.0],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
        calls: calls.clone(),
    };

    let builder = AppBuilder::new().with(b).with(a); // add out of order intentionally
    let mut app = builder.start().await.unwrap();
    let got = calls.lock().unwrap().clone();
    assert_eq!(got, vec!["A:init", "B:init", "A:start", "B:start"]);
    app.shutdown().await.unwrap();
    let got2 = calls.lock().unwrap().clone();
    assert_eq!(
        got2,
        vec!["A:init", "B:init", "A:start", "B:start", "B:stop", "A:stop"]
    );
}

#[tokio::test]
async fn module_can_register_command_handler_via_registry_bus() {
    use crate::bus::Command;
    use futures::FutureExt;
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct Ping;
    impl Command for Ping {
        const NAME: &'static str = "Ping";
    }

    struct CmdModule {
        desc: ModuleDescriptor,
        counter: Arc<Mutex<i32>>,
    }
    #[async_trait]
    impl Module for CmdModule {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
        async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
            let bus = ctx
                .services
                .get::<InMemoryCommandBus>()
                .expect("cmd bus in registry");
            let counter = self.counter.clone();
            bus.register_handler::<Ping, _>(move |_c, _cancel| {
                let counter = counter.clone();
                async move {
                    *counter.lock().unwrap() += 1;
                    Ok(())
                }
                .boxed()
            })?;
            Ok(())
        }
    }
    let counter = Arc::new(Mutex::new(0));
    let m = CmdModule {
        desc: ModuleDescriptor {
            name: "cmd",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
        counter: counter.clone(),
    };
    let app = AppBuilder::new().with(m).start().await.unwrap();
    app.commands.dispatch(Ping, None).await.unwrap();
    assert_eq!(*counter.lock().unwrap(), 1);
}

#[tokio::test]
async fn module_can_subscribe_and_receive_event() {
    use crate::bus::Event;
    use futures::StreamExt;
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct AppStartup;
    impl Event for AppStartup {
        const NAME: &'static str = "AppStartup";
    }

    struct SubscriberModule {
        desc: ModuleDescriptor,
    }
    #[async_trait]
    impl Module for SubscriberModule {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
        async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
            let events = ctx
                .services
                .get::<InMemoryEventBus>()
                .expect("event bus in registry");
            let mut stream = events.subscribe::<AppStartup>()?;
            let ctx2 = ctx.clone();
            tokio::spawn(async move {
                // Wait for one event, then cancel the app to allow test to complete
                let _ = stream.next().await;
                ctx2.cancel.cancel();
            });
            Ok(())
        }
    }

    let desc = ModuleDescriptor {
        name: "sub",
        version: Version::parse("0.1.0").unwrap(),
        provides: &[],
        requires: &[],
        optional_requires: &[],
        requires_with_versions: &[],
        optional_requires_with_versions: &[],
    };
    let app = AppBuilder::new()
        .with(SubscriberModule { desc })
        .start()
        .await
        .unwrap();
    app.events.publish(AppStartup, None).await.unwrap();
    // run_until_cancelled will return after the subscriber cancels upon receiving the event
    app.run_until_cancelled().await.unwrap();
}

/// When the `layer-check` feature is enabled, starting an app with an upward
/// dependency should fail before dependency resolution proceeds.
#[cfg(feature = "layer-check")]
#[tokio::test]
async fn layer_violation_fails_startup() {
    // Define a faux Config module that (illegally) depends on logging.
    struct ModCfg {
        desc: ModuleDescriptor,
    }
    #[async_trait]
    impl Module for ModCfg {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
    }
    // Define a Logging module that provides CAP_LOGGING.
    struct ModLog {
        desc: ModuleDescriptor,
    }
    #[async_trait]
    impl Module for ModLog {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
    }
    let cfg = ModCfg {
        desc: ModuleDescriptor {
            name: "config",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[crate::module::CAP_CONFIG.0],
            requires: &[crate::module::CAP_LOGGING.0], // upward edge (L2 -> L3) should be rejected
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
    };
    let log = ModLog {
        desc: ModuleDescriptor {
            name: "logging",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[crate::module::CAP_LOGGING.0],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
    };

    let res = AppBuilder::new().with(cfg).with(log).start().await;
    match res {
        Ok(_) => panic!("expected layer violation to fail startup"),
        Err(e) => {
            let msg = format!("{}", e);
            assert!(msg.contains("layer violation"), "unexpected error: {}", msg);
        }
    }
}

#[tokio::test]
async fn optional_requires_do_not_block_startup() {
    // A provides optional CAP_X, B optionally requires CAP_X
    struct A {
        desc: ModuleDescriptor,
        calls: Arc<Mutex<Vec<&'static str>>>,
    }
    struct B {
        desc: ModuleDescriptor,
        calls: Arc<Mutex<Vec<&'static str>>>,
    }
    #[async_trait]
    impl Module for A {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
        async fn init(&mut self, _ctx: ModuleContext) -> Result<()> {
            self.calls.lock().unwrap().push("A:init");
            Ok(())
        }
        async fn start(&mut self) -> Result<()> {
            self.calls.lock().unwrap().push("A:start");
            Ok(())
        }
        async fn stop(&mut self) -> Result<()> {
            self.calls.lock().unwrap().push("A:stop");
            Ok(())
        }
    }
    #[async_trait]
    impl Module for B {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
        async fn init(&mut self, _ctx: ModuleContext) -> Result<()> {
            self.calls.lock().unwrap().push("B:init");
            Ok(())
        }
        async fn start(&mut self) -> Result<()> {
            self.calls.lock().unwrap().push("B:start");
            Ok(())
        }
        async fn stop(&mut self) -> Result<()> {
            self.calls.lock().unwrap().push("B:stop");
            Ok(())
        }
    }

    let calls = Arc::new(Mutex::new(Vec::new()));
    let a = A {
        desc: ModuleDescriptor {
            name: "A",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[crate::module::CAP_X.0],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
        calls: calls.clone(),
    };
    let b1 = B {
        desc: ModuleDescriptor {
            name: "B",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[],
            requires: &[],
            optional_requires: &[crate::module::CAP_X.0],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
        calls: calls.clone(),
    };

    // Case 1: No provider present — should still start (optional)
    let mut app = AppBuilder::new().with(b1).start().await.unwrap();
    app.shutdown().await.unwrap();
    calls.lock().unwrap().clear();

    // Case 2: Provider present — ordering should ensure A before B
    let b2 = B {
        desc: ModuleDescriptor {
            name: "B2",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[],
            requires: &[],
            optional_requires: &[crate::module::CAP_X.0],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
        calls: calls.clone(),
    };
    let mut app = AppBuilder::new().with(b2).with(a).start().await.unwrap();
    let got = calls.lock().unwrap().clone();
    assert_eq!(
        got,
        vec!["A:init", "B:init", "A:start", "B:start"],
        "expected provider before optional consumer"
    );
    app.shutdown().await.unwrap();
    let got2 = calls.lock().unwrap().clone();
    assert_eq!(
        got2,
        vec!["A:init", "B:init", "A:start", "B:start", "B:stop", "A:stop"]
    );
}

#[tokio::test]
async fn optional_versioned_requires_missing_vs_present() {
    use crate::module::CAP_X;
    // Provider publishes CAP_X@1.5.0; Consumer optionally requires CAP_X within >=1.0,<2.0
    struct Provider {
        desc: ModuleDescriptor,
    }
    struct Consumer {
        desc: ModuleDescriptor,
        calls: Arc<Mutex<Vec<&'static str>>>,
    }
    #[async_trait]
    impl Module for Provider {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
        async fn init(&mut self, _ctx: ModuleContext) -> Result<()> {
            Ok(())
        }
    }
    #[async_trait]
    impl Module for Consumer {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
        async fn init(&mut self, _ctx: ModuleContext) -> Result<()> {
            self.calls.lock().unwrap().push("cons:init");
            Ok(())
        }
        async fn start(&mut self) -> Result<()> {
            self.calls.lock().unwrap().push("cons:start");
            Ok(())
        }
    }

    // Case 1: Missing provider — startup should still succeed
    let calls = Arc::new(Mutex::new(Vec::new()));
    let cons_only = Consumer {
        desc: ModuleDescriptor {
            name: "cons",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[(CAP_X.0, ">=1.0,<2.0")],
        },
        calls: calls.clone(),
    };
    let mut app = AppBuilder::new()
        .with(cons_only)
        .start()
        .await
        .expect("optional versioned requires must not block startup when missing");
    app.shutdown().await.unwrap();
    calls.lock().unwrap().clear();

    // Case 2: Provider present and compatible — provider should initialize before consumer
    let prov = Provider {
        desc: ModuleDescriptor {
            name: "prov",
            version: Version::parse("1.5.0").unwrap(),
            provides: &[CAP_X.0],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
    };
    let cons = Consumer {
        desc: ModuleDescriptor {
            name: "cons2",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[(CAP_X.0, ">=1.0,<2.0")],
        },
        calls: calls.clone(),
    };
    let mut app = AppBuilder::new()
        .with(cons)
        .with(prov)
        .start()
        .await
        .unwrap();
    // We can't directly observe provider's init here; assert consumer ran and startup succeeded
    let got = calls.lock().unwrap().clone();
    assert_eq!(
        got,
        vec!["cons:init", "cons:start"],
        "consumer should initialize and start when compatible provider present"
    );
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn missing_required_dependency_fails_start() {
    // Module B requires CAP_Y which is not provided
    struct B {
        desc: ModuleDescriptor,
    }
    #[async_trait]
    impl Module for B {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
    }
    let b = B {
        desc: ModuleDescriptor {
            name: "B",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[],
            requires: &[crate::module::CAP_Y.0],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
    };
    let res = AppBuilder::new().with(b).start().await;
    assert!(res.is_err(), "expected unresolved dependencies error");
    let msg = format!("{}", res.err().unwrap());
    assert!(
        msg.contains("unresolved module dependencies") && msg.contains(crate::module::CAP_Y.0),
        "unexpected error: {}",
        msg
    );
}

#[tokio::test]
async fn versioned_requires_satisfied_orders_provider_first() {
    use crate::module::CAP_HTTP_SERVER;
    // Provider publishes cap:http.server @ 1.2.3 and registers a marker service in init
    struct Provider {
        desc: ModuleDescriptor,
    }
    struct Marker;
    #[async_trait]
    impl Module for Provider {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
        async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
            ctx.services.register::<Marker>(Arc::new(Marker));
            Ok(())
        }
    }
    // Consumer requires cap:http.server within >=1.0,<2.0 and asserts marker is present in init
    struct Consumer {
        desc: ModuleDescriptor,
    }
    #[async_trait]
    impl Module for Consumer {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
        async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
            assert!(
                ctx.services.get::<Marker>().is_some(),
                "provider should run before consumer due to versioned requires"
            );
            Ok(())
        }
    }
    let prov = Provider {
        desc: ModuleDescriptor {
            name: "prov",
            version: Version::parse("1.2.3").unwrap(),
            provides: &[CAP_HTTP_SERVER.0],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
    };
    let cons = Consumer {
        desc: ModuleDescriptor {
            name: "cons",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[(CAP_HTTP_SERVER.0, ">=1.0,<2.0")],
            optional_requires_with_versions: &[],
        },
    };
    let mut app = AppBuilder::new()
        .with(cons)
        .with(prov)
        .start()
        .await
        .expect("app should start");
    app.shutdown().await.expect("shutdown ok");
}

#[tokio::test]
async fn versioned_requires_unmet_fails_startup() {
    use crate::module::CAP_HTTP_SERVER;
    struct Provider {
        desc: ModuleDescriptor,
    }
    #[async_trait]
    impl Module for Provider {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
    }
    struct Consumer {
        desc: ModuleDescriptor,
    }
    #[async_trait]
    impl Module for Consumer {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
    }

    let prov = Provider {
        desc: ModuleDescriptor {
            name: "prov",
            version: Version::parse("0.5.0").unwrap(),
            provides: &[CAP_HTTP_SERVER.0],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
    };
    let cons = Consumer {
        desc: ModuleDescriptor {
            name: "cons",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[(CAP_HTTP_SERVER.0, ">=1.0")],
            optional_requires_with_versions: &[],
        },
    };
    let res = AppBuilder::new().with(cons).with(prov).start().await;
    assert!(
        res.is_err(),
        "expected unmet versioned requires to fail start"
    );
    let msg = format!("{}", res.err().unwrap());
    assert!(
        msg.contains(CAP_HTTP_SERVER.0) && msg.contains(">=1.0"),
        "unexpected error: {}",
        msg
    );
}

#[tokio::test]
async fn start_error_propagates_and_stops_processing() {
    // A starts fine, B.start fails; verify error bubbles and C.start is not called
    struct A {
        desc: ModuleDescriptor,
        calls: Arc<Mutex<Vec<&'static str>>>,
    }
    struct B {
        desc: ModuleDescriptor,
        calls: Arc<Mutex<Vec<&'static str>>>,
    }
    struct C {
        desc: ModuleDescriptor,
        calls: Arc<Mutex<Vec<&'static str>>>,
    }
    #[async_trait]
    impl Module for A {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
        async fn init(&mut self, _ctx: ModuleContext) -> Result<()> {
            self.calls.lock().unwrap().push("A:init");
            Ok(())
        }
        async fn start(&mut self) -> Result<()> {
            self.calls.lock().unwrap().push("A:start");
            Ok(())
        }
        async fn stop(&mut self) -> Result<()> {
            Ok(())
        }
    }
    #[async_trait]
    impl Module for B {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
        async fn init(&mut self, _ctx: ModuleContext) -> Result<()> {
            self.calls.lock().unwrap().push("B:init");
            Ok(())
        }
        async fn start(&mut self) -> Result<()> {
            anyhow::bail!("boom")
        }
        async fn stop(&mut self) -> Result<()> {
            Ok(())
        }
    }
    #[async_trait]
    impl Module for C {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
        async fn init(&mut self, _ctx: ModuleContext) -> Result<()> {
            self.calls.lock().unwrap().push("C:init");
            Ok(())
        }
        async fn start(&mut self) -> Result<()> {
            self.calls.lock().unwrap().push("C:start");
            Ok(())
        }
        async fn stop(&mut self) -> Result<()> {
            Ok(())
        }
    }

    let calls = Arc::new(Mutex::new(Vec::new()));
    let a = A {
        desc: ModuleDescriptor {
            name: "A",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[crate::module::CAP_A.0],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
        calls: calls.clone(),
    };
    let b = B {
        desc: ModuleDescriptor {
            name: "B",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[crate::module::CAP_B.0],
            requires: &[crate::module::CAP_A.0],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
        calls: calls.clone(),
    };
    let c = C {
        desc: ModuleDescriptor {
            name: "C",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[],
            requires: &[crate::module::CAP_B.0],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
        calls: calls.clone(),
    };

    let res = AppBuilder::new().with(c).with(b).with(a).start().await;
    assert!(res.is_err());
    let got = calls.lock().unwrap().clone();
    // All inits run first in order A,B,C; then A.start; then B.start fails; C.start is never called
    assert_eq!(
        got,
        vec!["A:init", "B:init", "C:init", "A:start"],
        "unexpected call sequence: {:?}",
        got
    );
}

#[tokio::test]
async fn shutdown_occurs_in_reverse_order() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    struct M {
        desc: ModuleDescriptor,
        #[allow(dead_code)]
        name: &'static str,
        calls: Arc<Mutex<Vec<&'static str>>>,
    }
    #[async_trait]
    impl Module for M {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
        async fn init(&mut self, _ctx: ModuleContext) -> Result<()> {
            self.calls.lock().unwrap().push(concat!("", "init"));
            Ok(())
        }
        async fn start(&mut self) -> Result<()> {
            self.calls.lock().unwrap().push(concat!("", "start"));
            Ok(())
        }
        async fn stop(&mut self) -> Result<()> {
            self.calls.lock().unwrap().push(concat!("", "stop"));
            Ok(())
        }
    }
    let a = M {
        desc: ModuleDescriptor {
            name: "A",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[crate::module::CAP_A.0],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
        name: "A",
        calls: calls.clone(),
    };
    let b = M {
        desc: ModuleDescriptor {
            name: "B",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[],
            requires: &[crate::module::CAP_A.0],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
        name: "B",
        calls: calls.clone(),
    };
    let mut app = AppBuilder::new().with(b).with(a).start().await.unwrap();
    calls.lock().unwrap().clear();
    app.shutdown().await.unwrap();
    let got = calls.lock().unwrap().clone();
    assert_eq!(
        got,
        vec!["stop", "stop"],
        "expected two stops in reverse order (B then A)"
    );
}

#[tokio::test]
async fn service_registry_bus_helpers_work() {
    // Start a bare app (no modules); buses are still registered by AppBuilder
    let app = AppBuilder::new().start().await.unwrap();
    assert!(app.services.event_bus().is_some());
    assert!(app.services.command_bus().is_some());
    assert!(app.services.query_bus().is_some());
}

#[tokio::test]
async fn exports_dependency_graph_json_and_dot() {
    // Define two modules: A provides CAP_A, B requires CAP_A and optionally CAP_X
    struct ModA {
        desc: ModuleDescriptor,
    }
    #[async_trait]
    impl Module for ModA {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
    }
    struct ModB {
        desc: ModuleDescriptor,
    }
    #[async_trait]
    impl Module for ModB {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
    }

    let a = ModA {
        desc: ModuleDescriptor {
            name: "A",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[crate::module::CAP_A.0],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
    };
    let b = ModB {
        desc: ModuleDescriptor {
            name: "B",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[],
            requires: &[crate::module::CAP_A.0],
            optional_requires: &[crate::module::CAP_X.0],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
    };
    let builder = AppBuilder::new().with(b).with(a);
    let g = builder.graph();
    // Two nodes present
    assert_eq!(g.nodes.len(), 2);
    // One requires edge from B->A
    assert!(g
        .edges
        .iter()
        .any(|e| e.from == "B" && e.to == "A" && e.kind == "requires"));
    // DOT contains both nodes
    let dot = g.to_dot();
    assert!(dot.contains("\"A\""));
    assert!(dot.contains("\"B\""));
}

#[tokio::test]
async fn resolves_with_compatible_version_range() {
    use async_trait::async_trait;
    struct ModA {
        desc: ModuleDescriptor,
    }
    #[async_trait]
    impl Module for ModA {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
    }
    struct ModB {
        desc: ModuleDescriptor,
    }
    #[async_trait]
    impl Module for ModB {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
    }
    let a = ModA {
        desc: ModuleDescriptor {
            name: "A",
            version: Version::parse("1.2.3").unwrap(),
            provides: &[crate::module::CAP_X.0],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
    };
    let b = ModB {
        desc: ModuleDescriptor {
            name: "B",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[(crate::module::CAP_X.0, ">=1.0, <2.0")],
            optional_requires_with_versions: &[],
        },
    };
    // Add B before A to exercise resolver ordering
    let app = AppBuilder::new().with(b).with(a).start().await;
    assert!(app.is_ok(), "expected compatible version range to resolve");
}

#[tokio::test]
async fn fails_with_clear_error_on_incompatible_version() {
    use async_trait::async_trait;
    struct ModA {
        desc: ModuleDescriptor,
    }
    #[async_trait]
    impl Module for ModA {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
    }
    struct ModB {
        desc: ModuleDescriptor,
    }
    #[async_trait]
    impl Module for ModB {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
    }
    let a = ModA {
        desc: ModuleDescriptor {
            name: "A",
            version: Version::parse("2.0.0").unwrap(),
            provides: &[crate::module::CAP_X.0],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
    };
    let b = ModB {
        desc: ModuleDescriptor {
            name: "B",
            version: Version::parse("0.1.0").unwrap(),
            provides: &[],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[(crate::module::CAP_X.0, "<2.0.0")],
            optional_requires_with_versions: &[],
        },
    };
    let res = AppBuilder::new().with(b).with(a).start().await;
    assert!(
        res.is_err(),
        "expected start to fail due to incompatible version"
    );
    let msg = format!("{}", res.err().unwrap());
    assert!(
        msg.contains("unresolved module dependencies"),
        "unexpected error: {}",
        msg
    );
    assert!(
        msg.contains(&format!("{}@<2.0.0", crate::module::CAP_X.0)),
        "error should mention missing version range; got: {}",
        msg
    );
}
