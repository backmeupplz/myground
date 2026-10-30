#!/bin/sh
# MyGround installer — https://myground.online
# Usage: curl -fsSL https://myground.online/install.sh | sh
#
# Supports: Debian/Ubuntu, Arch Linux, Omarchy, Fedora, macOS
# Installs Docker if needed, downloads the latest binary, sets up a
# system service, and prints the dashboard URL on your LAN.
set -eu

REPO="backmeupplz/myground"
BIN_NAME="myground"
PORT=8080

# ── Helpers ──────────────────────────────────────────────────────────

info()  { printf '  \033[1;34m→\033[0m %s\n' "$*"; }
ok()    { printf '  \033[1;32m✓\033[0m %s\n' "$*"; }
warn()  { printf '  \033[1;33m!\033[0m %s\n' "$*"; }
err()   { printf '  \033[1;31m✗\033[0m %s\n' "$*" >&2; exit 1; }

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || err "Required command not found: $1"
}

run_sudo() {
  if [ "$(id -u)" -eq 0 ]; then
    "$@"
  elif command -v sudo >/dev/null 2>&1; then
    sudo "$@"
  else
    err "Root privileges required. Run as root or install sudo."
  fi
}

# ── Platform Detection ───────────────────────────────────────────────

detect_platform() {
  OS=$(uname -s)
  ARCH=$(uname -m)

  case "$OS" in
    Linux)  PLATFORM="linux" ;;
    Darwin) PLATFORM="macos" ;;
    *)      err "Unsupported OS: $OS" ;;
  esac

  case "$ARCH" in
    x86_64|amd64)  ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *)             err "Unsupported architecture: $ARCH" ;;
  esac

  case "$PLATFORM-$ARCH" in
    linux-x86_64)  TARGET="x86_64-unknown-linux-gnu" ;;
    linux-aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
    macos-x86_64)  TARGET="x86_64-apple-darwin" ;;
    macos-aarch64) TARGET="aarch64-apple-darwin" ;;
  esac

  DISTRO=""
  DISTRO_LIKE=""
  if [ "$PLATFORM" = "linux" ] && [ -f /etc/os-release ]; then
    . /etc/os-release
    DISTRO="${ID:-}"
    DISTRO_LIKE="${ID_LIKE:-}"
  fi
}

# Package family of the distro: debian, fedora, arch or unknown. Derivatives are matched through ID_LIKE.
distro_family() {
  for id in $DISTRO $DISTRO_LIKE; do
    case "$id" in
      debian|ubuntu|raspbian|linuxmint|pop) echo debian; return ;;
      fedora)                               echo fedora; return ;;
      arch|endeavouros|manjaro|garuda)      echo arch; return ;;
    esac
  done
  echo unknown
}

# ── Docker ───────────────────────────────────────────────────────────

# Run a command with its output hidden. If it fails, show the end of that output and abort.
# Fresh cloud VMs refresh and upgrade packages right after boot, so while apt reports its lock is held by
# another process, wait and retry (up to 10 minutes) instead.
quiet() {
  desc=$1
  shift
  deadline=$(($(date +%s) + 600))
  waited=0
  while :; do
    log=$(mktemp)
    if "$@" >"$log" 2>&1; then
      rm -f "$log"
      return
    else
      status=$?
    fi
    if [ "$(date +%s)" -ge "$deadline" ] ||
      ! grep -q '^E: \(Could not get lock\|Unable to acquire the dpkg frontend lock\)' "$log"; then
      break
    fi
    [ "$waited" = 1 ] || info "Waiting for another package manager to finish..."
    waited=1
    rm -f "$log"
    sleep 10
  done
  printf '\n' >&2
  tail -n 25 "$log" | sed 's/^/    /' >&2
  rm -f "$log"
  err "$desc failed (exit code $status)"
}

# MyGround runs `docker compose` (v2); the standalone `docker-compose` binary Homebrew installs also works.
compose_ok() {
  docker compose version >/dev/null 2>&1 && return 0
  [ "$PLATFORM" = "macos" ] && docker-compose version >/dev/null 2>&1
}

# Candidate version of an apt package, empty when the configured archives don't have it.
apt_candidate() {
  LC_ALL=C apt-cache policy "$1" 2>/dev/null | awk '$1 == "Candidate:" && $2 != "(none)" { print $2 }'
}

