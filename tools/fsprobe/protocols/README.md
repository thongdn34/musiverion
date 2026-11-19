# fsprobe protocols

A protocol is a TOML file that tells `fsprobe run` which folders to watch, what
the musician does in each step, which helper actions the tool runs, and which
questions it asks. Adding a DAW (Ableton Live, Reaper, Logic Pro, ...) needs
only a new protocol file, not code changes.

Check a protocol for mistakes before giving it to anyone:

```sh
cargo run -p fsprobe -- check --protocol tools/fsprobe/protocols/my-daw.toml
```

Unknown keys are errors, so typos are caught. The `shipped_protocols_are_valid`
unit test also loads every `*.toml` in this folder.

## Top level

| Key | Required | Meaning |
|---|---|---|
| `schema` | yes | Must be `1`. |
| `id` | yes | Stable id, e.g. `fl-studio`. A results folder belongs to one protocol id. |
| `title` | yes | Shown at the start. |
| `daw` | yes | DAW name, copied to `environment.json` (`daw.name`). |
| `revision` | no | Protocol revision, recorded in the results. Bump it when steps change. |
| `intro` | no | Shown once, at the start of a new session. |
| `outro` | no | Shown when every step is done. |
| `[settings]` | no | See below. |
| `[[watch]]` | no | Folders to ask for when `--watch` is not given. |
| `[targets.NAME]` | no | Named files that actions refer to. |
| `[[steps]]` | yes | The steps, in order. |

### `[settings]`

| Key | Default | Meaning |
|---|---|---|
| `quiet_ms` | `3000` | After the user says "done", nothing may change for this long before the step closes. |
| `max_wait_ms` | `120000` | Give up waiting for quiet after this long and carry on. |
| `ignore` | `[]` | File-name globs (or relative-path globs with `/`) left out of the summaries, e.g. `.DS_Store`, `._*`. They stay in `trace.jsonl`. |

### `[[watch]]`

Asked in order at the start of a new session. With `--watch`, the folders given
on the command line take these labels in the same order.

| Key | Meaning |
|---|---|
| `id` | Used by `targets.*.root`, `copy_in.root` and the `{watch.ID}` placeholder. |
| `label` | Short name shown in prompts and summaries ("Test folder"). |
| `explain` | Why the folder is watched and how to find it. |
| `suggest`, `suggest_macos`, `suggest_windows`, `suggest_linux` | Suggested path (`~` allowed); the OS-specific one wins. Enter accepts it. |
| `create` | Offer to create the folder if it is missing. |
| `subfolders` | Created inside it (with `create = true`). |
| `expect_empty` | Warn when the folder already contains files. |
| `optional` | The user may type `skip`. |

Paths in summaries are relative to the first watched folder; files in other
folders are shown as `Label: relative/path`.

### `[targets.NAME]`

A file that actions work on. The first time an action needs it the tool finds
candidates, asks the user to pick when there are several (Enter takes the
suggested one, which is the `prefer` match or else the newest), and remembers
the choice for the rest of the session (in `answers.json`).

| Key | Meaning |
|---|---|
| `label` | How the file is described ("the test project file (Song A.flp)"). |
| `pattern` | Glob relative to the watched folders. Without `/` it matches the file name in any subfolder; with `/` it matches the whole relative path. `*`, `?` and `**` are supported. |
| `root` | Only look in the watched folder with this `id`. |
| `prefer` | Suggest the match with exactly this file name. |
| `search` | Absolute globs outside the watched folders (`~` allowed, one `*`/`?` level per path part), used by `copy_in`. If nothing matches, the user can drag a file into the terminal. |

## `[[steps]]`

| Key | Default | Meaning |
|---|---|---|
| `id` | required | Stable id; answers are stored under it. No dots. |
| `number` | position | Label shown as "Step 7b of 15". The "of N" is the last step's number without letters. |
| `title` | required | Short title. |
| `text` | `""` | Instructions. |
| `macos`, `windows`, `linux` | none | Extra text appended on that OS. |
| `purpose` | none | What the step settles; shown in `summary.md`, not to the user. |
| `optional` | `false` | Marked optional on screen. |
| `user_action` | `true` | `true`: "press Enter to start", helper actions, "press Enter when finished", wait for quiet. `false`: no prompts, only actions and questions. |
| `start_prompt`, `done_prompt` | standard | Replace the two prompts. |
| `when` | `{}` | Run only if earlier answers match: `{ "s00.trial" = "yes" }`. Otherwise the step is recorded as not applicable. |
| `unless` | `{}` | Leave the step out if any of these earlier answers match. Prefer `unless` for "skip for X" so that an unanswered question does not drop the step. |
| `before` | `[]` | Helper actions run right after the start marker (before the user acts). |
| `after` | `[]` | Helper actions run after the user says "done" and the files are quiet. |
| `quiet_ms` | settings | Per-step override. |
| `[[steps.questions]]` | `[]` | Asked after the step window closes. |

Order inside a step: instructions, start prompt, **start marker**, `before`
actions, done prompt, **done marker**, wait for quiet, `after` actions, **end
marker**, questions, one-line summary of what the tool saw. The step's
classification covers the records between the start and end markers.

### Placeholders

In `text`, the OS extras and question `ask` strings:

