# DAW save behaviour: results

Phase 0, PLAN task 4. Each DAW is tested by a musician who follows a guided
`fsprobe` session (`docs/validation/RUN_FL_STUDIO.md` for FL Studio; protocols
in `tools/fsprobe/protocols/`). They send back a results folder; its
`summary.md` has one row per step. Fill the tables below from it, then the
design consequences.

How to fill a findings row:

- **Pattern** is the classifier's word from the step's detail table:
  `in_place_write`, `temp_then_rename` (give the temp name pattern, e.g.
  `{name}.tmp`, or "outside watched folders"), `delete_then_create`,
  `moved_aside_then_create`, `new_file`, `renamed`, `removed`, `transient`,
  `metadata_only`, `unclassified`.
- Paths are relative to the test folder; files elsewhere are given as
  `FL Studio data folder: ...`.
- Quote the musician's answers where they describe messages or dialogs.
- If a result looks wrong, re-run the classifier on the returned folder after
  fixing it: `cargo run -p fsprobe -- report --out <folder>`.

## FL Studio

### Run

| Item | Value |
|---|---|
| Results folder | |
| Date | |
| FL Studio version, edition, trial? | `environment.json` → `daw` |
| OS and version | `environment.json` → `os` |
| Filesystem of test folder / FL data folder | `environment.json` → `watched_folders` |
| Documents synced by a cloud service? | step 0 |
| Autosave setting / backup location / maximum (before the test) | step 0 |
| Steps skipped or not applicable | `summary.md` overview |

### Findings

| # | Question | Steps | Observed | Notes |
|---|---|---|---|---|
| F1 | How does an ordinary save write the `.flp`? Pattern, temp name | 1, 2 | | |
| F2 | Settle time of a save (ms) | 2 | | |
| F3 | Does every manual save also write a backup? Where, under what name? | 2 | | |
| F4 | Did FL offer its own project folder on first save? What does it create? | 1 | | |
| F5 | "Save new version" file name | 3 | | |
| F6 | Does "Save as" touch the old file? | 4 | | |
| F7 | Autosave/backup files: folder, name pattern, how often | 5, between steps | | |
| F8 | Recorded audio: folder (project or FL data), name pattern, write pattern | 6 | | |
| F9 | Outside media: copied into the project on save? | 7a | | |
| F10 | "Project data files" export: what was copied where | 7b | | |
| F11 | "Zipped loop package": what was created | 7c | | |
| F12 | Missing sample after rename: message text, options; still plays? | 7d | | |
| F13 | WAV render: pattern, temp files, settle time | 8a | | |
| F14 | WAV re-render to the same name: pattern; asked before replacing? | 8b | | |
| F15 | Files held open while idle / while playing (process, file, mode) | 9a, 9b | | |
| F16 | Read-only before opening: warning on open? Result of save (error text / Save as / silent)? File changed? Still read-only? | 10b | | |
| F17 | Read-only while open: result of save; file changed? | 11c | | |
| F18 | Autosave against a read-only file: error? backup written elsewhere? | 11d | | |
| F19 | File replaced on disk while open: did FL notice or reload? | 12b | | |
| F20 | Next save after that: did FL overwrite the newer file? | 12c | | |
| F21 | Quit without saving: files written or deleted? | 13a | | |
| F22 | Restart: recovery offered? From which file? | 13b | | |
| F23 | Force-quit: recovery files and offer (optional) | 14 | | |
| F24 | Menu names marked (check) that were different | all, 15 | | |
| F25 | Anything unexpected | 15, between steps | | |

### Design consequences

Keyed to `docs/SPEC.md` §10 "Unverified assumptions". Only item 1 is in scope
for this experiment; items 2 and 3 are PLAN task 3, item 4 is Phase 3, item 5 is
Phase 4.

**§10.1a: FL Studio with a read-only project file** (F16 to F20)

- [ ] `lockable` on `*.flp` keeps working (FL refuses to save a read-only file
      with a clear message) → keep the template line
      `*.flp filter=lfs diff=flp merge=binary -text lockable`.
