# The adapter's image: built from this repository, distroless, non-root, with no
# shell and no package manager. Its health check is the binary asking itself.

FROM rust:1.100.0-alpine3.22@sha256:8e71319e2fb470aa2d6335a8376a104ceea1957f86ec91b7efcbb9b4732973ae AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock build.rs plugin.toml ./
COPY src ./src
COPY config/recyclarr ./config/recyclarr
RUN cargo build --release --locked --bin recyclarr-adapter \
    && cp target/release/recyclarr-adapter /recyclarr-adapter

FROM gcr.io/distroless/static-debian12:nonroot@sha256:afa5c872c891853ca7fcf1f12c3edb23f7eeef36189728842dd51042ff57f7ab
COPY --from=build /recyclarr-adapter /recyclarr-adapter
USER nonroot:nonroot
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 CMD ["/recyclarr-adapter", "health"]
ENTRYPOINT ["/recyclarr-adapter"]
