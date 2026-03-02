#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

# Kill any existing server on port 8080
if pid=$(fuser 8080/tcp 2>/dev/null); then
  kill $pid 2>/dev/null || true
  # Wait for it to actually die
  for i in {1..20}; do
    fuser 8080/tcp >/dev/null 2>&1 || break
    sleep 0.25
  done
fi

# Rebuild frontend
echo ":: Building frontend..."
(cd web && npx vite build) 2>&1

# Rebuild backend
echo ":: Building backend..."
cargo build 2>&1

# Start server in background, tailing output
echo ":: Starting server..."
cargo run -- start 2>&1 &
SERVER_PID=$!

# Wait until port 8080 is actually listening
for i in {1..40}; do
  if fuser 8080/tcp >/dev/null 2>&1; then
    echo ":: Server ready (pid $SERVER_PID) at http://localhost:8080"
    exit 0
  fi
  sleep 0.25
done

echo ":: ERROR: Server did not start within 10s"
exit 1
