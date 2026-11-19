# Musiverion

Rust workspace (core engine) + Tauri v2 (desktop/mobile shell) + React/TypeScript UI. Open source, MIT OR Apache-2.0.

Read first: `docs/SPEC.md` (what and why), `docs/PLAN.md` (tasks), `docs/ROADMAP.md` (phases), `filemap.md` (where things live).

## Commands
- Install frontend deps: `pnpm install`
- Check everything (what CI runs): `just check`
- Rust tests: `cargo test --workspace`
- Lint: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && pnpm lint && pnpm typecheck`
- Format: `just fmt`
- Desktop dev: `pnpm --filter desktop tauri dev` (also regenerates `apps/desktop/src/bindings.ts`)
- Desktop debug bundle: `pnpm --filter desktop tauri build --debug`
- CLI: `cargo run -p mv-cli -- ping --json`
- Stale bindings: `UPDATE_BINDINGS=1 cargo test -p musiverion-desktop`

## Conventions
- `mv-core` does no I/O: types, traits and policy functions only. Implementations live in the other crates.
- All logic goes through `mv-app`. `src-tauri`, `mv-remote` and `mv-cli` are thin adapters; the frontend holds no business rules.
- Git and LFS work shells out to the bundled `git`/`git-lfs` via `mv-git`. Never add libgit2/gitoxide for network or LFS operations. Arguments are arrays, never shell strings.
- Never content-merge DAW project files or audio. Conflicts produce a conflict copy (SPEC §7.4).
- UI text uses studio language (SPEC §6). No git verbs in anything a user sees.
- Derived data (peaks, loudness, diffs) is cached by LFS OID in the OS cache dir, never written into the repo.
- Anything long-running is a cancellable job with progress events, not a blocking command.
- No `unwrap`/`expect` outside tests; errors are `thiserror` enums per crate.
- When you add or move files, update `filemap.md`. When a task is accepted, tick it in `docs/PLAN.md`.
- `reports/` and `research_notes/` are reference material; do not edit them.
