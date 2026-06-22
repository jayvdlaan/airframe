#![cfg(all(feature = "airframe-spacetime", feature = "airframe-interop"))]

use super::shim::SpacetimeShim;
use spacetime_async_core as st_async;
use spacetime_core as st;

#[async_trait::async_trait]
impl<T> SpacetimeShim for T
where
    T: st_async::easy::AsyncModule + Send,
    for<'a> <T as st_async::easy::AsyncModule>::Deps<'a>: Send,
{
    const NAME: &'static str = <T as st_async::easy::AsyncModule>::NAME;
    const VERSION: st::Version = <T as st_async::easy::AsyncModule>::VERSION;
    type Deps<'a>
        = <T as st_async::easy::AsyncModule>::Deps<'a>
    where
        T: 'a;

    async fn init_any(ctx: &mut st::InitCtx, deps: Self::Deps<'_>) -> Result<Self, st::InitError> {
        <T as st_async::easy::AsyncModule>::init_async(ctx, deps).await
    }

    async fn start_any(&mut self, rt: &crate::spacetime::StdRuntime) -> Result<(), st::StartError> {
        <T as st_async::easy::AsyncModule>::start_async(self, rt).await
    }

    async fn shutdown_any(&mut self) {
        <T as st_async::easy::AsyncModule>::shutdown_async(self).await
    }
}