# True when a Debian version string (with an optional epoch) is 2.x or newer.
is_v2_or_later() {
  v=${1#*:}
  [ "${v%%.*}" -ge 2 ] 2>/dev/null
}

dnf_has() {
  dnf -q info "$1" >/dev/null 2>&1
}

# Docker's own installer (adds Docker's package repository); only used when Docker isn't installed yet, since it
# conflicts with distro Docker packages.
install_docker_script() {
  need_cmd curl
  info "Using Docker's convenience script (get.docker.com)..."
  script=$(mktemp)
  quiet "Downloading https://get.docker.com" curl -fsSL -o "$script" https://get.docker.com
  quiet "Docker's convenience script" run_sudo env LC_ALL=C sh "$script"
  rm -f "$script"
}

install_docker_debian() {
  # apt waits for the dpkg lock by itself; quiet retries while the package lists are locked. English output for
  # quiet's lock detection.
  apt_lock="-o DPkg::Lock::Timeout=600"
  quiet "apt-get update" run_sudo env LC_ALL=C apt-get $apt_lock update
  # Compose v2 is docker-compose-v2 on Ubuntu, docker-compose on Debian 13+ (1.x before that), and
  # docker-compose-plugin only in Docker's own repository, which an existing docker-ce install comes from.
  if [ "$HAVE_DOCKER" = "1" ]; then
    candidates="docker-compose-plugin docker-compose-v2 docker-compose"
  else
    candidates="docker-compose-v2 docker-compose-plugin docker-compose"
  fi
  compose_pkg=""
  for pkg in $candidates; do
    if is_v2_or_later "$(apt_candidate "$pkg")"; then
      compose_pkg=$pkg
      break
    fi
  done

  if [ "$HAVE_DOCKER" = "1" ]; then
    [ -n "$compose_pkg" ] || compose_missing
    # don't pull in the distro's docker.io next to an existing Docker
    pkgs="--no-install-recommends $compose_pkg"
  elif [ -n "$compose_pkg" ] && [ -n "$(apt_candidate docker.io)" ]; then
    pkgs="docker.io $compose_pkg"
  else
    install_docker_script
    return
  fi
  info "Installing ${pkgs#--no-install-recommends }..."
  quiet "apt-get install $pkgs" run_sudo env LC_ALL=C DEBIAN_FRONTEND=noninteractive apt-get $apt_lock install -y $pkgs
}

install_docker_fedora() {
  if [ "$HAVE_DOCKER" = "1" ]; then
    for pkg in docker-compose-plugin docker-compose; do
      if dnf_has "$pkg"; then
        quiet "dnf install $pkg" run_sudo dnf install -y "$pkg"
        return
      fi
    done
    compose_missing
  elif dnf_has moby-engine && dnf_has docker-compose; then
    quiet "dnf install moby-engine docker-compose" run_sudo dnf install -y moby-engine docker-compose
  else
    install_docker_script
  fi
}

compose_missing() {
  err "Docker is installed, but no Docker Compose v2 package was found for $DISTRO. Install it: https://docs.docker.com/compose/install/linux/"
}

install_docker() {
  if command -v docker >/dev/null 2>&1; then
    # Existing setups with a standalone docker-compose keep working: MyGround falls back to it.
    if compose_ok || docker-compose version >/dev/null 2>&1; then
      ok "Docker already installed"
      return
    fi
    HAVE_DOCKER=1
    info "Docker is installed but Docker Compose is missing, installing it..."
  else
    HAVE_DOCKER=0
    info "Installing Docker..."
  fi

  case "$PLATFORM" in
    linux)
      run_sudo true # ask for the password (or fail visibly) before output is hidden
      case "$(distro_family)" in
        debian) install_docker_debian ;;
        fedora) install_docker_fedora ;;
        arch)
          if [ "$HAVE_DOCKER" = "1" ]; then
            quiet "pacman -S docker-compose" run_sudo pacman -Sy --noconfirm --needed docker-compose
          else
            quiet "pacman -S docker docker-compose" run_sudo pacman -Sy --noconfirm --needed docker docker-compose
          fi
          ;;
        *)
          if [ "$HAVE_DOCKER" = "1" ]; then
            compose_missing
          fi
          install_docker_script
          ;;
      esac
      ;;
    macos)
      if ! command -v brew >/dev/null 2>&1; then
        err "Homebrew is required on macOS. Install it: https://brew.sh"
      fi
      if [ "$HAVE_DOCKER" = "1" ]; then
        quiet "brew install docker-compose" brew install docker-compose
      else
        quiet "brew install docker docker-compose colima" brew install docker docker-compose colima
      fi
      ;;
  esac

  command -v docker >/dev/null 2>&1 || err "Docker was installed but the docker command is not on PATH"
  compose_ok || err "'docker compose version' still fails after installing Docker Compose. Install it manually: https://docs.docker.com/compose/install/linux/"
  ok "Docker installed ($(docker compose version 2>/dev/null || docker-compose version 2>/dev/null | head -1))"
}

