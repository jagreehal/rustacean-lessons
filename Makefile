.PHONY: check format lint test deny quality

check: format lint test ## The one gate: format, lints, every lesson, every compile-fail test

format:
	cargo fmt --all -- --check

lint:
	cargo clippy --all-targets -- -D warnings

test:
	cargo test

deny: ## Supply-chain policy. Needs: cargo install cargo-deny
	cargo deny check

quality: check deny ## What CI runs
