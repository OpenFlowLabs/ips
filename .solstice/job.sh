#!/usr/bin/env bash
set -euo pipefail
# Solstice CI legacy job script.
# NOTE: All environment and package setup is handled by per-OS setup scripts
# referenced in .solstice/workflow.kdl and executed by the workflow runner.
# This script intentionally contains no setup logic.

log() { printf "[job] %s\n" "$*" >&2; }

main() {
  # Keep a minimal representative build as a legacy hook. The workflow steps
  # already perform fmt/clippy/build/test; this is safe to remove later.
  log "building workflow-runner"
  cargo build --release || cargo build
  log "done"
}

main "$@"
