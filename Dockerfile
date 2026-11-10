# SPDX-License-Identifier: MIT
# The Rust user-service image (gluon ADR 0028 decision 7; PLAN.md RUST-5.1),
# built by gluon bin/k8s-local-up (USER_SERVICE_IMPL=rust) with
#   docker build -t user-service-rs:latest services/user-service-rs
# Written in the sandbox, which has no Docker: first built on the laptop.
#
# Like the Scala image: ports 8080 (API) and 9090 (metrics), the AsyncAPI HTML
# built in (scripts/asyncapi-html.sh's steps, html-template 3.5.6), served
# from ASYNCAPI_HTML_DIR.

FROM rust:1.99.1-bookworm AS build
WORKDIR /src
COPY . .
# rust-toolchain.toml pins 1.99.1, the image's own toolchain.
RUN cargo build --release --locked -p user-service --bin user-service --bin asyncapi-spec

FROM node:24-bookworm AS docs
WORKDIR /docs
COPY --from=build /src/target/release/asyncapi-spec /usr/local/bin/asyncapi-spec
ENV PUPPETEER_SKIP_DOWNLOAD=true SUPPRESS_NO_CONFIG_WARNING=true
RUN asyncapi-spec > asyncapi.json \
 && npm install --no-save --loglevel=error @asyncapi/cli@6.2.0 @asyncapi/html-template@3.5.6 > /dev/null \
 && node_modules/.bin/asyncapi generate fromTemplate asyncapi.json \
      "$PWD/node_modules/@asyncapi/html-template" \
      --output /docs/html --force-write --no-interactive

FROM debian:bookworm-slim
COPY --from=build /src/target/release/user-service /usr/local/bin/user-service
COPY --from=docs /docs/html /app/asyncapi-docs
ENV ASYNCAPI_HTML_DIR=/app/asyncapi-docs
WORKDIR /app
EXPOSE 8080 9090
# A numbered non-root user: no shell or home needed.
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/user-service"]