ensure_docker_running() {
  case "$PLATFORM" in
    linux)
      if command -v systemctl >/dev/null 2>&1; then
        run_sudo systemctl enable --now docker >/dev/null 2>&1 || true
      fi
      # Add current user to docker group
      if [ "$(id -u)" -ne 0 ]; then
        if ! id -nG "$(whoami)" 2>/dev/null | grep -qw docker; then
          run_sudo usermod -aG docker "$(whoami)" 2>/dev/null || true
          DOCKER_GROUP_CHANGED=1
        fi
      fi
      ;;
    macos)
      if command -v colima >/dev/null 2>&1; then
        if ! colima status >/dev/null 2>&1; then
          info "Starting Colima..."
          colima start >/dev/null 2>&1
        fi
      fi
      ;;
  esac

  # Verify Docker works
  if docker info >/dev/null 2>&1; then
    ok "Docker is running"
  elif [ "${DOCKER_GROUP_CHANGED:-0}" = "1" ]; then
    warn "Added $(whoami) to docker group — takes effect after re-login"
  else
    warn "Docker is installed but not accessible yet"
  fi
}

# ── Binary Install ───────────────────────────────────────────────────

install_binary() {
  need_cmd curl

  # Determine install directory
  if [ "$(id -u)" -eq 0 ] || command -v sudo >/dev/null 2>&1; then
    INSTALL_DIR="/usr/local/bin"
  else
    INSTALL_DIR="$HOME/.local/bin"
    mkdir -p "$INSTALL_DIR"
  fi

  # Fetch latest release tag
  info "Fetching latest release..."
  LATEST=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | grep '"tag_name"' | sed 's/.*"tag_name": *"//;s/".*//')
  [ -z "$LATEST" ] && err "Could not determine latest release"
  ok "Version: $LATEST"

  # Download binary + checksum
  ASSET="$BIN_NAME-$TARGET"
  BASE_URL="https://github.com/$REPO/releases/download/$LATEST"

  TMPDIR=$(mktemp -d)
  trap 'rm -rf "$TMPDIR"' EXIT

  info "Downloading $ASSET..."
  curl -fSL -o "$TMPDIR/$ASSET" "$BASE_URL/$ASSET"
  curl -fSL -o "$TMPDIR/$ASSET.sha256" "$BASE_URL/$ASSET.sha256"

  # Verify checksum
  info "Verifying checksum..."
  cd "$TMPDIR"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -c "$ASSET.sha256" >/dev/null
  else
    shasum -a 256 -c "$ASSET.sha256" >/dev/null
  fi
  cd - >/dev/null
  ok "Checksum verified"

  # Install
  chmod +x "$TMPDIR/$ASSET"
  run_sudo install -m 755 "$TMPDIR/$ASSET" "$INSTALL_DIR/$BIN_NAME"

  # Allow the service user to self-update and restart without a password.
  if [ "$(id -u)" -ne 0 ] && [ "$PLATFORM" = "linux" ]; then
    SUDOERS_FILE="/etc/sudoers.d/myground-update"
    {
      printf '%s ALL=(root) NOPASSWD: /usr/bin/install -m 755 /tmp/myground-update %s/%s\n' \
        "$(whoami)" "$INSTALL_DIR" "$BIN_NAME"
      printf '%s ALL=(root) NOPASSWD: /usr/bin/systemctl restart myground@*\n' "$(whoami)"
    } | run_sudo tee "$SUDOERS_FILE" >/dev/null
    run_sudo chmod 440 "$SUDOERS_FILE"
  fi

  ok "Installed to $INSTALL_DIR/$BIN_NAME"
}

# ── Service Setup ────────────────────────────────────────────────────

setup_service() {
  case "$PLATFORM" in
    linux)  setup_systemd ;;
    macos)  setup_launchd ;;
  esac
}

