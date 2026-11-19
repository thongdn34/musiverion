# App Architecture and Team Workflow for a Git + Git LFS Audio Versioning App (desktop + web)

Scope note: research done Nov 2026 with a limited tool budget (about 16 calls). Primary sources fetched: Anchorpoint file-locking docs, the dugite-native README, the Git LFS File-Locking wiki, the Tauri v2 sidecar docs, plus search-level evidence from Giftless, rudolfs, Forgejo storage docs, GitLab cleanup docs, peaks.js and wavesurfer.js docs. Statements marked "(prior knowledge, not re-verified)" in the Inferences sections come from background knowledge and were not checked this session. Every item under Cited Findings has a source.

---

## 1. Client architecture: desktop framework, embedding git + LFS, file watching, DAWs holding files open

### Takeaway
Shipping apps bundle the real `git` and `git-lfs` binaries and shell out to them. Libraries are not used for LFS because libgit2 and gitoxide lack working LFS. GitHub Desktop (Electron + dugite) proves this pattern. Tauri's `externalBin` sidecar mechanism is the equivalent for a Rust/Tauri client, and GitButler shows Tauri + Rust works for a production git client. File watching has to handle atomic save-by-rename and should debounce until the DAW finishes writing.

### Cited Findings
- **GitHub Desktop architecture:** the git layer is a TypeScript wrapper that shells out to git commands through the `dugite` library, which embeds a portable Git distribution. — [mintlify mirror of GitHub Desktop architecture docs](https://www.mintlify.com/livrasand/desktop/architecture/git-operations)
- **dugite-native contents:** dugite-native is the build toolchain for "a portable, cross-platform Git" "optimized for scripted usage in applications". Its README lists these customizations: **Git-LFS, Git Credential Manager, and a certificate bundle for Linux consumers**. It does not link to system libraries, uses symlinks to cut size, and drops the Perl runtime, the OpenSSL dependency and the Tcl/Tk GUI. It says it is "not intended to be installed by end users". Git is pinned as a submodule. — [desktop/dugite-native README](https://github.com/desktop/dugite-native)
- **Conflict:** an older mirror of the dugite-native README says the project removes Git-LFS, while the current README lists Git-LFS as included. The current primary README (above) takes precedence. — [older mirror](https://gh.r-mj.com/desktop/dugite-native) vs [current README](https://github.com/desktop/dugite-native)
- **GitHub Desktop docs:** Git LFS is installed when you install GitHub Desktop. — [GitHub Docs: About Git LFS and GitHub Desktop](https://docs.github.com/en/enterprise-server@3.22/desktop/configuring-and-customizing-github-desktop/about-git-large-file-storage-and-github-desktop)
- **libgit2 has no LFS support.** A NixOS PR notes "libgit2 does not have support for LFS". gitui's maintainers say it is not supported out of the box in upstream libgit2. — [NixOS/nix PR #8186](https://github.meowingcats01.workers.dev/NixOS/nix/pull/8186); [gitui discussion #1089](https://github.hscsec.cn/extrawurst/gitui/discussions/1089)
- **libgit2 filter API workaround:** libgit2 exposes a low-level filter API, and that is how third parties build LFS on top of it. Community crates such as `git2-lfs` add LFS on a forked git2-rs with filter support, because git2/libgit2 does not run filters automatically. — [DVC blog](https://dvc.org/blog/dvc-git-lfs/); [ejc3/git2-lfs](https://github.com/ejc3/git2-lfs); [gram-ax/git2-lfs](https://github.com/gram-ax/git2-lfs)
- **gitoxide:** the gitoxide `git-lfs` crate is described as a WIP crate at 0% coverage. Cloning LFS repos needs git-lfs installed locally, or the checkout fails. — [docs.rs git-lfs 0.0.0](https://docs.rs/crate/git-lfs/0.0.0); [gitoxide fork README](https://github.com/kdarkhan/gitoxide)
- **GitButler stack:** a Tauri desktop app with a Svelte/TypeScript UI and a Rust backend. The `but` CLI shares the same Rust backend engine. — [GitButler repo mirror](https://github.com/Byron/gitbutler); [best-of-web summary](https://best-of-web.builder.io/library/gitbutlerapp/gitbutler)
- **GitButler internals (low confidence):** an aggregator claims GitButler uses git2 and gitoxide together, with gix on hot paths such as diff and index traversal, and SQLite via rusqlite. This is unverified against the official repo. — [openapps.pro](https://openapps.pro/apps/gitbutler)
- **Tauri sidecars:** binaries are listed in `bundle.externalBin` in `tauri.conf.json`. Each file needs a `-$TARGET_TRIPLE` suffix, e.g. `binaries/git-lfs-x86_64-unknown-linux-gnu`, and `rustc --print host-tuple` prints the triple. The shell plugin capability must grant `shell:allow-execute` or `shell:allow-spawn` with `"sidecar": true`, and arguments are allowlisted, optionally with regex validators. Spawn from Rust with `app.shell().sidecar("name")`, which streams `CommandEvent::Stdout`, or from JS with `Command.sidecar('binaries/name')`. — [Tauri v2: Embedding External Binaries](https://v2.tauri.app/develop/sidecar/)
- **Anchorpoint architecture:** a desktop app that "handles Git and metadata". Engine plugins (Unreal, Unity) connect to the desktop app. Anchorpoint auto-configures Git LFS by editing `.gitattributes`. — [Unreal forum: Anchorpoint](https://forums.unrealengine.com/t/anchorpoint-anchorpoint-version-control-with-git/2567882); [Anchorpoint blog: Push and pull with Git LFS](https://www.anchorpoint.app/blog/push-and-pull-files-with-git-lfs)
- **Atomic saves:** many apps save by writing a temp file and renaming it over the original. A watcher that only listens for "modify" events can miss these saves, because the path is the same but the filesystem object behind it is new. On macOS the advice is to watch rename/delete events too and re-attach the watcher. — [Eclectic Light: How apps save changed files](https://eclecticlight.co/2020/04/17/how-apps-save-changed-files/); [Zimaspace: file watchers miss save-and-rename](https://shop.zimaspace.com/blogs/tech-ai-hub/local-ai-file-watchers-miss-rapid-save-rename-events)
- **Ableton save behavior:** until a Live project is saved, recordings sit in a Temporary Folder. On save they are copied into the Project Folder. — [Ableton: External Storage and Backup](https://help.ableton.com/hc/en-us/articles/5809449695900)
- **Ableton and file handles on Windows:** Ableton's Windows "error when closing the file" help article recommends "Save Live Set As..." with a new name when cloud-sync tools interfere. — [Ableton help](https://help.ableton.com/hc/en-us/articles/209071949--There-was-an-error-when-closing-the-file-Win-)

### Inferences
- **Recommended client stack:** Tauri v2 (Rust core, web UI shared with the web app) with bundled `git` + `git-lfs` sidecars per target triple. Use gitoxide (gix) or git2 only for fast read paths such as status, log, tree walks and diff of text DAW files. Do every network/LFS operation (clone, fetch, push, `lfs pull/push/lock`) through the CLI binaries. This is the GitHub Desktop model with a lighter runtime.
- **Electron + dugite** is the lowest-risk path if the team is JS-heavy: dugite ships git + LFS + GCM on every platform out of the box.
- **JUCE/Qt:** only worth it if the client must host audio plugins or do in-app low-latency playback. The review/preview player can run in the webview.
- **isomorphic-git (prior knowledge, not re-verified):** no LFS smudge/clean. It is acceptable for read-only browsing in the web app.
- **go-git (prior knowledge, not re-verified):** also lacks native LFS.
- **Credential handling:** bundle Git Credential Manager, as dugite does, or implement a git credential helper that hands out the app's OAuth token.
- **Watcher design:** use the Rust `notify` crate (FSEvents/inotify/ReadDirectoryChangesW).
  - Treat create+rename as a "save".
  - Debounce 2–5 s after the last event, then check the file is stable: size and mtime unchanged across two polls, and on Windows an exclusive-open attempt succeeds.
  - Ignore DAW temp/backup folders (Ableton `Backup/`, Logic autosaves, Reaper `.rpp-bak`, Pro Tools `Session File Backups/`).
  - Never auto-commit mid-recording. Ableton, for example, holds new recordings in a temp folder until save.
- **Windows file locks:** do not run checkout or pull on a file the DAW holds open. Detect the open handle (Restart Manager API on Windows, `lsof` on macOS) and queue the operation behind a "close the session in your DAW first" prompt.
- **Background sync:** a background `git fetch` + `git lfs fetch --recent` on an interval keeps "what's new" badges current without touching the working tree. Only an explicit "Get latest" does checkout/merge.

### Gaps
- I could not confirm from primary sources how Anchorpoint and Fork bundle git (embedded vs system git). Anchorpoint's docs pages fetched here don't mention it.
- GitButler's exact gix vs git2 split and whether it shells out to the git CLI for push/fetch are unverified (aggregator only).
- I found no authoritative documentation on whether Logic, Pro Tools, Ableton, FL Studio or Reaper write project files atomically. This must be tested empirically per DAW.

---

## 2. Server side: git hosting, LFS object storage, LFS server implementations, auth, locking API, webhooks

### Takeaway
Forgejo/Gitea with S3-compatible LFS storage (MinIO, R2, Garage) is the simplest self-hosted base: git hosting, LFS, locking settings and webhooks come in one Go binary. A standalone LFS server (Giftless, rudolfs) adds value when you want pluggable auth, encryption, or to front GitHub-hosted repos with your own object storage. Few LFS servers implement the locking API, so the server choice must be checked against locking first.

### Cited Findings
- **Forgejo/Gitea storage:** `STORAGE_TYPE = minio` in `[storage]` switches every subsystem to S3-compatible storage, and LFS objects go under an `lfs/` prefix. An `[lfs]` section with `MINIO_BASE_PATH` stores LFS elsewhere. `MINIO_BASE_PATH` must not be set in `[storage]`.
  - Keys: `MINIO_ENDPOINT`, `MINIO_ACCESS_KEY_ID`, `MINIO_SECRET_ACCESS_KEY`, `MINIO_BUCKET`, `MINIO_BUCKET_LOOKUP = auto`, `MINIO_LOCATION`, `MINIO_USE_SSL`.
  - The backend uses no MinIO-specific features, and the docs examples also use Garage.
  - — [Forgejo docs: Storage settings (v8)](https://forgejo.org/docs/v8.0/admin/storage); [Forgejo docs next](https://forgejo.org/docs/next/admin/storage/)
- **Gitea/Forgejo LFS locking:** the source defines lock-related LFS settings, e.g. `LFS_LOCKS_PAGING_NUM` for lock list page size, which confirms lock-API support in the codebase. — [Forgejo mirror: modules/setting/lfs.go](https://git.fb1.uni-bremen.de/davrot/forgejo_backup/src/commit/ff18c3ba65b230f30c07e99277730a217576ee99/modules/setting/lfs.go)
- **Giftless:** an MIT-licensed Python Git LFS server built for pluggable storage backends, transfer methods and auth methods.
  - Storage: local, Google Cloud Storage, Azure Blob and Amazon S3 (`giftless.storage.amazon_s3:AmazonS3Storage`, with bucket name and path prefix).
  - Giftless is typically deployed as the LFS API server while storage is offloaded to cloud storage, i.e. presigned direct upload/download.
  - Rubin Observatory runs it in production on GCS through the S3 protocol.
  - — [Giftless docs: storage backends](https://giftless.readthedocs.io/en/v0.5.0/storage-backends.html); [Giftless stable docs](https://giftless.datopian.com/en/stable/); [datopian/giftless](https://github.com/datopian/giftless); [Rubin Phalanx giftless](https://phalanx.lsst.io/v/DM-39636/applications/giftless/index.html)
- **Giftless client caveat (conflict):** one page says Giftless "requires a specialized Git LFS client" for some features (multipart transfer, via `giftless-client`). The basic transfer mode works with standard git-lfs. — [datahub.io Giftless page](https://datahub.io/docs/DMS/giftless)
- **rudolfs:** a Rust LFS server with an S3 backend and an optional local disk cache (`--max-cache-size`, default 50 GiB in 0.3.8). It encrypts objects in both the cache and permanent storage with a 32-byte `--key` / `RUDOLFS_KEY`.
  - If the key changes, or encryption is disabled, all existing objects become garbage and fail SHA-256 verification.
  - The CDN mode is incompatible with encryption and disables the disk cache.
  - — [rudolfs README (docs.rs)](https://docs.rs/crate/rudolfs/0.3.6/source/README.md); [rudolfs main.rs 0.3.8](https://docs.rs/crate/rudolfs/latest/source/src/main.rs)
- **rudolfs cipher:** the fork `believer-oss/lfs-rs` states objects are encrypted with XChaCha20. — [believer-oss/lfs-rs](https://github.com/believer-oss/lfs-rs)
- **Other S3-backed LFS servers** on the official implementations list include `msangals/git-lfs-s3-proxy`, which implements the Batch API on S3. — [git-lfs wiki: Implementations](https://github.com/github/git-lfs/wiki/Implementations)
- **Locking API support is rare:** the Git LFS File-Locking wiki says "few LFS servers implement the API". Unlock-others permissions vary by server, and some require admin. — [git-lfs wiki: File Locking](https://github.com/git-lfs/git-lfs/wiki/File-Locking)
- **Anchorpoint avoids LFS locking entirely.** It uses its own cloud metadata server for locks and warns not to combine the two. Its metadata cloud "never touches your production data". — [Anchorpoint docs: File locking](https://docs.anchorpoint.app/git/file-locking/); [Unreal forum post](https://forums.unrealengine.com/t/anchorpoint-anchorpoint-version-control-with-git/2567882)

### Inferences
- **Recommended MVP server:** Forgejo (actively maintained, GPL fork of Gitea) with `[lfs]` on Cloudflare R2 or MinIO, behind your own API service that:
  1. owns users/projects/roles and issues Forgejo tokens via the admin API,
  2. receives Forgejo push webhooks to enqueue processing jobs,
  3. stores comments, approvals and lock UX metadata in Postgres.
- **Server feature support (prior knowledge, not re-verified):** GitHub and GitLab both implement the LFS locking API, and Gitea/Forgejo do too (consistent with the settings above). The minimal reference `lfs-test-server` is not production-grade. Giftless locking support is unverified (see Gaps).
- **Auth:** HTTPS + token or OAuth is far friendlier for musicians than SSH keys. Use OAuth device flow or PKCE in the desktop app, then feed the token to git through a custom credential helper. Keep SSH as an "advanced" option.
- **Locking control:** run your own lock service, as Anchorpoint does, if you want lock UX beyond the LFS API: lock notifications, auto-release on push, cross-branch "outdated" warnings. Otherwise use LFS locks on Forgejo/GitLab/GitHub for interoperability with the plain git CLI.
- **Hosting providers (prior knowledge, not re-verified):** GitHub works as a host but LFS bandwidth/storage pricing and a 2 GB per-file LFS cap on most plans make it costly for multitrack audio. Verify current limits.

### Gaps
- I did not confirm Giftless locking-API support or Forgejo's exact lock endpoints and permissions in this session.
- I did not verify current GitHub LFS quotas and pricing (2026).
- Webhook payload details for Forgejo (whether push events include LFS OIDs) were not checked. Likely the job worker has to diff trees and parse pointer files itself.

---

## 3. Server-side processing pipeline: peaks, previews, loudness, spectrograms, diffs, queues, OID caching

### Takeaway
Run a push-webhook-driven job pipeline keyed by the LFS SHA-256 OID. Because LFS content is content-addressed, every derived artifact (peaks, preview, loudness, spectrogram) can be computed once per OID, cached forever, and shared across branches, versions and repos. BBC's audiowaveform + peaks.js are the established tools for precomputed peaks.

### Cited Findings
- **peaks.js input:** peaks.js consumes waveform data generated by BBC `audiowaveform`, in binary `.dat` (preferred, smaller) or JSON. It does not support 16-bit waveform files, so generate 8-bit. Development has moved to Codeberg (codeberg.org/chrisn/peaks.js). — [bbc/peaks.js README](https://github.com/bbc/peaks.js)
- **waveform-data.js** (BBC) reads audiowaveform output in the browser and can resample it for zoom levels. — [bbc/waveform-data.js](https://github.com/bbc/waveform-data.js)
- **wavesurfer.js precomputed peaks:** wavesurfer.js accepts `peaks` + `duration`, which skips fetching and decoding the audio up front. Playback can start before the waveform is drawn. `normalize: true` rescales the peaks.
  - Conflict: the docs pages disagree on the expected range (-1..1 vs 0..1). Use `normalize: true` or check the installed version.
  - — [wavesurfer.js: Pre-decoded peaks](https://wavesurfer.xyz/docs/peaks/); [wavesurfer.js: Performance & memory](https://wavesurfer.xyz/docs/performance/); [wavesurfer.js FAQ](https://wavesurfer.xyz/faq)
- **audiowaveform vs wavesurfer format:** audiowaveform output is not directly wavesurfer-compatible. The `wavesurfer-peakbuilder` package wraps audiowaveform to produce wavesurfer arrays. — [wavesurfer-peakbuilder](https://socket.dev/npm/package/wavesurfer-peakbuilder)
- **Content addressing:** rudolfs verifies objects by SHA-256 on download. That confirms the OID is the SHA-256 of the content and is a stable cache key. — [rudolfs README](https://docs.rs/crate/rudolfs/0.3.6/source/README.md)

### Inferences
- **Pipeline:**
  1. Forgejo push webhook.
  2. An API service enqueues `ProcessCommit(repo, before, after)`.
  3. A worker runs `git diff-tree` between `before` and `after` and reads the LFS pointer files (`oid sha256:…`, `size`) for changed audio paths.
  4. For each OID not yet in the `derived_assets` table, enqueue per-OID jobs that stream the object straight from S3 by OID, never through git:
     - `audiowaveform -i in.wav -o peaks.dat -b 8 -z 256` (plus a coarser zoom),
     - `ffmpeg` transcode to a streaming preview (Opus or AAC in fMP4/HLS, and MP3 128–192k as a fallback),
     - `ffmpeg -af ebur128` or `loudnorm` analysis for integrated LUFS, true peak and LRA, stored as JSON,
     - `sox` or `ffmpeg showspectrumpic` for a spectrogram PNG,
     - `ffprobe` metadata: sample rate, bit depth, channels, duration.
- **Storage:** derived assets live at `derived/{oid}/{kind}-{params-version}.ext`. Include a params version in the key so a changed encoder setting regenerates cleanly.
- **Version diffs:**
  - Audio: compare loudness JSON, duration and sample rate. Optionally compute a null-test/difference RMS or a time-aligned spectral difference between two OIDs, cached by `(oidA, oidB)`.
  - DAW projects: many formats are compressed XML (Ableton `.als` is gzipped XML; Reaper `.rpp` is plain text). Decompress and run a structured diff (tracks added or removed, plugin changes, tempo) for "what changed" summaries.
- **Queue options:** Postgres-backed queues (pg-boss, River for Go, graphile-worker) avoid running extra infrastructure at MVP. Move to Redis/BullMQ, Celery or Temporal later if fan-out grows.
- **Client-side quick preview:** the desktop app can run audiowaveform locally on save, giving instant local waveforms before push.

### Gaps
- I did not fetch the audiowaveform README for current flags or version. The flags above are prior knowledge and need verification.
- I found no open-source "audio diff between versions" tool to cite. This is novel work.
- I did not check the `.als` XML structure or other DAW formats (Logic `.logicx` bundles, Pro Tools `.ptx` is binary/proprietary) in this session.

---

## 4. Web review UI: streaming previews, waveform players, timestamped comments, approvals

### Takeaway
Build on wavesurfer.js (regions/markers plugins) or peaks.js (segments/points) with server-precomputed peaks plus streamed transcoded previews. Timestamped comments and version approvals are app-level data in your own DB, keyed to commit SHA + file path + LFS OID. Frame.io is the UX reference, but I found no open-source audio equivalent to cite.

### Cited Findings
- **peaks.js** is a BBC client-side component to display and interact with audio waveforms. It supports segments and points (markers), which map onto timestamped comments and ranges. — [bbc/peaks.js](https://github.com/bbc/peaks.js)
- **wavesurfer.js** supports pre-decoded peaks, so long files can play and render without full download or decode. It has documented performance and memory guidance for large files. — [wavesurfer.js peaks docs](https://wavesurfer.xyz/docs/peaks/); [wavesurfer.js performance](https://wavesurfer.xyz/docs/performance/)
- **waveform-playlist** (naomiaro) shows multitrack display from waveform-data, which suits stem-level A/B review. — [waveform-playlist waveform-data example](https://naomiaro.github.io/waveform-playlist/examples/waveform-data)
- **Real-time vs async:** existing music collaboration tools split this way. Splice-style tools do save-snapshot versioning ("every save creates a version"; not real-time). Avid Cloud Collaboration does shared Pro Tools sessions in the cloud. — [Soundbridge: version control in music projects](https://soundbridge.io/en/managing-version-control-in-music-projects); [MMR: Avid cloud collaboration](https://mmrmagazine.com/avid-launches-cloud-collaboration-for-pro-tools/)
- **Conflict on Splice Studio status:** one source says Splice Studio shut down in 2023, while 2026 marketing pages describe a current Splice snapshot feature. Treat its status as unverified. — [orphiq: Splice and collaboration platforms](https://orphiq.com/resources/splice-collaboration-platforms); [Soundbridge](https://soundbridge.io/en/managing-version-control-in-music-projects)

### Inferences
- **Data model:**
  - `comment(id, project, commit_sha, path, oid, t_start, t_end?, body, author, resolved)`.
  - `approval(project, ref|tag, commit_sha, role, status, note)`.
  - Comments anchored to an OID can be carried forward to later versions of the same path, shown as "from v3".
- **Player features:**
  - Version A/B toggle with loudness-matched playback, using LUFS from the pipeline to apply gain.
  - Loop region.
  - Keyboard shortcuts.
  - Comment pins on the waveform.
  - "Approve mix" button that creates a server-side annotated tag (e.g. `approved/mix-v3`) through the Forgejo API.
- **Streaming:** serve previews through presigned S3/R2 URLs or a CDN with HTTP range support. HLS is only needed for very long files.
- **Access:** client/artist "review links" can be tokenized guest access to the web UI only, with no git access at all.

### Gaps
- I found no open-source Frame.io-equivalent for audio review to cite as a reference implementation.
- The current wavesurfer.js major version and plugin API (regions/markers) were not fetched.

---

## 5. Workflow design for non-technical musicians

### Takeaway
Anchorpoint shows the winning pattern for artists on git: hide git verbs, auto-configure LFS, and auto-lock binaries on modify, releasing the lock on push. Plan for a lock-first model, since DAW projects and audio are unmergeable. Keep branching minimal: one main line, plus "alt mix/idea" branches that are explicitly lock-aware.

### Cited Findings
- **Anchorpoint auto-locks** binary files once modified. Text auto-locking is optional (Project Settings → Git), and automatic locks are removed when the files are pushed. — [Anchorpoint docs: File locking](https://docs.anchorpoint.app/git/file-locking/)
- **Read-only enforcement:** locked files are write-protected for others. Some tools allow forced overwrite with a warning, Unity ignores read-only without a plugin, and write protection doesn't work on NAS mounts. Users can unlock others' locks, and the unlock is logged in the sidebar. — [Anchorpoint docs: File locking](https://docs.anchorpoint.app/git/file-locking/)
- **Cross-branch conflict prevention:** Anchorpoint can mark files changed on other active branches as "outdated" and lock them on your branch.
  - It requires merge commits, not rebase.
  - The lookback defaults to 7 days (`cross_branch_lookback_days` in `.ap/config.ini`), and branches can be excluded by pattern (e.g. `release/*`).
  - For big restructures, have no one else working at the same time.
  - — [Anchorpoint docs: File locking](https://docs.anchorpoint.app/git/file-locking/)
- **Anchorpoint lock performance (vendor claim):** "lock 1,000 files in under a second" (no independent benchmark). — [Anchorpoint blog: Using file locking in Git](https://www.anchorpoint.app/blog/using-file-locking-in-git)
- **Git LFS native locking:**
  - `git lfs track "*.wav" --lockable` adds `lockable` to `.gitattributes`. A plain `lockable` attribute also works without LFS.
  - Lockable files are made **read-only** locally until locked.
  - On push, git-lfs checks you're not modifying files locked by others. If it cannot verify locks, it warns rather than blocks (`lfs.<url>.locksverify`).
  - — [git-lfs wiki: File Locking](https://github.com/git-lfs/git-lfs/wiki/File-Locking)
- **Anchorpoint productizes this for non-coders:** "Git for artists" positioning, with locking and LFS configuration as paid Team features. — [Anchorpoint: Git for artists](https://anchorpoint.app/features/git-for-artists); [Anchorpoint: alternative to GitHub Desktop](https://www.anchorpoint.app/compare/alternative-to-github-desktop)

### Inferences
**Vocabulary mapping** (UI never shows git verbs):

| Git | Studio language |
|---|---|
| commit | "Save version" / "Snapshot" with a required note ("vocal comp done") |
| push | part of "Publish" / "Sync" (commit + pull --rebase-or-merge + push + release locks) |
| pull/fetch | "Get latest" / "What's new" |
| branch | "Alt version" / "Idea" (e.g. "Alt mix – no strings") |
| merge | "Make this the main version" (for binaries this is a pick-one, never a content merge) |
| tag | "Milestone" / "Approved" (e.g. `approved/master-v1`, `sent/client-2026-11-19`) |
| lock | "I'm working on this" / "Checked out by Alex" |
| revert/checkout old | "Restore this version" / "Listen to v3" |

**Conflict policy:**
- Lock-first for DAW project files and any audio being edited: auto-lock on first modify, like Anchorpoint, or auto-lock when the DAW opens the project (detected through a file-open watcher or a DAW plugin).
- If a conflict still happens (offline work, a forced unlock), never attempt a merge. Keep both:
  - `Song.als` stays as the remote version,
  - the local file is saved as `Song (Alex's conflict copy 2026-11-19).als`, Dropbox-style,
  - a comment thread is opened for the producer to resolve.
- Mark the `.als`, `.logicx` and `.ptx` paths lockable, and use `merge=binary` or a custom merge driver that refuses.

**Branching strategy:** trunk-based.
- `main` is "the song".
- Alt ideas are short-lived branches, which the UI shows as "versions of the project".
- Mix revisions are tags on `main`, not branches.
- Stems and bounces are committed only at milestones to limit LFS growth.

**Roles:**

| Role | Permissions |
|---|---|
| Producer/owner | approve, tag, force-unlock |
| Engineer | write, lock |
| Artist | write in own folders (e.g. `Recordings/Artist/`), comment |
| Client/label | review-only web link, no git |

Enforce roles with Forgejo team permissions plus server-side checks in your API (e.g. only producers can create `approved/*` tags, via protected tags).

**Folder convention:**
```
Song-Title/
  Project/            # DAW project file(s) + DAW-native subfolders (lockable)
  Audio/Recordings/   # raw takes (immutable once committed; never edited in place)
  Audio/Edits/        # comped/edited audio
  Stems/v{N}/         # stem exports per handoff, named {Song}_{Stem}_{BPM}bpm_{Key}_{SR}k{Bit}b_v{N}.wav
  Bounces/            # mix bounces: {Song}_Mix_v{N}_{YYYY-MM-DD}.wav
  Masters/            # approved masters only
  Docs/               # lyrics, notes, plugins.json manifest
```
**Stem handoff rules:**
- Every stem starts at bar 1 / 0:00 and runs the full song length.
- Same sample rate and bit depth throughout.
- Effects printed or dry stated in the name.
- A BPM/key text file.

**`.gitattributes` template:**
```
*.wav  filter=lfs diff=lfs merge=lfs -text lockable
*.aif  filter=lfs diff=lfs merge=lfs -text lockable
*.aiff filter=lfs diff=lfs merge=lfs -text lockable
*.flac filter=lfs diff=lfs merge=lfs -text lockable
*.mp3  filter=lfs diff=lfs merge=lfs -text
*.m4a  filter=lfs diff=lfs merge=lfs -text
*.ogg  filter=lfs diff=lfs merge=lfs -text
*.mid  -text lockable
*.als  filter=lfs diff=lfs merge=lfs -text lockable
*.flp  filter=lfs diff=lfs merge=lfs -text lockable
*.ptx  filter=lfs diff=lfs merge=lfs -text lockable
*.cpr  filter=lfs diff=lfs merge=lfs -text lockable
*.song filter=lfs diff=lfs merge=lfs -text lockable
*.logicx/** filter=lfs diff=lfs merge=lfs -text lockable
# Reaper projects are plain text; keep them in git (diffable) but lockable
*.rpp  text lockable
# One pattern per line; gitattributes has no trailing comments or multi-pattern lines
*.nki  filter=lfs diff=lfs merge=lfs -text
*.fxp  filter=lfs diff=lfs merge=lfs -text
*.fxb  filter=lfs diff=lfs merge=lfs -text
```
Note on Logic: `.logicx` is a macOS package directory. Lock the main project data files inside it.

**`.gitignore` template:**
```
.DS_Store
Thumbs.db
# Ableton
Backup/
*.asd
Ableton Project Info/
# Pro Tools
Session File Backups/
WaveCache.wfm
*.pkf
# Logic
*.logicx/Alternatives/*/Undo Data*
Autosave/
# Reaper
*.rpp-bak
*.reapeaks
# Cubase
*.bak
*.peak
*.csh
# FL Studio
Backup/
# Rendered caches / freeze files
Freeze Files/
Bounced Files/tmp/
```
Verify the Ableton `.asd` analysis files and Pro Tools `WaveCache` patterns per DAW version. They regenerate, so ignoring them is safe in principle.

### Gaps
- I found no user research or published case studies on musicians using git/LFS specifically. The Anchorpoint and game-dev patterns are analogues.
- DAW-specific temp and cache file patterns above are prior knowledge and were not verified in this session.
- I did not verify whether any DAW honors the read-only flag gracefully. Some may fail to save silently or prompt "Save As", which affects lock-UX design.

---

## 6. Sample libraries and plugins

### Takeaway
Keep commercial sample libraries and plugins out of project repos. Track a manifest of required plugins and versions plus a content-hash manifest of used samples. Use a shared LFS "sample pack" repo or server-side dedupe for team-owned samples, which content-addressed LFS gives you nearly for free.

### Cited Findings
- **LFS deduplication:** LFS objects are content-addressed and verified by SHA-256, so identical files share one OID and one stored object. — [rudolfs README](https://docs.rs/crate/rudolfs/0.3.6/source/README.md)
- **Ableton file collection:** on save, Ableton copies temp recordings into the Project Folder. Its "Collect All and Save" behavior means projects can be made self-contained. — [Ableton: External Storage and Backup](https://help.ableton.com/hc/en-us/articles/5809449695900)
- **Selective checkout:** Anchorpoint offers "selective checkout" as a Team feature, which is relevant for large shared asset repos. — [Anchorpoint facts/pricing](https://www.anchorpoint.app/policies/facts)

### Inferences
- **Per-repo dedupe:** the LFS store is per repo on most servers, so dedupe across repos needs a shared bucket keyed by OID. rudolfs/Giftless with a single bucket and an OID-only key gives this. Forgejo stores LFS by OID path, so a shared bucket dedupes physically (prior knowledge, not re-verified), but per-repo access control must stay enforced in the API.
- **Third-party samples:** commit what the project uses (consolidated/collected audio). This is legally simplest when licenses allow redistribution within the team. Otherwise record a `samples.lock.json` (`path, sha256, size, source pack, license`) and have the client resolve missing samples from each user's local library by hash.
- **Plugin manifest (`Docs/plugins.json`):** parse DAW project files where possible (Ableton `.als` XML lists plugin names and versions; Reaper `.rpp` is text) to auto-generate `{name, vendor, format(VST3/AU/AAX), version}`. The desktop app checks the local plugin folders and warns "Missing: FabFilter Pro-Q 4" before opening.
- **Shared sample packs:** a separate repo cloned once per machine to a fixed path, not a submodule. Submodules confuse non-technical users, and DAW absolute paths break across machines.

### Gaps
- Licensing constraints of redistributing commercial samples inside private team repos were not researched.
- I did not verify plugin info in specific DAW project formats.

---

## 7. Security, backup, and cost

### Takeaway
Use per-project repos with team permissions on the forge, private buckets with presigned URLs, and optional at-rest encryption (rudolfs-style app-level encryption or S3 SSE). Unreferenced-LFS garbage collection is a known weak spot. GitLab has rake tasks but still lacks S3 orphan cleanup, so plan your own OID reference-count GC.

### Cited Findings
- **rudolfs encryption:** encrypts objects at rest in the cache and in S3 with a 32-byte key. Losing or changing the key irrecoverably breaks every object. CDN mode is incompatible with encryption. — [rudolfs README](https://docs.rs/crate/rudolfs/0.3.6/source/README.md)
- **rudolfs local disk cache** reduces S3 egress cost ("reduce your S3 bill"). — [rudolfs README](https://docs.rs/crate/rudolfs/0.3.6/source/README.md)
- **GitLab orphan cleanup:**
  - `gitlab:cleanup:orphan_lfs_files` removes unreferenced LFS files, which otherwise happens daily. `gitlab:cleanup:orphan_lfs_file_references` (PROJECT_PATH/PROJECT_ID, dry-run by default, `DRY_RUN=false`, `LIMIT=`) removes stale DB references so GC can reclaim space after history rewrites.
  - Don't run it within 12 hours of an upgrade.
  - — [GitLab docs: Clean up Rake tasks](https://docs.gitlab.com/administration/raketasks/cleanup/)
- **GitLab S3 gap:** an open issue requests a task to remove unreferenced LFS objects from S3 buckets, because `orphan_lfs_files` does not cover object storage. — [GitLab issue #391317](https://gitlab.com/gitlab-org/gitlab/-/issues/391317); [GitLab issue #215098](https://gitlab.com/gitlab-org/gitlab/-/issues/215098)
- **Anchorpoint data separation:** the locking metadata server keeps metadata separate from production data. — [Unreal forum: Anchorpoint](https://forums.unrealengine.com/t/anchorpoint-anchorpoint-version-control-with-git/2567882)

### Inferences
- **GC design:**
  - Maintain `lfs_object(oid, size, storage_class, first_seen, last_referenced)` and `lfs_ref(repo, oid)`, populated by the push-webhook worker as it scans pointers.
  - An object is deletable only if no repo references it in any reachable commit, after a grace period (e.g. 30 days, to cover in-flight pushes and the gap between upload and push).
  - Derived assets are deleted with their OID.
- **Tiering:**
  - Objects referenced only by old non-tagged commits move to infrequent-access or archive classes (S3 lifecycle rules by tag/prefix).
  - Tagged "approved" masters stay hot.
  - R2 has no egress fees (prior knowledge, not re-verified), which matters a lot for audio streaming and repeated clones.
- **Access control:** one repo per song or project. Roles are mapped to Forgejo teams. LFS download URLs are short-lived presigned URLs, and preview URLs are separately scoped so guest reviewers never get raw stems.
- **Backup:**
  - Forge DB plus git repos via Forgejo dump, or volume snapshots.
  - Bucket versioning or cross-region replication on the LFS bucket.
  - Back up the encryption keys separately (see rudolfs key-loss risk).

### Gaps
- No current (2026) R2/S3/B2 price points were fetched.
- Forgejo's own orphan-LFS cleanup tooling (e.g. admin "garbage collect LFS" cron) was not verified this session.

---

## 8. MVP scope and phased roadmap

### Takeaway
Existing tools validate four things:
- bundled git + LFS CLI (GitHub Desktop/dugite),
- auto-lock on modify with release on push (Anchorpoint),
- Tauri + Rust git clients (GitButler),
- precomputed-peak waveform players (BBC peaks.js/audiowaveform).

The MVP should combine these and avoid novel tech: no custom merge, no real-time co-editing, no custom LFS server.

### Cited Findings
- Bundled git + LFS + credential manager is a proven distribution model. — [dugite-native](https://github.com/desktop/dugite-native)
- Auto-lock on modify and release on push, as a proven artist-facing lock UX. — [Anchorpoint docs](https://docs.anchorpoint.app/git/file-locking/)
- Native LFS locking exists but is weakly enforced: warn-only push checks and few servers. — [git-lfs wiki](https://github.com/git-lfs/git-lfs/wiki/File-Locking)
- Precomputed peaks enable fast web waveform rendering. — [peaks.js](https://github.com/bbc/peaks.js); [wavesurfer.js](https://wavesurfer.xyz/docs/peaks/)
- Sidecar bundling in Tauri is first-class. — [Tauri sidecar docs](https://v2.tauri.app/develop/sidecar/)
- Forgejo + S3-compatible LFS storage is a supported configuration. — [Forgejo storage docs](https://forgejo.org/docs/v8.0/admin/storage)

### Inferences
**Phase 0–1 (MVP, about 3–4 months for a small team):**
- **Desktop app:** Tauri v2 + bundled git/git-lfs sidecars.
- **Project setup:** create or join a project (clone), with `.gitattributes`/`.gitignore` auto-written from templates.
- **Watcher:** detects saves; a "Save version" dialog with a note.
- **Publish:** commit + push.
- **Get latest.**
- **History list** with "Restore/Listen".
- **Locking:** LFS locks auto-acquired on first modify of lockable files and auto-released after a successful push. A "locked by" badge, force-unlock for producers.
- **Conflicts:** conflict-copy fallback.
- **Server:** Forgejo + R2/MinIO, a small API service (auth/OAuth, projects, roles, webhooks, comments) and a Postgres queue worker running audiowaveform + ffmpeg preview + LUFS per OID.
- **Web:** project page, version list, wavesurfer.js player with precomputed peaks, timestamped comments, "Approve" that creates a protected tag.

**Phase 2:**
- Alt versions (branches) with cross-branch "outdated" warnings, like Anchorpoint.
- A/B compare with loudness matching.
- Spectrograms.
- Guest review links.
- Plugin manifest extraction (Ableton/Reaper first).
- Partial checkout (`git lfs pull --include`, sparse checkout) for stems-only roles.
- Notifications (email/Slack/Discord).

**Phase 3:**
- DAW plugins/extensions for in-DAW "Save version" and check-out. Reaper ReaScript and Ableton Remote Scripts are the easiest.
- Structured project diffs.
- Shared sample-library resolution by hash.
- Storage tiering and OID GC.
- Encryption at rest with customer-managed keys.
- Optional own lock service for richer lock UX.

**Explicitly defer:**
- Real-time collaborative editing (Avid Cloud Collaboration territory).
- Audio content merges.
- A custom LFS server, unless a specific need arises (encryption → rudolfs; multi-backend → Giftless).

### Gaps
- No published effort estimates or post-mortems from comparable products were found. The timeline above is a judgment call.
- I found no data on adoption or retention of git-based tools among musicians, as opposed to game artists, to validate demand.
