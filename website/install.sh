#!/bin/sh
# MyGround installer — https://myground.online
# Usage: curl -fsSL https://myground.online/install.sh | sh
set -eu

REPO="backmeupplz/myground"
BIN_NAME="myground"

main() {
  echo "MyGround installer"
  echo "=================="
  echo

  # Detect architecture
  ARCH=$(uname -m)
  case "$ARCH" in
    x86_64|amd64)  TARGET="x86_64-unknown-linux-gnu" ;;
    aarch64|arm64) TARGET="aarch64-unknown-linux-gnu" ;;
    *)
      echo "Error: unsupported architecture: $ARCH" >&2
      exit 1
      ;;
  esac
  echo "Architecture: $ARCH ($TARGET)"

  # Check OS
  OS=$(uname -s)
  if [ "$OS" != "Linux" ]; then
    echo "Error: MyGround only supports Linux (detected: $OS)" >&2
    exit 1
  fi

  # Check Docker
  if ! command -v docker >/dev/null 2>&1; then
    echo "Error: Docker is required but not installed." >&2
    echo "Install Docker: https://docs.docker.com/engine/install/" >&2
    exit 1
  fi
  echo "Docker: found"

  # Determine install directory
  if [ "$(id -u)" -eq 0 ]; then
    INSTALL_DIR="/usr/local/bin"
  elif command -v sudo >/dev/null 2>&1; then
    INSTALL_DIR="/usr/local/bin"
    NEED_SUDO=1
  else
    INSTALL_DIR="$HOME/.local/bin"
    mkdir -p "$INSTALL_DIR"
    NEED_SUDO=0
  fi
  echo "Install directory: $INSTALL_DIR"

  # Fetch latest release tag
  echo
  echo "Fetching latest release..."
  LATEST=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed 's/.*"tag_name": *"//;s/".*//')
  if [ -z "$LATEST" ]; then
    echo "Error: could not determine latest release" >&2
    exit 1
  fi
  echo "Latest version: $LATEST"

  # Download binary and checksum
  ASSET="$BIN_NAME-$TARGET"
  BASE_URL="https://github.com/$REPO/releases/download/$LATEST"

  TMPDIR=$(mktemp -d)
  trap 'rm -rf "$TMPDIR"' EXIT

  echo "Downloading $ASSET..."
  curl -fSL -o "$TMPDIR/$ASSET" "$BASE_URL/$ASSET"
  curl -fSL -o "$TMPDIR/$ASSET.sha256" "$BASE_URL/$ASSET.sha256"

  # Verify checksum
  echo "Verifying checksum..."
  cd "$TMPDIR"
  sha256sum -c "$ASSET.sha256"
  cd - >/dev/null

  # Install binary
  echo "Installing to $INSTALL_DIR/$BIN_NAME..."
  chmod +x "$TMPDIR/$ASSET"
  if [ "$(id -u)" -eq 0 ]; then
    mv "$TMPDIR/$ASSET" "$INSTALL_DIR/$BIN_NAME"
  elif [ "${NEED_SUDO:-0}" = "1" ]; then
    sudo mv "$TMPDIR/$ASSET" "$INSTALL_DIR/$BIN_NAME"
  else
    mv "$TMPDIR/$ASSET" "$INSTALL_DIR/$BIN_NAME"
  fi

  echo "Installed: $INSTALL_DIR/$BIN_NAME"

  # Optionally install systemd service
  if command -v systemctl >/dev/null 2>&1; then
    echo
    printf "Install systemd service? [y/N] "
    read -r REPLY </dev/tty || REPLY="n"
    case "$REPLY" in
      [yY]|[yY][eE][sS])
        USER=$(whoami)
        SERVICE_FILE="/etc/systemd/system/$BIN_NAME@.service"
        SERVICE_CONTENT="[Unit]
Description=MyGround self-hosting platform (user %i)
After=network-online.target docker.service
Requires=docker.service
Wants=network-online.target

[Service]
Type=simple
User=%i
ExecStart=$INSTALL_DIR/$BIN_NAME start
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target"

        if [ "$(id -u)" -eq 0 ]; then
          echo "$SERVICE_CONTENT" > "$SERVICE_FILE"
        elif command -v sudo >/dev/null 2>&1; then
          echo "$SERVICE_CONTENT" | sudo tee "$SERVICE_FILE" >/dev/null
        else
          echo "Warning: cannot install service without root. Skipping." >&2
          echo
          echo "Done! Run 'myground start' to get started."
          return
        fi

        sudo systemctl daemon-reload
        sudo systemctl enable "$BIN_NAME@$USER"
        echo "Service installed: systemctl start $BIN_NAME@$USER"
        ;;
    esac
  fi

  echo
  echo "Done! Run 'myground start' to get started."
}

main