setup_systemd() {
  if ! command -v systemctl >/dev/null 2>&1; then
    warn "systemd not found — skipping service setup"
    warn "Start manually: myground start --address 0.0.0.0"
    return
  fi

  USER_NAME=$(whoami)
  SERVICE_FILE="/etc/systemd/system/$BIN_NAME@.service"

  run_sudo tee "$SERVICE_FILE" >/dev/null <<'UNIT'
[Unit]
Description=MyGround self-hosting platform (user %i)
After=network-online.target docker.service
Requires=docker.service
Wants=network-online.target

[Service]
Type=simple
User=%i
ExecStart=/usr/local/bin/myground start --address 0.0.0.0
Restart=always
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
UNIT

  run_sudo systemctl daemon-reload
  run_sudo systemctl enable "$BIN_NAME@$USER_NAME" >/dev/null 2>&1
  run_sudo systemctl restart "$BIN_NAME@$USER_NAME" >/dev/null 2>&1
  ok "Service enabled and started"
}

setup_launchd() {
  PLIST_DIR="$HOME/Library/LaunchAgents"
  PLIST_FILE="$PLIST_DIR/online.myground.plist"
  mkdir -p "$PLIST_DIR"

  cat > "$PLIST_FILE" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>online.myground</string>
  <key>ProgramArguments</key>
  <array>
    <string>/usr/local/bin/myground</string>
    <string>start</string>
    <string>--address</string>
    <string>0.0.0.0</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>/tmp/myground.log</string>
  <key>StandardErrorPath</key>
  <string>/tmp/myground.err</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>RUST_LOG</key>
    <string>info</string>
  </dict>
</dict>
</plist>
PLIST

  launchctl unload "$PLIST_FILE" 2>/dev/null || true
  launchctl load "$PLIST_FILE"
  ok "LaunchAgent installed and started"
}

# ── Network ──────────────────────────────────────────────────────────

get_lan_ip() {
  case "$PLATFORM" in
    linux)
      if command -v ip >/dev/null 2>&1; then
        ip route get 1.1.1.1 2>/dev/null | awk '{for(i=1;i<=NF;i++) if($i=="src") print $(i+1)}' | head -1
      elif command -v hostname >/dev/null 2>&1; then
        hostname -I 2>/dev/null | awk '{print $1}'
      fi
      ;;
    macos)
      ipconfig getifaddr en0 2>/dev/null || \
        ifconfig 2>/dev/null | grep 'inet ' | grep -v '127.0.0.1' | head -1 | awk '{print $2}'
      ;;
  esac
}

# ── Main ─────────────────────────────────────────────────────────────

main() {
  printf '\n'
  printf '  \033[1m╔══════════════════════════════════════╗\033[0m\n'
  printf '  \033[1m║         MyGround Installer           ║\033[0m\n'
  printf '  \033[1m╚══════════════════════════════════════╝\033[0m\n'
  printf '\n'

  detect_platform
  info "Platform: $PLATFORM ($ARCH)"
  [ -n "$DISTRO" ] && info "Distro: $DISTRO"

  printf '\n'
  install_docker
  ensure_docker_running

  printf '\n'
  install_binary

  printf '\n'
  setup_service

  LAN_IP=$(get_lan_ip)
  LAN_IP="${LAN_IP:-localhost}"

  printf '\n'
  printf '  \033[1;32m╔══════════════════════════════════════╗\033[0m\n'
  printf '  \033[1;32m║         MyGround is ready!           ║\033[0m\n'
  printf '  \033[1;32m╚══════════════════════════════════════╝\033[0m\n'
  printf '\n'
  printf '  Open the setup wizard to finish configuration:\n'
  printf '\n'
  printf '    \033[1;4mhttp://%s:%s\033[0m\n' "$LAN_IP" "$PORT"
  printf '\n'

  case "$PLATFORM" in
    linux)
      if command -v systemctl >/dev/null 2>&1; then
        printf '  \033[2mManage:  systemctl {start|stop|restart} myground@%s\033[0m\n' "$(whoami)"
        printf '  \033[2mLogs:    journalctl -u myground@%s -f\033[0m\n' "$(whoami)"
      fi
      ;;
    macos)
      printf '  \033[2mLogs:    tail -f /tmp/myground.log\033[0m\n'
      ;;
  esac
  printf '\n'
}

main
