#!/usr/bin/env bash
# Fetch the authoritative Material Symbols Rounded codepoints map from
# Google's repo and convert it to the JSON shape MESH's icon resolver
# expects. The repo file ships as `name codepoint` text; we transform it
# to `{ "name": "\uXXXX", ... }`.
#
# Usage:
#   ./fetch-codepoints.sh           # writes ./codepoints.json
#   ./fetch-codepoints.sh outlined  # for the Outlined variant
#
# Requires: curl, awk
set -euo pipefail

VARIANT="${1:-rounded}"
case "$VARIANT" in
  rounded)  URL="https://raw.githubusercontent.com/google/material-design-icons/master/variablefont/MaterialSymbolsRounded%5BFILL%2CGRAD%2Copsz%2Cwght%5D.codepoints" ;;
  outlined) URL="https://raw.githubusercontent.com/google/material-design-icons/master/variablefont/MaterialSymbolsOutlined%5BFILL%2CGRAD%2Copsz%2Cwght%5D.codepoints" ;;
  sharp)    URL="https://raw.githubusercontent.com/google/material-design-icons/master/variablefont/MaterialSymbolsSharp%5BFILL%2CGRAD%2Copsz%2Cwght%5D.codepoints" ;;
  *) echo "unknown variant: $VARIANT (expected rounded|outlined|sharp)" >&2; exit 1 ;;
esac

OUT="$(dirname "$0")/codepoints.json"
TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT

curl -fsSL "$URL" -o "$TMP"

awk '
  BEGIN { print "{"; first = 1 }
  /^[[:space:]]*$/ || /^#/ { next }
  {
    if (!first) printf ",\n"
    printf "  \"%s\": \"\\u%s\"", $1, $2
    first = 0
  }
  END { print "\n}" }
' "$TMP" > "$OUT"

echo "Wrote $OUT ($(wc -l <"$OUT") lines, variant=$VARIANT)"
