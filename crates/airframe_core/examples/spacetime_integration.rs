// Run with:
//   cargo run -p airframe_core --example spacetime_integration --features airframe-spacetime

use airframe_core::app::AppBuilder;
use airframe_core::module::{Module as AfModule, ModuleContext, ModuleDescriptor};
use async_trait::async_trait;
use semver::Version as SemverVersion;

#[cfg(feature = "airframe-spacetime")]
use airframe_core::spacetime::{StAsAf, SyncShim};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Native Airframe module
    struct NativeMod {
        desc: ModuleDescriptor,
    }
    #[async_trait]
    impl AfModule for NativeMod {
        fn descriptor(&self) -> &ModuleDescriptor {
            &self.desc
        }
        async fn init(&mut self, _ctx: ModuleContext) -> anyhow::Result<()> {
            Ok(())
        }
        async fn start(&mut self) -> anyhow::Result<()> {
            Ok(())
        }
        async fn stop(&mut self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    let native = NativeMod {
        desc: ModuleDescriptor {
            name: "native",
            version: SemverVersion::new(0, 1, 0),
            provides: &[],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        },
    };

    #[cfg(feature = "airframe-spacetime")]
    {
        // A tiny Spacetime module to be wrapped
        struct SMod;
        impl spacetime_core::Module for SMod {
            const NAME: &'static str = "smod";
            const VERSION: spacetime_core::Version = spacetime_core::Version {
                major: 0,
                minor: 1,
                patch: 0,
            };
            type Deps<'a> = ();
            fn init(
                _ctx: &mut spacetime_core::InitCtx,
                _deps: Self::Deps<'_>,
            ) -> Result<Self, spacetime_core::InitError>
            where
                Self: Sized,
            {
                Ok(SMod)
            }
            fn start(
                &mut self,
                _rt: &dyn spacetime_core::Runtime,
            ) -> Result<(), spacetime_core::StartError> {
                Ok(())
            }
        }

        // Wrap the Spacetime module for Airframe (sync modules use SyncShim)
        let wrapped = StAsAf::<SyncShim<SMod>, _>::bare(|_ctx| ());

        let mut app = AppBuilder::new().with(native).with(wrapped).start().await?;

        // Shut down immediately for the example
        app.shutdown().await?;
    }

    #[cfg(not(feature = "airframe-spacetime"))]
    {
        // Without the feature, just run the native module
        let mut app = AppBuilder::new().with(native).start().await?;
        app.shutdown().await?;
    }

    Ok(())
}
