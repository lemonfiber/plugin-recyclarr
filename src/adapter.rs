//! What this adapter says it is, and the kit that serves its contract.

use std::sync::Arc;

use lemonfiber_adapter_kit::{Kit, Readiness};
use lemonfiber_contract::adapter::{About, Release};
use lemonfiber_contract::capabilities::quality;

use crate::recyclarr::Recyclarr;

/// Where Recyclarr's configuration directory is, in the adapter's container, which the
/// adapter shares.
pub const ADDRESS: &str = "/recyclarr";

/// The upstream this adapter fronts, as its project names itself.
pub const UPSTREAM: &str = "Recyclarr";

/// The Recyclarr release this adapter's recordings were taken from.
pub const RELEASE: &str = "8.7.3";

/// The digest of the image that release's recordings were taken from.
pub const DIGEST: &str = "sha256:9c6c74d52cfc6f740a5a3624aa83867e171bb70b90453ddb7a4048b8c80445f9";

/// What this adapter says it is: the contract it speaks and the release it supports.
#[must_use]
pub fn about() -> About {
    About {
        speaks: vec![lemonfiber_contract::spoken(
            quality::sync::CAPABILITY,
            quality::sync::MAJOR,
        )],
        upstream: UPSTREAM.to_owned(),
        releases: vec![Release {
            version: RELEASE.to_owned(),
            digest: DIGEST.to_owned(),
        }],
    }
}

/// The contract, served from `recyclarr` under `key`, ready while Recyclarr's configuration
/// directory is there to write.
#[must_use]
pub fn kit(key: String, recyclarr: Arc<Recyclarr>) -> Kit {
    let asked = Arc::clone(&recyclarr);
    let ready: Readiness = Arc::new(move || {
        let asked = Arc::clone(&asked);
        Box::pin(async move { asked.answers().await })
    });
    Kit::new(key, about())
        .ready_when(ready)
        .serving(quality::sync::served(recyclarr))
}

#[cfg(test)]
mod tests;
