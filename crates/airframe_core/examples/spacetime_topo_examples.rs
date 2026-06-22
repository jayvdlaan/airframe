//! Example tests using spacetime-module's topo helpers.
//!
//! Tests are only compiled with the `airframe-spacetime` feature enabled, but this
//! file always provides a `main` so it can be built as an example when the feature
//! is disabled.

#[cfg(all(test, feature = "airframe-spacetime"))]
mod tests {
    use spacetime_core as st;
    use spacetime_module as stm;

    // Helper to build a simple node with no-ops for init/start.
    fn node(name: &'static str, deps: &'static [&'static str]) -> stm::ModuleNode {
        fn init_ok(_ctx: &mut st::InitCtx) -> Result<(), st::InitError> {
            Ok(())
        }
        fn start_ok(_rt: &dyn st::Runtime) -> Result<(), st::StartError> {
            Ok(())
        }
        stm::ModuleNode {
            descriptor: stm::ModuleDescriptor::new(name, st::Version::new(0, 1, 0)),
            init: init_ok,
            deps,
            start: Some(start_ok),
        }
    }

    #[test]
    fn validates_declared_order() {
        let nodes = [node("A", &[]), node("B", &["A"]), node("C", &["A", "B"])];
        let g = stm::ModuleGraph::new(&nodes);
        let ordered = stm::topo_order(&g).expect("declared order should be valid");
        assert_eq!(ordered.len(), 3);
    }

    #[test]
    fn detects_invalid_declared_order() {
        let nodes = [
            node("B", &["A"]), // B depends on A but appears first
            node("A", &[]),
        ];
        let g = stm::ModuleGraph::new(&nodes);
        let err = stm::topo_order(&g)
            .err()
            .expect("should fail topo validation");
        assert!(matches!(err, stm::GraphError::Cyclic));
    }
}

// Always provide a main so this example builds under all configurations.
fn main() {
    // This example only contains tests; nothing to run at runtime.
}
