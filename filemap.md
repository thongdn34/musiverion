# File map

Entries marked (planned) are created by the scaffold task.

- `CLAUDE.md` — agent guide: commands and conventions
- `filemap.md` — this index; update when files are added or moved
- `docs/SPEC.md` — goals, architecture, data model, flows, MVP definition
- `docs/ROADMAP.md` — long-term phases, exit criteria, risks
- `docs/PLAN.md` — ordered tasks with tiers, dependencies, verification
- `docs/design/DESIGN_SYSTEM.md` — Blue Glass tokens, components, accessibility rules
- `docs/design/mockups.html` — screen mockups the UI tasks implement
- `docs/validation/` — (planned) results of Phase 0 experiments
- `reports/` — research report the plan is based on; read-only
- `research_notes/` — notes behind the report; read-only
- `crates/mv-core/` — (planned) domain types, traits, policies; no I/O
- `crates/mv-git/` — (planned) git/git-lfs process runner, parsers, LFS lock client, credentials
- `crates/mv-audio/` — (planned) decode, metadata, hashes, peaks, loudness, null test, spectrogram
- `crates/mv-daw/` — (planned) DAW project parsers and semantic diff; FL Studio first
- `crates/mv-watch/` — (planned) file watcher and save detection
- `crates/mv-meta/` — (planned) comments and approvals on the meta branch
- `crates/mv-remote/` — (planned) companion server: pairing, auth, HTTP/WebSocket
- `crates/mv-app/` — (planned) application service, jobs, events; the only API adapters call
- `crates/mv-cli/` — (planned) `mv` command-line tool and git diff driver
- `apps/desktop/` — (planned) Tauri v2 desktop app; `src/` React UI, `src-tauri/` bridge
- `apps/mobile/` — (planned, Phase 4) Tauri v2 iOS/Android companion
- `packages/ui/` — (planned) React components shared by desktop and companion
- `tools/validation/` — (planned) re-runnable Phase 0 experiment scripts
