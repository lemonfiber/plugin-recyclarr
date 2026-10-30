use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use lemonfiber_contract::adapter::{about_path, ready_path, About};
use serde_json::json;
use tower::ServiceExt as _;

use super::{about, kit, DIGEST, RELEASE, UPSTREAM};
use crate::recyclarr::Recyclarr;

fn a_key() -> String {
    ('e'..='t').collect()
}

/// A configuration directory of its own, removed when dropped.
struct Directory(PathBuf);

impl Directory {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("recyclarr-adapter-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn answered(
    recyclarr: Arc<Recyclarr>,
    method: &str,
    path: &str,
    key: Option<&str>,
    body: &str,
) -> (StatusCode, Vec<u8>) {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(key) = key {
        request = request.header(header::AUTHORIZATION, format!("Bearer {key}"));
    }
    let Ok(request) = request.body(Body::from(body.to_owned())) else {
        return (StatusCode::IM_A_TEAPOT, Vec::new());
    };
    let Ok(response) = kit(a_key(), recyclarr).router().oneshot(request).await;
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .map(|bytes| bytes.to_vec())
        .unwrap_or_default();
    (status, body)
}

#[test]
fn it_says_it_speaks_quality_sync_for_the_recyclarr_its_recordings_were_taken_from() {
    let about = about();

    assert_eq!(about.speaks, vec!["quality.sync@1".to_owned()]);
    assert_eq!(about.upstream, UPSTREAM);
    assert_eq!(
        about.releases.first().map(|one| one.version.as_str()),
        Some(RELEASE)
    );
    assert_eq!(
        about.releases.first().map(|one| one.digest.as_str()),
        Some(DIGEST)
    );
}

#[tokio::test]
async fn it_names_itself_under_the_key_and_to_nobody_else() {
    let directory = Directory::new("about");
    let recyclarr = Arc::new(Recyclarr::at(&directory.0));

    let (status, body) = answered(
        Arc::clone(&recyclarr),
        "GET",
        &about_path(),
        Some(&a_key()),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_slice::<About>(&body).ok(), Some(about()));

    let (status, _) = answered(recyclarr, "GET", &about_path(), Some("another"), "").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn it_is_ready_while_recyclarrs_configuration_directory_is_there() {
    let directory = Directory::new("ready");
    let recyclarr = Arc::new(Recyclarr::at(&directory.0));
    let (_, before) = answered(
        Arc::clone(&recyclarr),
        "GET",
        &ready_path(),
        Some(&a_key()),
        "",
    )
    .await;

    assert!(std::fs::create_dir_all(&directory.0).is_ok());
    let (_, after) = answered(recyclarr, "GET", &ready_path(), Some(&a_key()), "").await;

    assert_eq!(before, b"false");
    assert_eq!(after, b"true");
}

#[tokio::test]
async fn a_preset_applied_over_the_contract_is_written_and_read_back() {
    let directory = Directory::new("apply");
    assert!(std::fs::create_dir_all(&directory.0).is_ok());
    let recyclarr = Arc::new(Recyclarr::at(&directory.0));

    let (status, _) = answered(
        Arc::clone(&recyclarr),
        "POST",
        "/lemonfiber/quality.sync/v1/apply",
        Some(&a_key()),
        &json!({ "profiled": {
            "which": "movies", "host": "radarr", "port": 7878,
            "api_key": a_key(), "preset": "maximum"
        }})
        .to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::RESET_CONTENT);

    let (status, body) = answered(
        recyclarr,
        "POST",
        "/lemonfiber/quality.sync/v1/applied",
        Some(&a_key()),
        &json!({ "which": "movies" }).to_string(),
    )
    .await;
    assert!(status.is_success(), "{status}");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).ok(),
        Some(json!("maximum"))
    );
}
