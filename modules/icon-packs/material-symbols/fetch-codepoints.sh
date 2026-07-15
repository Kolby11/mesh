#!/usr/bin/env bash
# Fetch the authoritative Material Symbols font, codepoints map, and license
# from Google's repository. The repo map ships as `name codepoint` text; we
# transform it to the JSON shape MESH's icon resolver expects.
#
# Usage:
#   ./fetch-codepoints.sh
#
# Requires: curl, awk
set -euo pipefail

UPSTREAM_COMMIT="819d78680a849ceef4c78f863d8753e3160b7c89"
BASE_URL="https://raw.githubusercontent.com/google/material-design-icons/${UPSTREAM_COMMIT}"
CODEPOINTS_URL="${BASE_URL}/variablefont/MaterialSymbolsRounded%5BFILL%2CGRAD%2Copsz%2Cwght%5D.codepoints"
FONT_URL="${BASE_URL}/variablefont/MaterialSymbolsRounded%5BFILL%2CGRAD%2Copsz%2Cwght%5D.ttf"
LICENSE_URL="${BASE_URL}/LICENSE"

PACK_DIR="$(dirname "$0")"
OUT="${PACK_DIR}/codepoints.json"
FONT_OUT="${PACK_DIR}/assets/MaterialSymbolsRounded.ttf"
LICENSE_OUT="${PACK_DIR}/LICENSE.google-material-icons"
TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT

mkdir -p "${PACK_DIR}/assets"
curl -fsSL "$CODEPOINTS_URL" -o "$TMP"
curl -fsSL "$FONT_URL" -o "$FONT_OUT"
curl -fsSL "$LICENSE_URL" -o "$LICENSE_OUT"

awk '
  function hex_to_int(hex,    digits, value, i, digit) {
    digits = "0123456789abcdef"
    hex = tolower(hex)
    value = 0
    for (i = 1; i <= length(hex); i++) {
      digit = index(digits, substr(hex, i, 1)) - 1
      value = (value * 16) + digit
    }
    return value
  }
  BEGIN { print "{"; first = 1 }
  /^[[:space:]]*$/ || /^#/ { next }
  {
    if (!first) printf ",\n"
    codepoint = hex_to_int($2)
    if (codepoint <= 65535) {
      encoded = sprintf("\\u%04x", codepoint)
    } else {
      codepoint -= 65536
      high = 55296 + int(codepoint / 1024)
      low = 56320 + (codepoint % 1024)
      encoded = sprintf("\\u%04x\\u%04x", high, low)
    }
    printf "  \"%s\": \"%s\"", $1, encoded
    first = 0
  }
  END { print "\n}" }
' "$TMP" > "$OUT"

echo "Wrote $OUT ($(wc -l <"$OUT") lines)"
echo "Wrote $FONT_OUT ($(wc -c <"$FONT_OUT") bytes)"
echo "Wrote $LICENSE_OUT"
