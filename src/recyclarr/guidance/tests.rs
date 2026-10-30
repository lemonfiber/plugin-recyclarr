use std::collections::BTreeSet;

use lemonfiber_ports::media::preset::Preset;
use lemonfiber_ports::media::Kind;

use super::{guidance, names, of_section, pointed, rewrite, section};

/// The `recyclarr.yml` the plugin ships: its includes are the Balanced ones.
const SHIPPED: &str = include_str!("../../../config/recyclarr/recyclarr.yml");

/// Both curators held to `preset`.
fn everywhere(config: &str, preset: Preset) -> String {
    rewrite(&rewrite(config, Kind::Tv, preset), Kind::Movies, preset)
}

#[test]
fn every_preset_names_an_include_the_plugin_ships_for_that_curator() {
    for kind in Kind::ALL {
        for preset in Preset::ALL {
            let path = guidance(kind, preset).path();
            let file = path.rsplit('/').next().unwrap_or_default();
            assert!(
                file.starts_with(section(kind)),
                "{path} is not a {kind:?} include"
            );
            let shipped = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("config/recyclarr/includes")
                .join(file);
            assert!(shipped.is_file(), "{path} is named but not shipped");
        }
    }
}

#[test]
fn the_presets_that_differ_name_different_includes() {
    for kind in Kind::ALL {
        let paths: BTreeSet<_> = Preset::ALL
            .iter()
            .map(|preset| guidance(kind, *preset).path())
            .collect();
        assert!(paths.len() > 1, "{kind:?} resolves every preset alike");
    }
}

#[test]
fn each_section_names_the_kind_it_holds_and_no_other_key_does() {
    assert_eq!(of_section("sonarr"), Some(Kind::Tv));
    assert_eq!(of_section("radarr"), Some(Kind::Movies));
    assert_eq!(of_section("prowlarr"), None);
}

#[test]
fn the_three_1080p_presets_collapse_for_television_and_film_keeps_all_four() {
    assert_eq!(
        guidance(Kind::Tv, Preset::SpaceSaving),
        guidance(Kind::Tv, Preset::HighQuality)
    );
    assert_ne!(
        guidance(Kind::Tv, Preset::HighQuality),
        guidance(Kind::Tv, Preset::Maximum)
    );
    for (first, second) in [
        (Preset::SpaceSaving, Preset::Balanced),
        (Preset::Balanced, Preset::HighQuality),
        (Preset::HighQuality, Preset::Maximum),
    ] {
        assert_ne!(
            guidance(Kind::Movies, first),
            guidance(Kind::Movies, second)
        );
    }
}

#[test]
fn balanced_everywhere_reproduces_the_shipped_file() {
    assert_eq!(everywhere(SHIPPED, Preset::Balanced), SHIPPED);
}

#[test]
fn a_preset_reaches_only_the_section_of_its_curator() {
    let rewritten = rewrite(SHIPPED, Kind::Movies, Preset::Maximum);

    assert!(rewritten.contains("radarr-uhd-bluray-web.yml"));
    assert!(rewritten.contains("sonarr-web-1080p.yml"));
    assert!(!rewritten.contains("/radarr-hd-bluray-web.yml"));
}

#[test]
fn every_preset_leaves_each_curator_with_one_include() {
    for preset in Preset::ALL {
        let config = everywhere(SHIPPED, preset);
        for kind in Kind::ALL {
            let included = config.lines().filter(|line| {
                line.trim().starts_with("- config:") && line.contains(section(kind))
            });
            assert_eq!(
                included.count(),
                1,
                "{preset:?} left {kind:?} misconfigured"
            );
            assert!(names(&config, kind, guidance(kind, preset)));
        }
    }
}

#[test]
fn rewriting_is_idempotent_and_keeps_comments() {
    let once = everywhere(SHIPPED, Preset::HighQuality);

    assert_eq!(everywhere(&once, Preset::HighQuality), once);
    assert!(once.contains("# Recyclarr"));
}

#[test]
fn an_operators_own_section_and_entries_are_left_untouched() {
    let config = "\
lidarr:
  main:
    include:
      - template: something-of-my-own
sonarr:
  main:
    include:
      - template: theirs
      - config: /config/includes/sonarr-web-1080p.yml
";
    let rewritten = rewrite(config, Kind::Tv, Preset::Maximum);

    assert!(rewritten
        .contains("lidarr:\n  main:\n    include:\n      - template: something-of-my-own\n"));
    assert!(rewritten.contains("- template: theirs"));
    assert!(rewritten.contains("sonarr-web-2160p.yml"));
    assert!(!rewritten.contains("sonarr-web-1080p.yml"));
}

#[test]
fn an_empty_include_is_filled_at_the_derived_indent() {
    let config = "\
sonarr:
  main:
    include:
radarr:
";
    let rewritten = rewrite(config, Kind::Tv, Preset::Balanced);

    assert!(rewritten.contains("      - config: /config/includes/sonarr-web-1080p.yml"));
}

#[test]
fn no_stale_include_survives_a_reshaped_include_block() {
    let config = "\
radarr:
  main:
    include:

      - config: /config/includes/radarr-hd-bluray-web.yml
      # the one I keep meaning to revisit
      - config: /config/includes/radarr-sqp-1-web-1080p.yml
";
    let rewritten = rewrite(config, Kind::Movies, Preset::Maximum);

    assert!(rewritten.contains("radarr-uhd-bluray-web.yml"));
    assert!(!rewritten.contains("/radarr-hd-bluray-web.yml"));
    assert!(!rewritten.contains("radarr-sqp-1-web-1080p.yml"));
    assert!(rewritten.contains("\n\n"));
    assert!(rewritten.contains("# the one I keep meaning to revisit"));
}

#[test]
fn a_section_key_carrying_an_inline_comment_is_still_recognised() {
    let config = "\
sonarr: # primary television instance
  main:
    include: # profiles
      - config: /config/includes/sonarr-web-1080p.yml
";
    let rewritten = rewrite(config, Kind::Tv, Preset::Maximum);

    assert!(rewritten.contains("sonarr-web-2160p.yml"));
    assert!(rewritten.contains("sonarr: # primary television instance"));
}

#[test]
fn a_curator_is_pointed_at_its_address_and_its_secret_and_nothing_else_moves() {
    let pointed = pointed(SHIPPED, Kind::Movies, "http://films:7000", "radarr_api_key");

    assert!(
        pointed.contains("    base_url: http://films:7000\n    api_key: !secret radarr_api_key\n")
    );
    let moved: Vec<(&str, &str)> = SHIPPED
        .lines()
        .zip(pointed.lines())
        .filter(|(shipped, now)| shipped != now)
        .collect();
    assert_eq!(moved.len(), 2, "{moved:?}");
    assert_eq!(pointed.lines().count(), SHIPPED.lines().count());
}

#[test]
fn an_include_named_only_in_another_section_is_not_named_in_this_one() {
    let maximum = guidance(Kind::Tv, Preset::Maximum);

    assert!(!names(SHIPPED, Kind::Tv, maximum));
    assert!(names(
        SHIPPED,
        Kind::Movies,
        guidance(Kind::Movies, Preset::Balanced)
    ));
    assert!(!names(
        SHIPPED,
        Kind::Tv,
        guidance(Kind::Movies, Preset::Balanced)
    ));
}
