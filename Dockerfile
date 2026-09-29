# The service image. The build reads the committed .sqlx query metadata (SQLX_OFFLINE=true), so it needs no
# database; rustup in the build stage installs the toolchain rust-toolchain.toml pins. Both images are pinned
# by digest; Renovate moves them.
FROM rust:1.98.1-slim-trixie@sha256:4cd829461bd5c4d511c32e269da9cb8929223b666519d8004e35fc8d1d771ab7 AS build
WORKDIR /src
COPY . .
ENV SQLX_OFFLINE=true
RUN cargo build --release --locked --bin starter

FROM gcr.io/distroless/cc-debian13:nonroot@sha256:54df941ed0d06a1bd95ef5e0ce391fd8d9f94b64782dc9a60062727849ee3f97
COPY --from=build /src/target/release/starter /app/starter
USER nonroot
EXPOSE 8080
ENTRYPOINT ["/app/starter"]
