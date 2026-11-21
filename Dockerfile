# Bot for the Builders "Mech Royale" track (mechfight/1). Pure Rust, no crates.
FROM rust:1.90-slim-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked --bin bot

FROM debian:bookworm-slim
COPY --from=build /src/target/release/bot /bot/bot
ENTRYPOINT ["/bot/bot"]
