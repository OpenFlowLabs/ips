.PHONY: all release test


all: release

test:
	cargo test

release:
	cargo build --release
	mkdir artifacts
	cp target/release/pkg6dev artifacts/