.PHONY: build release test serve import fmt lint clean

build:
	cargo build

release:
	cargo build --release

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
