# Git + Git LFS as the Storage/Versioning Backbone for Large Binary Audio Assets (music-studio collaboration app)

Research date: 2026-11-19. Pricing was checked on live official pages on that date unless flagged. Items marked **[background, unverified this session]** come from prior knowledge and were not confirmed against a live source in this session. Treat them as leads to check, not citable facts.

---

## 1. How does Git LFS work, and what does it NOT do?

### Takeaway
Git LFS replaces each tracked binary in Git history with a pointer file of about 130 bytes (version, sha256 oid, size). It uses clean/smudge filters set up through `.gitattributes`, keeps the real bytes in `.git/lfs/objects`, and moves them over HTTP through a JSON "batch" API whose `href` actions can point at any storage URL. It stores every revision of a file whole, with no delta or chunk dedup, so one changed byte in a 500 MB WAV costs another 500 MB.

### Cited Findings
**Pointer files**
- A pointer is UTF-8 text with one `key value` per line. Keys are sorted with `version` first, and the pointer must be under 1024 bytes. Required keys are `version`, `oid` (currently only `sha256`) and `size` in bytes. Example: `version https://git-lfs.github.com/spec/v1` / `oid sha256:4d7a2146…2393` / `size 12345`. Empty files pass through unchanged. — [git-lfs spec](https://github.com/git-lfs/git-lfs/blob/main/docs/spec.md)
- Local object store layout is `.git/lfs/objects/OID[0:2]/OID[2:4]/OID`, for example `.git/lfs/objects/4d/7a/4d7a2146…`. — [git-lfs spec](https://github.com/git-lfs/git-lfs/blob/main/docs/spec.md)

**.gitattributes**
- The spec's example lines are `*.mp3 filter=lfs -text` and `*.zip filter=lfs -text`. `git lfs install` sets up the global filters, and `git lfs track` adds entries to `.gitattributes`. — [git-lfs spec](https://github.com/git-lfs/git-lfs/blob/main/docs/spec.md)
- `git lfs track "*.jpg" --lockable` adds the `lockable` attribute to the line. — [git-lfs wiki: File Locking](https://github.com/git-lfs/git-lfs/wiki/File-Locking)

**Clean filter (on `git add`)**
- Git pipes file content to the filter on STDIN. The filter streams it to a temp file while computing SHA-256, atomically moves it into the object store if it isn't already there, and writes the pointer to STDOUT. **The clean filter does not upload.** Uploads happen on `git push` through a pre-push hook. — [git-lfs spec](https://github.com/git-lfs/git-lfs/blob/main/docs/spec.md)

**Smudge filter (on checkout)**
- The filter reads the first 100 bytes. If they parse as a pointer, it finds the object locally or downloads it, then writes the real content to STDOUT. Otherwise it passes the input through unchanged. — [git-lfs spec](https://github.com/git-lfs/git-lfs/blob/main/docs/spec.md)

**Batch API**
- Clients send `POST <lfs-url>/objects/batch`, for example `https://git-server.com/foo/bar.git/info/lfs/objects/batch`, with `Accept`/`Content-Type: application/vnd.git-lfs+json`. — [Batch API doc](https://github.com/git-lfs/git-lfs/blob/main/docs/api/batch.md)
- Request fields: `operation` (`download`/`upload`), optional `transfers` (adapter names; `basic` assumed if omitted), optional `ref.name` (added in v2.4), `objects[]` each with `{oid,size}`, and optional `hash_algo` (default `sha256`). — [Batch API doc](https://github.com/git-lfs/git-lfs/blob/main/docs/api/batch.md)
- Response fields: the chosen `transfer` and per-object `actions` (`download`/`upload`/`verify`), each with `href`, an optional `header` map, and `expires_in` or `expires_at`. Objects can instead carry per-object errors: 404 not found, 409 hash algorithm mismatch, 410 removed, 422 validation error. If the server already has an object for upload, it omits `actions`, which gives dedup of whole identical files. Request-level codes include 401 with `LFS-Authenticate`, 403, 404, 422, and optional 406/413/429/501/507/509. Clients retry on 500/502/503/504. — [Batch API doc](https://github.com/git-lfs/git-lfs/blob/main/docs/api/batch.md)
- An optional `verify` action lets the server confirm an upload after it completes. — [Batch API doc](https://github.com/git-lfs/git-lfs/blob/main/docs/api/batch.md)
- The doc says `basic` is the only adapter currently supported in the spec, and lists others for future compatibility. The core client does support resumable downloads with HTTP `Range` and resumable uploads with tus.io. — [Batch API doc](https://github.com/git-lfs/git-lfs/blob/main/docs/api/batch.md); [custom transfers doc](https://github.com/git-lfs/git-lfs/blob/main/docs/custom-transfers.md)
- `lfs.tustransfers=true` turns on tus.io resumable uploads. `lfs.basictransfersonly=true` restricts the client to plain HTTP. — [git-lfs-config](https://github.com/git-lfs/git-lfs/blob/main/docs/man/git-lfs-config.adoc)

**Custom transfer agents**
- Configured with `lfs.customtransfer.<name>.path`, `.args`, `.concurrent` (default true, scaled by `lfs.concurrenttransfers`) and `.direction` (`download`/`upload`/`both`). Git LFS talks to the agent over stdin/stdout using line-delimited JSON: `init` (operation, remote, concurrent, concurrenttransfers), `upload` (oid, size, path, action), `download` (oid, size, action; the agent returns a path), `progress` (bytesSoFar, bytesSinceLast), `complete` (path or error), and `terminate`. Custom agents do not handle `verify`. — [custom-transfers.md](https://github.com/git-lfs/git-lfs/blob/main/docs/custom-transfers.md)
- **Standalone transfer agents** (`lfs.standalonetransferagent` or `lfs.<url>.standalonetransferagent`) skip the LFS API server entirely. The agent decides where files go based on the `remote` value, and the docs' example is an NFS agent. There is also an internal `lfs-standalone-file` adapter. — [custom-transfers.md](https://github.com/git-lfs/git-lfs/blob/main/docs/custom-transfers.md); [git-lfs-config](https://github.com/git-lfs/git-lfs/blob/main/docs/man/git-lfs-config.adoc)
- Other config defaults: `lfs.concurrenttransfers` 8; `lfs.transfer.maxretries` 8; `lfs.storage` defaults to `lfs` inside the git dir and can be moved, for example to a shared cache volume. — [git-lfs-config](https://github.com/git-lfs/git-lfs/blob/main/docs/man/git-lfs-config.adoc)

**What LFS does NOT do**
- No delta or chunk dedup across versions. GitHub's billing doc says pushing a changed LFS file counts the entire file again, so each new version adds its full size. — [GitHub Docs: Git LFS billing](https://docs.github.com/en/billing/concepts/product-billing/git-lfs)
- Hugging Face puts it this way: "Git LFS is designed to notice only that a file has changed and store the entirety of that revision." — [HF Xet deduplication](https://huggingface.co/docs/hub/xet/deduplication)
- LFS content takes up disk space twice on clients: once in `.git/lfs/objects` and once in the working tree (for example, 20 GB of LFS files needs about 40 GB of disk). Secondary source. — [codenote.net comparison](https://codenote.net/en/posts/github-large-binaries-git-lfs-alternatives/) (blog; consistent with the spec's storage layout)
- Without the server's locking API, LFS has no concurrency control. See section 3.

### Inferences
- For a music app, the batch API's `href` plus `header` design is the main extension point. A custom LFS server can answer batch requests with presigned S3, R2 or GCS URLs, so audio bytes go straight between client and object storage and the app server handles only metadata and auth. The spec does not name presigned URLs, but `href` combined with expiring tokens supports this pattern, and rudolfs and Gitea's `SERVE_DIRECT` do it (see section 2).
- A custom transfer agent is the hook for adding chunked or deduplicated transport later, for example a Xet-like CDC uploader, while keeping standard Git and pointer semantics. The pointer stays `sha256(whole file)`, so the agent would keep its own chunk manifest per oid.
- **[background, unverified this session]** Modern Git LFS uses the long-running `filter.lfs.process = git-lfs filter-process` protocol rather than one process per file. `git lfs install` also sets `filter.lfs.required=true` and installs pre-push, post-checkout, post-commit and post-merge hooks. The post-* hooks enforce read-only state on lockable files.

### Gaps
- I did not fetch the filter-process protocol details or the SSH transfer protocol (`git-lfs-transfer`, "pure SSH" LFS, added in git-lfs 3.x). Soft Serve is noted as implementing pure-SSH LFS ([search result](https://github.com/github/git-lfs/wiki/Implementations)), but I did not confirm this against a primary source.

---

## 2. Storage/bandwidth quotas, pricing and max file sizes by host (as of Nov 2026)

### Takeaway
GitHub meters LFS at $0.07/GiB-month for storage and $0.0875/GiB for download bandwidth after small free quotas (10 GiB on Free/Pro, 250 GiB on Team/Enterprise Cloud), with a per-file cap of 2–5 GB. Bitbucket charges $10 per 100 GB of LFS storage with no file-size cap. GitLab.com Free caps repo plus LFS at 10 GiB per project. Azure Repos LFS is free with no published quota. For an audio-heavy team, GitHub's **download bandwidth** charges are the risk to watch, which argues for a self-hosted LFS server on S3-compatible storage, or a host that does not bill bandwidth.

### Cited Findings
**GitHub** (official docs checked 2026-11-19)
- Included per month: Free, Pro and Free-for-orgs get 10 GiB bandwidth and 10 GiB storage. Team and Enterprise Cloud get 250 GiB bandwidth and 250 GiB storage. — [GitHub Docs: Git LFS billing](https://docs.github.com/en/billing/concepts/product-billing/git-lfs)
- Overage: **storage $0.07 per GiB-month, bandwidth $0.0875 per GiB**. — [GitHub Docs: about billing for Git LFS](https://docs.github.com/en/billing/using-the-new-billing-platform/about-billing-for-git-large-file-storage); [GitHub changelog 2024-06-03 (metered LFS for new Enterprise accounts)](https://github.blog/changelog/2024-06-03-new-enterprise-accounts-have-metered-billing-for-git-lfs/). The current concepts page does not list rates and points to the [pricing calculator](https://github.com/pricing/calculator?feature=lfs). The rates come from the 2024 changelog and the billing-platform doc, and I found no sign they changed.
- Billing mechanics:
  - Storage accrues hourly, covers all LFS objects in the repo whatever their age, and the accrued total resets each billing cycle. Deleting objects mid-month does not lower that month's charge.
  - Only downloads count as bandwidth, including downloads by GitHub Actions. Uploads count toward storage but not bandwidth.
  - Forks and pulls are billed to the parent repo owner. Source archives that include LFS objects count as bandwidth.
  - Pre-paid data packs were replaced by metered billing.
  — [GitHub Docs: Git LFS billing](https://docs.github.com/en/billing/concepts/product-billing/git-lfs)
- Over quota without a payment method: if storage is over, clones receive only pointer files and new pushes fail. If bandwidth is over, LFS is disabled until next month. With a $0 budget, usage is blocked. — [GitHub Docs: Git LFS billing](https://docs.github.com/en/billing/concepts/product-billing/git-lfs)
- **Max LFS file size:** Free 2 GB, Pro 2 GB, Team 4 GB, Enterprise Cloud 5 GB. Anything over 5 GB is rejected. — [GitHub Docs: About Git LFS](https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-git-large-file-storage)
- Plain Git (non-LFS) on GitHub warns at 50 MiB and blocks at 100 MiB. Secondary source. — [codenote.net](https://codenote.net/en/posts/github-large-binaries-git-lfs-alternatives/)

**GitLab.com**
- Free tier: **10 GiB per project for repository plus LFS combined**. A project over the limit becomes read-only, and going above 10 GiB requires buying storage, which renews annually. Premium/Ultimate: 500 GiB per project. — [GitLab docs: storage usage quotas](https://docs.gitlab.com/user/storage_usage_quotas)
- Conflict: some third-party sources describe the limit per *namespace* rather than per project. Older GitLab pages proposed 5 GiB namespace limits, and I could not confirm whether those were enforced. — [GitLab forum](https://forum.gitlab.com/t/storage-limits-per-namespace/97015); [Spendbase](https://www.spendbase.co/blog/saas-management/gitlab-pricing/)

**Bitbucket Cloud**
- LFS allowance: Free 1 GB, Standard 5 GB, Premium 10 GB. Usage is counted **across all repos in the workspace**. **No limit on LFS file size.** — [Atlassian: Storage policy for Git LFS](https://support.atlassian.com/bitbucket-cloud/docs/storage-policy-for-git-lfs-with-bitbucket/)
- Extra storage costs **$10/month per 100 GB** and is added automatically on paid plans. Sources conflict on whether Free plans can buy it: [Atlassian licensing](https://www.atlassian.com/licensing/bitbucket) says no, while an [Atlassian billing help page](https://support.atlassian.com/bitbucket-cloud/docs/manage-your-plan-and-billing/) suggests yes. — also [Atlassian community](https://community.atlassian.com/t5/Bitbucket-questions/If-I-use-more-than-5GB-without-realizing-it-will-the-additional/qaq-p/2125348)
- I found no LFS bandwidth fee in the Bitbucket sources.

**Azure DevOps (Azure Repos)**
- Microsoft says Azure DevOps offers LFS for free. It sets no strict file-size limit but does impose a one-hour time limit per file upload. — [MS Learn: Manage large files](https://learn.microsoft.com/bg-bg/azure/devops/repos/git/manage-large-files?view=azure-devops); [MS Learn (newer)](https://learn.microsoft.com/vsts/git/manage-large-files)
- LFS objects do not count toward the repository size limit. — [MS Learn: Git limits](https://learn.microsoft.com/en-us/azure/devops/repos/git/limits)
- Conflicting third-party claims put free LFS at 1 GB or 2 GB per org. Neither figure appears in Microsoft docs. — [oneuptime blog (2026-02)](https://oneuptime.com/blog/post/2026-02-16-how-to-set-up-azure-repos-git-lfs-for-managing-large-binary-files-in-repositories/view)
- Azure Repos does not support SSH for repos with LFS files, so HTTPS is required. — (search summary of [MS Learn](https://learn.microsoft.com/vsts/git/manage-large-files))

**Gitea / Forgejo (self-hosted)**
- `LFS_MAX_FILE_SIZE` in `[server]` is in bytes, with 0 meaning no limit (the default). Uploads over the limit get HTTP 413. `LFS_MAX_BATCH_SIZE` caps pointers per batch (default unlimited). `LFS_HTTP_AUTH_EXPIRY` was raised to 24h so slow, large pushes don't fail. `LFS_LOCKS_PAGING_NUM` exists, which implies a lock-listing API. LFS storage can use `STORAGE_TYPE = minio` (S3-compatible) with `SERVE_DIRECT`, which redirects clients to signed S3 URLs. — [Forgejo `modules/setting/lfs.go` (mirror)](https://git.fb1.uni-bremen.de/davrot/forgejo_fusion/src/branch/main/modules/setting/lfs.go) (mirror of source, version uncertain)

**Standalone self-hosted LFS servers**
- **rudolfs**: a Rust "high-performance, caching Git LFS server with an AWS S3 back-end", plus a local-disk backend, with a docker-compose example that runs MinIO. — [rudolfs on docs.rs](https://docs.rs/crate/rudolfs/latest/source/docker-compose.minio.yml); [GitHub](https://github.com/silverjam/rudolfs)
- **Giftless**: a Python, pluggable LFS server. Its docs say "Locking: no support at present". — [Giftless docs (datahub)](https://datahub.io/docs/dms/giftless)
- **lfs-test-server**: GitHub's reference server implementation, not production-grade. — [git-lfs Implementations wiki](https://github.com/github/git-lfs/wiki/Implementations)
- MinIO or S3 alone does not speak the LFS API, so an LFS server must sit in front. — [serverfault Q&A](https://kiwix.lema.org/serverfault.com_en_all_2023-11/questions/1130341/how-do-i-give-git-lfs-access-my-self-hosted-minio-instance)
- Other options include lfsproxy (topics list file-locking and MinIO, unverified) and Soft Serve (HTTP and pure-SSH LFS). — [git-lfs Implementations wiki](https://github.com/github/git-lfs/wiki/Implementations); [lfsproxy](https://awesome.ecosyste.ms/projects/github.com%2Fmohamedbelasy%2Flfsproxy)

**Commercial alternatives' storage pricing (for comparison)**
- **Diversion**: Indie plan has 100 GB free (up to 20 users, 5 repos, for companies under $100K revenue and under $1M funding). Extra storage is $10 per 100 GB per month, auto-added. Diversion markets "no need to pay for bandwidth". — [Diversion pricing](https://diversion.dev/pricing); [Diversion storage docs](https://docs.diversion.dev/concepts/storage); [Diversion vs Git LFS](https://diversion.dev/compare-diversion-to-git-lfs). Seat prices per G2 (third-party): Professional $25 and Studio $35 per user per month. — [G2](https://www.g2.com/products/diversion-version-control/pricing)
- **Anchorpoint**: Personal plan is free for non-commercial single users. Team plan is €20/user/month billed annually or €25 monthly, with storage on your own Git host. — [Anchorpoint facts](https://www.anchorpoint.app/policies/facts)

### Inferences
- **Bandwidth matters more than storage for a team with collaborators.** On GitHub, a 100 GiB project that five collaborators each fully fetch once is about 500 GiB of egress. On Team, that is (500−250) × $0.0875 ≈ **$21.9/month** extra, and each re-clone or CI run adds more. Storage for the same 100 GiB stays inside Team's 250 GiB. On Free/Pro it would be (100−10) × $0.07 = $6.30/month for storage and (500−10) × $0.0875 ≈ $42.9 for bandwidth. The per-file cap (2 GB on Free/Pro) is roughly a 1h 56m stereo 48k/24-bit WAV (2 GB ÷ 17.28 MB/min), or a long multitrack session file. Sample-library packs (.nki, .exs, or large monolithic sample containers) can exceed it.
- Bitbucket's flat $10 per 100 GB with no bandwidth charge found is roughly $0.10/GB-month, a little above GitHub's $0.07 on storage but with no egress risk. Diversion matches at $10 per 100 GB.
- Self-hosting on Cloudflare R2 (no egress fee) or Backblaze B2 behind rudolfs or Gitea would likely be cheapest at scale. **[background, unverified this session]**: R2 is about $0.015/GB-month with $0 egress, and S3 Standard is about $0.023/GB-month plus egress. Confirm before quoting.

### Gaps
- I could not find GitLab's current price for additional storage, or confirm the namespace vs. project limit, from an official 2026 page.
- Azure DevOps' actual LFS quota, if one exists, is undocumented.
- I did not confirm Gitea/Forgejo docs on LFS locking directly; the code points to a lock API. **[background, unverified]**: Gitea implements the LFS locks API.
- I did not check Perforce Helix Core Cloud or Unity Version Control (Plastic) storage pricing.

---

## 3. Git LFS file locking: how it works, and is it suitable for non-mergeable DAW projects?

### Takeaway
LFS locking is a server-side advisory lock registry keyed by path. On the client, files marked `lockable` are read-only until you `git lfs lock` them, and on push the client calls `/locks/verify` and refuses to push changes to files someone else has locked. It suits DAW project files (.als, .logicx, .ptx, .cpr, .flp, .rpp), which cannot be merged. But it depends on server support, can be bypassed, works per path rather than per branch, and gives no "someone else changed it" alert. So the app should build its own lock UX on top (or alongside, as Anchorpoint does).

### Cited Findings
- **API** (all under `<lfs-url>/locks`, `application/vnd.git-lfs+json`):
  - `POST /locks` with `{path, ref?}`: returns 201, 409 if the path is already locked, 403 without push access.
  - `GET /locks?path=&id=&cursor=&limit=&refspec=`: requires pull access, paginated.
  - `POST /locks/verify` with `{ref?, cursor?, limit?}`: requires push access, returns `ours` and `theirs`.
  - `POST /locks/:id/unlock` with `{force?, ref?}`.
  - The server sets the lock `owner` from credentials. Without `force`, users can delete only their own locks.
  — [Locking API](https://github.com/git-lfs/git-lfs/blob/main/docs/api/locking.md)
- On push, Git LFS calls `/locks/verify`. If a pushed file is locked by someone else, the push stops. If it is your own lock, LFS lists it so you can unlock. Servers without locking return 404, which does **not** stop pushes. — [Locking API](https://github.com/git-lfs/git-lfs/blob/main/docs/api/locking.md)
- `lockable` attribute: matching files are made read-only locally to "prevent users from accidentally editing a file without locking it first". `git lfs lock <path…>` takes multiple paths since v3.0.0. `git lfs locks` lists path, owner and ID. `git lfs unlock <path>|--id=N [--force]`. Force-unlock permissions vary by server, and some require admin. Locking first shipped as an early release in v2.0.0, and "few LFS servers implement the locking API". — [git-lfs wiki: File Locking](https://github.com/git-lfs/git-lfs/wiki/File-Locking)
- `lfs.setlockablereadonly` defaults to true. `lfs.locksverify` defaults to unset (attempt the check and warn on errors); `true` halts pushes on lock errors and `false` skips the check. — [git-lfs-config](https://github.com/git-lfs/git-lfs/blob/main/docs/man/git-lfs-config.adoc)
- `lockable` can be set on non-LFS files too, for example `*.yml lockable`. — [git-lfs wiki: File Locking](https://github.com/git-lfs/git-lfs/wiki/File-Locking)
- Server support: Giftless has no locking. — [Giftless](https://datahub.io/docs/dms/giftless)
- Anchorpoint **does not use Git LFS locking**. It runs its own locking through the Anchorpoint metadata server, which works for Git repos and also Dropbox or Google Drive folders. — [Anchorpoint docs: file locking](https://docs.anchorpoint.app/docs/general/workspaces-and-projects/file-locking) (a third-party site claims otherwise: [usourcecontrol](https://usourcecontrol.com/compare/anchorpoint))

### Inferences
- **Suitability for DAWs:**
  - Good fit for the single-writer model of a project file such as `.als`, `.logicx`, `.ptx` or `.cpr`, because these cannot be merged.
  - Limitations to design around:
    1. Locks are advisory and per-path. The optional `ref` lets servers scope them to a branch, but the server decides whether to enforce that.
    2. Some DAW "projects" are directory bundles. Logic's `.logicx` is a macOS package directory and Pro Tools sessions sit beside folders of audio and fades. Locking one file in the bundle does not protect the rest, so the app must lock a canonical file or all files together.
    3. Read-only flags can confuse DAWs that autosave or write backup and cache files inside the project folder (Ableton's `Backup/`, Logic's autosave). Those paths should be gitignored or kept out of `lockable`.
    4. Recorded audio files are usually write-once with unique names, so locking them adds little. Lock project and session files, plus anything edited destructively in place.
- A collaboration app should likely put its own lock UI on top of the LFS locks API: who holds the lock, since when, a notify-on-release option, admin force-unlock, and auto-lock when the DAW opens the project. That way plain git CLI users get the same enforcement at push time.

### Gaps
- I did not verify which hosts implement the locks API (GitHub, GitLab Premium, Gitea and Bitbucket probably do **[background, unverified]**), or how each scopes locks to a `ref`.
- I found no DAW-specific (Ableton/Logic/Pro Tools) guidance on LFS locking.

---

## 4. Performance tricks: partial/sparse checkout, include/exclude fetch, recent refs, shallow clone, prune, migrate

### Takeaway
Git LFS has fine-grained controls over which binaries get pulled locally: include/exclude path filters, "recent" windows, and pruning of objects already pushed. Combined with sparse checkout, shallow clones and `GIT_LFS_SKIP_SMUDGE`, a musician can pull only the songs or stems they need. Old history can be rewritten into LFS with `git lfs migrate`.

### Cited Findings
- `lfs.fetchinclude` / `lfs.fetchexclude` take comma-separated path or filename patterns that limit which LFS objects are fetched. The same filters are available as `git lfs fetch --include/--exclude` (`-I`/`-X`). — [git-lfs-config](https://github.com/git-lfs/git-lfs/blob/main/docs/man/git-lfs-config.adoc)
- `lfs.fetchrecentrefsdays` defaults to **7** days, meaning refs with commits in the last N days are fetched. `lfs.fetchrecentremoterefs` defaults to true. `lfs.fetchrecentcommitsdays` defaults to **0**, so older revisions on each ref are not fetched. `lfs.fetchrecentalways` defaults to false, so `--recent` is not implied. — [git-lfs-config](https://github.com/git-lfs/git-lfs/blob/main/docs/man/git-lfs-config.adoc)
- `lfs.pruneoffsetdays` defaults to **3**, the extra days added to the recent windows before objects become prunable. `lfs.pruneremotetocheck` names the remote that must already have the objects. `lfs.pruneverifyremotealways` makes `git lfs prune` always behave as `--verify-remote`. — [git-lfs-config](https://github.com/git-lfs/git-lfs/blob/main/docs/man/git-lfs-config.adoc)
- `lfs.concurrenttransfers` defaults to 8, and raising it helps on high-bandwidth links with many stems. — [git-lfs-config](https://github.com/git-lfs/git-lfs/blob/main/docs/man/git-lfs-config.adoc)
- `lfs.storage` can move the LFS object store, for example onto a large external audio drive. — [git-lfs-config](https://github.com/git-lfs/git-lfs/blob/main/docs/man/git-lfs-config.adoc)
- Gitea raised `LFS_HTTP_AUTH_EXPIRY` to 24h because large pushes over slow links were failing with the old 20-minute token. This matters for multi-GB stem uploads from home studios. — [Forgejo lfs.go mirror](https://git.fb1.uni-bremen.de/davrot/forgejo_fusion/src/branch/main/modules/setting/lfs.go)

### Inferences
- **[background, unverified this session]** Other standard techniques to verify:
  - `GIT_LFS_SKIP_SMUDGE=1 git clone …`, then `git lfs pull -I "Songs/SongA/**"`, clones metadata only and hydrates on demand.
  - `git sparse-checkout set --cone Songs/SongA` with `git clone --filter=blob:none --sparse` limits the working tree. LFS only smudges files that are checked out.
  - `git clone --depth=1` (shallow clone) avoids history. LFS objects are fetched only for the checked-out commit anyway.
  - `git lfs prune` deletes local LFS objects that are not recent or current and are already pushed.
  - `git lfs migrate import --include="*.wav,*.aif*,*.flac,*.mp3" --everything` rewrites existing history to move binaries into LFS, which changes commit SHAs and requires a force-push. `git lfs migrate export` reverses it, and `git lfs migrate info` reports sizes by extension.
  - `git lfs fetch --all` pulls every historical version and is the expensive one for bandwidth billing.
- For the app, the "song" or "session" is the natural unit of hydration. Default to metadata-only clones and fetch LFS objects per song on demand, which also caps GitHub bandwidth billing. A local shared LFS cache (`lfs.storage`) avoids re-downloading samples used across projects.

### Gaps
- I did not fetch the `git-lfs-migrate`, `git-lfs-prune` or `git-lfs-fetch` man pages, or recent benchmarks of LFS clone and fetch speed on large media repos. The commands above are well established but I did not verify them this session.

---

## 5. Alternatives and complements for large binaries, and how chunk-level dedup changes costs for audio edits

### Takeaway
The main architectural choice is **whole-file content addressing** (Git LFS, DVC, git-annex) versus **content-defined chunking** (Hugging Face Xet, and XetHub before it). CDC stores only changed roughly 64 KB chunks, so appending a take to a WAV costs about the new audio plus a couple of chunks instead of the whole file. Gain, normalize, sample-rate conversion and re-encoding change every sample, so they dedup to roughly nothing either way. Game-industry tools (Diversion, Anchorpoint, Perforce, Unity VCS) mostly offer better UX and locking rather than smaller storage.

### Cited Findings
**Hugging Face Xet (formerly XetHub; acquired by HF)**
- Xet uses **content-defined chunking at ~64 KB**. A rolling hash sets chunk boundaries from the content, so an insertion only changes the chunk it lands in. Only new chunks are stored. New chunks are grouped into **64 MB blocks** ("xorbs") in a content-addressed store. Dedup checks run against the current session, then a local metadata cache, then a global dedup query on a subset of chunks. — [HF Xet deduplication](https://huggingface.co/docs/hub/xet/deduplication)
- A 20 GB file is about 312,500 chunks at 64 KB. Xet "enables storing only the modified content in a file (which might only be a few KB or MB) and … deduplicates shared blocks across repositories". HF recommends files of at most 200 GB. — [HF Xet deduplication](https://huggingface.co/docs/hub/xet/deduplication)
- The Hub still uses pointer files plus `.gitattributes`, keeps Git LFS backward compatible, and publishes an open Xet protocol spec for third-party clients. — [HF Xet index](https://huggingface.co/docs/hub/xet/index); [Xet protocol spec](https://huggingface.co/docs/xet/index)
- Background reading: "Git is for Data" (Low et al., CIDR 2023). — [paper](https://www.cidrdb.org/cidr2023/papers/p43-low.pdf)

**git-annex**
- Stores only a reference in Git and keeps content in "special remotes" (S3, rsync, USB drives, offline media, IPFS and more). Files are not duplicated in the checkout because it uses symlinks. The learning curve is steep, and symlinks make Windows awkward. — [codenote.net summary](https://codenote.net/en/posts/github-large-binaries-git-lfs-alternatives/); [LWN](https://lwn.net/Articles/774125/rss)
- Forgejo-aneksajo is a Forgejo fork with git-annex support. — [forgejo-aneksajo mirror](https://git.fb1.uni-bremen.de/davrot/forgejo-aneksajo/src/branch/test-push-to-create/modules/setting/lfs.go) (existence only)

**DVC**
- Small `.dvc` pointer files in Git. Any cloud storage (S3, GCS, SSH) can be a remote, so no special server is needed. Aimed at ML pipelines (`dvc repro`). `dvc status` can be slow because it checksums files, and DVC needs Python. — [DVC user guide](https://doc.dvc.org/user-guide); [DVC vs Git LFS blog](https://dvc.org/blog/dvc-git-lfs/)

**git-sfs**
- A newer single-author tool that uses symlinks plus rclone to any backend, with no LFS server. — [dev.to](https://dev.to/redeyed/git-sfs-large-file-storage-without-the-lfs-server-5cco)

**Diversion**
- A cloud-native VCS aimed at games: 100 GB free, then $10 per 100 GB, no bandwidth fees (vendor claims). Epic recommends it for Unreal. It claims up to 70% lower cost than Perforce (vendor claim). — [Diversion vs Git LFS](https://diversion.dev/compare-diversion-to-git-lfs); [Diversion pricing](https://diversion.dev/pricing)

**Anchorpoint**
- A Git-based desktop client for artists. It auto-configures `.gitattributes` for LFS, understands image, video, audio and 3D formats, offers selective checkout, and has its own file locking. — [Anchorpoint facts](https://www.anchorpoint.app/policies/facts); [Anchorpoint: Git for artists](https://www.anchorpoint.app/features/git-for-artists); [Anchorpoint: 5 alternatives to Git LFS](https://www.anchorpoint.app/blog/5-alternatives-to-git-lfs-for-game-development)

### Inferences
**How CDC applies to audio edits (reasoning, not measured)**
- WAV/AIFF are mostly raw PCM after a small header, so byte-level CDC fits them well:
  - **Appending a take or extending a recording**: the RIFF header's size fields change (the first chunk), the old PCM is byte-identical, and the new audio is new chunks. Cost is about 1–2 × 64 KB plus the new audio. With LFS the cost is the whole new file.
  - **Punch-in or overwriting a region in place**: only the chunks covering that region change.
  - **Inserting or deleting audio mid-file (ripple edit, destructive)**: CDC resynchronises after the edit, so cost is roughly the changed region. Fixed-size chunking would invalidate everything downstream.
  - **Gain change, normalize, fade on the whole file, dither, SRC, or bit-depth change**: every sample changes, so there is about 0% dedup under any scheme.
  - **FLAC/MP3/AAC re-encodes**: compressed streams usually change throughout after a re-encode, so expect little dedup. Unchanged files shared across projects (sample libraries, loops) dedup fully under both LFS (same sha256, same oid) and CDC.
  - **DAW project files**: often gzip-compressed XML (Ableton `.als` **[background, unverified]**), so a small edit changes the whole compressed stream and CDC gains little. These files are small anyway, typically KB to a few MB.
- **Key practical point**: most DAWs treat recorded audio as *non-destructive*. New takes are new files and edits live in the project file. So the main driver of LFS growth is (a) new recordings, (b) repeated renders of bounces, stems, mixes and masters, and (c) destructive processing. The penalty from LFS not deduplicating shows up mainly with (c) and with "render-in-place" workflows that rewrite the same filename. CDC's biggest audio win would be on long files that get appended to or partially re-rendered.
- An app could get most of the value of CDC without Xet by (1) enforcing write-once audio naming, (2) storing bounces as new versions so they can be pruned or tiered to cold storage, and (3) running dedup at the object-store level (identical sha256 already dedups across the whole server).

**[background, unverified this session]** Brief notes on the other tools:
- **Perforce Helix Core**: centralized, exclusive checkout or locking, server stores full revisions (compressed), the game and media industry standard, free tier for small teams.
- **Unity Version Control (formerly Plastic SCM)**: centralized or distributed, exclusive checkout, "Gluon" artist mode for partial workspaces.
- **lakeFS**: Git-like branching over object stores for data lakes, not file-level DAW workflows.
- **Oxen.ai**: Rust VCS for ML datasets that claims chunk-level dedup.

### Gaps
- I did not verify current 2026 pricing or features for Perforce, Unity VCS, lakeFS or Oxen.ai.
- I found no published benchmark of CDC dedup on real audio/DAW workloads; the edit-by-edit figures above are reasoning only.
- I did not check whether HF's `git-xet` or the open-source xet-core can be self-hosted outside HF.

---

## 6. Storage growth math for a typical music project

### Takeaway
Uncompressed 48 kHz/24-bit stereo audio is 288,000 B/s, which is 17.28 MB/min (16.48 MiB/min, the "≈16.5 MB/min" figure). A 40-track × 4-minute song is about 1.4 GB if tracks are mono and about 2.8 GB if stereo. Under LFS, every full re-render of all stems adds that amount again, so 20 full-stem revisions of one song is about 28–55 GB. Non-destructive workflows add far less.

### Cited Findings
- LFS stores each changed version at full size, and GitHub bills storage for "all Git LFS objects … regardless of upload date". — [GitHub Docs: Git LFS billing](https://docs.github.com/en/billing/concepts/product-billing/git-lfs)
- GitHub rates: $0.07/GiB-month storage, $0.0875/GiB download. — [GitHub Docs](https://docs.github.com/en/billing/using-the-new-billing-platform/about-billing-for-git-large-file-storage)

### Inferences (calculations)
**Bitrate arithmetic**
- 48,000 samples/s × 3 bytes × 2 channels = 288,000 B/s = 17.28 MB/min (decimal) = 16.48 MiB/min.
- Mono 48/24: 8.64 MB/min. Stereo 44.1/16 (CD): 10.58 MB/min. Stereo 96/24: 34.56 MB/min.
- FLAC is typically about 50–70% of WAV size for music (**[background, unverified]**). MP3 at 320 kbps is 2.4 MB/min.

**One song, 40 tracks × 4 min at 48/24**
- All stereo: 160 track-minutes × 17.28 MB = **2.76 GB (2.57 GiB)**.
- All mono (typical for recorded mics or DI): **1.38 GB**.
- Realistic mix, for example 25 mono plus 15 stereo: 25×4×8.64 + 15×4×17.28 = 864 + 1,037 MB ≈ **1.9 GB**.

**Revision scenarios for one song (mixed tracks ≈1.9 GB base)**

| Scenario | Per-revision growth | After 20 revisions |
|---|---|---|
| A. Non-destructive: project-file edits only (~1–5 MB) | ~5 MB | ~1.9 GB + 0.1 GB ≈ **2.0 GB** |
| B. Each revision adds 3 new takes (3 × 4 min mono = 104 MB) plus a stereo mix bounce (69 MB) | ~175 MB | 1.9 + 3.5 ≈ **5.4 GB** |
| C. Each revision re-renders all 40 stems under the same filenames (worst case, e.g. stem exchange between studios) | ~1.9 GB | 1.9 × 21 ≈ **40 GB** (all-stereo: ~58 GB) |
| C with CDC (Xet-like), stems unchanged except edited regions (say 10% changed) | ~0.19 GB | ~5.7 GB |

**Team-level example**
- Assumptions: 10 songs per album project; scenario B for production, plus 3 rounds of scenario C for stem exchange or mixing. 10 × (5.4 + 3 × 1.9) ≈ **111 GB per album**.

**GitHub cost for the album example**
- Team plan, 111 GB stored: within the 250 GiB included storage.
- Bandwidth: five collaborators each pulling the whole album once is about 555 GB. That is (555−250) × $0.0875 ≈ **$26.7** that month.
- Free/Pro plan: storage (111−10) × $0.07 ≈ $7.1/month, plus bandwidth (555−10) × $0.0875 ≈ $47.7.

**Bitbucket or Diversion**
- 111 GB comes to about 2 × $10 = $20/month, with no egress fee found.

**Takeaway for design**
- Storage growth is dominated by workflow, not by track count. Encouraging non-destructive edits, write-once takes, and FLAC for archived stems (about 40% saving) matters more than the choice of backend. Per-song selective fetch keeps bandwidth near "current versions only".

### Gaps
- I did not find real-world data on how many revisions or how much churn music teams produce, or on typical `.als`, `.logicx` or `.ptx` file sizes. These would need user research or sample projects.

---

## 7. Programmatic access: libgit2, go-git, isomorphic-git, JGit, dulwich; git-lfs as a library; shelling out

### Takeaway
LFS support in Git libraries is patchy. Dulwich now ships a `dulwich.lfs` module with a filter driver. libgit2 exposes only the generic filter API, so LFS needs a third-party crate or shim. isomorphic-git has only community shims. JGit has an LFS extension **[background, unverified]**. The most robust route for a desktop app is to bundle and shell out to the `git` and `git-lfs` CLIs, and call the LFS batch and locks HTTP APIs directly for UI features such as lock status and per-file download progress.

### Cited Findings
- **libgit2 / pygit2**: no native LFS. libgit2 offers a low-level filter API that LFS-style filters can use. The Rust crate `git2-lfs` shows that git2/libgit2 does not run filters automatically, so callers must invoke the LFS filter themselves. — [DVC blog on dulwich/pygit2](https://dvc.org/blog/tags/pygit2/); [git2-lfs (ejc3)](https://github.com/ejc3/git2-lfs); [git2-lfs (gram-ax)](https://github.com/gram-ax/git2-lfs); [libgit2 discussion #6789](https://www.github.com/libgit2/libgit2/discussions/6789)
- **Dulwich** (pure Python): early versions had no LFS. Releases 1.0 and 1.1 (packaged in Debian) include `dulwich.lfs`, an `LFSFilterDriver`, and compatibility tests against the git-lfs binary. — [Debian dulwich 1.1.0 tests/test_lfs.py](https://sources.debian.org/src/dulwich/1.1.0-3/tests/test_lfs.py); [Debian dulwich 1.0.0 compat test](https://sources.debian.org/src/dulwich/1.0.0-2/tests/compat/test_lfs.py); [dulwich-discuss](https://groups.google.com/g/dulwich-discuss/c/vPOYbToMwRc)
- **isomorphic-git** (JS): no built-in LFS. A community shim, `isogit-lfs`, exists. — [gitui discussion #1089](https://github.hscsec.cn/extrawurst/gitui/discussions/1089)
- **Rust**: a `git-lfs-filter` crate exists. — [docs.rs git-lfs-filter](https://docs.rs/git-lfs-filter)
- **git-lfs as a Go library**: its packages are importable, for example `github.com/github/git-lfs/git`. Third-party Go LFS packages such as `blake.io/git-lfs` also exist. — [pkg.go.dev git-lfs/git](https://pkg.go.dev/github.com/github/git-lfs/git); [pkg.go.dev blake.io/git-lfs](https://pkg.go.dev/blake.io/git-lfs)
- **SVNKit/TMate**: LFS support discussion. — [tmatesoft forum](https://support.tmatesoft.com/t/git-lfs-support/64)
- **Server-side building blocks** for a custom LFS service:
  - Batch API: [batch.md](https://github.com/git-lfs/git-lfs/blob/main/docs/api/batch.md)
  - Locking API: [locking.md](https://github.com/git-lfs/git-lfs/blob/main/docs/api/locking.md)
  - Custom transfer protocol: [custom-transfers.md](https://github.com/git-lfs/git-lfs/blob/main/docs/custom-transfers.md)

### Inferences
- Recommended layering for an Electron or Tauri desktop app:
  1. Bundle pinned `git` and `git-lfs` binaries and shell out for clone, fetch, checkout, commit and push. This gives the best compatibility with hosts and with power users' own CLI.
  2. Parse `git lfs` progress output, or use `GIT_TRACE`, or run your own custom transfer agent, which receives `progress` events per oid, to drive per-stem progress bars.
  3. Call `/locks` and `/objects/batch` directly over HTTP for lock dashboards, existence checks and pre-signed preview downloads (for example, streaming a WAV preview without checking it out).
  4. Use a library such as libgit2 or isomorphic-git only for read-only history browsing, where pointer files are enough.
- The git-lfs Go packages are internal-facing and not a stable public API, so treat them as unstable if embedding.
- **[background, unverified this session]**: JGit ships `org.eclipse.jgit.lfs` (client and server LFS support, used by Gerrit), and go-git has no built-in LFS smudge/clean support.

### Gaps
- I did not verify go-git's current LFS status or the scope of JGit's LFS module.
- I did not determine whether git-lfs exposes a supported embedding API or performance numbers for shelling out versus libraries.
