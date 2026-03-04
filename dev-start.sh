#!/usr/bin/env bash
# Start backend (cargo-watch) and frontend (vite dev) with hot-reload.
# Detaches both processes and exits immediately.
set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"

# Backend: cargo-watch rebuilds + restarts on src/ changes
nohup cargo watch \
  --watch "$DIR/src" \
  --watch "$DIR/Cargo.toml" \
  -x "run -- start" \
  > /tmp/myground-backend.log 2>&1 &

# Frontend: vite dev server with HMR, proxies /api to :8080
nohup sh -c "cd '$DIR/web' && npx vite dev --host 0.0.0.0 --port 5173" \
  > /tmp/myground-frontend.log 2>&1 &

echo "Dev servers started."
echo "  Backend:  http://localhost:8080  (log: /tmp/myground-backend.log)"
echo "  Frontend: http://localhost:5173  (log: /tmp/myground-frontend.log)"
