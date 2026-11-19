# Musiverion — Specification

Open-source desktop app (plus a phone companion) that gives a team of musicians version history, locking, and audio-aware diffs for studio projects, on top of plain git and Git LFS. Background and evidence: `reports/Git LFS audio versioning app.md`.

## 1. Goals

1. **Version history without git vocabulary.** Save a version, publish it, get the latest, listen to or restore any earlier version.
2. **Lock-first collaboration.** DAW project files are single-writer documents. The app checks them out (locks) automatically and never content-merges them.
3. **Audio-native diff.** Compare any two versions of an audio file: metadata, loudness, "is the audio actually different", changed time ranges, spectral difference, loudness-matched A/B playback.
4. **Readable FL Studio project diffs.** "Channel 'Kick' volume 78% → 100%" rather than "binary file changed". Other DAWs get locking, ignore templates and audio diff first; semantic diffs later.
5. **Team-owned data.** Everything, including comments and approvals, lives in an ordinary git repository on any git + LFS host. No Musiverion server is required.
6. **Phone companion.** Paired with the desktop app: project status, save version / publish, lock and unlock, A/B listening, comments, approvals.

## 2. Non-goals

- Real-time co-editing; automatic merging of project files or audio.
- A hosted service, accounts, or billing. A custom server is not part of v1.
- DAW transport control (play/stop/record) from the phone.
- Custom chunked storage (Xet-style). Revisit only with usage data.
- Storing plugins or commercial sample libraries in repos.

## 3. Stack

| Layer | Choice |
|---|---|
| Core engine | Rust stable, edition 2024, Cargo workspace; `tokio`, `serde`, `thiserror`, `tracing` |
| Git | Bundled `git` + `git-lfs` binaries run as child processes (Tauri sidecars). No libgit2/gitoxide for network or LFS work |
| Audio | Pure Rust: `symphonia` (decode WAV/FLAC/MP3/AIFF), `ebur128` (LUFS/true peak), `rustfft` (alignment, spectrograms) |
| Desktop shell | Tauri v2 (macOS, Windows, Linux) |
| Frontend | React + TypeScript + Vite, pnpm workspace; wavesurfer.js fed precomputed peaks |
| Typed bridge | `tauri-specta` generates TypeScript bindings from Rust commands and events |
| Companion transport | `axum` HTTP + WebSocket server embedded in the desktop app, `mdns-sd` discovery, `rustls` |
| Mobile | Stage 1: web companion served by the desktop app. Stage 2: Tauri v2 iOS/Android app reusing the same UI package |
| License | MIT OR Apache-2.0 (git and git-lfs ship as separate executables under their own licenses) |

Exact crate and npm versions are the latest stable at scaffold time and are pinned by lockfiles.

## 4. Architecture

```
apps/desktop (Tauri window + React UI) ──┐
apps/mobile  (Tauri mobile, stage 2)     │ typed commands/events
packages/ui  (shared React components)   │
                                         ▼
crates/mv-app     application service: one API used by Tauri, the companion server and the CLI
   ├─ mv-core     domain model and studio-language operations
   ├─ mv-git      git/git-lfs process runner, output parsing, LFS lock HTTP client, credentials
   ├─ mv-audio    decode, metadata, PCM hash, peaks, loudness, null test, spectrogram
   ├─ mv-daw      DAW project parsers and semantic diff (FL Studio first), plugin manifest
   ├─ mv-watch    file watcher, save debouncing, open-file detection
   ├─ mv-meta     comments, approvals, team notes stored in the repo's meta branch
   └─ mv-remote   companion server: pairing, auth, HTTP/WebSocket API, preview streaming
crates/mv-cli     headless CLI over mv-app; also the git textconv/diff driver
```

Rules:

- **`mv-core` has no I/O.** It defines types, traits (`GitBackend`, `AudioAnalyzer`, `ProjectParser`, `Clock`) and the rules (lock policy, conflict-copy policy, naming). Everything else implements those traits, so the rules are unit-testable with fakes.
- **One application API.** The Tauri bridge, the companion server and the CLI are thin adapters over `mv-app`. No business logic in `src-tauri` or in the frontend.
- **Long work is a job.** Publish, fetch, analysis and diff run as cancellable jobs that emit progress events; the UI never blocks on a command.
- **Everything derived is cached by content.** Peaks, loudness, PCM hash, previews keyed by LFS OID (SHA-256); pairwise diffs keyed by `(oidA, oidB, paramsVersion)`; stored in the OS cache directory, never in the repo.

