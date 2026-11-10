#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Vendors Swagger UI into crates/telemetry/assets/swagger-ui/ from the same
# webjar the Scala services serve (org.webjars:swagger-ui, via tapir's
# SwaggerInterpreter): the files the page loads, the webjar's pom (licence:
# Apache-2.0), Swagger UI's LICENSE and NOTICE (the webjar has neither, so they
# come from the npm package swagger-ui-dist of the same version, checked to
# carry the same bundle) and a MANIFEST of each file's size and modification
# time, which the ETag and Last-Modified headers are made from, as tapir makes them
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
curl -sSf -o "$SCRATCH/swagger-ui-dist.tgz" \
  "https://registry.npmjs.org/swagger-ui-dist/-/swagger-ui-dist-$VERSION.tgz"
expected="$(curl -sSf "$BASE.jar.sha1" | cut -c1-40)"
sha1() { if command -v sha1sum >/dev/null; then sha1sum "$1"; else shasum "$1"; fi; }  # macOS: shasum
actual="$(sha1 "$SCRATCH/swagger-ui.jar" | cut -c1-40)"
if [ "$expected" != "$actual" ]; then
  echo "SHA-1 mismatch: Maven Central says $expected, downloaded $actual" >&2
  exit 1
fi
rm -rf "$OUT"
mkdir -p "$OUT"
python3 - "$SCRATCH/swagger-ui.jar" "$VERSION" "$OUT" "$actual" "$PUBLISHED_MS" "$SCRATCH/swagger-ui-dist.tgz" << 'PY'
import sys, tarfile, zipfile
from pathlib import Path

jar, version, out, sha1, published = sys.argv[1], sys.argv[2], Path(sys.argv[3]), sys.argv[4], sys.argv[5]
dist = sys.argv[6]
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
    bundle = archive.read(f"META-INF/resources/webjars/swagger-ui/{version}/swagger-ui-bundle.js")
# Apache-2.0 asks redistributors for the licence and NOTICE; take them from
# the npm release only if it is the same build as the webjar.
with tarfile.open(dist) as package:
    if package.extractfile("package/swagger-ui-bundle.js").read() != bundle:
        sys.exit(f"swagger-ui-dist {version} doesn't carry the webjar's bundle")
    for name in ("LICENSE", "NOTICE"):
        (out / name).write_bytes(package.extractfile(f"package/{name}").read())
print(f"vendored swagger-ui {version} ({len(RUNTIME)} files) into {out}")
PY
