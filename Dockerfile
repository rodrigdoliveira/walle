FROM rust:1.94-bookworm AS tooling

WORKDIR /workspace
COPY Cargo.toml Cargo.lock* ./
COPY src ./src

RUN rustup component add rustfmt clippy
RUN cargo fetch --locked

CMD ["cargo", "test", "--locked", "--all-targets"]

FROM node:24-bookworm-slim AS ui-tooling

WORKDIR /workspace/ui
COPY ui/package.json ui/package-lock.json ./
RUN npm ci

COPY ui ./

CMD ["sh", "-c", "npm run typecheck && npm test && npm run build"]

FROM ui-tooling AS ui-bundle

RUN npm run build

FROM rust:1.94-bookworm AS native-tooling

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       build-essential \
       file \
       libayatana-appindicator3-dev \
       librsvg2-dev \
       libssl-dev \
       libwebkit2gtk-4.1-dev \
       libxdo-dev \
       pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY src-tauri ./src-tauri
COPY --from=ui-bundle /workspace/target/ui-dist ./target/ui-dist

RUN rustup component add rustfmt clippy
RUN cargo fetch --manifest-path src-tauri/Cargo.toml --locked

CMD ["sh", "-c", "cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check && cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings && cargo test --manifest-path src-tauri/Cargo.toml --locked --all-targets"]