## 5. Data model

**In the repository (travels with the project):**

- Working tree in the fixed layout: `Project/`, `Audio/Recordings/` (write-once takes), `Audio/Edits/`, `Stems/v{N}/`, `Bounces/`, `Masters/`, `Docs/`.
- `.gitattributes` / `.gitignore` generated from per-DAW templates (LFS tracking, `lockable` on project files, `merge=binary`).
- `.musiverion/project.toml` — schema version, DAW, layout, lock policy.
- `Docs/plugins.json` — generated manifest of plugins the project needs.
- Version = git commit with a required note. Milestone / Approved = annotated tag (`milestone/*`, `approved/*`). Alt version = short-lived branch.
- **Meta branch** `musiverion-meta` (orphan branch, never checked out into the working tree): one JSON file per comment or approval record, append-only, so concurrent writers never conflict. Comments reference a commit, a file path and an optional time range.

**On the machine only:** app settings, known projects, host credentials (OS keychain), paired devices and their tokens, the OID-keyed analysis cache.

## 6. Studio language

| Shown to the user | Underneath |
|---|---|
| Save version (note required) | commit |
| Publish | commit if needed → pull → push → release locks |
| Get latest | fetch + fast-forward |
| Checked out by Alex | LFS lock |
| Alt version | branch |
| Make this the main version | pick one side; never a content merge |
| Milestone / Approved | annotated tag |
| Conflict copy | local file saved beside the canonical one, remote kept |

Git verbs never appear in the UI. They may appear in logs and the CLI.

## 7. Key flows

1. **Create or join a project.** Pick a folder and DAW → app writes templates, initializes LFS, connects to a host URL, checks lock support. Joining fetches metadata only, then audio per song or folder on demand.
2. **Work.** Watcher sees the DAW save the project file (including save-by-rename), waits until size and mtime settle, takes the lock if not held, and marks "unsaved changes since last version".
3. **Save version / Publish.** Portability lint runs first (media outside the repo, missing collected samples). Publish refuses to proceed if someone else holds a lock on a changed file.
4. **Conflict.** After offline work or a forced unlock: remote stays canonical, local file is saved as `Name (Alex's conflict copy YYYY-MM-DD).ext`, a comment thread is opened.
5. **History.** Timeline of versions with notes, authors, milestone badges. Per version: changed files, FL Studio change list, listen, restore.
6. **Compare audio.** Tiers, stopping at the first that answers: OID equal → metadata and loudness → decoded-PCM hash → per-second hashes → aligned, gain-matched null test with changed regions → spectrogram difference. A/B player switches sources on one clock with loudness matching.
7. **Companion.** Desktop shows a QR code → phone opens the companion → pairing token stored on both sides → phone shows status, locks, versions, plays previews, triggers save version / publish, comments and approves.

## 8. External integrations

- **Git hosts:** any host with git over HTTPS and the Git LFS batch API. Forgejo on S3-compatible storage is the documented reference setup; GitHub, GitLab, Gitea and Bitbucket are tested targets. Lock support is detected per host; without it the app warns and falls back to advisory-only behaviour.
- **Auth:** personal access token or OAuth device flow, stored in the OS keychain and handed to git through a bundled credential helper. SSH as an advanced option.
- **DAWs:** no in-DAW plugins in v1. Integration is the filesystem.

## 9. Security

- Companion server binds only while pairing or with paired devices present; LAN only by default; TLS with a certificate pinned at pairing; per-device revocable tokens; every remote action is a named `mv-app` operation, never a shell or path API.
- Tauri capabilities are minimal; git sidecars run with an allowlisted argument set.
- Preview streams are derived audio, not raw multitracks.

## 10. Unverified assumptions (resolved in Phase 0)

1. How each target DAW behaves with a read-only project file, and whether it saves atomically.
2. Lock API behaviour on Forgejo, GitHub, GitLab, Gitea, Bitbucket, and `lfs.locksverify` blocking.
3. Whether a git textconv driver receives real audio or the LFS pointer.
4. FL Studio 25 `.flp` parseability (sources disagree).
5. Tauri v2 mobile maturity for background audio and local-network access on iOS.

## 11. Definition of done — MVP (end of Phase 2)

A team of three on macOS and Windows, using a Forgejo or GitHub repo, can: create and join a project; save, publish and get versions of an FL Studio project with audio; never overwrite each other thanks to automatic locks; see history and restore a version; and compare two bounces with changed regions and loudness-matched A/B — without typing a git command. Signed installers build in CI for macOS, Windows and Linux.
