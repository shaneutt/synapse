.PHONY: build release test lint fmt audit check clean

build:
	cargo build --workspace

release:
	cargo build --workspace --release

test:
	cargo test --workspace

lint:
	cargo clippy --workspace -- -D warnings
	cargo +nightly fmt --all -- --check

fmt:
	cargo +nightly fmt --all

audit:
	cargo audit
	cargo deny check

check: lint test audit

clean:
	cargo clean
	rm -rf .synapse-cache target
