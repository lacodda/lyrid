# The stand is a Raspberry Pi, so this image is built for aarch64. Building it
# on the Pi itself keeps the architecture honest: no cross-compilation, and no
# chance of shipping an x86 binary that only fails on the stand.

# ---------------------------------------------------------------- the SPA
FROM node:22-slim AS web
WORKDIR /web

# Dependencies first, so a source-only change does not reinstall them.
COPY web/package.json web/pnpm-lock.yaml ./
RUN corepack enable && corepack prepare --activate && pnpm install --frozen-lockfile

COPY web/ ./
RUN pnpm build

# ------------------------------------------------------------- the server
FROM rust:1-slim-trixie AS server
WORKDIR /src

# A stub main lets the dependency build cache survive source changes, which is
# what makes a rebuild on a Pi bearable rather than a coffee break.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && cargo build --release && rm -rf src

COPY src/ src/
COPY migrations/ migrations/
# Migrations are embedded at compile time, so a new one has to invalidate the
# build. Touching main.rs is what tells cargo the stub is stale.
RUN touch src/main.rs && cargo build --release

# ------------------------------------------------------------- the runtime
FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# Unprivileged: nothing here needs root, and a web-facing process least of all.
RUN useradd --uid 10001 --no-create-home --shell /usr/sbin/nologin lyrid
WORKDIR /app

COPY --from=server /src/target/release/lyrid /usr/local/bin/lyrid
COPY --from=web /web/dist /app/static

# The tile directory has to exist in the image and be owned by the runtime
# user. Docker only copies ownership onto a fresh named volume from what the
# image already has at the mount point; an absent path leaves the volume
# root-owned, and `lyrid layout --tiles` then cannot write a single tile.
RUN mkdir -p /app/static/tiles && chown -R 10001:10001 /app/static

ENV LYRID_STATIC=/app/static \
    LYRID_ADDR=0.0.0.0:8080
EXPOSE 8080
USER 10001

# /health reports degraded without a database, so it distinguishes "alive but
# cannot work" from "not listening at all".
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/health || exit 1

CMD ["lyrid", "serve"]
