#![cfg(any(feature = "adapters-axum", feature = "http"))]

use std::sync::Arc;

use axum::{http::StatusCode, Router};
use tokio::sync::RwLock;
use tower::ServiceExt; // for `oneshot`

use airframe_health::HealthStatus;
use airframe_health::{
    health_router,
    http::{PATH_LIVENESS, PATH_READINESS},
};

#[tokio::test(flavor = "current_thread")]
async fn readiness_and_liveness_routes_map_status() {
    // Initial state: Unhealthy("starting")
    let state = Arc::new(RwLock::new(HealthStatus::Unhealthy("starting".into())));

    // Build a router that serves health routes
    let app: Router = health_router(state.clone());

    // Readiness should be 503 for Unhealthy("starting")
    let resp = app
        .clone()
        .oneshot(
            axum::http::Request::get(PATH_READINESS)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    // Liveness should be 503 for transitional states like "starting"
    let resp = app
        .clone()
        .oneshot(
            axum::http::Request::get(PATH_LIVENESS)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    // Transition to Healthy and re-test
    {
        let mut g = state.write().await;
        *g = HealthStatus::Healthy;
    }

    let resp = app
        .clone()
        .oneshot(
            axum::http::Request::get(PATH_READINESS)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .clone()
        .oneshot(
            axum::http::Request::get(PATH_LIVENESS)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
