# Task runner for lemonfiber/plugin-recyclarr. `just` with no argument lists these.
default:
    @just --list

# Turn on the repository's own git hooks. Once per clone.
hooks:
    git config core.hooksPath .githooks
    @echo "hooks on: .githooks/commit-msg, .githooks/pre-push"

# The gates that need nothing but this tree, in CI's order. The plugin verdict
# (spec's first-party-plugin workflow) and the adapter's image, which need
# Docker, run in CI.
ci: hooks fmt clippy test coverage deny typos links

# The adapter's format.
fmt:
    cargo fmt --check

# The adapter's lints, every warning an error.
clippy:
    cargo clippy --all-targets --locked -- -D warnings

# The adapter's tests.
test:
    cargo test --locked

# Every line of the adapter the tests can reach, reached.
coverage:
    cargo llvm-cov --locked --ignore-filename-regex "$(cat .config/coverage-skipped)" --fail-under-lines 100

# The adapter's dependencies, held to `deny.toml`.
deny:
    cargo deny --log-level warn check

# Spell check.
typos:
    typos

# Link check.
links:
    lychee --no-progress .