- [ ] FL saves anyway (it replaces the file or clears the flag) → read-only
      gives no protection; rely on auto-lock and the publish-time lock check
      only, and say so in SPEC §7.
- [ ] FL fails silently or loses work → do not mark `*.flp` lockable; take the
      lock when the project is opened instead of on first modification.
- Auto-lock trigger: on first modification (the save we see) or on open
      (needs F15 to show the file being opened or held).

**§10.1b: Does FL Studio save atomically?** (F1, F2, F13, F14)

- Save-detection rule for `mv-watch` (PLAN task 11): which event pattern
  marks a finished save, which temporary names to ignore, and how long to wait
  for size and mtime to settle.
- Bounces: whether a render can be mistaken for a finished file before it ends.

**Template and layout (PLAN task 8)**

- `.gitignore` entries for FL backups and autosaves (F3, F7, F21 to F23), and
  whether they land inside the project folder.
- Where recordings go and whether the layout rule `Audio/Recordings/` needs an
  FL setting (F8).
- Whether FL's own project-folder feature (F4) fits or fights the Musiverion
  layout.

**Portability lint (SPEC §7.3)** (F9 to F12)

- What the lint must flag (absolute paths outside the repo) and whether FL's
  "Project data files" export can be offered as the fix.

**Get latest and open projects (SPEC §7.4, report "Desktop client")** (F15, F19, F20)

- Whether Get latest must refuse while the project is open, and whether the
  open-handle check (lsof / Restart Manager) can detect that, or the app needs
  another signal (e.g. FL's window title).

**SPEC changes needed**

- (list each change with the finding that requires it)

## Ableton Live

Not run yet. Protocol to write: `tools/fsprobe/protocols/ableton.toml`
(see `tools/fsprobe/protocols/README.md`, "Adding a DAW").

Questions specific to Live: the `Backup/` folder next to the `.als`; Collect All
and Save (what moves into `Samples/`); where new recordings live until the set
is saved (report: a temp folder); whether `Ableton Project Info/` is written on
every save; `.als` is gzip, so check that saves stay `in_place`/`temp_then_rename`
like other files.

| # | Question | Steps | Observed | Notes |
|---|---|---|---|---|
| | Save pattern of the `.als` | | | |
| | Backup folder contents and naming | | | |
| | Recording location before and after save | | | |
| | Read-only `.als` before opening / while open | | | |
| | File replaced while open | | | |
| | Files held open | | | |

Design consequences: (fill as for FL Studio)

## Reaper

Not run yet. Protocol to write: `tools/fsprobe/protocols/reaper.toml`.

Questions specific to Reaper: `.rpp-bak` and timestamped backups; autosave
interval and location; `.RPP-PROX` and peak files (`.reapeaks`); the `.rpp` is
text, so check whether saves rewrite it in place.

| # | Question | Steps | Observed | Notes |
|---|---|---|---|---|
| | Save pattern of the `.rpp` | | | |
| | Backup and autosave files | | | |
| | Peak and proxy files | | | |
| | Read-only `.rpp` before opening / while open | | | |
| | File replaced while open | | | |
| | Files held open | | | |

Design consequences: (fill as for FL Studio)

## Logic Pro

Not run yet. Protocol to write: `tools/fsprobe/protocols/logic.toml`.

Questions specific to Logic: `.logicx` is a folder bundle, so watch inside it
(`Alternatives/*/ProjectData`, `Autosave/`, `Undo Data`, resources); which
files inside change on a save; whether read-only on `ProjectData` alone breaks
saving or autosave; how "Save a copy as" and alternatives behave.

| # | Question | Steps | Observed | Notes |
|---|---|---|---|---|
| | Files inside the bundle changed by a save | | | |
| | Autosave files inside the bundle | | | |
| | Read-only `ProjectData` before opening / while open | | | |
| | Bundle replaced while open | | | |
| | Files held open | | | |

Design consequences: (fill as for FL Studio)
