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

# Gas a Linear ticket - fetch, create worktree, launch Claude
gas identifier:
    cargo run --bin vibe -- gas {{identifier}}

# Gas a Linear ticket in plan mode
gas-plan identifier:
    cargo run --bin vibe -- gas --plan {{identifier}}

# Watch Linear for ~gasit tickets and auto-gas them
watch:
    cargo run --bin vibe -- watch

# Setup worktree with prebuild
setup:
    cargo build

# Teardown worktree - clean build artifacts
teardown:
    cargo clean
