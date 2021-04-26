.PHONY: all release test clean


all: clean release

test:
	cargo test

clean:
	rm -rf target artifacts

release:
	cargo build --release
	mkdir -p artifacts
	cp target/release/pkg6dev artifacts/