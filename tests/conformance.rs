//! Every conformance recording, read by the client the core asks the adapter with.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use lemonfiber_contract::client::Witness;
use lemonfiber_contract::conformance::{cases, Expect};
use lemonfiber_contract::{Contracted, Failure};
use lemonfiber_ports::http::{Http, Request, Response, Unreachable};
use serde_json::Value;

const ACCEPT: &str = "application/json";

struct Replayed(Response);

#[async_trait]
impl Http for Replayed {
    async fn send(&self, _request: &Request) -> Result<Response, Unreachable> {
        Ok(self.0.clone())
    }
}

#[derive(Default)]
struct Told(Mutex<Vec<String>>);

impl Witness for Told {
    fn nonconforming(&self, _operation: &str, why: &str) {
        if let Ok(mut told) = self.0.lock() {
            told.push(why.to_owned());
        }
    }
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn manifest() -> toml::Table {
    std::fs::read_to_string(root().join("plugin.toml"))
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or_default()
}

fn services() -> Vec<toml::Table> {
    manifest()
        .get("service")
        .and_then(toml::Value::as_array)
        .map(|services| {
            services
                .iter()
                .filter_map(|one| one.as_table().cloned())
                .collect()
        })
        .unwrap_or_default()
}

fn text(table: &toml::Table, field: &str) -> String {
    table
        .get(field)
        .and_then(toml::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn adapter() -> toml::Table {
    services()
        .into_iter()
        .find(|one| one.contains_key("speaks"))
        .unwrap_or_default()
}

fn pinned() -> String {
    let fronted = text(&adapter(), "fronts");
    services()
        .into_iter()
        .find(|one| text(one, "id") == fronted)
        .map(|upstream| format!("{}@{}", text(&upstream, "image"), text(&upstream, "digest")))
        .unwrap_or_default()
}

fn spoken() -> Vec<String> {
    adapter()
        .get("speaks")
        .and_then(toml::Value::as_array)
        .map(|speaks| {
            speaks
                .iter()
                .filter_map(toml::Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn read(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|why| why.to_string())?;
    serde_json::from_str(&text).map_err(|why| why.to_string())
}

fn field<'a>(recording: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(recording, |at, key| at.get(key))
}

fn said(recording: &Value, path: &[&str]) -> String {
    field(recording, path)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn status(recording: &Value) -> u16 {
    field(recording, &["response", "status"])
        .and_then(Value::as_u64)
        .and_then(|status| u16::try_from(status).ok())
        .unwrap_or_default()
}

fn replayed(recording: &Value) -> Response {
    Response {
        status: status(recording),
        headers: field(recording, &["response", "headers"])
            .and_then(Value::as_object)
            .map(|headers| {
                headers
                    .iter()
                    .map(|(name, value)| {
                        (name.clone(), value.as_str().unwrap_or_default().to_owned())
                    })
                    .collect()
            })
            .unwrap_or_default(),
        body: field(recording, &["response", "json"]).map_or_else(
            || said(recording, &["response", "body_starts_with"]),
            ToString::to_string,
        ),
    }
}

#[tokio::test]
async fn every_case_of_every_contract_spoken_is_recorded_and_read_as_the_core_reads_it() {
    let pinned = pinned();
    let spoken = spoken();
    assert_eq!(spoken, ["quality.sync@1"]);
    let mut faults = Vec::new();
    for capability in lemonfiber_contract::capabilities::all() {
        let named = lemonfiber_contract::spoken(capability.name, capability.major);
        if !spoken.contains(&named) {
            continue;
        }
        for case in cases(&capability) {
            let path = root()
                .join("conformance")
                .join(&named)
                .join(format!("{}.json", case.case));
            let recording = match read(&path) {
                Ok(recording) => recording,
                Err(why) => {
                    faults.push(format!("{named}/{}: {why}", case.case));
                    continue;
                }
            };
            if said(&recording, &["recorded_from"]) != pinned
                || said(&recording, &["note"]).is_empty()
                || said(&recording, &["request", "method"]) != "POST"
                || said(&recording, &["request", "path"]) != case.operation.path()
                || said(&recording, &["request", "accept"]) != ACCEPT
            {
                faults.push(format!(
                    "{named}/{}: not a recording of this case",
                    case.case
                ));
                continue;
            }
            match case.expect {
                Expect::Status(statuses) => {
                    if !statuses.contains(&status(&recording)) {
                        faults.push(format!(
                            "{named}/{}: answered {}",
                            case.case,
                            status(&recording)
                        ));
                    }
                }
                Expect::Conforms => {
                    let told = Arc::new(Told::default());
                    let client = Contracted::new(
                        Arc::new(Replayed(replayed(&recording))),
                        "http://recorded",
                        "recyclarr-adapter",
                        "recorded",
                    )
                    .witnessed_by(told.clone());
                    let read = case.operation.judged(&client).await;
                    let said = told.0.lock().map(|said| said.clone()).unwrap_or_default();
                    if matches!(read, Err(Failure::Unauthorised { .. })) || !said.is_empty() {
                        faults.push(format!("{named}/{}: {said:?}", case.case));
                    }
                }
            }
        }
    }
    assert!(faults.is_empty(), "{faults:#?}");
}

#[test]
fn the_manifest_names_the_image_and_version_this_crate_publishes() {
    assert_eq!(
        text(&adapter(), "image"),
        format!("ghcr.io/lemonfiber/{}", env!("CARGO_PKG_NAME"))
    );
    let version = manifest()
        .get("plugin")
        .and_then(toml::Value::as_table)
        .map(|plugin| text(plugin, "version"))
        .unwrap_or_default();
    assert_eq!(version, env!("CARGO_PKG_VERSION"));
}
