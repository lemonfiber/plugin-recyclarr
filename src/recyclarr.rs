//! Recyclarr, told the preset each curator's quality profiles are held to by the
//! configuration it reads when it starts.
//!
//! The adapter rewrites `recyclarr.yml` in Recyclarr's configuration directory, which it
//! shares, through the kit's shared-directory helper: the section of the curator asked
//! about is pointed at it and its include named for the preset, and nothing else
//! changes. The curator's key goes into `secrets.yml` beside it, owner-only, named by
//! `!secret`. The first line of a `recyclarr.yml` the adapter writes carries the digest
//! of the rest, so one whose digest no longer holds, that is not the one the plugin
//! ships and not one the core wrote before this adapter did, is someone's own: it is
//! named in the refusal and never written over. The presets applied are kept beside it.
//! Recyclarr reads its configuration when it starts, so a preset applied is in force on a
//! restart, which the core then makes.

pub mod guidance;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use lemonfiber_adapter_kit::shared::{Shared, SharedName, Unshared};
use lemonfiber_ports::media::preset::Preset;
use lemonfiber_ports::media::Kind;
use lemonfiber_ports::service::{Applied, Failure, Profiled, Syncing};
use sha2::{Digest as _, Sha256};

use self::guidance::{guidance, names, pointed, rewrite, section};

/// What a failure names Recyclarr as.
const SERVICE: &str = "Recyclarr";

/// The file Recyclarr reads its configuration from.
pub const CONFIGURATION: &str = "recyclarr.yml";

/// The file Recyclarr reads every `!secret` from.
pub const SECRETS: &str = "secrets.yml";

/// The presets the adapter applied, by the media type of each curator.
pub const PRESETS: &str = "lemonfiber-presets.json";

/// The configuration the plugin ships.
const SHIPPED: &str = include_str!("../config/recyclarr/recyclarr.yml");

/// The configuration the core wrote, before this adapter did, from the file the
/// plugin shipped then.
const CORE_SHIPPED: &str = include_str!("recyclarr/lemonfiber-wrote.yml");

/// What the first line of a configuration the adapter wrote starts with, before the
/// digest of the rest.
const MARK: &str = "# lemonfiber wrote this; sha256 ";

/// The mode of `secrets.yml`, which carries every curator's key: Recyclarr's alone.
const SECRET: u32 = 0o600;

/// The mode of the files that carry nothing secret.
const READABLE: u32 = 0o644;

/// The longest key a curator is reached with.
const LONGEST_KEY: usize = 128;

/// The longest host a curator is reached on.
const LONGEST_HOST: usize = 253;

/// One Recyclarr, told what to hold each curator to through its configuration
/// directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recyclarr {
    directory: PathBuf,
}

impl Recyclarr {
    /// The Recyclarr whose configuration directory is shared at `directory`.
    #[must_use]
    pub fn at(directory: &Path) -> Self {
        Self {
            directory: directory.to_path_buf(),
        }
    }

    /// Whether Recyclarr's configuration directory is there to write.
    pub async fn answers(&self) -> bool {
        tokio::fs::metadata(&self.directory)
            .await
            .is_ok_and(|held| held.is_dir())
    }

    /// What `file` holds, or none where it is not there.
    async fn read(&self, file: &str) -> Result<Option<String>, Failure> {
        Shared::at(&self.directory)
            .read(&SharedName::new(file).map_err(unshared)?)
            .await
            .map_err(unshared)?
            .map(|bytes| {
                String::from_utf8(bytes).map_err(|_| refused(&format!("{file} is not text")))
            })
            .transpose()
    }

    /// Write `contents` to `file` with `mode`, in one step.
    async fn put(&self, file: &str, contents: String, mode: u32) -> Result<(), Failure> {
        Shared::at(&self.directory)
            .write(
                &SharedName::new(file).map_err(unshared)?,
                contents.into_bytes(),
                mode,
            )
            .await
            .map_err(unshared)
    }

    /// The configuration as the adapter may rewrite it: the one it last wrote, the one
    /// the plugin ships where there is none, or one the core wrote; refused naming the
    /// file where it is someone's own.
    async fn configuration(&self) -> Result<String, Failure> {
        let Some(held) = self.read(CONFIGURATION).await? else {
            return Ok(SHIPPED.to_owned());
        };
        if let Some(rest) = ours(&held) {
            return Ok(rest.to_owned());
        }
        if held == SHIPPED || core_wrote(&held) {
            return Ok(held);
        }
        Err(customised())
    }

