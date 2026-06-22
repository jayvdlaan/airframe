#[cfg(feature = "module")]
#[test]
fn graph_has_optional_health_edge() {
    use airframe_core::app::AppBuilder;
    let builder = AppBuilder::new()
        .with(airframe_health::HealthModule::new())
        .with(airframe_redis::RedisModule::new());
    let g = builder.graph();
    let has_edge = g
        .edges
        .iter()
        .any(|e| e.from == "redis" && e.to == "health" && e.kind == "optional");
    assert!(
        has_edge,
        "expected optional edge from redis -> health, got: {:?}",
        g.to_dot()
    );
}
