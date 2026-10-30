use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use lemonfiber_ports::media::preset::Preset;
use lemonfiber_ports::media::Kind;
use lemonfiber_ports::service::{Applied, Failure, Profiled, Syncing as _};

use super::guidance::rewrite;
use super::{Recyclarr, CONFIGURATION, CORE_SHIPPED, LARGEST_EDIT, PRESETS, SECRETS, SHIPPED};

const KEY: &str = "0123456789abcdef0123456789abcdef";

/// A configuration directory of its own, removed when dropped.
struct Directory(PathBuf);

impl Directory {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("recyclarr-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        let _ = std::fs::create_dir_all(&path);
        Self(path)
    }

    fn holding(name: &str, configuration: &str) -> Self {
        let directory = Self::new(name);
        let _ = std::fs::write(directory.0.join(CONFIGURATION), configuration);
        directory
    }

    fn file(&self, name: &str) -> String {
        std::fs::read_to_string(self.0.join(name)).unwrap_or_default()
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn profiled(which: Kind, preset: Preset) -> Profiled {
    Profiled {
        which,
        host: "radarr".to_owned(),
        port: 7878,
        api_key: KEY.to_owned(),
        preset,
    }
}

#[tokio::test]
async fn a_preset_applied_is_written_with_its_key_owner_only_and_reads_back() {
    let directory = Directory::holding("apply", SHIPPED);
    let recyclarr = Recyclarr::at(&directory.0);

    assert!(matches!(
        recyclarr
            .apply(&profiled(Kind::Movies, Preset::Maximum))
            .await,
        Ok(Applied::OnRestart)
    ));

    let written = directory.file(CONFIGURATION);
    assert!(written.starts_with("# lemonfiber wrote this; sha256 "));
    assert!(written.contains("radarr-uhd-bluray-web.yml"));
    assert!(written.contains("api_key: !secret radarr_api_key"));
    assert!(!written.contains(KEY));
    assert_eq!(
        directory.file(SECRETS),
        format!("radarr_api_key: \"{KEY}\"\n")
    );
    let mode = std::fs::metadata(directory.0.join(SECRETS))
        .map(|held| held.permissions().mode() & 0o777)
        .unwrap_or_default();
    assert_eq!(mode, 0o600);
    assert_eq!(
        recyclarr.applied(Kind::Movies).await.ok(),
        Some(Some(Preset::Maximum))
    );
    assert_eq!(recyclarr.applied(Kind::Tv).await.ok(), Some(None));
}

#[tokio::test]
async fn each_curator_keeps_its_own_key_and_preset_and_an_operators_secret_stays() {
    let directory = Directory::new("both");
    let _ = std::fs::write(
        directory.0.join(SECRETS),
        "sonarr_api_key: \"old\"\nmine: \"kept\"\n",
    );
    let recyclarr = Recyclarr::at(&directory.0);

    assert!(recyclarr
        .apply(&profiled(Kind::Tv, Preset::HighQuality))
        .await
        .is_ok());
    assert!(recyclarr
        .apply(&profiled(Kind::Movies, Preset::SpaceSaving))
        .await
        .is_ok());

    let secrets = directory.file(SECRETS);
    assert!(secrets.contains("mine: \"kept\""));
    assert!(secrets.contains(&format!("sonarr_api_key: \"{KEY}\"")));
    assert!(secrets.contains(&format!("radarr_api_key: \"{KEY}\"")));
    assert!(!secrets.contains("old"));
    assert_eq!(
        recyclarr.applied(Kind::Tv).await.ok(),
        Some(Some(Preset::HighQuality))
    );
    assert_eq!(
        recyclarr.applied(Kind::Movies).await.ok(),
        Some(Some(Preset::SpaceSaving))
    );
}

#[tokio::test]
async fn a_configuration_the_core_wrote_is_taken_as_ours() {
    for series in Preset::ALL {
        let core = rewrite(
            &rewrite(CORE_SHIPPED, Kind::Tv, series),
            Kind::Movies,
            Preset::Maximum,
        );
        let directory = Directory::holding("core", &core);

        let applied = Recyclarr::at(&directory.0)
            .apply(&profiled(Kind::Tv, Preset::Balanced))
            .await;

        assert!(applied.is_ok(), "{series:?}: {applied:?}");
        let written = directory.file(CONFIGURATION);
        assert!(written.contains("api_key: !secret sonarr_api_key"));
        assert!(written.contains("api_key: !env_var RADARR_API_KEY"));
        assert!(written.contains("radarr-uhd-bluray-web.yml"));
    }
}

#[tokio::test]
async fn a_configuration_changed_by_hand_is_named_and_never_written_over() {
    let directory = Directory::holding("edited", SHIPPED);
    let recyclarr = Recyclarr::at(&directory.0);
    assert!(recyclarr
        .apply(&profiled(Kind::Tv, Preset::Balanced))
        .await
        .is_ok());
    let edited = directory
        .file(CONFIGURATION)
        .replace("sonarr-web-1080p", "sonarr-mine");
    let _ = std::fs::write(directory.0.join(CONFIGURATION), &edited);

    for answer in [
        recyclarr.apply(&profiled(Kind::Tv, Preset::Maximum)).await,
        recyclarr.applied(Kind::Tv).await.map(|_| Applied::Now),
    ] {
        assert!(
            matches!(&answer, Err(Failure::Refused { detail, .. }) if detail.contains(CONFIGURATION)),
            "{answer:?}"
        );
    }
    assert_eq!(directory.file(CONFIGURATION), edited);
}

#[tokio::test]
async fn a_preset_record_changed_by_hand_is_customised() {
    let directory = Directory::holding("record", SHIPPED);
    let recyclarr = Recyclarr::at(&directory.0);
    assert!(recyclarr
        .apply(&profiled(Kind::Tv, Preset::Balanced))
        .await
        .is_ok());

    let _ = std::fs::write(directory.0.join(PRESETS), r#"{"tv":"maximum"}"#);
    let moved = recyclarr.applied(Kind::Tv).await;
    let _ = std::fs::write(directory.0.join(PRESETS), "not json");
    let broken = recyclarr.applied(Kind::Tv).await;

    for answer in [moved, broken] {
        assert!(matches!(answer, Err(Failure::Refused { .. })), "{answer:?}");
    }
}

#[tokio::test]
async fn an_address_or_key_out_of_its_grammar_is_refused_before_anything_is_written() {
    let directory = Directory::holding("grammar", SHIPPED);
    let recyclarr = Recyclarr::at(&directory.0);

    for (host, port, key) in [
        ("radarr\n", 7878, KEY),
        ("", 7878, KEY),
        ("radarr", 0, KEY),
        ("a/b", 7878, KEY),
        ("radarr", 7878, "a\"b"),
        ("radarr", 7878, ""),
    ] {
        let mut asked = profiled(Kind::Movies, Preset::Balanced);
        host.clone_into(&mut asked.host);
        asked.port = port;
        key.clone_into(&mut asked.api_key);
        assert!(matches!(
            recyclarr.apply(&asked).await,
            Err(Failure::Refused { .. })
        ));
    }
    assert_eq!(directory.file(CONFIGURATION), SHIPPED);
    assert!(directory.file(SECRETS).is_empty());
}

#[tokio::test]
async fn a_link_or_a_file_that_is_not_text_is_refused_naming_it() {
    let linked = Directory::new("linked");
    let outside = Directory::holding("linked-outside", SHIPPED);
    let _ = std::os::unix::fs::symlink(outside.0.join(CONFIGURATION), linked.0.join(CONFIGURATION));
    let binary = Directory::new("binary");
    let _ = std::fs::write(binary.0.join(CONFIGURATION), [0xff, 0xfe]);

    for at in [&linked, &binary] {
        let refused = Recyclarr::at(&at.0)
            .apply(&profiled(Kind::Tv, Preset::Balanced))
            .await;
        assert!(
            matches!(&refused, Err(Failure::Refused { detail, .. }) if detail.contains(CONFIGURATION)),
            "{refused:?}"
        );
    }
    assert_eq!(outside.file(CONFIGURATION), SHIPPED);
}

#[tokio::test]
async fn a_directory_that_is_not_there_is_unavailable() {
    let recyclarr = Recyclarr::at(Path::new("/nonexistent/recyclarr-configuration"));

    assert!(!recyclarr.answers().await);
    assert!(matches!(
        recyclarr.apply(&profiled(Kind::Tv, Preset::Balanced)).await,
        Err(Failure::Unavailable { .. })
    ));
}

#[tokio::test]
async fn a_configuration_that_is_the_adapters_own_has_no_edit_and_reapply_is_apply() {
    let directory = Directory::holding("own", SHIPPED);
    let recyclarr = Recyclarr::at(&directory.0);

    assert!(matches!(
        recyclarr.edited(&profiled(Kind::Tv, Preset::Maximum)).await,
        Ok(None)
    ));
    let reapplied = recyclarr
        .reapply(&profiled(Kind::Tv, Preset::Maximum))
        .await;

    assert!(
        matches!(&reapplied, Ok(done) if done.replaced.is_none() && done.applied == Applied::OnRestart),
        "{reapplied:?}"
    );
    assert_eq!(
        recyclarr.applied(Kind::Tv).await.ok(),
        Some(Some(Preset::Maximum))
    );
}

#[tokio::test]
async fn a_hand_edit_is_answered_whole_beside_what_putting_the_profiles_back_writes() {
    let directory = Directory::holding("handed", SHIPPED);
    let recyclarr = Recyclarr::at(&directory.0);
    assert!(recyclarr
        .apply(&profiled(Kind::Movies, Preset::Maximum))
        .await
        .is_ok());
    let edited = "sonarr:\n  mine:\n    base_url: http://elsewhere:1\n";
    let _ = std::fs::write(directory.0.join(CONFIGURATION), edited);

    let answer = recyclarr
        .edited(&profiled(Kind::Tv, Preset::Balanced))
        .await;

    let Ok(Some(edit)) = answer else {
        unreachable!("a hand edit is answered: {answer:?}");
    };
    assert_eq!(edit.path, "recyclarr.yml");
    assert_eq!(edit.on_disk, edited);
    assert!(edit.preset.starts_with("# lemonfiber wrote this; sha256 "));
    assert!(edit.preset.contains("sonarr-web-1080p.yml"));
    assert!(edit.preset.contains("radarr-uhd-bluray-web.yml"));
    assert!(!edit.preset.contains(KEY));
    assert_eq!(directory.file(CONFIGURATION), edited);
}

#[tokio::test]
async fn reapply_writes_over_a_hand_edit_and_answers_what_it_replaced() {
    let directory = Directory::holding("reapply", "sonarr:\n  mine: {}\n");
    let _ = std::fs::write(directory.0.join(PRESETS), "not json");
    let recyclarr = Recyclarr::at(&directory.0);

    let answer = recyclarr
        .reapply(&profiled(Kind::Tv, Preset::Maximum))
        .await;

    let Ok(done) = answer else {
        unreachable!("reapply writes: {answer:?}");
    };
    assert_eq!(done.applied, Applied::OnRestart);
    assert_eq!(
        done.replaced.map(|edit| (edit.on_disk, edit.preset)),
        Some((
            "sonarr:\n  mine: {}\n".to_owned(),
            directory.file(CONFIGURATION)
        ))
    );
    assert_eq!(
        recyclarr.applied(Kind::Tv).await.ok(),
        Some(Some(Preset::Maximum))
    );
    assert!(directory.file(SECRETS).contains(KEY));
}

#[tokio::test]
async fn an_edit_larger_than_an_edit_carries_is_refused_and_left() {
    let large = format!("# {}\n", "a".repeat(LARGEST_EDIT));
    let directory = Directory::holding("large", &large);
    let recyclarr = Recyclarr::at(&directory.0);

    for answer in [
        recyclarr
            .edited(&profiled(Kind::Tv, Preset::Balanced))
            .await
            .map(|_| ()),
        recyclarr
            .reapply(&profiled(Kind::Tv, Preset::Balanced))
            .await
            .map(|_| ()),
    ] {
        assert!(
            matches!(&answer, Err(Failure::Refused { detail, .. }) if detail.contains("larger than")),
            "{answer:?}"
        );
    }
    assert_eq!(directory.file(CONFIGURATION), large);
}

#[tokio::test]
async fn an_edit_asked_with_a_curator_out_of_grammar_is_refused() {
    let directory = Directory::holding("edit-grammar", SHIPPED);
    let mut asked = profiled(Kind::Tv, Preset::Balanced);
    asked.port = 0;

    assert!(matches!(
        Recyclarr::at(&directory.0).edited(&asked).await,
        Err(Failure::Refused { .. })
    ));
}

#[test]
fn every_secret_the_shipped_configuration_names_is_shipped_blank() {
    let secrets = include_str!("../../config/recyclarr/secrets.yml");
    let named: Vec<&str> = SHIPPED
        .lines()
        .filter_map(|line| line.split_once("!secret ").map(|(_, name)| name.trim()))
        .collect();

    assert_eq!(named, ["sonarr_api_key", "radarr_api_key"]);
    for name in named {
        assert!(
            secrets.lines().any(|line| line == format!("{name}: \"\"")),
            "{name}"
        );
    }
}
