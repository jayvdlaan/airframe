use airframe_api::http::{Method, Uri};
use airframe_api::{is_valid_path_template, ApiError, ApiSpec, CodeSpec};
use bytes::Bytes;

#[test]
fn api_error_serde_roundtrip_and_defaulting() {
    let err = ApiError {
        code: 404,
        message: Some("not found".into()),
    };
    let json = serde_json::to_string(&err).unwrap();
    let back: ApiError = serde_json::from_str(&json).unwrap();
    assert_eq!(back, err);
    assert!(back.is_client_error());
    assert!(!back.is_server_error());

    // Defaulting for message: missing field -> None
    let raw = "{\"code\":500}";
    let e2: ApiError = serde_json::from_str(raw).unwrap();
    assert_eq!(e2.code, 500);
    assert!(e2.message.is_none());
    assert!(e2.is_server_error());

    // Default impl yields code=0, message=None
    let d = ApiError::default();
    assert_eq!(d.code, 0);
    assert!(d.message.is_none());
}

#[test]
fn path_template_validation() {
    assert!(is_valid_path_template("/v1/items/{id}"));
    assert!(is_valid_path_template("/v1/{a}/x/{b}"));
    assert!(!is_valid_path_template("/v1/items/{id")); // missing close
    assert!(!is_valid_path_template("/v1/items/id}")); // extra close
    assert!(!is_valid_path_template("/v1/{a/{b}}")); // mis-nested
}

#[test]
fn build_request_happy_path_and_encoding() {
    let base: Uri = "https://api.example.com/".parse().unwrap();
    let spec = CodeSpec::new(base)
        .route("getItem", Method::GET, "/v1/items/{id}")
        .route("search", Method::GET, "/v1/search");

    // id contains spaces to exercise urlencoding
    let params = serde_json::json!({
        "id": "a b",
        "query": {"q": "rust lang", "page": 2, "exact": true}
    });
    let req = spec.build_request("getItem", &params, None).unwrap();
    assert_eq!(req.method(), &Method::GET);
    let uri = req.uri().to_string();
    assert!(uri.starts_with("https://api.example.com/v1/items/a%20b"));
    assert!(uri.contains("q=rust%20lang"));
    assert!(uri.contains("page=2"));
    assert!(uri.contains("exact=true"));

    // Body defaults to empty when None
    assert_eq!(req.body().len(), 0);

    // Request with body passes through as-is
    let body = Bytes::from_static(b"hello");
    let req2 = spec
        .build_request(
            "search",
            &serde_json::json!({"query": {"q": "x"}}),
            Some(body.clone()),
        )
        .unwrap();
    assert_eq!(req2.body(), &body);
}

#[test]
fn build_request_missing_param_errors() {
    let base: Uri = "http://localhost".parse().unwrap();
    let spec = CodeSpec::new(base).route("op", Method::GET, "/a/{b}/c");
    let params = serde_json::json!({}); // missing b
    let err = spec.build_request("op", &params, None).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("missing path params"));
}
