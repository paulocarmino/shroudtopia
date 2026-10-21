# syntax=docker/dockerfile:1
# @shroudtopia/server as a container: the Fastify sync server (app/CONTRACT.md), API at
# https://shroudtopia-api.paulocarmino.com, and the landing page (app/landing) for https://shroudtopia.paulocarmino.com
# when SHROUDTOPIA_LANDING_HOST names it. No manifests here: it is deployed by gaia-iac (charts/shroudtopia-sync,
# cluster/90-shroudtopia).
#
# Node 24 runs the .ts sources as they are; there is no build step. Every dependency is plain JavaScript and SQLite is
# Node's own node:sqlite, so the install runs on the build machine's platform and the arm64 stage runs no command at
# all: the image builds on an amd64 PC without QEMU.
# The landing page is outside this context: it comes in as a second one named `landing` (without it the build stops).
#   docker buildx build --platform linux/arm64 -t shroudtopia-server --build-context landing=app/landing \
#     --build-arg SHROUDTOPIA_BUILD_COMMIT=$(git rev-parse --short=7 HEAD) app/server
# Run:
#   docker run --rm -p 8800:8800 -v shroudtopia-data:/data shroudtopia-server
#   curl http://localhost:8800/healthz

FROM --platform=$BUILDPLATFORM node:24-bookworm-slim AS deps
ENV PNPM_HOME=/pnpm PATH=/pnpm:$PATH CI=true
RUN corepack enable
WORKDIR /app
COPY package.json pnpm-lock.yaml ./
RUN --mount=type=cache,id=pnpm,target=/pnpm/store pnpm install --frozen-lockfile --prod --ignore-scripts

FROM node:24-bookworm-slim
ARG SHROUDTOPIA_BUILD_COMMIT=
ENV NODE_ENV=production \
    SHROUDTOPIA_BUILD_COMMIT=$SHROUDTOPIA_BUILD_COMMIT \
    HOST=0.0.0.0 \
    PORT=8800 \
    SHROUDTOPIA_DATA_DIR=/data \
    SHROUDTOPIA_TRUST_PROXY=1 \
    SHROUDTOPIA_LANDING_DIR=/app/landing
WORKDIR /app
COPY --from=deps /app/node_modules node_modules
COPY package.json ./
COPY src src
COPY --from=landing . landing
# /data is the volume (SQLite); the pod's fsGroup makes it writable for `node`
USER node
EXPOSE 8800
CMD ["node", "src/main.ts"]
