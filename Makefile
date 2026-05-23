.PHONY: build release install test serve import fmt lint clean

build:
	cargo build

release:
	cargo build --release

install: release
	cp target/release/llp ~/.local/bin/llp

test:
	cargo test

serve:
	cargo run -- serve

import:
	cargo run -- import

fmt:
	cargo fmt

lint:
	cargo clippy

clean:
	cargo clean
