#!/bin/sh
# make.sh: renders card.html into one 1200x630 JPEG share card per line of pages.tsv (file, title and subtitle,
# tab-separated) with headless Chrome, into $OUT. Run it after adding or retitling a page, then commit the images.
# macOS: uses Google Chrome and sips; elsewhere set CHROME and have ImageMagick's convert.
set -e
cd "$(dirname "$0")"
CHROME=${CHROME:-"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"}
OUT=${OUT:?set OUT to the folder the site serves the cards from}
mkdir -p "$OUT"
while IFS='	' read -r name title sub; do
  case "$name" in ""|"#"*) continue;; esac
  q=$(python3 -c 'import sys, urllib.parse as u; print(u.urlencode({"t": sys.argv[1], "s": sys.argv[2]}))' "$title" "$sub")
  "$CHROME" --headless=new --hide-scrollbars --force-device-scale-factor=1 --window-size=1200,630 --virtual-time-budget=5000 \
    --screenshot="$OUT/$name.png" "file://$PWD/card.html?$q" >/dev/null 2>&1
  if command -v sips >/dev/null; then sips -s format jpeg -s formatOptions 84 "$OUT/$name.png" --out "$OUT/$name.jpg" >/dev/null
  else convert "$OUT/$name.png" -quality 84 "$OUT/$name.jpg"; fi
  rm "$OUT/$name.png"
  echo "$OUT/$name.jpg"
done < pages.tsv
