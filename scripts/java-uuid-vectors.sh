#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Regenerates crates/telemetry/tests/fixtures/java-uuid-vectors.jsonl: known-
# answer vectors for java.util.UUID.fromString, the parser behind tapir's UUID
# path codec and purerest's http_route template (gluon PLAN.md RUST-3.3,
# RUST-1.9). The JDK is the oracle: each line is an input and what the JDK
# returns for it (null when it throws). Inputs are fixed edge cases plus a
# seeded random sample, so a rerun on the same JDK reproduces the file.
#   scripts/java-uuid-vectors.sh     (needs `java` 11+ on the PATH)
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
OUT=crates/telemetry/tests/fixtures/java-uuid-vectors.jsonl
SRC="$(mktemp -d)/JavaUuidVectors.java"
trap 'rm -rf "$(dirname "$SRC")"' EXIT
cat > "$SRC" << 'JAVA'
import java.util.*;
public class JavaUuidVectors {
  static String esc(String s) {
    StringBuilder b = new StringBuilder("\"");
    for (char c : s.toCharArray()) {
      if (c == '"' || c == '\\') b.append('\\').append(c);
      else if (c < 0x20 || c > 0x7e) b.append(String.format("\\u%04x", (int) c));
      else b.append(c);
    }
    return b.append('"').toString();
  }
  static String parsed(String s) {
    try { return "\"" + UUID.fromString(s) + "\""; } catch (Exception e) { return "null"; }
  }
  public static void main(String[] args) {
    List<String> cases = new ArrayList<>(List.of(
      "691b03e8-f1d7-45d9-a561-5183ffae9d0b", "691B03E8-F1D7-45D9-A561-5183FFAE9D0B",
      "00000000-0000-0000-0000-000000000000", "ffffffff-ffff-ffff-ffff-ffffffffffff",
      "1-2-3-4-5", "+1-+2-+3-+4-+5", "fffffffff-1-1-1-1", "0123456789abcdef0123456789abcdef",
      "{691b03e8-f1d7-45d9-a561-5183ffae9d0b}", "urn:uuid:691b03e8-f1d7-45d9-a561-5183ffae9d0b",
      "1--2-3-4", "", "-", "----", "-----", "1-2-3-4", "1-2-3-4-5-6", "-1-1-1-1-1", "1-1-1-1-",
      "+-1-1-1-1", "++1-1-1-1-1", "1-1-1-1-+", "7fffffffffffffff-1-1-1-1", "8000000000000000-1-1-1-1",
      "000000000000000000000001-1-1-1-1", "0000000000000000000000000000001-1-1",
      "1-1-1-1-ffffffffffffffff", "1-1-1-1-7fffffffffffffff", "g-1-1-1-1", "1-1-1-1-g",
      " 1-1-1-1-1", "1-1-1-1-1 ", "691b03e8-f1d7-45d9-a561-5183ffae9d0",
      "691b03e8-f1d7-45d9-a561-5183ffae9d0bb", "691b03e8f-1d7-45d9-a561-5183ffae9d0b",
      "691b03e-8f1d7-45d9-a561-5183ffae9d0b", "\uff21-1-1-1-1", "\uff46\uff46-1-1-1-1",
      "\uff47-1-1-1-1", "\u0663-\u0664-1-1-1", "\u0966\u0967-1-1-1-1", "\uff10\uff11-1-1-1-1",
      "1-1-1-1-\u00b2", "1-1-1-1-\u2155", "1-1-1-1-\ud835\udfce", "abcdef-ABCDEF-aBcD-EfAb-123456789abc",
      "0x1-1-1-1-1", "1_0-1-1-1-1", "691b03e8-f1d7-45d9-a561+5183ffae9d0b",
      "+691b03e-f1d7-45d9-a561-5183ffae9d0b", "691b03e8-f1d7-45d9-a561-+5183ffae9d0",
      "%7B1-2-3-4-5%7D", "\u0e50-\u0e51-\u0e52-\u0e53-\u0e54", "\u1040-1-1-1-1", "\uabf0-1-1-1-1",
      "\uff41\uff42\uff43-1-1-1-1", "\uff3a-1-1-1-1", "Z-1-1-1-1"));
    Random random = new Random(20261109L);
    String alphabet = "0123456789abcdefABCDEF-+gz \u0663\uff21\uff46";
    for (int i = 0; i < 300; i++) {
      StringBuilder b = new StringBuilder();
      for (int j = 1 + random.nextInt(38); j > 0; j--) b.append(alphabet.charAt(random.nextInt(alphabet.length())));
      cases.add(b.toString());
    }
    for (int i = 0; i < 200; i++) {
      StringBuilder b = new StringBuilder();
      for (int g = 0; g < 5; g++) {
        if (g > 0) b.append('-');
        int n = random.nextInt(14);
        if (random.nextInt(8) == 0) b.append('+');
        for (int j = 0; j < n; j++) b.append("0123456789abcdefABCDEF".charAt(random.nextInt(22)));
      }
      cases.add(b.toString());
    }
    for (int i = 0; i < 50; i++) {
      String u = new UUID(random.nextLong(), random.nextLong()).toString();
      cases.add(u);
      cases.add(u.toUpperCase());
    }
    System.out.println("{\"jdk\": \"" + System.getProperty("java.version") + "\"}");
    for (String c : cases) System.out.println("{\"input\": " + esc(c) + ", \"uuid\": " + parsed(c) + "}");
  }
}
JAVA
java "$SRC" > "$OUT"
echo "wrote $OUT ($(($(wc -l < "$OUT") - 1)) vectors, $(head -1 "$OUT"))"
