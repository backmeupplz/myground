#!/usr/bin/env bash
# Stop all dev processes (backend, frontend, cargo-watch)
set -euo pipefail

pkill -9 -f "cargo watch" 2>/dev/null || true
pkill -9 -f "target/debug/myground" 2>/dev/null || true
pkill -9 -f "vite.*5173" 2>/dev/null || true

# Wait for ports to free
for port in 8080 5173; do
  for _ in {1..20}; do
    fuser "$port/tcp" >/dev/null 2>&1 || break
    sleep 0.2
  done
done

echo "Dev processes stopped."
