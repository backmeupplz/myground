#!/usr/bin/env bash
# Start backend (cargo-watch) and frontend (vite dev) with hot-reload.
set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"

# Backend: cargo-watch rebuilds + restarts on src/ changes
cargo watch \
  --watch "$DIR/src" \
  --watch "$DIR/Cargo.toml" \
  -x "run -- start" \
  &
BACKEND_PID=$!

# Frontend: vite dev server with HMR, proxies /api to :8080
(cd "$DIR/web" && npx vite dev --host 0.0.0.0 --port 5173) &
FRONTEND_PID=$!

echo ""
echo "  Backend:  cargo-watch (PID $BACKEND_PID) → http://localhost:8080"
echo "  Frontend: vite dev    (PID $FRONTEND_PID) → http://localhost:5173"
echo ""
echo "  Open http://localhost:5173 for development"
echo "  Press Ctrl+C to stop both."
echo ""

trap "kill $BACKEND_PID $FRONTEND_PID 2>/dev/null; exit" INT TERM
wait
