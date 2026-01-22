default:
    @just --list

# Run the TUI
vibe:
    cargo run --bin vibe

# Run with logging
vibe-log:
    RUST_LOG=info cargo run --bin vibe

# Run with file watching
vibe-live:
    cargo watch -x "run --bin vibe"

# Build release
build:
    cargo build --release

# Install to ~/.cargo/bin
install:
    cargo install --path .

# Run tests
test:
    cargo test

# Lint
lint:
    cargo clippy -- -D warnings
    cargo fmt --check

# Format
fmt:
    cargo fmt

# Run migration tool
migrate:
    cargo run --bin vibe-migrate
