//! A quality preset in Recyclarr's words, and the lines of `recyclarr.yml` that carry
//! it for one curator.
//!
//! A preset resolves to one include file the plugin ships, named by a section's
//! `include:` list as a `- config:` entry. Holding a curator to a preset rewrites that
//! entry, and the section's `base_url` and `api_key`, and nothing else: every comment,
//! every other section and every `- template:` entry an operator added stays exactly
//! where it was.

use lemonfiber_ports::media::preset::Preset;
use lemonfiber_ports::media::Kind;

/// The file a preset's guidance is shipped in, as `recyclarr.yml` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guidance(&'static str);

impl Guidance {
    /// Where the file sits, as Recyclarr reads it from inside its container.
    #[must_use]
    pub const fn path(self) -> &'static str {
        self.0
    }
}

/// What a preset asks the guides for, for the curator of `kind`.
///
/// The guides offer series only a WEB-1080p and a WEB-2160p profile, so the three
/// 1080p presets resolve to the same television guidance. Film has the full range.
#[must_use]
pub const fn guidance(kind: Kind, preset: Preset) -> Guidance {
    match (kind, preset) {
        (Kind::Tv, Preset::SpaceSaving | Preset::Balanced | Preset::HighQuality) => {
            Guidance("/config/includes/sonarr-web-1080p.yml")
        }
        (Kind::Tv, Preset::Maximum) => Guidance("/config/includes/sonarr-web-2160p.yml"),
        (Kind::Movies, Preset::SpaceSaving) => {
            Guidance("/config/includes/radarr-sqp-1-web-1080p.yml")
        }
        (Kind::Movies, Preset::Balanced) => Guidance("/config/includes/radarr-hd-bluray-web.yml"),
        (Kind::Movies, Preset::HighQuality) => {
            Guidance("/config/includes/radarr-remux-web-1080p.yml")
        }
        (Kind::Movies, Preset::Maximum) => Guidance("/config/includes/radarr-uhd-bluray-web.yml"),
    }
}

/// The top-level section the curator of `kind` is configured under.
#[must_use]
pub const fn section(kind: Kind) -> &'static str {
    match kind {
        Kind::Tv => "sonarr",
        Kind::Movies => "radarr",
    }
}

/// The kind whose section a top-level key names, or none for any other key.
fn of_section(key: &str) -> Option<Kind> {
    Kind::ALL.into_iter().find(|kind| section(*kind) == key)
}

/// The width of a line's leading whitespace.
fn indent_width(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// `line` with any trailing inline comment removed.
fn without_comment(line: &str) -> &str {
    line.find(" #")
        .map_or(line, |at| line.get(..at).unwrap_or(line))
}

/// The section a top-level key names: the bare key, its colon and any comment gone.
fn section_key(line: &str) -> &str {
    without_comment(line).trim_end().trim_end_matches(':')
}

/// The leading whitespace of `line` where it is the `include:` key.
fn include_indent(line: &str) -> Option<&str> {
    let body = line.trim_start();
    (without_comment(body).trim_end() == "include:")
        .then(|| line.get(..line.len() - body.len()).unwrap_or_default())
}

/// The leading whitespace of `line` where it is a `- config:` entry.
fn entry_indent(line: &str) -> Option<&str> {
    let body = line.trim_start();
    body.starts_with("- config:")
        .then(|| line.get(..line.len() - body.len()).unwrap_or_default())
}

/// Whether `line` opens a top-level section: a key in the first column, not a comment.
fn top_level_key(line: &str) -> bool {
    line.chars()
        .next()
        .is_some_and(|first| !first.is_whitespace() && first != '#')
}

/// Every line of `config`, each marked with the kind whose section it lies in.
fn sectioned(config: &str) -> Vec<(Option<Kind>, &str)> {
    let mut section = None;
    config
        .lines()
        .map(|line| {
            if top_level_key(line) {
                section = of_section(section_key(line));
            }
            (section, line)
        })
        .collect()
}

/// `config` with the `- config:` entries of every `include:` in the section of `kind`
/// replaced by the one `preset` asks for, and every other line kept. Line endings
/// become LF and the result ends in one newline.
#[must_use]
pub fn rewrite(config: &str, kind: Kind, preset: Preset) -> String {
    let lines = sectioned(config);
    let asked = guidance(kind, preset).path();
    let mut out = String::with_capacity(config.len());
    let mut at = 0;
    while let Some((section, line)) = lines.get(at).copied() {
        at += 1;
        push(&mut out, line);
        let Some(indent) = include_indent(line).filter(|_| section == Some(kind)) else {
            continue;
        };
        let mut block = Vec::new();
        while let Some((_, next)) = lines
            .get(at)
            .copied()
            .filter(|(_, next)| next.trim().is_empty() || indent_width(next) > indent.len())
        {
            block.push(next);
            at += 1;
        }
        let entry = block
            .iter()
            .find_map(|line| entry_indent(line))
            .map_or_else(|| format!("{indent}  "), str::to_owned);
        let mut written = false;
        for kept in &block {
            if entry_indent(kept).is_none() {
                push(&mut out, kept);
            } else if !written {
                push(&mut out, &format!("{entry}- config: {asked}"));
                written = true;
            }
        }
        if !written {
            push(&mut out, &format!("{entry}- config: {asked}"));
        }
    }
    out
}

/// `config` with the `base_url` and `api_key` of the section of `kind` pointed at
/// `base_url` and the secret named `secret`, and every other line kept.
#[must_use]
pub fn pointed(config: &str, kind: Kind, base_url: &str, secret: &str) -> String {
    let mut out = String::with_capacity(config.len());
    for (section, line) in sectioned(config) {
        let body = line.trim_start();
        let indent = line.get(..line.len() - body.len()).unwrap_or_default();
        match (section == Some(kind), without_comment(body).split_once(':')) {
            (true, Some(("base_url", _))) => {
                push(&mut out, &format!("{indent}base_url: {base_url}"));
            }
            (true, Some(("api_key", _))) => {
                push(&mut out, &format!("{indent}api_key: !secret {secret}"));
            }
            _ => push(&mut out, line),
        }
    }
    out
}

/// Whether the section of `kind` in `config` names `guidance` among its includes.
#[must_use]
pub fn names(config: &str, kind: Kind, guidance: Guidance) -> bool {
    sectioned(config).into_iter().any(|(section, line)| {
        section == Some(kind)
            && entry_indent(line).is_some()
            && without_comment(line.trim_start())
                .trim_start_matches("- config:")
                .trim()
                == guidance.path()
    })
}

/// Append `line` and its newline.
fn push(out: &mut String, line: &str) {
    out.push_str(line);
    out.push('\n');
}

#[cfg(test)]
mod tests;
