#!/usr/bin/env bash

set -ex

export RUST_LOG=debug
PKG6REPO_PATH="sample_data/pkg6-repo"
PKG5REPO_PATH="sample_data/sample-repo.p5p"
PKG6REPO_BIN="./target/debug/pkg6repo"

if [ -d "$PKG6REPO_PATH" ]; then
  rm -rf $PKG6REPO_PATH
fi

cargo build

$PKG6REPO_BIN create $PKG6REPO_PATH
$PKG6REPO_BIN add-publisher -s $PKG6REPO_PATH openindiana.org
$PKG6REPO_BIN import-pkg5 -s $PKG5REPO_PATH -d $PKG6REPO_PATH
