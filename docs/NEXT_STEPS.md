# Next steps — continuing on another machine

Written 2026-11-20. State at commit `02ac197` on `main`; CI green on macOS, Linux and Windows.

## 1. Set up the machine

1. **SSH access.** Add an SSH key from this machine to the `thongdn34` GitHub account (the key used on the first Mac stays there).
2. **Clone.**
   ```sh
   git clone git@github.com:thongdn34/musiverion.git
   cd musiverion
   ```
3. **Commit identity** (only if the machine's global git identity is a different account):
   ```sh
   git config user.name thongdn34
   git config user.email <the email of the thongdn34 GitHub account>
   ```
   If the machine has several GitHub keys, pin the right one for this repo:
   ```sh
   git config core.sshCommand "ssh -i ~/.ssh/<key file> -o IdentitiesOnly=yes"
   ```
4. **Tools.**
   - Rust stable via <https://rustup.rs> (the repo's `rust-toolchain.toml` selects the toolchain and components).
   - Node 24 and pnpm 10 (`corepack enable` picks the pinned pnpm version).
   - `just` (`brew install just`, `cargo install just`, or `winget install Casey.Just`).
   - `git` and `git-lfs` on PATH (the `mv-git` tests use them).
   - Linux only: Tauri system packages — see the `apt-get install` line in `.github/workflows/ci.yml`.
   - Windows only: Visual Studio Build Tools (C++ workload) and WebView2.
5. **Verify.**
   ```sh
   pnpm install
   just check                              # fmt, clippy, tests, lint, typecheck, format
   cargo run -p mv-cli -- ping --json      # prints {"message":"Musiverion is running",...}
   pnpm --filter desktop tauri dev         # opens the window with the ping panel
   ```
   On Windows run `cargo test --workspace --exclude musiverion-desktop` (see Known gaps).

## 2. Where things stand

| Task (`docs/PLAN.md`) | State |
|---|---|
| 1 Scaffold | Done |
| 2 Git process runner (`crates/mv-git`) | Done |
| 3 Validation: git hosts and LFS | Not started |
| 4 Validation: DAW save behaviour | Tool and protocol done (`tools/fsprobe`); **FL Studio run not done yet** |
| 5 Bundled git strategy | Not started |
| 6–15 Phase 1 | Not started |

Read `CLAUDE.md` first, then `docs/SPEC.md`, `docs/PLAN.md`, `docs/ROADMAP.md`, `filemap.md`. Design: `docs/design/DESIGN_SYSTEM.md` and `docs/design/mockups.html` (approved).

## 3. What to do next, in order

### A. Run the FL Studio test (human, 60–90 minutes)
Follow `docs/validation/RUN_FL_STUDIO.md`. One command:
```sh
cargo run -p fsprobe --release -- run --protocol tools/fsprobe/protocols/fl-studio.toml --out validation-results/fl-studio
```
- A licensed FL Studio gives complete results; the trial works with the built-in demo-song workaround (step 7d is left out).
- `validation-results/` is git-ignored. Keep the folder on the machine where the run happened, or copy it by hand.
- Afterwards: fill the FL Studio section of `docs/validation/daws.md` from `summary.md`, and correct any menu name marked "(check)" in `tools/fsprobe/protocols/fl-studio.toml`.
- If results contradict the design (for example FL Studio cannot cope with a read-only project file), update `docs/SPEC.md` §7 and §10 **before** starting tasks 10–12.

### B. Task 3 — Validation: git hosts and LFS (can run in parallel with A)
Scripted experiments in `tools/validation/`, results in `docs/validation/hosts.md`:
- LFS lock API on Forgejo (local container), GitHub and GitLab; does `lfs.locksverify=true` block a push that touches someone else's lock?
- `lockable` read-only behaviour on each OS.
- Does a git textconv driver receive the real audio or the LFS pointer?
- Metadata-only clone (skip smudge) plus per-folder `lfs.fetchinclude`.
Needs Docker for Forgejo and throwaway test repos on GitHub and GitLab.

### C. Task 5 — Bundled git strategy (after task 2, independent of A and B)
Script that downloads pinned `git` + `git-lfs` builds per target (dugite-native releases), verifies checksums, places them as Tauri sidecars; runtime lookup already exists in `mv-git` (`GitLocation::discover(bundled_dir)`).

### D. Then Phase 1
Start with task 6 (domain model in `mv-core`) and task 7 (jobs and events in `mv-app`). Tasks 10–12 wait for the results of A and B.

## 4. Known gaps to keep in mind

- **Windows process cleanup:** on cancel or timeout `mv-git` kills only the direct `git` process; helpers it spawned can survive. Needs a Windows Job Object.
- **Windows tests:** the Tauri bridge crate's test binary cannot start on Windows (missing app manifest), so CI excludes `musiverion-desktop` there. `fsprobe` filesystem tests are skipped on Windows.
- **LFS upload progress** is tested only against hand-written sample lines.
- **macOS `.dmg` bundling** was not verified (it timed out in an automated session); `--bundles app` works.
- **The ping window** has never been visually checked — do it once with `pnpm --filter desktop tauri dev`.
- `tauri-specta` is pinned to a release candidate (2.0.0-rc.29); TypeScript is held at 6.0.x by the lint tooling.
- The `repository` URL in the root `Cargo.toml` is still a placeholder (`github.com/musiverion/musiverion`); it should point at `github.com/thongdn34/musiverion`.
- The CLI binary is `mv-cli`, not `mv` as PLAN task 15 says.

## 5. Working with an AI agent on the new machine

The chat history does not travel; the repo does. Start a session in the repo root and say which task from section 3 to do. `CLAUDE.md` is loaded automatically and points at the spec and plan. When a task is accepted, tick it in `docs/PLAN.md`, update `filemap.md`, and update or delete this file.
