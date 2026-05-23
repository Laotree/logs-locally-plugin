.PHONY: build release install hooks test serve import fmt lint clean

build:
	cargo build

release:
	cargo build --release

install: release hooks
	cp target/release/llp ~/.local/bin/llp

hooks:
	cp hooks/pre-commit .git/hooks/pre-commit
	chmod +x .git/hooks/pre-commit

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
