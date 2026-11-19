# Musiverion — Long-term roadmap

Durations are judgment estimates for 2–3 part-time contributors, not commitments. Each phase ends with a tagged release and its exit criteria met. Detailed tasks for the current phases are in `PLAN.md`.

| Phase | Release | Theme | Estimate |
|---|---|---|---|
| 0 | v0.0.x | Foundations and validation | 4–6 weeks |
| 1 | v0.1 | Versioning core: save, publish, lock, history | 3 months |
| 2 | v0.2 (MVP) | Audio engine and audio diff | 2–3 months |
| 3 | v0.3 | FL Studio project diff and portability | 2 months |
| 4 | v0.4 | Phone companion | 2–3 months |
| 5 | v0.5 | Review: comments, approvals, alt versions | 2 months |
| 6 | v1.0 | Distribution, stability, community | 2–3 months |
| 7 | v1.x | More DAWs, off-LAN remote, optional server pack | ongoing |

## Phase 0 — Foundations and validation
Workspace, CI on three OSes, git/git-lfs runner, and the experiments that the design depends on.
- Scaffold: Cargo workspace, Tauri v2 shell, pnpm workspace, lint/test/CI.
- `mv-git` process runner with progress parsing; bundled-binary strategy per OS.
- Validation reports in `docs/validation/`: DAW read-only and atomic-save behaviour (FL Studio first, then Ableton, Reaper, Logic); host lock-API matrix; textconv-with-LFS behaviour; FL 25 parse check.
- **Exit:** CI green on macOS/Windows/Linux; each assumption in SPEC §10 has a written answer; any answer that breaks the design is reflected in SPEC before Phase 1 starts.

## Phase 1 — Versioning core (v0.1)
- `mv-core` domain and policies; `mv-app` job system and event bus.
- Project create/join with templates; host connection and credential helper.
- Watcher with save detection; auto-lock; Save version, Publish, Get latest.
- History timeline, restore, conflict copies; per-folder selective fetch and local prune.
- Desktop UI for all of the above; `mv-cli` parity.
- **Exit:** three people complete a week of real FL Studio work on one repo with no overwritten project file and no terminal use.

## Phase 2 — Audio engine (v0.2, MVP)
- Decode, metadata, decoded-PCM hash, multi-resolution peaks, LUFS/true peak; OID-keyed cache.
- Diff tiers 0–5: hashes, per-second change map, aligned gain-matched null test, spectrogram difference.
- Waveform history view, A/B player with loudness matching, changed-region overlay.
- `mv-cli textconv` as a git diff driver.
- **Exit:** SPEC §11 definition of done; null test verified against a fixture corpus (gain change, offset, tag-only edit, punch-in, format conversion, MP3 padding).

## Phase 3 — FL Studio diff and portability (v0.3)
- Rust `.flp` event parser (FL 12–25 corpus), project model, semantic diff with collapsed repeats and hashed plugin state.
- Plugin manifest (`Docs/plugins.json`), missing-plugin warning, media-outside-repo lint.
- `ProjectParser` trait stabilized so other DAW parsers are separate crates.
- **Exit:** parser round-trips the corpus without panics (fuzzed); diffs match hand-checked expectations on 20 real edits.

## Phase 4 — Phone companion (v0.4)
- `mv-remote`: QR pairing, pinned TLS, device tokens, WebSocket event stream, preview transcode and range streaming.
- Stage 1: web companion served by the desktop app (works on any phone, nothing to install).
- Stage 2: Tauri v2 iOS/Android app with the same UI package: saved pairing, background audio, notifications.
- **Exit:** from a phone on the same network: see status and locks, save version, publish, A/B two versions, comment. Security review of the remote surface done.

## Phase 5 — Review and collaboration (v0.5)
- `mv-meta`: comments with time ranges, approvals, milestones on the meta branch.
- Alt versions with "outdated" warnings; "make this the main version".
- Roles mapped to host permissions and protected tags; notifications.
- **Exit:** a mix goes through comment → revise → approve entirely in the app, and the record survives a fresh clone on another host.

## Phase 6 — v1.0
- Signed and notarized installers, auto-update, crash reporting (opt-in), accessibility pass, localization framework.
- User docs, reference Forgejo + object storage setup guide, contributor guide, parser plugin guide.
- Performance budget: 100 GB project, 10 000 versions, 2-hour files.
- **Exit:** v1.0 with a stability promise on the repo format (`.musiverion/` schema and meta branch).

## Phase 7 — After 1.0 (ordered by expected demand; re-rank with usage data)
- Ableton `.als` and Reaper `.rpp` semantic diffs; DAWproject sidecar diffs; lock/ignore templates for Logic, Pro Tools, Cubase.
- Off-LAN companion through an optional self-hostable relay with end-to-end encryption.
- Optional server pack: Forgejo + object storage + a worker that precomputes analysis and serves guest review links.
- In-DAW actions (Reaper scripts, Ableton remote scripts).
- Storage: tiering advice, unreferenced-object cleanup tools, chunked transfer agent if stem re-renders dominate.

## Deferred indefinitely
Real-time co-editing; content merges of projects or audio; DAW transport control; hosted SaaS.

## Standing risks
| Risk | Mitigation |
|---|---|
| DAW mishandles read-only project files | Phase 0 test; fall back to advisory locks without the read-only flag per DAW |
| Host lacks lock API | Capability detection; clear warning; documented reference host |
| FL 25 format obfuscation | Corpus tests per FL version; degrade to "project changed" plus audio diff |
| Tauri mobile gaps on iOS | Web companion ships first and remains supported |
| Bundling git on three OSes | Copy dugite-native's approach; system git as dev fallback |
| Contributor bandwidth | Strict crate boundaries, fakes for every trait, small well-scoped issues |
