#!/bin/sh
# website-app-count.sh: replaces {{APP_COUNT}} in website/ with the number of apps in the catalog (apps/*.toml, the
# files the binary embeds), so the site can't drift from it. The Pages workflow runs it right before upload. It edits
# website/ in place, so to preview the real numbers locally, commit or stash your website/ edits first and
# `git checkout website` afterwards.
set -e
cd "$(dirname "$0")/.."
n=$(find apps -maxdepth 1 -name '*.toml' | wc -l | tr -d ' ')
[ "$n" -gt 0 ] || { echo "no apps/*.toml found" >&2; exit 1; }
files=$(grep -rl '{{APP_COUNT}}' website || true)
[ -n "$files" ] || { echo "no {{APP_COUNT}} placeholder in website/" >&2; exit 1; }
for f in $files; do
  perl -pi -e "s/\{\{APP_COUNT\}\}/$n/g" "$f"
  echo "$f: $n apps"
done
