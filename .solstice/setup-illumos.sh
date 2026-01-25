#!/usr/bin/env bash
set -euo pipefail
# Solstice CI per-OS environment prepare (illumos / SunOS)
# Installs baseline tools (curl, git, gtar, compilers, rust) where possible.

log() { printf "[setup-illumos] %s\n" "$*" >&2; }

install_packages() {
  if command -v pkg >/dev/null 2>&1; then
    # OpenIndiana / IPS
    sudo pkg refresh || true
    # Prefer GNU tar (gtar) to match runner expectations
    sudo pkg install -v \
      web/curl \
      developer/build/gnu-make \
      developer/gcc-13 \
      developer/protobuf \
      developer/clang \
      archiver/gnu-tar \
      developer/rustc || true
    # CA certs where package exists
    sudo pkg install -v web/ca-certificates || true
    # mozilla-rootcerts when available
    if command -v mozilla-rootcerts >/dev/null 2>&1; then
      sudo mozilla-rootcerts install || true
    fi
  elif command -v pkgin >/dev/null 2>&1; then
    # SmartOS/NetBSD pkgin
    sudo pkgin -y update || true
    sudo pkgin -y install curl gmake gcc protobuf clang gtar rust || true
    sudo pkgin -y install mozilla-rootcerts || true
    if command -v mozilla-rootcerts >/dev/null 2>&1; then
      sudo mozilla-rootcerts install || true
    fi
  else
    log "no known package manager found (pkg/pkgin); skipping installs"
  fi
}

main() {
  install_packages
  # Prefer GNU tar on PATH when available
  if command -v gtar >/dev/null 2>&1 && ! command -v tar >/dev/null 2>&1; then
    ln -sf "$(command -v gtar)" "$HOME/bin/tar" 2>/dev/null || true
  fi
}

main "$@"
