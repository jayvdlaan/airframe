use arc_swap::{ArcSwap, Guard};
use dashmap::mapref::entry::Entry;
use std::{any::TypeId, ops::Deref, sync::Arc};

/// A typed, zero-cost handle to a capability resolved from the `ServiceRegistry`.
///
/// Wraps an `Arc<T>` obtained once at initialization. Dereferences directly to `T`
/// for ergonomic access with no runtime lookup cost after the initial resolve.
#[derive(Debug)]
pub struct CapabilityHandle<T: Send + Sync + 'static> {
    inner: Arc<T>,
}

impl<T: Send + Sync + 'static> Clone for CapabilityHandle<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: Send + Sync + 'static> Deref for CapabilityHandle<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T: Send + Sync + 'static> CapabilityHandle<T> {
    /// Get a reference to the underlying `Arc<T>`.
    pub fn arc(&self) -> &Arc<T> {
        &self.inner
    }
}

/// A handle supporting hot-reload via atomic pointer swap.
///
/// Wraps `Arc<ArcSwap<T>>` so multiple readers can access the current value
/// with zero-copy loads while a writer atomically replaces it.
#[derive(Debug)]
pub struct SwappableHandle<T> {
    inner: Arc<ArcSwap<T>>,
}

impl<T> Clone for SwappableHandle<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T> SwappableHandle<T> {
    /// Zero-copy load of the current value. The returned `Guard` keeps the
    /// `Arc<T>` alive for the duration of its lifetime.
    pub fn load(&self) -> Guard<Arc<T>> {
        self.inner.load()
    }

    /// Atomically replace the stored value. Old value drops when the last
    /// reader releases its `Guard`.
    pub fn swap(&self, new: T) {
        self.inner.store(Arc::new(new));
    }

    /// Load a full `Arc<T>` clone (slightly heavier than `load()` but owns the Arc).
    pub fn load_full(&self) -> Arc<T> {
        self.inner.load_full()
    }
}

#[derive(Clone, Default)]
pub struct ServiceRegistry {
    inner: Arc<dashmap::DashMap<TypeId, Box<dyn std::any::Any + Send + Sync>>>,
}

