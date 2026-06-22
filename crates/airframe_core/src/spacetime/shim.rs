#![cfg(feature = "airframe-spacetime")]

use crate::spacetime::StdRuntime;
use spacetime_core as st;

/// Async-normalized shim over both sync and async Spacetime modules.
#[async_trait::async_trait]
pub trait SpacetimeShim: Send {
    const NAME: &'static str;
    const VERSION: st::Version;
    type Deps<'a>: Send
    where
        Self: 'a;

    async fn init_any(ctx: &mut st::InitCtx, deps: Self::Deps<'_>) -> Result<Self, st::InitError>
    where
        Self: Sized;

    async fn start_any(&mut self, rt: &StdRuntime) -> Result<(), st::StartError>;

    async fn shutdown_any(&mut self);
}
