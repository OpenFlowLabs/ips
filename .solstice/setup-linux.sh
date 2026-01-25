#!/usr/bin/env bash
set -euo pipefail
# Solstice CI per-OS environment prepare (Linux)
# Installs baseline tools needed by the workflow runner and builds.

log() { printf "[setup-linux] %s\n" "$*" >&2; }

export DEBIAN_FRONTEND=${DEBIAN_FRONTEND:-noninteractive}

detect_pm() {
  if command -v apt-get >/dev/null 2>&1; then echo apt; return; fi
  if command -v dnf >/dev/null 2>&1; then echo dnf; return; fi
  if command -v yum >/dev/null 2>&1; then echo yum; return; fi
  if command -v zypper >/dev/null 2>&1; then echo zypper; return; fi
  if command -v apk >/dev/null 2>&1; then echo apk; return; fi
  if command -v pacman >/dev/null 2>&1; then echo pacman; return; fi
  echo none
}

install_packages() {
  local pm; pm=$(detect_pm)
  case "$pm" in
    apt)
      sudo -n true 2>/dev/null || true
      sudo apt-get update -y || apt-get update -y || true
      sudo apt-get install -y --no-install-recommends \
        curl ca-certificates git build-essential pkg-config libssl-dev \
        protobuf-compiler cmake clang libclang-dev || true
      ;;
    dnf)
      sudo dnf install -y curl ca-certificates git gcc gcc-c++ make pkgconf-pkg-config openssl-devel protobuf-compiler clang clang-libs || true
      ;;
    yum)
      sudo yum install -y curl ca-certificates git gcc gcc-c++ make pkgconfig openssl-devel protobuf-compiler clang clang-libs || true
      ;;
    zypper)
      sudo zypper --non-interactive install curl ca-certificates git gcc gcc-c++ make pkg-config libopenssl-devel protobuf clang || true
      ;;
    apk)
      sudo apk add --no-cache curl ca-certificates git build-base pkgconfig openssl-dev protoc clang clang-libs || true
      ;;
    pacman)
      sudo pacman -Sy --noconfirm curl ca-certificates git base-devel pkgconf openssl protobuf clang || true
      ;;
    *)
      log "unknown package manager ($pm); skipping package install"
      ;;
  esac
}

ensure_rust() {
  if command -v cargo >/dev/null 2>&1; then return 0; fi
  log "installing Rust toolchain with rustup"
  curl -fsSL https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  if [ -f "$HOME/.cargo/env" ]; then . "$HOME/.cargo/env"; else export PATH="$HOME/.cargo/bin:$PATH"; fi
}

main() {
  install_packages
  ensure_rust
  if ! command -v protoc >/dev/null 2>&1; then
    log "WARNING: protoc not found; prost/tonic builds may fail"
  fi
}

main "$@"
