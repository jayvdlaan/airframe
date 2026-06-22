//! `ServiceRegistry` convenience accessors for the scheduler service.

use std::sync::Arc;

use airframe_core::registry::ServiceRegistry;

use crate::scheduler::InMemoryScheduler;

// Convenience accessors on ServiceRegistry for Scheduler service.
pub trait ServiceRegistrySchedulerExt {
    fn scheduler(&self) -> Option<Arc<InMemoryScheduler>>;
}
impl ServiceRegistrySchedulerExt for ServiceRegistry {
    fn scheduler(&self) -> Option<Arc<InMemoryScheduler>> {
        self.get::<InMemoryScheduler>()
    }
}
