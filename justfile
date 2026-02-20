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
    @echo 'use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString}; use argon2::Argon2; fn main() { let salt = SaltString::generate(&mut OsRng); let hash = Argon2::default().hash_password(std::env::args().nth(1).unwrap().as_bytes(), &salt).unwrap(); println!("{}", hash.to_string()); }' | cargo +stable script --edition 2021 -- "{{PASSWORD}}" 2>/dev/null || echo "Requires 'cargo script' or use: cargo run --example hash_password -- {{PASSWORD}}"

# Clean build artifacts
clean:
    cargo clean
