#![cfg(feature = "airframe-spacetime")]

use super::shim::SpacetimeShim;
use spacetime_core as st;

/// Wrapper for sync Spacetime modules to implement the shim without blanket impl overlap.
pub struct SyncShim<T>(pub T);

#[async_trait::async_trait]
impl<T> SpacetimeShim for SyncShim<T>
where
    T: st::Module + Send,
    for<'a> <T as st::Module>::Deps<'a>: Send,
{
    const NAME: &'static str = <T as st::Module>::NAME;
    const VERSION: st::Version = <T as st::Module>::VERSION;
    type Deps<'a>
        = <T as st::Module>::Deps<'a>
    where
        T: 'a;

    async fn init_any(ctx: &mut st::InitCtx, deps: Self::Deps<'_>) -> Result<Self, st::InitError> {
        let inner = <T as st::Module>::init(ctx, deps)?;
        Ok(SyncShim(inner))
    }

    async fn start_any(&mut self, rt: &crate::spacetime::StdRuntime) -> Result<(), st::StartError> {
        <T as st::Module>::start(&mut self.0, rt)
    }

    async fn shutdown_any(&mut self) {
        <T as st::Module>::shutdown(&mut self.0)
    }
}
