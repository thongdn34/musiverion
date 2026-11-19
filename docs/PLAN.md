# Musiverion — Task plan

Phases and exit criteria: `ROADMAP.md`. Spec: `SPEC.md`. Tiers: Light, Medium, Careful, Heavy, Fable. Status: ☐ open, ☑ accepted. Phases 0–1 are broken into tasks; later phases are broken down when the phase before them is two-thirds done.

## Phase 0 — Foundations and validation

**1. Scaffold** ☑ — Medium — deps: none
Install the Rust toolchain if missing. Create the Cargo workspace with empty crates `mv-core`, `mv-git`, `mv-audio`, `mv-daw`, `mv-watch`, `mv-meta`, `mv-remote`, `mv-app`, `mv-cli` (each compiling, one smoke test), `apps/desktop` (Tauri v2 + React + TypeScript + Vite, one `ping` command through `mv-app` with `tauri-specta` bindings), `packages/ui` (empty shared package), pnpm workspace, `rustfmt`/`clippy -D warnings`, ESLint/Prettier, a `justfile`, dual license files, README, GitHub Actions CI (fmt, clippy, test, frontend lint/typecheck/build on macOS, Windows, Linux). Update `CLAUDE.md` commands and `filemap.md`.
Verify: `just check` passes locally; `pnpm --filter desktop tauri build --debug` produces an app that shows the ping result.

**2. Git process runner** ☑ — Careful — deps: 1
`mv-git`: locate git and git-lfs (bundled path, then system), run with a scrubbed environment (`GIT_TERMINAL_PROMPT=0`, fixed locale), stream stdout/stderr, cancel, time out, typed errors, parse `--porcelain=v2 -z` status, log, and LFS progress (`GIT_LFS_PROGRESS`). No shell interpolation; arguments as arrays only.
Verify: integration tests against temp repos and a local bare remote, including paths with spaces and non-ASCII names.

**3. Validation: git hosts and LFS** ☐ — Heavy — deps: 2
Scripted experiments, results in `docs/validation/hosts.md`: lock API on Forgejo (local container), GitHub, GitLab; `lfs.locksverify` blocking; `lockable` read-only behaviour on each OS; textconv receiving pointer vs audio; skip-smudge clone plus `fetchinclude` per folder. State what in SPEC must change.
Verify: the scripts are in `tools/validation/` and re-runnable.

**4. Validation: DAW save behaviour** ☑ tool and protocol built (`tools/fsprobe`, `docs/validation/RUN_FL_STUDIO.md`); FL Studio results pending a human run — Heavy — deps: 1
A small watcher probe tool that logs filesystem events with sizes and timestamps, plus a written test protocol a human runs in FL Studio (first), Ableton, Reaper, Logic: save, save-as, autosave, backup files, read-only project file, file handles held open. Results template in `docs/validation/daws.md`. The worker builds the tool and protocol; a human with the DAWs fills in results.
Verify: probe tool runs on macOS and Windows and records rename-based saves correctly in an automated test.

**5. Bundled git strategy** ☐ — Medium — deps: 2
Script that downloads pinned git and git-lfs builds per target (dugite-native releases), verifies checksums, and places them as Tauri sidecars; runtime lookup in `mv-git`; licenses included in the bundle.
Verify: debug build on macOS runs `git --version` and `git lfs version` from the bundle with system git hidden from PATH.

## Phase 1 — Versioning core (v0.1)

**6. Domain model and policies** ☐ — Careful — deps: 1
`mv-core`: project, version, lock, milestone, alt version, conflict copy types; traits `GitBackend`, `Clock`; lock policy, conflict-copy naming, layout and template rules as pure functions with fakes.
Verify: unit tests cover every policy branch in SPEC §6–7.

**7. Job system and event bus** ☐ — Careful — deps: 6
`mv-app`: cancellable jobs with progress, one job per repo for mutating operations, event stream consumed by Tauri and later by `mv-remote`.
Verify: concurrency tests (cancel mid-job, two mutating jobs queued, panic in a job).

**8. Project create/join and templates** ☐ — Medium — deps: 2, 6
Templates for `.gitattributes`, `.gitignore`, `.musiverion/project.toml` per DAW (FL Studio complete; others from the report, marked unverified). Create, join with metadata-only clone, per-folder audio fetch.
Verify: integration tests against a local bare remote with LFS.

**9. Host connection and credentials** ☐ — Careful — deps: 2
Token storage in the OS keychain, bundled credential helper, host capability probe (LFS, locks), clear errors for auth and quota failures.
Verify: tests against a local Forgejo container; no secret ever in logs or process arguments.

**10. LFS lock client and auto-lock** ☐ — Careful — deps: 3, 6, 9
HTTP client for the locks API, lock state cache with refresh, auto-lock on first modification, release after publish, force-unlock for permitted users, behaviour when the host lacks locks.
Verify: two-client integration test where the second client is refused.

**11. Watcher and save detection** ☐ — Heavy — deps: 4, 6
`mv-watch`: `notify`-based watcher that treats write-then-rename as a save, debounces until size and mtime settle, ignores template-ignored paths, detects open handles before checkout.
Verify: automated tests replaying the event traces captured in task 4.

**12. Save version, Publish, Get latest** ☐ — Careful — deps: 7, 8, 10, 11
The three core operations end to end, including the pre-publish lock check and conflict-copy path.
Verify: scenario tests with two simulated collaborators, including offline work and forced unlock.

**13. History and restore** ☐ — Medium — deps: 12
Timeline query (paged), changed files per version, restore a file or the whole project as a new version, local prune of old audio.
Verify: tests on a generated 1 000-version repo; timeline page loads under 100 ms.

**14. Desktop UI for Phase 1** ☐ — Careful — deps: 12, 13 — design fixed by `docs/design/`
Project list, status bar with locks, Save version dialog, Publish/Get latest with progress, history timeline, conflict notice. Implements the approved mockups; no new design decisions by the worker.
Verify: orchestrator reviews the running app against the mockups; Playwright smoke test over the mocked bridge.

**15. CLI parity** ☐ — Light — deps: 12, 13
`mv` subcommands for every `mv-app` operation with `--json` output.
Verify: snapshot tests.

## Later phases (to be broken down)
- Phase 2: decode and cache → metadata/loudness/PCM hash → peaks → diff tiers → A/B player UI → textconv driver.
- Phase 3: `.flp` event parser → project model → semantic diff → plugin manifest → lint.
- Phase 4: remote API and pairing → preview streaming → web companion → Tauri mobile packaging → security review.
- Phase 5: meta branch store → comments UI → approvals and tags → alt versions.
- Phase 6: signing and updates → docs → performance pass → format stability.
