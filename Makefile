.PHONY: all release test clean publish-all


all: clean release

test:
	cargo test

clean:
	rm -rf target artifacts

release:
	cargo build --release
	mkdir -p artifacts
	cp target/release/pkg6dev artifacts/

publish-all: publish.libips publish.userland publish.pkg6dev

publish.%: CRATE=$*
publish.%:
	cd $(CRATE); cargo publish