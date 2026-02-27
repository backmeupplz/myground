.PHONY: dev dev-server dev-web build build-web build-server test test-server test-web clean

# Development: run both server and web dev servers
dev:
	@echo "Run 'make dev-server' and 'make dev-web' in separate terminals"

dev-server:
	cargo run -- start

dev-web:
	cd web && bun run dev

# Build: web first (so rust-embed can include it), then server
build: build-web build-server

build-web:
	cd web && bun install && bun run build

build-server:
	cargo build --release

# Test: run both backend and frontend tests
test: test-server test-web

test-server:
	cargo test

test-web:
	cd web && bunx vitest run

clean:
	cargo clean
	rm -rf web/dist web/node_modules
