# AGENTS.md — plugin-recyclarr

> **Start at the roadmap and board on [lemonfiber.app](https://lemonfiber.app),
> rendered from the report of where every unreleased version stands. Then the
> rules** every repository shares:
> [working in the repositories](https://github.com/lemonfiber/spec/blob/main/50-governance/working-in-the-repositories.md)
> and [the rules for agents](https://github.com/lemonfiber/spec/blob/main/50-governance/ai-contributors.md).
> This file holds only what is true of this repository.

## What this repo is

Recyclarr as a first-party lemonfiber plugin: `plugin.toml` and the adapter that
speaks the `quality.sync@1` contract in front of Recyclarr, by rewriting the
`recyclarr.yml` it reads in its configuration directory. The adapter is the
Rust crate at the root, built on `lemonfiber-adapter-kit` from the core at the
commit `Cargo.toml` pins, into the image the `Dockerfile` builds. `release.yml`
publishes that image from every commit on `main` and from the train's tag.
`plugin.toml` runs Recyclarr and that adapter beside it. The
[bundle contract](https://github.com/lemonfiber/spec/blob/main/20-architecture/contracts/bundle.md)
says how the catalogue pins this repository and how lemonfiber carries it.

## The rules you cannot break

- **The plugin id is the service's id** (`ARCH-R266`). The container, the
  configuration directory, the settings and the journal keep their names.
- **Two services: `recyclarr` and `recyclarr-adapter`.** `recyclarr` speaks no
  contract and declares `replaces` naming its own id, which is how it takes the
  id, the port and the hostname the bundle reserves for it (`ARCH-R222`).
  `recyclarr-adapter` fronts it, speaks `quality.sync@1`, provides
  `quality.sync` and shares Recyclarr's configuration directory at
  `/recyclarr` (`ARCH-R339`).
- **Its proofs' recordings sit under `recordings/recyclarr/`**, and its starting
  files, where it has any, under `config/recyclarr/` (`ARCH-R271`).
- **`conformance/` holds one recording per case of `quality.sync@1`**, written
  by `lemonfiber plugin record .` at the commit `targets.toml` names, the adapter
  at the digest `plugin.toml` pins in front of Recyclarr at its pinned digest.
  Re-record whenever the adapter's answers or Recyclarr's digest change.
  `tests/conformance.rs` reads every recording with the core's own client.
- **Every service declares `reaches` and `asks_for`** (`ARCH-R270`).
- **No Python here.** The reader is spec's `first-party-plugin` workflow's.
- **No `unwrap`, `expect`, `panic` or indexing** outside tests. The lints in
  `Cargo.toml` are the core's.
- **A preset rewrites one curator's section and nothing else**: its `include:`
  entry, its `base_url` and its `api_key`, which names a `!secret`. Every
  comment, every other section and every `- template:` entry stays as it was.
- **A curator's key goes into `secrets.yml`, owner-only** (`0600`), and every
  file is reached through `lemonfiber_adapter_kit::shared`.
- **A configuration changed by hand is never written over by `apply`.** The first
  line of one the adapter writes carries the digest of the rest. One whose digest
  no longer holds, that is not the shipped file and not one the core wrote from
  `src/recyclarr/lemonfiber-wrote.yml` for some pair of presets, is named in the
  refusal. A hand edit to `lemonfiber-presets.json` is refused the same way.
- **Only `reapply` writes over a hand edit, and `edited` shows it first.** Both
  answer the whole file on disk beside the whole file putting the profiles back
  writes, named by its path in the configuration directory, each at most 64 KiB;
  the core masks and compares them, so the adapter builds no diff. What is
  written starts from the shipped file, with every other curator held to the
  preset `lemonfiber-presets.json` last kept for it.
- **The core's crates move with the train.** The `rev` in `Cargo.toml` is the
  core commit `targets.toml` names, the same for every `lemonfiber-*` crate.
- **Moving Recyclarr's digest** means recording every response again against
  the new image in the same change, moving `RELEASE` and `DIGEST` in
  `src/adapter.rs` with it, and committing the `proofs.json` the checks write.
  `tests/manifest.rs` fails while the adapter's `DIGEST` and the image the
  manifest runs differ.
