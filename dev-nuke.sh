#!/usr/bin/env bash
# Nuke all myground docker containers, volumes, and ~/.myground data. No sudo.
set -euo pipefail

# Stop and remove all myground containers
docker ps -a --filter "name=myground-" --format '{{.ID}}' | xargs -r docker rm -f 2>/dev/null || true

# Remove all myground docker volumes
docker volume ls --filter "name=myground" -q | xargs -r docker volume rm -f 2>/dev/null || true
# Also remove volumes with naming pattern used in compose (e.g. beszel_ts-beszel-state)
docker volume ls -q | grep -E "(myground|ts-.*-state|beszel|agent)" | xargs -r docker volume rm -f 2>/dev/null || true

# Remove ~/.myground using a docker container (avoids sudo for root-owned files)
if [ -d "$HOME/.myground" ]; then
  docker run --rm -v "$HOME/.myground:/data" alpine sh -c "rm -rf /data/* /data/.[!.]* /data/..?*" 2>/dev/null || true
  rm -rf "$HOME/.myground" 2>/dev/null || true
fi

echo "Nuked all containers, volumes, and ~/.myground."
