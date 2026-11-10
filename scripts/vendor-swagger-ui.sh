#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Vendors Swagger UI into crates/telemetry/assets/swagger-ui/ from the same
# webjar the Scala services serve (org.webjars:swagger-ui, via tapir's
# SwaggerInterpreter): the files the page loads, the webjar's pom (licence:
# Apache-2.0) and a MANIFEST of each file's size and modification time, which
# the ETag and Last-Modified headers are made from, as tapir makes them
# (gluon PLAN.md RUST-4.1). The jar's SHA-1 is checked against Maven Central's.
#
# tapir reads a webjar file's modification time from the jar file itself
# (JarURLConnection delegates to the jar's file: URL), i.e. when the jar was
# published to Maven Central, which the sandbox's proxy doesn't pass on. So it
# is recorded here: 5.20.7's, 2025-04-07T12:50:53Z, as the Scala service sent
# it (capture 20261109 http/docs-index.txt: Last-Modified, ETag 196104dbfc8-2de).
#   scripts/vendor-swagger-ui.sh [VERSION PUBLISHED_MS]   (default 5.20.7 1744030253000)
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
VERSION="${1:-5.20.7}"
PUBLISHED_MS="${2:-1744030253000}"
BASE="https://repo1.maven.org/maven2/org/webjars/swagger-ui/$VERSION/swagger-ui-$VERSION"
OUT=crates/telemetry/assets/swagger-ui
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
curl -sSf -o "$SCRATCH/swagger-ui.jar" "$BASE.jar"
expected="$(curl -sSf "$BASE.jar.sha1" | cut -c1-40)"
actual="$(sha1sum "$SCRATCH/swagger-ui.jar" | cut -c1-40)"
if [ "$expected" != "$actual" ]; then
  echo "SHA-1 mismatch: Maven Central says $expected, downloaded $actual" >&2
  exit 1
fi
rm -rf "$OUT"
mkdir -p "$OUT"
python3 - "$SCRATCH/swagger-ui.jar" "$VERSION" "$OUT" "$actual" "$PUBLISHED_MS" << 'PY'
import sys, zipfile
from pathlib import Path

jar, version, out, sha1, published = sys.argv[1], sys.argv[2], Path(sys.argv[3]), sys.argv[4], sys.argv[5]
# What index.html and the initializer load, plus the OAuth2 redirect page.
RUNTIME = [
    "index.html", "index.css", "swagger-initializer.js", "swagger-ui.css",
    "swagger-ui-bundle.js", "swagger-ui-standalone-preset.js",
    "favicon-16x16.png", "favicon-32x32.png", "oauth2-redirect.html",
]

with zipfile.ZipFile(jar) as archive:
    lines = [f"# swagger-ui {version} webjar, SHA-1 {sha1}: name size mtime-ms"]
    for name in RUNTIME:
        info = archive.getinfo(f"META-INF/resources/webjars/swagger-ui/{version}/{name}")
        (out / name).write_bytes(archive.read(info))
        lines.append(f"{name} {info.file_size} {published}")
    (out / "pom.xml").write_bytes(archive.read("META-INF/maven/org.webjars/swagger-ui/pom.xml"))
    (out / "MANIFEST").write_text("\n".join(lines) + "\n")
print(f"vendored swagger-ui {version} ({len(RUNTIME)} files) into {out}")
PY
