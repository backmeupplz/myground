#!/bin/sh
# check-website-apps.sh: fails unless every catalog app (apps/*.toml) has a row in the docs app table
# (website/docs.html) and a card on the homepage (website/index.html), and neither lists an app the catalog lacks,
# so adding or removing an app can't leave the site behind.
set -e
cd "$(dirname "$0")/.."
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
for f in apps/*.toml; do sed -n 's/^name = "\(.*\)"$/\1/p' "$f" | head -n 1; done | sort > "$tmp/catalog"
[ -s "$tmp/catalog" ] || { echo "no apps/*.toml found" >&2; exit 1; }
sed -n 's|^ *<tr><td>\([^<]*\)</td><td data-label="Category">.*|\1|p' website/docs.html | sort > "$tmp/docs"
sed -n 's|^ *<div class="app-card-name">\([^<]*\)</div>.*|\1|p' website/index.html | sort > "$tmp/home"
list() { paste -sd, - | sed 's/,/, /g'; }
fail=0
for page in docs home; do
  case $page in
    docs) where="website/docs.html (#app-catalog)" ;;
    home) where="website/index.html (#apps)" ;;
  esac
  missing=$(comm -23 "$tmp/catalog" "$tmp/$page" | list)
  extra=$(sort -u "$tmp/$page" | comm -13 "$tmp/catalog" - | list)
  dupes=$(uniq -d "$tmp/$page" | list)
  [ -z "$missing" ] || { echo "$where is missing: $missing" >&2; fail=1; }
  [ -z "$extra" ] || { echo "$where lists apps not in apps/*.toml: $extra" >&2; fail=1; }
  [ -z "$dupes" ] || { echo "$where lists twice: $dupes" >&2; fail=1; }
done
[ "$fail" = 0 ] || exit 1
echo "website lists all $(wc -l < "$tmp/catalog" | tr -d ' ') catalog apps"
