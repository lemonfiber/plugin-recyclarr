//! The manifest, as the core at the pinned commit reads it.

use toml::{Table, Value};

fn manifest() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/plugin.toml")).unwrap_or_default()
}

#[test]
fn the_pinned_core_refuses_nothing_in_the_manifest() {
    let read: Result<lemonfiber_plugin::Manifest, _> = toml::from_str(&manifest());
    let refused: Vec<String> = match read {
        Ok(manifest) => lemonfiber_plugin::refusals(&manifest, &[])
            .into_iter()
            .map(|violation| format!("{}: {}", violation.location, violation.message))
            .collect(),
        Err(why) => vec![why.to_string()],
    };
    assert!(refused.is_empty(), "{refused:#?}");
}

/// The digest of the image each service the manifest's adapters front runs.
fn fronted_digests(manifest: &Table) -> Vec<String> {
    let services: Vec<&Table> = manifest
        .get("service")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_table)
        .collect();
    services
        .iter()
        .filter_map(|service| service.get("fronts").and_then(Value::as_str))
        .filter_map(|fronted| {
            services
                .iter()
                .find(|service| service.get("id").and_then(Value::as_str) == Some(fronted))
        })
        .filter_map(|service| service.get("digest").and_then(Value::as_str))
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_adapter_names_the_image_of_the_recyclarr_it_fronts() {
    let named: Vec<String> = recyclarr_adapter::adapter::about()
        .releases
        .into_iter()
        .map(|release| release.digest)
        .collect();
    let fronted = fronted_digests(&manifest().parse().unwrap_or_default());
    assert_eq!(fronted, vec![recyclarr_adapter::adapter::DIGEST.to_owned()]);
    assert_eq!(named, fronted);
}
