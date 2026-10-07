# The service image. The build reads the committed .sqlx query metadata (SQLX_OFFLINE=true), so it needs no
# database; rustup in the build stage installs the toolchain rust-toolchain.toml pins. Both images are pinned
# by digest; Renovate moves them.
FROM rust:1.98.1-slim-trixie@sha256:4cd829461bd5c4d511c32e269da9cb8929223b666519d8004e35fc8d1d771ab7 AS build
WORKDIR /src
COPY . .
ENV SQLX_OFFLINE=true
RUN cargo build --release --locked --bin starter

FROM gcr.io/distroless/cc-debian13:nonroot@sha256:e792ab3d241a468a4fd7519ddbbebe66b49b5f365771716ea688ad40b6c6f1c2
COPY --from=build /src/target/release/starter /app/starter
USER nonroot
EXPOSE 8080
ENTRYPOINT ["/app/starter"]
