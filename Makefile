.PHONY: build release install hooks test serve import rescore fmt lint clean

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

rescore:
	cargo run -- rescore

fmt:
	cargo fmt

lint:
	cargo clippy

clean:
	cargo clean
