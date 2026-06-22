#![cfg(feature = "airframe-spacetime")]

use async_trait::async_trait;
use semver::Version as SemverVersion;
use spacetime_core as st;

use crate::module::{Module as AfModule, ModuleContext, ModuleDescriptor};
use crate::spacetime::StdRuntime;

use super::shim::SpacetimeShim;

/// Unified adapter that can wrap either sync or async Spacetime modules via the SpacetimeShim.
pub struct StAsAf<M, F>
where
    M: SpacetimeShim,
    F: for<'a> Fn(&'a ModuleContext) -> M::Deps<'a> + Send + Sync,
{
    inner: Option<M>,
    desc: ModuleDescriptor,
    deps_fn: F,
    rt: StdRuntime,
}

impl<M, F> StAsAf<M, F>
where
    M: SpacetimeShim,
    F: for<'a> Fn(&'a ModuleContext) -> M::Deps<'a> + Send + Sync,
{
    /// Create a new adapter with explicit capability metadata.
    pub fn new(
        provides: &'static [&'static str],
        requires: &'static [&'static str],
        optional_requires: &'static [&'static str],
        requires_with_versions: &'static [(&'static str, &'static str)],
        optional_requires_with_versions: &'static [(&'static str, &'static str)],
        deps_fn: F,
    ) -> Self {
        let v = M::VERSION;
        let desc = ModuleDescriptor {
            name: M::NAME,
            version: SemverVersion::new(v.major as u64, v.minor as u64, v.patch as u64),
            provides,
            requires,
            optional_requires,
            requires_with_versions,
            optional_requires_with_versions,
        };
        Self {
            inner: None,
            desc,
            deps_fn,
            rt: StdRuntime::new(),
        }
    }

    /// Convenience ctor for modules without capability metadata.
    pub fn bare(deps_fn: F) -> Self {
        Self::new(&[], &[], &[], &[], &[], deps_fn)
    }
}

#[async_trait]
impl<M, F> AfModule for StAsAf<M, F>
where
    M: SpacetimeShim,
    F: for<'a> Fn(&'a ModuleContext) -> M::Deps<'a> + Send + Sync,
{
    fn descriptor(&self) -> &ModuleDescriptor {
        &self.desc
    }

    async fn init(&mut self, ctx: ModuleContext) -> anyhow::Result<()> {
        let mut sctx = st::InitCtx;
        let deps = (self.deps_fn)(&ctx);
        let m = M::init_any(&mut sctx, deps)
            .await
            .map_err(|e| anyhow::anyhow!("spacetime init error: {:?}", e))?;
        self.inner = Some(m);
        Ok(())
    }

    async fn start(&mut self) -> anyhow::Result<()> {
        if let Some(m) = &mut self.inner {
            M::start_any(m, &self.rt)
                .await
                .map_err(|e| anyhow::anyhow!("spacetime start error: {:?}", e))?;
        }
        Ok(())
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        if let Some(m) = &mut self.inner {
            M::shutdown_any(m).await;
        }
        Ok(())
    }
}