    /// The presets last applied, by media type; refused as customised where the file is
    /// not one the adapter wrote.
    async fn presets(&self) -> Result<BTreeMap<String, Preset>, Failure> {
        self.read(PRESETS)
            .await?
            .map(|text| serde_json::from_str(&text).map_err(|_| customised()))
            .transpose()
            .map(Option::unwrap_or_default)
    }

    /// `secrets.yml` with the secret `name` holding `key`, every other line kept.
    async fn secrets(&self, name: &str, key: &str) -> Result<String, Failure> {
        let held = self.read(SECRETS).await?.unwrap_or_default();
        let mut out: Vec<&str> = held
            .lines()
            .filter(|line| {
                line.split_once(':')
                    .is_none_or(|(named, _)| named.trim() != name)
            })
            .collect();
        let ours = format!("{name}: \"{key}\"");
        out.push(&ours);
        Ok(out.join("\n") + "\n")
    }
}

#[async_trait]
impl Syncing for Recyclarr {
    async fn apply(&self, profiled: &Profiled) -> Result<Applied, Failure> {
        let base_url = address(&profiled.host, profiled.port)?;
        let key = key(&profiled.api_key)?;
        let named = format!("{}_api_key", section(profiled.which));
        let configuration = self.configuration().await?;
        let mut presets = self.presets().await?;
        let written = pointed(
            &rewrite(&configuration, profiled.which, profiled.preset),
            profiled.which,
            &base_url,
            &named,
        );
        presets.insert(profiled.which.media_type().to_owned(), profiled.preset);
        let secrets = self.secrets(&named, key).await?;
        self.put(SECRETS, secrets, SECRET).await?;
        self.put(
            PRESETS,
            serde_json::to_string(&presets).unwrap_or_default(),
            READABLE,
        )
        .await?;
        self.put(CONFIGURATION, marked(&written), READABLE).await?;
        Ok(Applied::OnRestart)
    }

    async fn applied(&self, which: Kind) -> Result<Option<Preset>, Failure> {
        let configuration = self.configuration().await?;
        let Some(preset) = self.presets().await?.get(which.media_type()).copied() else {
            return Ok(None);
        };
        if names(&configuration, which, guidance(which, preset)) {
            Ok(Some(preset))
        } else {
            Err(customised())
        }
    }
}

/// The address a curator on `host` and `port` is reached at, where the host is a plain
/// name or address.
fn address(host: &str, port: u16) -> Result<String, Failure> {
    let plain = !host.is_empty()
        && host.len() <= LONGEST_HOST
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.'))
        && port != 0;
    if plain {
        Ok(format!("http://{host}:{port}"))
    } else {
        Err(refused(&format!(
            "{host:?} on port {port} is not a curator's address"
        )))
    }
}

/// `value`, where it is a key of letters and digits.
fn key(value: &str) -> Result<&str, Failure> {
    if !value.is_empty()
        && value.len() <= LONGEST_KEY
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        Ok(value)
    } else {
        Err(refused("a curator's key is not letters and digits"))
    }
}

/// `configuration`, its first line carrying the digest of the rest.
fn marked(configuration: &str) -> String {
    format!(
        "{MARK}{}\n{configuration}",
        hex::encode(Sha256::digest(configuration.as_bytes()))
    )
}

/// The configuration under the first line of `held`, where `held` is one the adapter
/// wrote and nobody has changed since.
fn ours(held: &str) -> Option<&str> {
    let (first, rest) = held.split_once('\n')?;
    let written = first.strip_prefix(MARK)?;
    (written == hex::encode(Sha256::digest(rest.as_bytes()))).then_some(rest)
}

/// Whether `held` is a configuration the core wrote: the file it shipped, its includes
/// rewritten for some preset of television and some of film.
fn core_wrote(held: &str) -> bool {
    Preset::ALL.into_iter().any(|series| {
        Preset::ALL.into_iter().any(|film| {
            rewrite(&rewrite(CORE_SHIPPED, Kind::Tv, series), Kind::Movies, film) == held
        })
    })
}

/// The refusal of a configuration someone changed by hand.
fn customised() -> Failure {
    refused(&format!(
        "{CONFIGURATION} was changed by hand, so it is left as it is"
    ))
}

/// A refusal saying `detail`.
fn refused(detail: &str) -> Failure {
    Failure::Refused {
        service: SERVICE.to_owned(),
        detail: detail.to_owned(),
    }
}

/// A file in Recyclarr's configuration directory that could not be reached: a link or
/// a file that is not plain is refused by name, and a directory that is not there to
/// read or write leaves Recyclarr unavailable.
fn unshared(why: Unshared) -> Failure {
    match why {
        Unshared::Failed(..) => Failure::Unavailable {
            service: SERVICE.to_owned(),
        },
        refusal => refused(&refusal.to_string()),
    }
}

#[cfg(test)]
mod tests;
