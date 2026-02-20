set dotenv-load

# List available commands
default:
    @just --list

# Run the proxy with cargo-watch for hot-reload
dev:
    cargo watch -x run

# Run the proxy without hot-reload
run:
    cargo run

# Build release binary
build:
    cargo build --release

# Run cargo check
check:
    cargo check

# Format code
fmt:
    cargo fmt

# Run clippy linter
clippy:
    cargo clippy -- -D warnings

# Run tests
test:
    cargo test

# Generate argon2 hash for a password
hash-password PASSWORD:
    @echo "{{PASSWORD}}" | cargo run --bin grpc-proxier-hash

# Clean build artifacts
clean:
    cargo clean