impl ServiceRegistry {
    pub fn register<T: ?Sized + Send + Sync + 'static>(&self, svc: Arc<T>) {
        self.inner.insert(TypeId::of::<T>(), Box::new(svc));
    }
    pub fn get<T: ?Sized + Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.inner
            .get(&TypeId::of::<T>())
            .and_then(|e| e.value().downcast_ref::<Arc<T>>().cloned())
    }
    /// Atomically get an existing instance of T from the registry, or create and register it.
    ///
    /// This method is idempotent and safe to call from multiple initializers: at most one
    /// instance will be registered and the same instance will be returned to all callers.
    pub fn get_or_register<T, F>(&self, factory: F) -> Arc<T>
    where
        T: ?Sized + Send + Sync + 'static,
        F: FnOnce() -> Arc<T>,
    {
        // Fast path
        if let Some(existing) = self.get::<T>() {
            return existing;
        }

        let key = TypeId::of::<T>();
        match self.inner.entry(key) {
            Entry::Occupied(o) => {
                // Another thread registered it in the meantime
                if let Some(existing) = o.get().downcast_ref::<Arc<T>>() {
                    existing.clone()
                } else {
                    // TypeId collision with different value type should be impossible.
                    // Fallback to replace with the expected type from factory.
                    let created = factory();
                    o.replace_entry(Box::new(created.clone()));
                    created
                }
            }
            Entry::Vacant(v) => {
                let created = factory();
                v.insert(Box::new(created.clone()));
                created
            }
        }
    }

    /// Run a closure once per process keyed by the Token type. Further calls with the same Token
    /// are no-ops. Useful for idempotent side-effect registrations.
    pub fn run_once<Token, F>(&self, f: F)
    where
        Token: 'static + Send + Sync,
        F: FnOnce(),
    {
        // Use a unique zero-sized marker type per Token key.
        struct Marker<T: 'static + Send + Sync>(std::marker::PhantomData<T>);

        let key = TypeId::of::<Marker<Token>>();
        match self.inner.entry(key) {
            Entry::Occupied(_) => {
                // Already ran
            }
            Entry::Vacant(v) => {
                // Mark as done first, then execute.
                //
                // IMPORTANT: Calling user code (`f`) while holding the DashMap entry guard can
                // deadlock if `f` re-enters the ServiceRegistry (e.g., by calling `register`,
                // `get_or_register`, or another `run_once`) and hits the same underlying shard.
                v.insert(Box::new(Arc::new(Marker::<Token>(
                    std::marker::PhantomData,
                ))));
                f();
            }
        }
    }
    /// Returns the service or a descriptive error indicating which type was missing.
    pub fn get_or_err<T: ?Sized + Send + Sync + 'static>(&self) -> anyhow::Result<Arc<T>> {
        self.get::<T>().ok_or_else(|| {
            let name = std::any::type_name::<T>();
            anyhow::anyhow!("Service `{}` not found in ServiceRegistry", name)
        })
    }
    /// Returns true if a service of type T is present.
    pub fn has<T: ?Sized + Send + Sync + 'static>(&self) -> bool {
        self.get::<T>().is_some()
    }

    /// Resolve a typed capability handle. Returns a `CapabilityHandle<T>` wrapping
    /// the `Arc<T>` found in the registry, or an error describing the missing type.
    pub fn resolve<T: Send + Sync + 'static>(&self) -> anyhow::Result<CapabilityHandle<T>> {
        self.get::<T>()
            .map(|arc| CapabilityHandle { inner: arc })
            .ok_or_else(|| {
                let name = std::any::type_name::<T>();
                anyhow::anyhow!("Capability `{}` not found in ServiceRegistry", name)
            })
    }

    /// Try to resolve a typed capability handle. Returns `None` if not registered.
    pub fn try_resolve<T: Send + Sync + 'static>(&self) -> Option<CapabilityHandle<T>> {
        self.get::<T>().map(|arc| CapabilityHandle { inner: arc })
    }

    /// Register a swappable value and return a `SwappableHandle<T>` for hot-reload.
    ///
    /// The handle supports zero-copy reads via `load()` and atomic replacement via `swap()`.
    pub fn register_swappable<T: Send + Sync + 'static>(&self, value: T) -> SwappableHandle<T> {
        let swappable = Arc::new(ArcSwap::from_pointee(value));
        let handle = SwappableHandle {
            inner: swappable.clone(),
        };
        self.inner
            .insert(TypeId::of::<ArcSwap<T>>(), Box::new(swappable));
        handle
    }

    /// Retrieve a previously registered `SwappableHandle<T>`.
    pub fn get_swappable<T: Send + Sync + 'static>(&self) -> Option<SwappableHandle<T>> {
        self.inner
            .get(&TypeId::of::<ArcSwap<T>>())
            .and_then(|e| e.value().downcast_ref::<Arc<ArcSwap<T>>>().cloned())
            .map(|inner| SwappableHandle { inner })
    }

    // Convenience accessors for common in-memory buses registered by AppBuilder.
    pub fn event_bus(&self) -> Option<Arc<crate::bus::inmem::InMemoryEventBus>> {
        self.get::<crate::bus::inmem::InMemoryEventBus>()
    }
    pub fn command_bus(&self) -> Option<Arc<crate::bus::inmem::InMemoryCommandBus>> {
        self.get::<crate::bus::inmem::InMemoryCommandBus>()
    }
    pub fn query_bus(&self) -> Option<Arc<crate::bus::inmem::InMemoryQueryBus>> {
        self.get::<crate::bus::inmem::InMemoryQueryBus>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn register_and_get_roundtrip() {
        #[derive(Debug)]
        struct MyService(pub i32);

        let reg = ServiceRegistry::default();
        let svc = Arc::new(MyService(7));
        reg.register::<MyService>(svc.clone());

        let got = reg.get::<MyService>().expect("service present");
        assert!(Arc::ptr_eq(&got, &svc));
        assert_eq!(got.0, 7);

        // Negative case: wrong type should return None
        #[derive(Debug)]
        struct Other;
        assert!(reg.get::<Other>().is_none());
    }

    #[test]
    fn get_or_err_reports_missing_type() {
        #[derive(Debug)]
        struct Missing;
        let reg = ServiceRegistry::default();
        let err = reg.get_or_err::<Missing>().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Service") && msg.contains("Missing"),
            "unexpected error: {}",
            msg
        );
    }

    #[test]
    fn get_or_register_is_idempotent() {
        struct Thing(i32);
        let reg = ServiceRegistry::default();

        let first = reg.get_or_register::<Thing, _>(|| Arc::new(Thing(1)));
        let second = reg.get_or_register::<Thing, _>(|| Arc::new(Thing(2)));

        // Both calls must return the same Arc (the first one wins)
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.0, 1);
        assert_eq!(second.0, 1);
    }

    #[test]
    fn run_once_executes_only_once() {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        struct Token;
        let reg = ServiceRegistry::default();

        reg.run_once::<Token, _>(|| {
            COUNTER.fetch_add(1, Ordering::SeqCst);
        });
        reg.run_once::<Token, _>(|| {
            COUNTER.fetch_add(1, Ordering::SeqCst);
        });

        assert_eq!(COUNTER.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn resolve_returns_capability_handle() {
        struct Svc(i32);
        let reg = ServiceRegistry::default();
        reg.register::<Svc>(Arc::new(Svc(42)));

        let handle = reg.resolve::<Svc>().expect("should resolve");
        assert_eq!(handle.0, 42);
        // Deref works
        let val: &Svc = &handle;
        assert_eq!(val.0, 42);
    }

    #[test]
    fn resolve_missing_returns_error() {
        #[derive(Debug)]
        struct Missing;
        let reg = ServiceRegistry::default();
        let err = reg.resolve::<Missing>().unwrap_err();
        assert!(err.to_string().contains("Capability"));
        assert!(err.to_string().contains("Missing"));
    }

    #[test]
    fn try_resolve_returns_none_when_missing() {
        struct Missing;
        let reg = ServiceRegistry::default();
        assert!(reg.try_resolve::<Missing>().is_none());
    }

    #[test]
    fn try_resolve_returns_some_when_present() {
        struct Svc;
        let reg = ServiceRegistry::default();
        reg.register::<Svc>(Arc::new(Svc));
        assert!(reg.try_resolve::<Svc>().is_some());
    }

    #[test]
    fn capability_handle_clone_shares_arc() {
        struct Svc(#[allow(dead_code)] i32);
        let reg = ServiceRegistry::default();
        reg.register::<Svc>(Arc::new(Svc(7)));
        let h1 = reg.resolve::<Svc>().unwrap();
        let h2 = h1.clone();
        assert!(Arc::ptr_eq(h1.arc(), h2.arc()));
    }

    #[test]
    fn swappable_handle_load_and_swap() {
        let reg = ServiceRegistry::default();
        let handle = reg.register_swappable::<String>("hello".to_string());

        // Initial load
        assert_eq!(&**handle.load(), "hello");

        // Swap to new value
        handle.swap("world".to_string());
        assert_eq!(&**handle.load(), "world");
    }

    #[test]
    fn swappable_handle_clone_shares_state() {
        let reg = ServiceRegistry::default();
        let h1 = reg.register_swappable::<i32>(10);
        let h2 = h1.clone();

        h1.swap(20);
        assert_eq!(*h2.load_full(), 20);
    }

    #[test]
    fn get_swappable_retrieves_handle() {
        let reg = ServiceRegistry::default();
        let _h = reg.register_swappable::<u64>(99);

        let retrieved = reg.get_swappable::<u64>().expect("should retrieve");
        assert_eq!(*retrieved.load_full(), 99);
    }

    #[test]
    fn swappable_concurrent_access() {
        use std::thread;

        let reg = ServiceRegistry::default();
        let handle = reg.register_swappable::<AtomicUsize>(AtomicUsize::new(0));

        let readers: Vec<_> = (0..4)
            .map(|_| {
                let h = handle.clone();
                thread::spawn(move || {
                    for _ in 0..1000 {
                        let guard = h.load();
                        let _ = guard.load(Ordering::Relaxed);
                    }
                })
            })
            .collect();

        // Writer thread performing swaps
        let writer_handle = handle.clone();
        let writer = thread::spawn(move || {
            for i in 0..100 {
                writer_handle.swap(AtomicUsize::new(i));
            }
        });

        writer.join().unwrap();
        for r in readers {
            r.join().unwrap();
        }
    }
}