| Placeholder | Becomes |
|---|---|
| `{mod}` | `Cmd` on macOS, `Ctrl` elsewhere |
| `{opt}` | `Option` on macOS, `Alt` elsewhere |
| `{os}` | `macOS`, `Windows` or `Linux` |
| `{watch.ID}` | Absolute path of that watched folder |
| `{answer.STEP.QUESTION}` | An earlier answer, or "(not answered)" |

Unknown placeholders are left as written.

### Questions

| Key | Meaning |
|---|---|
| `id` | Unique within the step. |
| `ask` | The question. |
| `type` | `yes_no`, `choice` or `text`. Every type accepts `skip`; an empty text answer counts as skipped. |
| `options` | Choices for `choice` (number or exact text accepted). |
| `other` | `choice` only: add "Something else" with free text (stored as `other: ...`). |
| `when` | Ask only if answers match. Keys without a dot are questions earlier in the same step (`{ warned = "yes" }`); `"step.question"` refers to earlier steps. Matching ignores case. |
| `env` | Also copy the answer to `environment.json` under `daw.<env>`. |

## Helper actions

Each action is an inline table, e.g. `{ action = "set_readonly", target = "project" }`.
Actions that change or read files in the watched folders (`set_readonly`,
`clear_readonly`, `restore_file`, `copy_in`, `backup_file`,
`compare_with_backup`) are tagged in the trace and left out of the step's
classification, so the summary shows what the DAW did, not the tool. A failed
action is reported in plain language and the step carries on.

| Action | Keys | What it does |
|---|---|---|
| `set_readonly` | `target` or `pattern` | Makes the file read-only the way Git LFS does for `lockable` files (macOS/Linux: removes all write bits; Windows: read-only attribute). |
| `clear_readonly` | `target` or `pattern` | Gives the owner write access back (Windows: clears the attribute). |
| `list_open_handles` | none | macOS/Linux: runs `lsof` and records which processes have files open inside the watched folders (process, pid, access mode, path). Windows: records "not supported yet". |
| `snapshot_tree` | none | Saves every file under the watched folders (size, mtime, read-only) to `snapshots/<step>-<n>.json`. |
| `backup_file` | `target`/`pattern`, `slot` | Copies the file to the backups folder beside the results folder (not inside it, so no project data is sent back). |
| `restore_file` | `slot`, `method` | Puts the copy back over the original. `replace` (default) deletes the file and writes the copy as a new file, like a git checkout; `overwrite` rewrites the same file in place. |
| `compare_with_backup` | `slot` | Says whether the file still equals the copy, and whether it is read-only now. |
| `stat_file` | `target`/`pattern` | Shows size, read-only flag and modification time. |
| `wait` | `seconds`, `message` | Countdown while the DAW works (for autosave); Enter ends it early. Recording continues. |
| `copy_in` | `target` (with `search`), `to`, `root` | Copies a file from outside into a watched folder, replacing what is there (used for the FL Studio trial workaround). |

`slot` defaults to `default`. Slots and remembered targets survive a pause.

## Results folder

| File | Content |
|---|---|
| `trace.jsonl` | Every record, one JSON object per line: `run_start`, `event` (kind, paths relative to a watch root, fresh stat and previous stat per path, rename cookie, `tool` tag), `poll` (changes found by polling recently touched files every 200 ms), `final` (stat at step end), `marker`, `action`, `watch_error`. Each has `run`, `seq`, monotonic `t_us` and wall-clock `wall`. |
| `answers.json` | Session state: watched folders, runs, remembered targets, backups, and per step its status, answers, action results and one-line summary. Resume uses it. |
| `summary.md` | Generated: overview table, changes between steps, per-step detail tables. Rebuild with `fsprobe report --out <folder>`. |
| `environment.json` | OS and version, filesystem type and cloud-sync hint per watched folder, tool version, protocol, DAW answers marked with `env`, runs. |
| `protocol.toml` | Copy of the protocol that was run. |
| `snapshots/` | Output of `snapshot_tree`. |

## How changes are classified

Per file, between a step's start and end markers: `temp_then_rename` (with the
temporary name as a pattern such as `{name}.tmp`, or "outside the watched
folders"), `in_place_write`, `delete_then_create`, `moved_aside_then_create`
(old file renamed to a backup, new one written), `new_file`, `renamed`,
`removed`, `transient` (appeared and disappeared), `metadata_only` (read-only
flag or permissions only), `unchanged`, and `unclassified` (raw sequence kept in
`summary.md`). Each also reports settle time (first sign of writing until size
and mtime stop changing), how many separate bursts happened, and the read-only
flag before and after.

The classifier relies on file identity (inode) and existence from the stats
first, and on event kinds second, because macOS FSEvents reports bare "renamed"
flags without pairing the two sides and can repeat flags left over from
earlier changes to the same path. Linux rename halves are paired by cookie and
Windows by order.

## Adding a DAW

1. Copy `fl-studio.toml` to `<daw>.toml`, change `id`, `title`, `daw`.
2. Adjust `[[watch]]` (project folder, the DAW's backup/autosave/recording folders) and `[targets]` (project file pattern, e.g. `*.als`, `*.rpp`; for Logic's `.logicx` bundle, a pattern inside it).
3. Rewrite the step texts with that DAW's menu names; keep the step ids meaningful, because `docs/validation/daws.md` refers to findings by step.
4. Mark menu names you could not verify with "(check)".
5. Run `fsprobe check`, then a short dry run yourself.
