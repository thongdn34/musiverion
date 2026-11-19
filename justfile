# Musiverion task runner. `just --list` shows all recipes.

# Everything CI checks: Rust fmt/clippy/tests, frontend lint/typecheck/format.
check:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    pnpm lint
    pnpm typecheck
    pnpm format:check

# Rust tests.
test:
    cargo test --workspace

# Format Rust and frontend code.
fmt:
    cargo fmt --all
    pnpm format

# Run the desktop app in development mode.
dev:
    pnpm --filter desktop tauri dev

# Build the frontend and the desktop app bundle (release).
build:
    pnpm build
    pnpm --filter desktop tauri build
