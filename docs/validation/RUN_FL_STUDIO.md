# Test how FL Studio saves files

This guide is for the musician who runs the FL Studio experiment. You do
ordinary things in FL Studio (save, render, record, quit) while a small tool
called **fsprobe** writes down which files appear, change or disappear. The
tool tells you every step on screen. At the end you send back one folder.

Nothing here changes your real projects. The test uses a new, empty folder.

## Before you start

1. **FL Studio is installed and has been started once** (so it has created its
   data folder). A licensed (unlocked) FL Studio gives the most complete
   results. The free trial also works, with a workaround (see
   [If you use the FL Studio trial](#if-you-use-the-fl-studio-trial)).
2. **You have the Musiverion folder** on your computer (for example
   `~/Documents/musiverion`).
3. **Rust is installed.** Open Terminal (macOS) or PowerShell (Windows) and
   type `cargo --version`. If it prints a version, you are set. If not:
   - macOS: run `xcode-select --install` and follow the prompts, then install
     Rust from <https://rustup.rs> (copy the command shown there into Terminal).
   - Windows: download and run `rustup-init.exe` from <https://rustup.rs>. If it
     offers to install the Visual Studio build tools, say yes.
   Close and reopen the terminal afterwards.
4. **Set aside about 60 to 90 minutes.** Three steps include waits (3 minutes,
   2.5 minutes, and an optional 1 minute) so that FL Studio's autosave can run.
   The first start also takes a few minutes while the tool is built.
5. Close other programs you do not need, especially anything that saves into
   your Documents folder.

## Start

In the terminal, go to the Musiverion folder (adjust the path if yours is
elsewhere), then run the one command:

```sh
cd ~/Documents/musiverion
cargo run -p fsprobe --release -- run --protocol tools/fsprobe/protocols/fl-studio.toml --out validation-results/fl-studio
```

The tool then asks for two folders:

- **Test folder**: press Enter to accept the suggestion
  `~/Music/musiverion-test/Song A`, and type `yes` to let the tool create it
  with the subfolders `Project`, `Audio` and `Bounces`. Do not use a folder that
  iCloud Drive, OneDrive or Dropbox syncs.
- **FL Studio data folder**: FL Studio keeps backups, autosaves and recordings
  there. The default is `~/Documents/Image-Line`, so press Enter. If yours is
  elsewhere, find it in FL Studio under *Options > File settings > User data
  folder*. On Windows it may be inside OneDrive, e.g.
  `C:\Users\you\OneDrive\Documents\Image-Line`. You can type a path or drag the
  folder from Finder or Explorer into the terminal window.

On a Mac, macOS may ask whether Terminal may access your Documents or Music
folder. Click **Allow**, otherwise the tool cannot see what happens there.

After that, follow the steps on screen. Keep the terminal next to FL Studio.

## During the session

- **Enter** moves on. Most steps have two Enters: one to start the step and one
  when you have finished it. After the second Enter the tool waits a few
  seconds until FL Studio has finished writing files, then asks its questions.
- **skip**: type it if a step or question is impossible or does not apply. That
  is a useful result too. You can add a short reason.
- **(check)** next to a menu name means it could not be confirmed in
  Image-Line's manual. If the name is different on your computer, say so in
  that step's answers or in the last step.
- After each step the tool prints **What the tool saw**, for example
  `Project/Song A.flp was replaced via a temporary file: Song A.flp.tmp -> Song A.flp`.
  You do not need to do anything with it.
- Some steps change the test project's **read-only** flag or put an older copy
  back. That is on purpose: it simulates what versioning will do.

## Pause and continue

- Type **pause** at any prompt (or just close the terminal window, or press
  Ctrl+C). Everything up to the last finished step is saved.
- To continue, run **the same command** again (press the Up arrow key in the
  terminal to get it back). The tool continues with the first step you have
  not finished. A step you were in the middle of starts again from its
  beginning.
- To start over completely, use a different folder name at the end of the
  command, e.g. `--out validation-results/fl-studio-2`.

## If you use the FL Studio trial

Image-Line's terms, as of November 2026: the trial is "fully functional and
never expires", and you "can save your projects while in Trial", but "you won't
be able to reopen those saved projects until you purchase FL Studio"
([FL Studio manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/app_feature.htm),
[download page](https://www.image-line.com/fl-studio-download/)). The limit only
applies to files the trial saved: projects saved by a licensed copy can be
opened ([Image-Line knowledge base](https://support.image-line.com/action/knowledgebase?ans=672)),
and the trial can open the demo songs that come with FL Studio.

These steps need to reopen a project:

| Step | What it needs | With the trial |
|---|---|---|
| 7d | Reopen the project after renaming the outside sound | Left out automatically (no workaround) |
| 10b | Open the read-only project | Works with the workaround below |
| 11b | Open the project again | Works with the workaround below |
| 12a to 12c | Keep the project open; FL may offer to reload | Works (the project stays open) |
| 13b | Open a recovered autosave, if FL offers one | The offer is recorded; opening it may be refused. Say what happens |
| 14 (optional) | Open the project before force-quitting | Open the demo song copy |

**The workaround is built in.** In step 0 answer *yes* to "Is this the free
trial". Before steps 10b and 11b the tool then copies one of FL Studio's demo
songs into the test folder as `Project/Song A.flp` (steps 10t and 11t). On
macOS it looks in `/Applications/FL Studio*.app/Contents/Resources/FL/Data/Projects/Demo songs/`;
on Windows in `C:\Program Files\Image-Line\FL Studio*\Data\Projects\Demo songs\`
(check). If it finds none, it asks you to drag a demo `.flp` into the terminal.
If FL Studio refuses to open the demo copy, type skip for that step.

A licensed FL Studio still gives the most complete results, because step 7d
only works with it.

## What to send back

When the tool says **All done**, it shows the folder to send, normally:

```
~/Documents/musiverion/validation-results/fl-studio
```

Zip it and send it. It contains:

- `summary.md`: a readable overview of every step
- `trace.jsonl`: the detailed log of file activity
- `answers.json`: your answers
- `environment.json`: your OS, FL Studio version and settings, and the type
  of disk
- `protocol.toml` and `snapshots/`: the steps that were run, and file lists

It contains **file names, folder paths** (which include your user name, e.g.
`/Users/yourname/...`), sizes, times and your answers. It contains **no audio
and no project contents**.

Do **not** send `validation-results/fl-studio-backups`: it holds copies of your
test project. You can delete it, together with the test folder
`~/Music/musiverion-test` and `~/Desktop/outside-sample.wav`, when you are done.

If the tool stops with a message starting with **Problem:**, send that message
along with the folder. Nothing recorded so far is lost.

## What we need to learn and why

| Step | What we learn | Design decision it settles |
|---|---|---|
| 0 | FL version and edition; autosave, backup location and maximum; user data folder; cloud sync | Context for every result; whether FL's backup location setting changes the template (SPEC §10.1) |
| 1 | How the first save writes `Song A.flp`; whether FL proposes its own project folder | Save detection in the watcher (SPEC §7.2, PLAN task 11); FL project layout and template (task 8) |
| 2 | Pattern and settle time of ordinary saves; whether each save also writes a backup | Debounce and settle rule (task 11); backup ignore rules (task 8) |
| 3 | Name of "Save new version" copies | Which new project files the app tracks and locks; version-copy naming in the template |
| 4 | "Save as" in the same folder; is the old file touched? | Detecting a project that changes name; lock on the new file |
| 5 | Where autosaves and backups go and how they are named | `.gitignore` for FL backups; autosave must not trigger auto-lock or "unsaved changes" (SPEC §7.2) |
| 6 | Where recordings land (project folder or FL data folder) | Layout rule for `Audio/Recordings`; portability lint (SPEC §5, §7.3) |
| 7a to 7d | Whether outside media gets copied; what "Project data files" and "Zipped loop package" produce; what happens when a sample goes missing | Portability lint before Save version (SPEC §7.3) |
| 8a, 8b | How a render writes and overwrites a WAV, and how long it takes to settle | The watcher must never version a half-written bounce; changed file vs replaced file for bounces |
| 9a, 9b | Which files FL keeps open while idle and while playing | Whether Get latest must ask to close the project first (open-handle check before checkout) |
| 10a to 10b | FL with a read-only project file before opening it: warning, save error, silent failure, Save as? Does the flag survive a save? | Whether `lockable` (read-only until checked out) works for `.flp`, and the auto-lock trigger (SPEC §10.1) |
| 11a to 11d | FL when the file becomes read-only while open, including autosave | Behaviour after a forced unlock or lost lock (SPEC §7.4, §10.1) |
| 12a to 12c | Does FL notice a newer file arriving while it is open, and does its next save overwrite it? | Whether Get latest may update an open project; conflict-copy path (SPEC §7.4) |
| 13a, 13b | Files written when quitting without saving; recovery offered on restart | Ignore rules; no false "unsaved changes" after quitting |
| 14 (optional) | Recovery files after a crash | Ignore rules; whether recovery files need special handling |
| 15 | Anything unexpected; corrected menu names | Protocol fixes for the next DAW run |
