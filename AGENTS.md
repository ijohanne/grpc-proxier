# Agent Guidelines

## Rust

### Ownership and Cloning

- Never use `.clone()` unless the value is behind an `Arc`, `Rc`, or similar shared-ownership type. Cloning data to satisfy the borrow checker is not acceptable.
- Prefer transferring ownership (move semantics) even when it requires restructuring code. If a function needs data, take it by value rather than borrowing and cloning.
- Use references (`&T`, `&mut T`) when the caller retains ownership and the callee only needs temporary access.
- If shared ownership is genuinely required, wrap the value in `Arc` (or `Rc` for single-threaded contexts) and clone the handle, not the data.

### Error Handling

- Never use `.unwrap()` or `.expect()`. These panic at runtime and are not acceptable in any code path, including tests.
- Propagate errors with `?` wherever possible. Define domain-specific error types and use `thiserror` for ergonomic error derivation.
- Fail early at system boundaries (startup, config loading) by returning errors to the caller. Do not panic.
- For cases where a value is logically guaranteed to exist, restructure the code to make the guarantee visible to the type system (e.g., use an enum, newtype, or parse-don't-validate pattern) rather than reaching for `.unwrap()`.

### General

- Prefer `&str` over `String` in function signatures when the function does not need ownership.
- Use `impl Trait` in argument position for flexibility and in return position to avoid boxing when there is a single return type.
- Avoid `Box<dyn Trait>` unless dynamic dispatch is genuinely required.
- Keep dependencies minimal. Do not add crates for functionality that can be achieved with a few lines of code.

## Nix

- All dependencies and tooling must be managed through Nix. Do not install tools imperatively or rely on system-global state.
- Use `flake.nix` with `devShells` for development environments. Pin all inputs with `flake.lock`.
- Prefer `buildRustPackage` (or `crane`/`naersk` if already in use) for building Rust projects in Nix. Do not shell out to `cargo build` inside derivations.
- Keep derivations pure. Never use `impureEnvVars`, `fetchurl` without a hash, or `builtins.fetchGit` without a `rev`.
- Format Nix files with `nixfmt` (RFC 166 style).

## Git

### Commit Signing

- All commits must be GPG-signed. Never pass `--no-gpg-sign` or skip signing.
- Never use `--no-verify` to bypass pre-commit hooks.
- If a commit fails because the GPG hardware key (e.g., YubiKey) is not present or the agent cannot access it, stop and ask the user to connect their key. Do not retry, skip signing, or modify the git config to work around it.

### Commit Hygiene

- Write concise commit messages in imperative mood that describe the "why", not the "what".
- Never amend a commit unless the user explicitly requests it. Create new commits instead.
- Never force-push to `main` or `master`.
