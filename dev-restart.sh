#!/usr/bin/env bash
set -e

DIR="$(cd "$(dirname "$0")" && pwd)"

# Kill any running myground processes
pkill -9 -f "target/debug/myground" 2>/dev/null || true
sleep 1

# Nuke containers and test data
"$DIR/target/debug/myground" nuke --data-dir /tmp/myground-test 2>/dev/null || true
rm -rf /tmp/myground-test
mkdir -p /tmp/myground-test

# Rebuild frontend + backend
(cd "$DIR/web" && npx vite build)
cargo build --manifest-path "$DIR/Cargo.toml"

# Start fresh in background
"$DIR/target/debug/myground" start --data-dir /tmp/myground-test &
MGPID=$!

# Wait for it to be ready
for i in $(seq 1 15); do
  if curl -s http://localhost:8080 > /dev/null 2>&1; then
    echo "MyGround running at http://localhost:8080 (PID: $MGPID)"
    exit 0
  fi
  sleep 1
done
echo "ERROR: server did not start within 15s"
exit 1
