#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Renders the AsyncAPI HTML the service serves at /docs/asyncapi/ (gluon
# PLAN.md RUST-4.2), as sbt-purekafka-docs' asyncApiGenerate does for the Scala
# services: the spec (here from the asyncapi-spec binary) through
# @asyncapi/cli + @asyncapi/html-template, installed once into target/ and
# rendered from the local template path. html-template is 3.5.6 as in the
# Scala build; the cli is 6.2.0 because the sandbox's npm lacks the Scala
# build's 6.2.2 (the HTML is byte-identical either way: capture 20261109).
#
#   scripts/asyncapi-html.sh [OUTPUT_DIR]    (default target/asyncapi-docs, the
#                                             service's ASYNCAPI_HTML_DIR default)
# Needs Node >= 24.11 (@asyncapi/generator 3.x): $NODE, else node24 (gluon
# bin/sandbox-setup), else node; npm for the one-off install.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
CLI_VERSION=6.2.0
TEMPLATE_VERSION=3.5.6
MIN_NODE=24.11.0
OUT="${1:-target/asyncapi-docs}"
SPEC=target/asyncapi-docs-spec/asyncapi.json
TOOLS="target/asyncapi-cli/cli-${CLI_VERSION}_html-${TEMPLATE_VERSION}"

node=""
for candidate in ${NODE:-} node24 node; do
  found="$(command -v "$candidate" 2> /dev/null || true)"
  [ -n "$found" ] || continue
  version="$("$found" -v | sed 's/^v//')"
  if [ "$(printf '%s\n%s\n' "$MIN_NODE" "$version" | sort -V | head -1)" = "$MIN_NODE" ]; then
    node="$(realpath "$found")"
    break
  fi
done
if [ -z "$node" ]; then
  echo "asyncapi-html: needs Node >= $MIN_NODE (set NODE, or run gluon bin/sandbox-setup --only node)" >&2
  exit 1
fi
# npm and the cli run on whichever `node` comes first on the PATH: make it this one.
export PATH="$(dirname "$node"):$PATH"
export SUPPRESS_NO_CONFIG_WARNING=true PUPPETEER_SKIP_DOWNLOAD=true

echo "asyncapi-html: spec from the asyncapi-spec binary → $SPEC"
cargo build -q --locked -p user-service --bin asyncapi-spec
mkdir -p "$(dirname "$SPEC")"
target/debug/asyncapi-spec > "$SPEC"

if [ ! -x "$TOOLS/node_modules/.bin/asyncapi" ] || [ ! -d "$TOOLS/node_modules/@asyncapi/html-template" ]; then
  echo "asyncapi-html: installing @asyncapi/cli@$CLI_VERSION and @asyncapi/html-template@$TEMPLATE_VERSION into $TOOLS (once)"
  mkdir -p "$TOOLS"
  npm install --no-save --loglevel=error --prefix "$TOOLS" \
    "@asyncapi/cli@$CLI_VERSION" "@asyncapi/html-template@$TEMPLATE_VERSION" > /dev/null
fi

# Clear the contents, not the directory (a bind mount keeps working).
mkdir -p "$OUT"
find "$OUT" -mindepth 1 -delete
echo "asyncapi-html: rendering with Node $(node -v) → $OUT"
"$TOOLS/node_modules/.bin/asyncapi" generate fromTemplate "$SPEC" \
  "$(realpath "$TOOLS/node_modules/@asyncapi/html-template")" \
  --output "$(realpath "$OUT")" --force-write --no-interactive > "$TOOLS/generate.log" 2>&1 \
  || { cat "$TOOLS/generate.log" >&2; exit 1; }
echo "asyncapi-html: $(find "$OUT" -type f | wc -l) files in $OUT"
