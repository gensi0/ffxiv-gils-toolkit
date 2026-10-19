# Image du serveur FFXIV Gils Toolkit : API + client web (PWA).
#   docker build -t gils-toolkit .

# --- Client web ---------------------------------------------------------------
FROM node:22-bookworm-slim AS web
WORKDIR /src
RUN npm install -g pnpm@12
ENV HUSKY=0
COPY package.json pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY index.html vite.config.ts tsconfig*.json components.json* ./
COPY public public
COPY src src
RUN pnpm exec vite build --outDir /web

# --- Plugin Dalamud (servi comme dépôt personnalisé : /plugin/pluginmaster.json)
FROM mcr.microsoft.com/dotnet/sdk:10.0 AS plugin
WORKDIR /src
ENV DALAMUD_HOME=/dalamud/ DOTNET_CLI_TELEMETRY_OPTOUT=1
RUN apt-get update && apt-get install -y --no-install-recommends unzip curl \
    && mkdir -p /dalamud \
    && curl -fsSL https://goatcorp.github.io/dalamud-distrib/latest.zip -o /tmp/dalamud.zip \
    && unzip -q /tmp/dalamud.zip -d /dalamud && rm /tmp/dalamud.zip
COPY plugin plugin
RUN dotnet build plugin/GilsToolkit/GilsToolkit.csproj -c Release \
    && mkdir -p /out \
    && cp plugin/GilsToolkit/bin/Release/GilsToolkit/latest.zip plugin/GilsToolkit/bin/Release/GilsToolkit/GilsToolkit.json /out/

# --- Serveur ------------------------------------------------------------------
FROM rust:1-bookworm AS server
WORKDIR /src
# Dépendances d'abord : couche en cache tant que les Cargo.toml ne changent pas.
COPY src-tauri/core/Cargo.toml src-tauri/core/Cargo.toml
COPY server/Cargo.toml server/Cargo.lock* server/
RUN mkdir -p src-tauri/core/src server/src \
    && echo "" > src-tauri/core/src/lib.rs \
    && echo "fn main() {}" > server/src/main.rs \
    && cargo build --release --manifest-path server/Cargo.toml || true
COPY src-tauri/core src-tauri/core
COPY server server
RUN touch src-tauri/core/src/lib.rs server/src/main.rs \
    && cargo build --release --manifest-path server/Cargo.toml

# --- Image finale -------------------------------------------------------------
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tini \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 1000 --home /data gils \
    && mkdir -p /data && chown gils /data
COPY --from=server /src/server/target/release/ffxiv-gils-server /usr/local/bin/
COPY --from=web /web /app/web
COPY --from=plugin /out /app/web/plugin
USER gils
ENV DATA_DIR=/data WEB_DIR=/app/web PORT=8080 RUST_LOG=info,sqlx=warn
VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s \
  CMD ["/usr/local/bin/ffxiv-gils-server", "--healthcheck"]
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/usr/local/bin/ffxiv-gils-server"]
