# DAW Project File Formats and Git Diff/Version/Merge Feasibility

Scope: internal project-file structure per DAW, how media references are stored, existing parsers, the DAWproject interchange format, git integration (textconv, diff/merge drivers, locking, ignore lists), plugin state blobs, and prior art on git-versioning DAW sessions. Current as of Nov 2026. Items marked "(training knowledge, unverified)" in Inferences were not confirmed by a fetched source in this session. Treat them as leads to verify, not facts.

---

## 1. Per-DAW project format, documentation status, parsers, and semantic diff

### Takeaway
Only three native formats are practically text-diffable: Reaper `.rpp` (plain text), Ardour `.ardour` (plain XML), and Ableton `.als` (gzipped XML). Studio One `.song` is a ZIP of XML parts, so it can be diffed after unzipping. FL Studio `.flp` is a binary event stream, but the parser ecosystem is mature (pyflp, dawtool, and flpdiff, which already ships a git diff driver). Logic, GarageBand, Pro Tools, Cubase/Nuendo, Bitwig and Reason are proprietary binaries with little or no open parsing support. For those, the realistic options are metadata-only extraction or a "binary + lock" workflow.

### Cited Findings

**Ableton Live (.als): gzipped XML**
- An `.als` file is gzip-compressed XML. Decompress with `gzip -cd MyProject.als > MyProject.xml` and recompress with `gzip -S .als MyProject.xml > MyProject.als` (then rename). The author warns that if you don't recompress with gzip, Ableton "may bark about a corrupted project file." — [Sample-Offline/ableton-under-the-hood](https://github.com/Sample-Offline/ableton-under-the-hood)
- Sample references are stored in `<SampleRef>` → `<FileRef>`, with children `RelativePathType` (example value 3), `RelativePath`, `Path`, `Type`, `LivePackName`, `LivePackId`, `OriginalFileSize` and `OriginalCrc`. Sibling elements are `LastModDate`, `SourceContext`, `SampleUsageHint`, `DefaultDuration` and `DefaultSampleRate`. To repoint a sample, edit `<Path Value>`. — [ableton-under-the-hood](https://github.com/Sample-Offline/ableton-under-the-hood)
- Simple gunzip/gzip round-tripping (`cat project.als | gunzip > project.xml`; `cat project.xml | gzip > project.als`) is all it takes to get a text version. — [dayvonjersen/ableton-project-version-control-thing](https://www.github.com/dayvonjersen/ableton-project-version-control-thing)
- One analysis warns that Ableton stores a HexValue for every VST inserted on a track, so a plain text diff is not optimal. — summarized in search results pointing to the [Ableton forum "Decoding ALS file format"](https://forum.ableton.com/viewtopic.php?p=1434673) thread (not fetched directly)
- Parsers:
  - dawtool (offlinemark) has reverse-engineered parsers for Ableton `.als` v8–12 and FL Studio `.flp` (v10–11, 20), plus cue sheets. It is Python 3.7+. Tempo automation is accurate only when the automation is linear. — [offlinemark/dawtool](https://www.github.com/offlinemark/dawtool)
  - bravoh-daw is a Rust crate that parses `.als`, `.flp`, `.logicx` and `.rpp` into one `ParsedIntelligence` schema: daw/version, bpm, time signature, duration, swing, tracks (name/type/plugins), plugins, samples, midi_tracks, markers, automated_params, and mixer counts with a sidechain flag. Its policy is "never guess". — [docs.rs bravoh-daw 0.1.0](https://docs.rs/crate/bravoh-daw/0.1.0)
  - DawVert reads and writes Ableton Live 11 `.als`. — [SatyrDiamond/DawVert](https://github.com/SatyrDiamond/DawVert)

**FL Studio (.flp): binary event stream, undocumented**
- PyFLP is an unofficial Python parser that can read and write (`pyflp.parse()` / `pyflp.save()`) on CPython/PyPy 3.8+. Its 2.0 notes say the FLP format is "essentially undocumented" and that every new FL version may break the parser. It credits Monad.FLParser as prior art. — [PyPI pyflp](https://pypi.org/project/pyflp), [pyflp 2.0.0a0](https://pypi.org/project/pyflp/2.0.0a0/)
- flpdiff (dawhubapp) is a clean-room TypeScript/Bun parser that produces semantic diffs grouped by category. Example output: `~ Channel sampler 'Kick' modified … Channel volume 78% → 100%`; `+ Added channel sampler 'Dub Vocal' (sample: vocals/dubmix.mp3)`; similar clips are collapsed, e.g. `+ 9 clips of 'Dub Vocal' added (length 5.854 beats, beats 62 … 206)`. Exit codes are 0/1/2, so it works in CI. Other features: `info`, `--format json|canonical`, and `flpdiff git-setup`, which configures `diff.flp.command` plus `.gitattributes` and supports `--global`, `--lfs` and `--textconv`. It covers FL 9–25 and claims a 100% parse success rate on 85 real projects. MIT licensed. A hosted collaboration product, dawhub.app, is built on it. — [dawhubapp/flpdiff](https://github.com/dawhubapp/flpdiff)
- Other FLP parsers: FLParser (.NET, based on andrewrk's node-flp) and ekkx/flparser. — search results listing [monadgroup/FLParser](https://awesome.ecosyste.ms/projects/github.com%2Fmonadgroup%2FFLParser), [ekkx/flparser](https://github.com/ekkx/flparser)
- DawVert reads and writes `.flp`. — [DawVert](https://github.com/SatyrDiamond/DawVert)

**Logic Pro (.logicx): macOS package with an undocumented binary core**
- Since Logic Pro X, the project file is a package (bundle), which is a folder that looks like a file. Projects can also be saved as a plain folder. — [Library of Congress format description](https://loc.gov/preservation/digital/formats/fdd/fdd000640.shtml); [Apple Logic help: Projects overview](https://help.apple.com/logicpro/mac/10.1/en.lproj/lgcpce072a17.html)
- Layout: `Project.logicx/Alternatives/000/` contains `MetaData.plist` and the binary `ProjectData`, plus a `Resources` folder. The optional `Media` folder holds audio/video (it is absent if media is not stored with the project). — [geoffmyers/logicx-analyzer (via proxy)](https://proxy-dev.blitzz.co/proxy/123456/github.com/geoffmyers/logicx-analyzer); [LoC](https://loc.gov/preservation/digital/formats/fdd/fdd000640.shtml)
- `Alternatives/000/ProjectData` is an undocumented little-endian binary blob. — [jonkubis/LogicProFormatWriter](https://github.com/jonkubis/LogicProFormatWriter)
- Alternatives are stored inside the project and share the same assets. — [Apple: New concepts in Logic Pro X](https://help.apple.com/logicpro/mac/10.1/en.lproj/lgcp9a5ac3ca.html)
- bravoh-daw extracts only what string-level analysis of ProjectData yields: plugin chains, samples, markers and routing. — [bravoh-daw](https://docs.rs/crate/bravoh-daw/0.1.0)
- Sync tools that don't understand bundles (Dropbox, for example) can turn `.logicx` packages into ordinary folders that Logic then won't open. — [Dropbox forum](https://www.dropboxforum.com/discussions/101001014/logic-pro-x-files-being-converted-to-folders-cant-open/364580); [Apple discussions](https://discussions.apple.com/thread/8317771)

**Pro Tools (.ptx / .ptf / .pts): proprietary binary**
- ptformat (libptformat, LGPL-2.1, by Damien Zammit and Robin Gareus) is a C++ library that reads Pro Tools sessions. It extracts audio and MIDI sources, regions and tracks, plus MIDI note events. Ardour uses it for Session > Import PT Session. — [zamaudio/ptformat](https://www.github.com/zamaudio/ptformat); [Ardour manual: Interchange](https://manual.ardour.org/working-with-sessions/interchange-with-other-daws/)
- The Ardour manual calls PT import "basic" and incomplete. PT 5 and 8–11 were tested "with varying degrees of success"; versions 6–7 are unsupported. Takes are imported as separate tracks. — [Ardour manual](https://manual.ardour.org/working-with-sessions/interchange-with-other-daws/); [Ardour tracker](https://tracker.ardour.org/view.php?id=7212)
- Session folder layout: an `Audio Files` folder (created by default), `Session File Backups` (autosaves, with count and frequency set in Preferences > Operation), and `WaveCache.wfm` (waveform display cache, rebuilt automatically if deleted). — [Educator.com Pro Tools session folders](https://www.educator.com/music-theory/introduction-to-pro-tools/edler/session-folders.php); [Avid WaveCache help](https://apps.avid.com/proToolsFirstHelp/version12.0/enu/Pro%20Tools%20First%20Help/sess6.Workspace.16.065.html); [Puremix file management](https://www.puremix.com/blog/john-paterno-file-management)

**Cubase / Nuendo (.cpr / .npr): RIFF-like binary**
- A `.cpr` is "almost" a RIFF file: there is an extra `NUND` tag after the RIFF header, sizes are big-endian, and strings are prefixed with a 32-bit big-endian length. The open-source Rust/nom parser cbconv is a work in progress. — [cbconv (code.volse.net)](https://code.volse.net/audio/cbconv.git/about); [cbconv main.rs](https://code.volse.net/audio/cbconv.git/plain/src/main.rs)
- Byte-order conflict: standard RIFF uses little-endian chunk sizes ([Kaitai RIFF](https://formats.kaitai.io/riff)), while cbconv reports big-endian. Verify against real files.
- Since Cubase/Nuendo 13.0.30 the software detects whether a project uses the old 32-bit layout or a new 64-bit one; the original format was not designed for files over 2 GB. A parser must handle both layouts. — [Steinberg help: Invalid project after saving](https://www.steinberg.help/r/cubase/nuendo-invalid-project-after-saving)
- fileinfo.com's claim that CPR is XML appears to be wrong; the RIFF evidence contradicts it. — [fileinfo.com .cpr](https://fileinfo.com/extension/cpr) vs [cbconv](https://code.volse.net/audio/cbconv.git/about)
- OwlPlug has an open issue requesting Cubase project support, which reflects how hard this format is to parse. — [OwlPlug #218](https://www.github.com/DropSnorz/OwlPlug/issues/218)

**Studio One / Fender Studio Pro (.song): ZIP of XML**
- A `.song` is a ZIP archive of XML files; you can rename it to `.zip` to browse it. — [filext.com .song](https://filext.com/file-extension/SONG)
- OwlPlug's Studio One reader extracts `metainfo.xml`, `Devices/audiomixer.xml` and `Devices/audiosynthfolder.xml`, and reads plugin names from `classInfo`. — [OwlPlug PR #394 (mirror)](https://gh.r-mj.com/DropSnorz/OwlPlug/pull/394)
- Other entries reported: `notepad.xml`, `settings.xml`, `Devices/mixerconsole.xml`, and `Song`, `Presets` and `Envelopes` folders. This listing is generic and was not confirmed against PreSonus docs. — [filext.com](https://filext.com/file-extension/SONG)
- Community tool SongLens browses metadata in Studio One 7 and Fender Studio 8 `.song` files. One forum post shows an XML parse error from an undeclared entity, so the XML is not always strictly well-formed. — [studiooneforum.com SongLens](https://studiooneforum.com/threads/songlens-a-free-open-source-windows-desktop-tool-for-browsing-metadata-from-presonus-studio-one-7-and-fender-studio-8-song-files.2313/)
- No official schema exists for the native `.song` format.

**Reaper (.rpp): plain text chunk format**
- RPP is plain text, a nested angle-bracket chunk markup, and can be opened in a text editor. — [fileinfo.com .rpp](https://fileinfo.com/extension/rpp); [bravoh-daw](https://docs.rs/crate/bravoh-daw/0.1.0) ("plain-text chunk format")
- Plugin state sits inside `<VST …>` blocks as base64-encoded data. — [Atlas Audio Reaper extractor](https://file-atlas-app-website.pages.dev/docs/extractors/atlas-audio/pack-atlas-audio-reaper); [X-Raym ReaScript gist](https://gist.github.com/X-Raym/448e8afea7d91bce96b520ca12ddc698)
- ReaperSharp warns that its parser may crash on projects with heavy binary VST param data and that it can't handle Kontakt tracks. — [ReaperSharp README (mirror)](https://github.blesdmm.dns-dynamic.net/BuriedStPatrick/ReaperSharp/blob/master/README.md)
- Parsers:
  - Perlence/rpp: a Python parser/emitter (PLY) with `rpp.loads()`/`rpp.dumps()` and an Element tree supporting find/findall/iterfind. — [perlence/rpp](https://github.com/perlence/rpp)
  - rppxml: C++ on WDL with Python bindings. Its author benchmarks a 14.8 MB file at 1.48 s versus 19.1 s for Perlence/rpp (self-reported). — [PyPI rppxml](https://pypi.org/project/rppxml/)
  - Also npm `rppp` and the Rust `rpp-parser`. — [npm rppp](https://www.npmjs.com/package/rppp); [docs.rs rpp-parser](https://docs.rs/rpp-parser/latest/src/rpp_parser/parser.rs.html)
  - Writers: reathon and py-pyper. reapy controls a live REAPER via ReaScript rather than parsing files. — [jamesb93/reathon](https://github.com/jamesb93/reathon); [python-reapy](https://pypi.org/project/python-reapy/0.10.0)
  - DawVert outputs Reaper. git-moss ProjectConverter converts between Reaper and DAWproject. — [DawVert](https://github.com/SatyrDiamond/DawVert); [bitwig/dawproject README](https://github.com/bitwig/dawproject)
- Subprojects render an `.RPP-PROX` file next to the `.rpp` on every save. — [Sound On Sound: Reaper subprojects](https://www.soundonsound.com/techniques/reaper-subprojects)
- FMOD's Reaper integration already treats `.rpp` as a source-controlled asset and re-renders linked assets whenever the file changes, including after a source-control sync. — [FMOD docs: Reaper integration](https://fmod.com/docs/2.03/studio/appendix-b-reaper-integration.html)

**Bitwig Studio (.bwproject): proprietary, undocumented**
- Project files use the `.bwproject` extension. No public documentation or reverse-engineering of the internals was found. A KVR user's opinion is that Bitwig is unlikely to publish it. — [Bitwig support: Working with Projects](https://bitwig.freshdesk.com/support/solutions/folders/15000005461); [KVR thread](https://www.kvraudio.com/forum/viewtopic.php?p=6956604)
- DAWproject's stated non-goals include being any DAW's native format, so it is a separate export, not the save format. — [bitwig/dawproject](https://github.com/bitwig/dawproject)

**Ardour (.ardour): plain XML, open source**
- The main session file `my_session.ardour` is XML. Ardour periodically writes a `.bak` copy and keeps `.history` (undo), `interchange/` (imported and recorded audio/MIDI), `peaks/`, `analysis/`, `export/` and `dead sounds/`. Copy the whole session folder, not just the `.ardour` file, and don't rename internal files. — [Ardour manual: What's in a Session](https://manual.ardour.org/working-with-sessions/whats-in-a-session); [Ardour tutorial: saving a session](https://git.ardour.org/ardour/ardour-tutorial/src/branch/main/content/saving-sessions/saving-a-session/index.en.md)
- Mixbus, which is built on Ardour, writes `.pending` snapshots when recording starts so audio can be recovered after a crash. — [Harrison Mixbus manual: session folder layout](https://rsrc.harrisonconsoles.com/mixbus/mixbus-live-manual/9/en/topic/power-user-topics)

**Reason (.reason) and GarageBand (.band)**
- `.reason` is the primary project format of modern Reason; older songs use `.rns`. It is proprietary and binary. Only third-party extension sites describe it and none documents the internals. — [filext.com .reason](https://filext.com/file-extension/REASON); [fileinfo.com .reason](https://fileinfo.com/extension/reason)
- No source on GarageBand `.band` internals was found in this session (see Gaps).

### Inferences
- Tiering for a git collaboration app:
  - Tier A (real line diff plus a semantic layer): Reaper, Ardour, Ableton (after gunzip).
  - Tier B (structured semantic diff via a parser, no line diff): FL Studio (flpdiff/pyflp), Studio One (unzip, then XML), DAWproject exports.
  - Tier C (opaque; metadata or string extraction only): Logic, GarageBand, Pro Tools (ptformat gives tracks, regions and sources only), Cubase/Nuendo (cbconv is WIP), Bitwig, Reason.
- A semantic diff should be modeled on flpdiff's output style: category headers (Tempo/Transport, Tracks, Devices/Plugins, Clips/Arrangement, Mixer, Media refs), +/~/- markers, before→after values, and collapsing of repeated clip edits. bravoh-daw's unified schema is a ready-made cross-DAW summary model: tracks, plugins, samples, markers and automated params.
- Ableton .als XML detail (training knowledge, unverified):
  - Root is `<Ableton MajorVersion=… MinorVersion=… Creator="Ableton Live 12.x">`.
  - Objects carry numeric `Id` attributes, and there is a `NextPointeeId` counter.
  - VST2 state is in `<VstPluginInfo>…<Buffer>` hex; VST3 state is in `<Vst3PluginInfo>` processor/controller state blobs.
  - Many view-state fields (scroll positions, selected track, `LomId`, chooser bars) change on every save. This creates diff noise and makes line-level 3-way merges unsafe. The Id counters in particular collide when both branches add objects.
- Reaper detail (training knowledge, unverified):
  - Media is referenced as `<SOURCE WAVE FILE "Audio/foo.wav">`. Paths are stored relative when media lives under the project directory.
  - `.reapeaks` peak files, `.rpp-bak` and `.RPP-UNDO` are regenerable side files.
  - "Save as" offers "Copy all media into project directory".
  - Each track/item carries a GUID, which helps semantic matching.
- Logic/GarageBand detail (training knowledge, unverified):
  - Inside `.logicx` there are `Alternatives/NNN/Project File Backups/` and `Autosave` folders, plus `Resources/ProjectInformation.plist`.
  - GarageBand `.band` is a similar macOS package with a binary `projectData`.
  - Bundles must be handled as directories in git. Avoid tools that flatten them.
- Pro Tools `.ptx` (training knowledge, unverified): the file is XOR-obfuscated ("encrypted"), and ptformat decrypts it before parsing.

### Gaps
- No primary documentation on Bitwig `.bwproject`, Reason `.reason`, or GarageBand `.band` internals; no open parser was found for any of them.
- Ableton's own XML schema is undocumented. Specific noisy elements (LomId, Id counters) were not confirmed by a fetched source this session.
- No current (2026) maintenance status was checked for dawtool, pyflp (FL 2025 compatibility) or ptformat (PT 2024+ compatibility).
- The Studio One `.song` internal layout is only partly documented, by third parties.

---

## 2. DAWproject open interchange format

### Takeaway
DAWproject 1.0 (MIT, Bitwig + PreSonus, 2023) is a ZIP containing `project.xml` + `metadata.xml` (with XSD schemas), plus embedded or referenced audio and always-embedded plugin states. It is supported by Bitwig, Studio One/Fender Studio Pro, Cubase 14/Nuendo, Cubasis, VST Live and n-Track. Ableton, Apple (Logic), Avid (Pro Tools), Cakewalk/Sonar and native Reaper do not support it. Because it has a published schema it is the best neutral, diffable representation, but only for supporting DAWs and only as a lossy secondary export.

### Cited Findings
- Format: a ZIP container with UTF-8 XML parts `project.xml` and `metadata.xml`, schemas `Project.xsd` and `MetaData.xsd`, and a `Reference.html` DOM reference. The exporting DAW chooses the directory layout for media and plugin files. — [bitwig/dawproject](https://github.com/bitwig/dawproject)
- Content covered:
  - Tracks, clips and arrangement timelines.
  - Audio events, notes and note expressions.
  - Automation of tempo, time signature, MIDI messages, volume, pan, mute, sends, plugin params and built-in device params.
  - Generic EQ, compressor, gate and limiter devices.
  - Clip launcher clips and scenes.
- Excluded: low-level MIDI events and view settings/preferences.
- Plugin states are always embedded, referenced by path (e.g. `.clap-preset`). Audio can be embedded or referenced (e.g. `audio/….wav`). Spec version 1.0 is "stable". — [bitwig/dawproject](https://github.com/bitwig/dawproject)
- Supported DAWs listed in the repo: Bitwig Studio 5.0.9, Studio One 6.5, Cubase 14, Cubasis 3.7.1, VST Live 2.2 and n-Track Studio 10.2.2. Converters: DawVert and git-moss ProjectConverter (Reaper). — [bitwig/dawproject](https://github.com/bitwig/dawproject)
- Bitwig FAQ (as of Nov 6, 2024):
  - Bitwig 5.0.9 imports and exports. Studio One Alias Clips become regular clips, and Bitwig does not read Studio One AU plugin data.
  - Studio One 6.5 imports and exports. It does not read Bitwig Clip Launcher data or Bitwig CLAP plugin data, and generic devices were "coming soon".
  - The format is said to cover plugin states and automation for VST2, VST3, AU and CLAP.
  - [Bitwig DAWproject FAQ](https://www.bitwig.com/support/technical_support/system-requirements-62/)
- 2026 status (from secondary or vendor sources, so lower confidence):
  - Fender Studio Pro 8 (Jan 2026, the rebranded Studio One) carries DAWproject forward.
  - A Sept 2026 article says free Fender Studio is export-only, which conflicts with full import in Studio Pro.
  - Nuendo 14 supports it.
  - As of mid-2026 Ableton has no support and no announced plans. Apple and Avid have none. Cakewalk/Sonar has none, with BandLab favoring its own CXF.
  - Reaper is supported only via third-party tools.
  - Sources: [Doseedo: Does DAWproject support Logic or Ableton (2026)](https://doseedo.com/guides/dawproject-logic-ableton); [Doseedo: open .dawproject](https://doseedo.com/guides/open-dawproject-file); [Production Expert](https://www.production-expert.com/production-expert-1/dawproject-is-it-the-fix-weve-been-waiting-for); [Logic Pro Help forum feature request](https://www.logicprohelp.com/forums/topic/160570-dawproject-file-format/). Doseedo sells conversion tools; treat it as a vendor source.
- PreSonus KB article introducing DAWproject. — [PreSonus support](https://support.presonus.com/hc/en-us/articles/19743606863629-Introducing-DAW-Project)
- Libraries beyond the Java reference (Java 16+, Gradle): `dawproject-typescript` (npm) and the Rust `dawproject` crate (0.8.2). — [dawproject-typescript README](https://cdn.jsdelivr.net/npm/dawproject-typescript@1.0.7/README.md); [rust dawproject README](https://rust-digger.code-maven.com/src/dawproject-0.8.2/dawproject/README.md)
- DawVert reads and writes `.dawproject` alongside FLP, ALS (Live 11), LMMS, MIDI, trackers and others. Its README does not mention Bitwig, Studio One, Cubase, Logic or Ardour native formats. — [DawVert](https://github.com/SatyrDiamond/DawVert)

### Inferences
- Use DAWproject as a sidecar "review" artifact: on commit, a hook or app-side step exports or converts the project to `.dawproject`, unzips it, and diffs `project.xml` against the schema. That gives a stable, documented diff surface across Bitwig, Studio One and Cubase, even though the native files stay opaque.
- The XSD lets the app build a schema-aware differ that matches by element IDs instead of lines.
- Do not use DAWproject as the merge target. It is lossy (no view state, partial device mapping, plugin-format gaps between DAWs), and exports cannot be automated from most DAWs without a UI action.
- For Ableton and FL, DawVert can produce DAWproject-like structures offline, but fidelity is limited.

### Gaps
- No primary-source list of DAWproject support updated for 2026 (the Bitwig FAQ snapshot is from Nov 2024). Fender Studio Pro 8 and Nuendo 14 support is confirmed only by secondary sources.
- Unknown whether any DAW offers headless/CLI DAWproject export that would allow automated sidecar generation.

---

## 3. Media reference handling and repo portability

### Takeaway
Every DAW defaults to storing references to media, not copies. Portability requires (a) a single project-root folder per song, (b) running the DAW's "collect/consolidate" function so all user audio lives inside that folder (where most DAWs store relative paths), and (c) keeping sample libraries and plugins out of the repo, installed identically on each machine.

### Cited Findings
- Ableton:
  - A normal save stores only references.
  - File > Collect All and Save copies audio into `Samples/Collected` inside the Project folder, video into `Collected`, and Max for Live devices into `Presets`.
  - Plugins cannot be collected and must be installed on each computer.
  - Collecting lets the Project folder move freely without unlinking samples.
  - Removing the `Ableton Project Info` folder can trigger "Missing media files" errors even after collecting.
  - Deselecting "Files from the Library" and "Factory Files from the Library" keeps references to Max devices intact.
  - Some Max for Live devices break after collecting.
  - [Ableton: Collect All and Save](https://help.ableton.com/hc/en-us/articles/209775645); [Ableton: Max for Live devices stop working after Collect All and Save](https://help.ableton.com/hc/en-us/articles/209069749-Max-for-Live-devices-stop-working-after-Collect-All-and-Save); [Ableton: Transferring Projects](https://help.ableton.com/hc/en-us/articles/209071909); [Ableton: Saving Projects](https://help.ableton.com/hc/articles/115000915804)
- Ableton `FileRef` stores both `RelativePath` + `RelativePathType` and an absolute `Path`, plus `OriginalFileSize`/`OriginalCrc`, which can be used to relocate missing files. — [ableton-under-the-hood](https://github.com/Sample-Offline/ableton-under-the-hood)
- Reaktor ensembles are not collected by Ableton's Collect All and Save. — [NI community](https://community.native-instruments.com/discussion/comment/67940)
- Berklee recommends "Save a Copy", then opening the copy and running Collect All and Save on it, so the working Set is not altered. — [Berklee Online help](https://online.berklee.edu/help/en_US/ableton-live/1871088-how-to-include-all-audio-files-in-an-ableton-live-project)
- Logic: the project can save media inside the package or project folder (Media/Audio Files), or reference it externally. The `Media` folder is absent when media is not stored with the project. — [LoC](https://loc.gov/preservation/digital/formats/fdd/fdd000640.shtml)
- Pro Tools: recordings go to the session's `Audio Files` folder by default. Moving sessions between systems follows Avid's "Save Copy In" with media-copy options. — [Educator.com](https://www.educator.com/music-theory/introduction-to-pro-tools/edler/session-folders.php); [Puremix: transferring Pro Tools sessions](https://v2.puremix.com/library/tutorials/daws/transferring-pro-tools-sessions)
- Ardour: all recorded and imported material lives in the session's `interchange/` folder, and the whole session folder must be copied. — [Ardour manual](https://manual.ardour.org/working-with-sessions/whats-in-a-session)
- DAWproject: the exporter picks the media layout. Audio can be embedded in the ZIP or referenced by relative path. — [bitwig/dawproject](https://github.com/bitwig/dawproject)

### Inferences
- App policy:
  - Enforce one song = one repo (or a subfolder) with the DAW's standard project layout.
  - Run a pre-commit "portability lint" that parses the project and flags absolute paths pointing outside the repo. For Ableton, check `FileRef/Path` values that are not under the project root. For Reaper, check `FILE "/Users/..."`.
  - Prompt the user to run Collect All and Save, or the DAW's equivalent, before pushing.
- Equivalent functions (training knowledge, unverified):
  - Reaper: "Save as… → Copy all media into project directory".
  - Studio One: "Copy External Files".
  - Cubase: "Back up Project" / "Prepare Archive".
  - Logic: File > Project Management > Consolidate.
  - Pro Tools: "Save Copy In" with "All audio files".
- Factory and library content (Ableton Core Library, Logic loops, Kontakt libraries) should not be committed. Instead, record a manifest of required packs, libraries and plugins (bravoh-daw/dawtool can extract plugin and sample lists) and show "missing on your machine" warnings.
- Store audio via Git LFS. Media files are mostly immutable once recorded, so LFS plus file locking fits well.

### Gaps
- Exact relative-path semantics (`RelativePathType` enum values in Ableton, Reaper's relative-path rules, Studio One/Cubase path storage) were not confirmed from primary docs this session.
- No source covered cross-OS path issues (macOS vs Windows separators and drive letters) per DAW.

---

## 4. Git integration: textconv, diff/merge drivers, merge risk, locking, ignore lists

### Takeaway
Read-only diffing is easy: textconv `gzip -dc` for `.als`, unzip-to-stdout for `.song`/`.dawproject`, the native text for `.rpp`/`.ardour`, and flpdiff's external diff driver for `.flp`. Automatic 3-way merging is unsafe for every DAW. Internal ID counters, cross-references and binary plugin blobs mean a line-clean merge can still produce a corrupt or semantically broken project. Use `merge=binary`/lock-based workflows (Git LFS locking), with branches as "alternatives", and treat merges as human-mediated (pick one side, then re-apply the other side's changes in the DAW).

### Cited Findings
- Git textconv works like this: define `[diff "<driver>"] textconv=<command>` in git config and set `diff=<driver>` in `.gitattributes`. `git diff` and `git blame` then run the converted text. — [git wiki: Textconv](https://git.wiki.kernel.org/index.php/Textconv) (archived; current docs at git-scm.com gitattributes)
- Gist recipes for gzipped files:
  - `*.als diff=gzip` with a gzip textconv.
  - Alternatively, a clean/smudge filter (`clean = gzip -d`, `smudge = gzip`) that stores decompressed XML in git. The gist notes that `git add` inside a pre-commit hook doesn't work perfectly.
  - [dcosson gist 1968831](https://gist.github.com/dcosson/1968831); [dcosson gist 1968853](https://gist.github.com/dcosson/1968853)
- ableton-git (cartmedia, Rust) wraps git: audio goes through git-lfs, and `.als` is unzipped to XML so "some conflicts can be resolved by looking at the diff". It recommends GitLab for its free 10 GB per repo. — [cartmedia/ableton-git](https://github.com/cartmedia/ableton-git)
- ableton-under-the-hood recommends committing decompressed XML so projects can be versioned without LFS, then recompressing with gzip. — [ableton-under-the-hood](https://github.com/Sample-Offline/ableton-under-the-hood)
- flpdiff `git-setup` writes `diff.flp.command` plus `.gitattributes`, with `--lfs` and `--textconv` modes and `git-verify` for diagnostics. — [flpdiff](https://github.com/dawhubapp/flpdiff)
- Ableton auto-backups: Live creates a `Backup` folder inside the Project (it appears after the second save) holding the 10 most recent saved versions, with timestamps in brackets in the filenames. The oldest is deleted once there are more than 10 (moved to Trash). A 2023 forum thread reports macOS deletions bypassing the Trash. — [Ableton: Backup Sets](https://help.ableton.com/hc/en-us/articles/360000377870-Backup-Sets); [Ableton forum: Backup sets no longer end up in trash](https://forum.ableton.com/viewtopic.php?p=1824886)
- Pro Tools: `Session File Backups` (autosave copies) and `WaveCache.wfm` (regenerable). — [Educator.com](https://www.educator.com/music-theory/introduction-to-pro-tools/edler/session-folders.php); [Avid WaveCache](https://apps.avid.com/proToolsFirstHelp/version12.0/enu/Pro%20Tools%20First%20Help/sess6.Workspace.16.065.html)
- Ardour: `.bak`, `.history`, `peaks/`, `analysis/`, `export/`, `dead sounds/`. Mixbus also has `.pending`. — [Ardour manual](https://manual.ardour.org/working-with-sessions/whats-in-a-session); [Mixbus manual](https://rsrc.harrisonconsoles.com/mixbus/mixbus-live-manual/9/en/topic/power-user-topics)
- Reaper: subprojects write `.RPP-PROX` render files on save, which are derived artifacts. — [Sound On Sound](https://www.soundonsound.com/techniques/reaper-subprojects)
- Skeptical view: an experienced git user on the BassBuzz forum sees little benefit for Reaper because projects are mostly binary audio that does not diff or merge. — [BassBuzz forum](https://forum.bassbuzz.com/t/daw-in-combination-with-code-repository/10121)

### Inferences
- Why auto-merge is risky (reasoned from format structure):
  - XML/RPP files contain global ID counters (Ableton `NextPointeeId`/`Id`, Reaper GUIDs, Ardour IDs) and cross-references between objects (routing, sends, automation targets that point to device/param IDs). Two branches that each add a track will assign colliding IDs.
  - A line merge can interleave half-edited blocks that are syntactically valid but semantically broken, for example automation pointing at a deleted device.
  - Base64/hex plugin blobs are opaque: a merge picks one side or produces garbage.
  - Gzip and ZIP containers must be re-packed exactly as the DAW expects.
  - The DAW may not validate on load, which risks silent corruption.
- Recommended git config pattern for the app:
  - `.gitattributes`:
    - `*.als diff=als merge=binary`
    - `*.rpp diff merge=binary` (text diff, but no auto-merge)
    - `*.ardour diff merge=binary`
    - `*.song diff=zipxml merge=binary`
    - `*.dawproject diff=zipxml merge=binary`
    - `*.flp diff=flp merge=binary`
    - `*.logicx/** -diff merge=binary lockable`
    - `*.ptx -diff merge=binary lockable`
    - `*.cpr -diff merge=binary lockable`
    - audio (`*.wav *.aif *.aiff *.flac *.mp3`) `filter=lfs diff=lfs merge=lfs -text lockable`
  - textconv:
    - `als`: `gzip -dc`
    - `zipxml`: `unzip -p "$1"` or a script that dumps sorted entries
  - Optionally, layer a semantic differ (flpdiff style) on top via `diff.<x>.command`.
- Store `.als` compressed (not via a clean filter) to avoid smudge/clean fragility and guarantee the DAW opens the checked-out file. The textconv gives the diff view anyway. Storing decompressed XML saves LFS quota and allows git delta compression, but one bad smudge produces an unopenable project.
- Lock-based workflow: use Git LFS file locking (`lockable` + `git lfs lock`) on the project file, so only one collaborator edits the session at a time. Others branch ("alternative mix", "alt arrangement"), and the app resolves a "merge" by having the user choose a branch, then manually porting changes with semantic-diff guidance.
- A "stem/part ownership" pattern (each member owns separate tracks or bounces in separate files) avoids session-level merges entirely.
- Starter `.gitignore` (partly from cited sources, partly training knowledge, unverified):
  - Ableton: `Backup/`; `Ableton Project Info/` is debatable, since Ableton says deleting it can cause missing-media errors, so it is probably better kept. The `*.asd` analysis files are regenerable (training knowledge).
  - Reaper: `*.reapeaks`, `*.rpp-bak`, `*.RPP-UNDO`, `*.RPP-PROX` (derived), `peaks/` (training knowledge, except RPP-PROX, which is cited).
  - Pro Tools: `Session File Backups/`, `WaveCache.wfm`, `*.wfm`.
  - Logic: `*.logicx/Alternatives/*/Project File Backups/`, `*.logicx/Alternatives/*/Autosave/`, `*.logicx/Alternatives/*/Undo Data*` (training knowledge, unverified).
  - FL Studio: `Backup/` folder (FL writes `* (overwritten at …).flp` / autosave backups to its Backup folder; training knowledge, unverified).
  - Studio One: `History/`, `Cache/`, `*.autosave`, `*.song.bak` (training knowledge, unverified).
  - Cubase: `*.bak`, `*.csh` (image/waveform cache), `Edits/` is debatable (training knowledge, unverified).
  - Ardour: `*.bak`, `*.pending`, `peaks/`, `analysis/`, `export/`, `dead sounds/`.
  - OS: `.DS_Store`.

### Gaps
- No authoritative docs found for FL Studio backup folder naming, Logic in-package autosave/backup folder names in Logic 11/12, Studio One `History` folder behavior, or Cubase `.bak`/`.csh`. These came from training knowledge and should be verified.
- No sources describe production use of a custom 3-way *merge* driver for any DAW. I found none, which itself suggests nobody has done it robustly.

---

## 5. Plugin state blobs and missing-plugin issues

### Takeaway
Every DAW embeds third-party plugin state as opaque binary: hex in Ableton XML, base64 in Reaper RPP, binary in the native formats, and embedded files in DAWproject. These blobs dominate diff noise and cannot be merged or interpreted generically. A plugin's state can change on save even when no user-visible parameter changed. Missing plugins on a collaborator's machine are a primary failure mode, and no format solves them.

### Cited Findings
- Ableton stores a hex value for each VST on a track, which makes plain text diffing noisy. — [Ableton forum: Decoding ALS](https://forum.ableton.com/viewtopic.php?p=1434673) (via search summary)
- Reaper stores plugin state as base64 inside `<VST>` blocks. Parsers decode it to identify plugins. Heavy binary VST data (e.g. Kontakt) crashes some parsers. — [Atlas Audio Reaper extractor](https://file-atlas-app-website.pages.dev/docs/extractors/atlas-audio/pack-atlas-audio-reaper); [ReaperSharp](https://github.blesdmm.dns-dynamic.net/BuriedStPatrick/ReaperSharp/blob/master/README.md)
- DAWproject always embeds plugin states as files referenced by path, but cross-DAW reading is incomplete: Studio One doesn't read Bitwig CLAP state, and Bitwig doesn't read Studio One AU state. — [bitwig/dawproject](https://github.com/bitwig/dawproject); [Bitwig FAQ](https://www.bitwig.com/support/technical_support/system-requirements-62/)
- Plugins cannot be collected into an Ableton project; they must be installed on each machine. — [Ableton: Collect All and Save](https://help.ableton.com/hc/en-us/articles/209775645)
- OwlPlug, an open-source plugin manager, reads Studio One project files to list the plugins each project uses (from `classInfo`). — [OwlPlug PR #394](https://gh.r-mj.com/DropSnorz/OwlPlug/pull/394)
- Plugin preset/state codecs exist as reference implementations: npm `vst2-preset-parser` (FXP/FXB) and the cargo-truce `preset_codec.rs`. — [vst2-preset-parser](https://npmjs.com/package/vst2-preset-parser); [cargo-truce preset_codec](https://docs.rs/crate/cargo-truce/0.58.3/source/src/preset_codec.rs)

### Inferences
- In the diff UI, collapse plugin blobs to `plugin <name> (<vendor>, <format>) state changed (hash a1b2→c3d4, +312 bytes)` rather than showing hex or base64. Where the DAW also records an explicit parameter list (Ableton exposes automatable params as XML values; Reaper stores some params as text after the blob; DAWproject has parameter automation), diff those values semantically.
- Maintain a per-repo "plugin manifest" (name, vendor, format VST2/VST3/AU/CLAP, version), extracted on each commit with bravoh-daw/dawtool/OwlPlug-style parsing. Compare it against each collaborator's locally scanned plugins and warn before opening.
- Recommend team-standard plugin versions, prefer DAW-native devices for shared sessions, and offer a "print/bounce" policy (commit rendered stems via LFS alongside the session) so a missing plugin doesn't block listening or review.
- AU (macOS-only) and Windows-only VST2 cause cross-platform gaps in mixed teams.

### Gaps
- No source quantified how often plugin blobs change on re-save without user edits (non-deterministic state serialization). This is plausible but unverified.
- No source found on how Logic/Pro Tools/Cubase encode plugin state internally, beyond "binary".

---

## 6. Existing tools and projects that version DAW sessions with git, and their lessons

### Takeaway
There is a small but real ecosystem, mostly Ableton- and FL-focused: wrappers (ableton-git), auto-commit-on-save tools, semantic differs (flpdiff, which backs the commercial dawhub.app), and unified parsers (bravoh-daw, dawtool). Lessons from these projects: diffs are useful for history and recovery, merges remain manual, LFS is needed for audio, and practitioners split between enthusiasm and "not very useful in practice".

### Cited Findings
- cartmedia/ableton-git (Rust) offers `ableton-git init/clone`, LFS for audio, and XML-unzipped `.als`. Conflicts are only partly resolvable by reading diffs. — [cartmedia/ableton-git](https://github.com/cartmedia/ableton-git)
- dayvonjersen/ableton-project-version-control-thing auto-commits on every Ctrl+S and supports labels, `log`, `checkout [hash]` (reload in Live via Open Recent) and `save`/`cancel`. Auto-commit pauses while an old version is checked out. The motivation: Ableton's Crash Recovery clears undo history on save, and the author regrets changes they can't recall. The project is a WIP. — [dayvonjersen repo](https://www.github.com/dayvonjersen/ableton-project-version-control-thing)
- Other Ableton git tools found:
  - lucas-ywk/ableton-save — [ableton-save](https://github.com/lucas-ywk/ableton-save)
  - raphaeldkhn/gitdaw — [gitdaw](https://awesome.ecosyste.ms/projects/github.com%2Fraphaeldkhn%2Fgitdaw)
  - trobonox/abletonconvert — [abletonconvert](https://github.com/trobonox/abletonconvert)
  - danielbayley/ableton-live-tools — [ableton-live-tools](https://awesome.ecosyste.ms/projects/github.com%2Fdanielbayley%2Fableton-live-tools)
  - These were not fetched individually.
- Ableton forum thread "Has anyone tried Git style versioning with .ALS files?" (2021–2024):
  - Users value local saves without commits, branching and tagging.
  - One (jbaylies, 2023) found it "hasn't proven to be super useful".
  - LoopInfinitum (2024) built a real-time XML-tracking tool for "logical versioning".
  - Gral (2024) pushes the whole Music folder to a self-hosted git server and verified recovery by checkout.
  - The thread does not discuss merge conflicts, diff noise or LFS.
  - [Ableton forum](https://forum.ableton.com/viewtopic.php?p=1807386)
- flpdiff + dawhub.app is a commercial hosted collaboration product with browser UI, sharing and cloud version history, built on the semantic FLP differ. It is the closest direct competitor/prior art for a git-based musician-team app. — [dawhubapp/flpdiff](https://github.com/dawhubapp/flpdiff)
- bravoh-daw is a unified cross-DAW "intelligence" parser (ALS/FLP/LOGICX/RPP) built with fuzzing and graceful errors on corrupt files. — [bravoh-daw](https://docs.rs/crate/bravoh-daw/0.1.0)
- FMOD Studio + Reaper treats `.rpp` as a source-controlled asset in game-audio pipelines (re-render on sync). — [FMOD](https://fmod.com/docs/2.03/studio/appendix-b-reaper-integration.html)
- BassBuzz forum skeptic: git adds little for mostly-binary audio projects. — [BassBuzz](https://forum.bassbuzz.com/t/daw-in-combination-with-code-repository/10121)
- Doseedo, a vendor, markets an MCP server and AI agents operating on `.dawproject` files, so DAWproject is becoming a tooling surface. — [Doseedo: Bitwig MCP server](https://doseedo.com/guides/bitwig-mcp-server)

### Inferences
- Lessons for the app:
  1. Snapshot, history and recovery is the proven value. Merge is not.
  2. Auto-commit on save is popular but needs squashing or labels, or history becomes noise.
  3. Semantic, human-readable diffs (flpdiff style) matter more than raw XML diffs for musicians.
  4. LFS plus locking is required.
  5. Restoring an old version requires the DAW to reload the file, so the app should integrate "open this version" and "open as copy" actions rather than silently checking out over an open project.
  6. Never write into a project file while the DAW has it open.
- Competitive landscape: dawhub.app (FL), plus older commercial attempts not researched here (e.g. Splice Studio, discontinued). Differentiate with multi-DAW support plus git interoperability.

### Gaps
- Did not fetch LoopInfinitum's tool, gitdaw, ableton-save or abletonconvert, so their implementation details and lessons are unknown.
- No Reaper-specific git workflow write-ups (blog or forum) with lessons were found, despite RPP being the most git-friendly format.
- No post-mortems were found from commercial DAW-versioning services (Splice Studio etc.) in this session.
