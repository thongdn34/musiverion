# Audio Diff Techniques for a Git + Git LFS Music-Studio Versioning App

Scope: how to compare two versions of an audio file (.wav/.flac/.mp3/.aiff) in a useful way, at the metadata, sample, perceptual and visual levels. Covers which open-source tools implement each approach and how to wire them into git (textconv and external diff drivers) and into an app UI. Research date: 2026-11-19. Note: about 22 tool calls were used. Some facts come from search-result summaries and not from full page reads; these are marked "(snippet)". Anything taken from the researcher's general knowledge and not checked this session sits under Inferences and is labelled that way.

## Q1. Metadata diffs and git integration (textconv / diff drivers)

### Takeaway
The cheapest useful "audio diff" is a text dump of metadata, plus optional loudness stats, wired in as a git `textconv` driver with `cachetextconv = true`. Git handles colorizing, word-diff and caching. Results are cached per blob under `refs/notes/textconv/<driver>`, so even costly loudness scans run only once per version. Use `diff.<name>.command` (an external driver with 7 arguments) only when the output is not line-oriented text.

### Cited Findings
- The `diff` attribute has four states: set (`*.c diff`, treat as text), unset (`*.ps -diff`, which prints "Binary files differ"), unspecified (git guesses), and a string (`*.wav diff=<name>`, which uses a driver configured under `diff.<name>.*`). — [git gitattributes docs](https://git-scm.com/docs/gitattributes)
- `diff.<name>.textconv` names a program that takes one argument (a file path) and writes text to stdout. Only `git diff` and the `git log` family (`log`, `show`, `whatchanged`) run it; `format-patch` never does. Textconv output can't be applied as a patch, but it supports colors, `--word-diff` and combined merge diffs. — [git gitattributes docs](https://git-scm.com/docs/gitattributes)
- `cachetextconv = true` caches the converter output for each blob indefinitely, in `refs/notes/textconv/<driver>`. Changing the textconv config value invalidates the cache automatically. After a tool upgrade, invalidate by hand with `git update-ref -d refs/notes/textconv/<driver>`. `git diff --no-textconv` bypasses conversion. — [git gitattributes docs](https://git-scm.com/docs/gitattributes)
- `diff.<name>.binary = true` keeps textconv while still treating the file as binary for other purposes. You can't combine `-diff` with `diff=name`. The built-in macro `binary` expands to `-diff -merge -text`. — [git gitattributes docs](https://git-scm.com/docs/gitattributes)
- `diff.<name>.command` runs an external program with the same 7 parameters as `GIT_EXTERNAL_DIFF`. With `trustExitCode = true` the program must exit 1 when there are significant changes and 0 when there are none. External drivers don't get textconv caching and don't have to produce unified-diff format. — [git gitattributes docs](https://git-scm.com/docs/gitattributes) (the page refers to git(1) for the parameter list, which is path, old-file, old-hex, old-mode, new-file, new-hex, new-mode)
- Loudness: FFmpeg's `ebur128` filter has a `peak` option with `sample` and `true` modes. True-peak mode uses an oversampling (swresample) context. Integrated loudness uses 400 ms gating blocks with 75% overlap, per BS.1770. A 2023 commit exported the integrated loudness (LUFS) and true peak (dBFS) results as read-only options. — [FFmpeg-cvslog 2023-05](https://lists.mplayerhq.hu/pipermail/ffmpeg-cvslog/2023-May/137179.html); [f_ebur128.c source](https://ffbox0-bg.ffmpeg.org/doxygen/7.0/f__ebur128_8c_source.html)
- BWF MetaEdit (from FADGI / MediaArea) imports, edits, embeds and exports bext and INFO chunks. Since v21.07 it also handles `cue` and `adtl` chunks (markers and labels). It can export aXML, iXML and XMP chunks as sidecar files. It ships as both a CLI and a GUI on Windows, macOS and Linux, and has an XML report output option. — [FADGI BWF MetaEdit help](https://www.digitizationguidelines.gov/audio-visual/documents/help_home.html); [LoC blog on cue chunks → IIIF](https://blogs.loc.gov/thesignal/2022/08/collaborations-with-embedded-audio-metadata-reusing-cue-chunk-data-for-iiif-web-annotations/); [bwfmetaedit.sourceforge.net](https://bwfmetaedit.sourceforge.net/) (snippet)
- iXML `<MARKERS>` and WAV `cue` chunks are different structures. I couldn't confirm that BWF MetaEdit reads iXML markers as cue points. — [ExifTool forum: cue markers in WAVE](https://exiftool.org/forum/index.php?msg=65148) (snippet); see Gaps
- `audiodiff` (Python) is an existing "tags + stream" comparator. It treats two files as equal when they have the "same audio streams and normalized tags". Tags are read through mutagen and the `encodedby` tag is ignored. `checksum()` is a SHA1 of the decoded PCM, taken as signed 24-bit LE via FFmpeg. It recognizes WAV, FLAC, M4A and MP3 by extension, and assumes WAV files carry no tags. Its tag reader is described as "not yet stable". — [audiodiff docs](https://audiodiff.readthedocs.io/)

### Inferences
- Recommended driver (general knowledge; check flags against your installed versions):
  ```gitconfig
  [diff "audio"]
      textconv = musiversion-audio-textconv   # script below
      cachetextconv = true
      binary = true
  ```
  ```gitattributes
  *.wav  filter=lfs diff=audio merge=lfs -text
  *.flac filter=lfs diff=audio merge=lfs -text
  *.mp3  filter=lfs diff=audio merge=lfs -text
  *.aif* filter=lfs diff=audio merge=lfs -text
  ```
  `git lfs track` writes `filter=lfs diff=lfs merge=lfs -text`. Swapping `diff=lfs` for `diff=audio` should be safe, because git applies the LFS smudge filter before textconv sees the content. That isn't verified here: test that textconv receives real audio and not the LFS pointer text, particularly when objects haven't been fetched yet. In that case the script should detect the pointer (`version https://git-lfs.github.com/spec/v1`) and print the oid and size instead.
- The textconv script should print stable, sorted, one-fact-per-line output, so diffs stay minimal. For example:
  - `ffprobe -v error -show_format -show_streams -of flat` or `-of json` gives codec, sample_rate, channels, channel_layout, bits_per_raw_sample/sample_fmt, duration_ts, and tags. (general knowledge)
  - `mediainfo --Output=JSON`, or `exiftool -G1 -a -s` for ID3/Vorbis/RIFF INFO/bext/iXML/XMP groups. (general knowledge)
  - `bwfmetaedit --out-xml` (or the equivalent) for bext and cue. The flag names weren't confirmed this session.
  - One loudness line from `ffmpeg -nostats -i f -af ebur128=peak=true -f null -` (I, LRA, TP), plus a decoded-PCM hash (see Q5).
  - Leave out volatile fields such as encoder version, file mtime and `encodedby`, as audiodiff does.
- Loudness takes a full decode, so it is slow on long multitrack sessions. `cachetextconv` makes this a one-time cost per blob. The app should keep its own cache keyed by LFS oid (SHA-256) so it doesn't depend on git notes.
- For richer, non-textual output (JSON for the UI), use a separate app-side command, or `diff.audio.command` for CLI users, and keep textconv for plain `git diff`/`git log -p` users.

### Gaps
- I didn't fetch the official ffmpeg-filters page sections for `ebur128`, `loudnorm`, `astats` or `aresample`. The page was too large and the excerpt cut off, so exact option and output field names are unverified.
- I didn't confirm the exact BWF MetaEdit CLI flags, or whether iXML markers are parsed.
- I didn't check sox `--i` / `stat` / `stats`, mediainfo or exiftool docs this session. Their capabilities above come from general knowledge.
- I didn't verify how git-lfs interacts with a custom `diff=` driver.

## Q2. Sample-level diffs (null test, alignment, changed regions, format and codec issues)

### Takeaway
A sample-level diff works like this: decode both files to float PCM at a common rate and channel layout, and correct the known codec delay. Then estimate the offset with FFT cross-correlation (GCC-PHAT if the spectra differ), match gain, and subtract. Finally, report residual RMS/peak in dBFS and the time ranges where windowed residual energy goes above a threshold. Few open-source tools do the whole chain, so expect to write about 200 lines of numpy/scipy (or Rust) yourself.

### Cited Findings
- SciPy pattern for finding the offset: `scipy.signal.correlate(x, y, mode="full", method="fft")`, then `correlation_lags(len(x), len(y), mode="full")`, then `lags[argmax(corr)]`. The docs example recovers a 100-sample delay. `method='fft'` is faster for numeric arrays. — [scipy correlation_lags docs](https://docs.scipy.org/doc/scipy/reference/generated/scipy.signal.correlation_lags.html); [scipy correlate docs](https://docs.scipy.org/doc/scipy/reference/generated/scipy.signal.correlate.html)
- Getting the zero-lag index wrong is a common bug, and `correlation_lags` handles that bookkeeping. — [Julia discourse delay thread](https://discourse.julialang.org/t/determine-the-delay-between-two-signals/109140); [scipy issue #11020](https://github.com/scipy/scipy/issues/11020)
- Existing alignment libraries:
  - `audalign` (Python) aligns recordings by fingerprint, cross-correlation or spectrogram correlation, and has a `fine_align` refinement step. Correlation is "more precise than fingerprints". — [audalign on PyPI](https://pypi.org/project/audalign/0.7.2/) (snippet)
  - `find_delay` (Python) finds where one series appears inside another by cross-correlation, with audio examples. — [find_delay docs](https://find-delay.readthedocs.io/en/2.14/_sources/index.rst.txt) (snippet)
  - One ComfyUI node runs the complete chain: GCC-PHAT delay, then gain match, then null test. — [ComfyUI Audio Align (XCorr)](https://www.runcomfy.com/comfyui-nodes/ComfyUI-Egregora-Audio-Super-Resolution/audio-align-x-corr) (snippet)
- MP3 codec delay: the LAME tag in the Xing frame stores encoder delay (prepended samples) and padding (appended samples). Decoders add their own delay, usually given as 529 samples, though some sources say 528. A gapless decoder skips the encoder delay and stops at the original sample count. Fraunhofer files use a VBRI tag that stores only combined delay+padding. — [libmp3lame gapless article](https://libmp3lame.web.app/article/how-libmp3lame-achieves-gapless-mp3-playback); [hydrogenaudio thread](https://hydrogenaudio.org/index.php/topic,97891.0/prev_next,prev.html) (snippet)
- Toolchain pitfalls:
  - FFmpeg trac #9755: after `-c copy`, an MP3 decoded with extra trailing samples; Audacity showed a 35-sample difference. — [FFmpeg trac #9755](https://trac.ffmpeg.org/ticket/9755)
  - Audacity 3.2.0 stopped trimming padding using LAME metadata. — [Audacity forum](https://forum.audacityteam.org/t/3-2-2-mp3-duration-wrong/66369)
  - So the same MP3 can decode to different lengths depending on the decoder.
- Symphonia (Rust): gapless output needs support in both the demuxer and the decoder. In the 0.5.x tables AIFF, ALAC and ADPCM are gapless-capable while AAC-LC and CAF/MP4 are not. By default only royalty-free formats are enabled. MP3 gapless via the LAME header comes only from a third-party page and is unverified. — [Symphonia README (docs.rs)](https://docs.rs/crate/symphonia/latest/source/README.md); [Symphonia GitHub](https://github.com/pdeljanov/Symphonia)
- python-soundfile (libsndfile) has `soundfile.blocks(blocksize=..., overlap=...)` for block-wise streaming reads. It opens everything libsndfile supports (WAV, FLAC, OGG, …). MP3 support arrived in soundfile 0.11.0 (2022-06-02) with the libsndfile 1.1.0 update. — [python-soundfile docs](https://python-soundfile.readthedocs.io/en/latest/)
- Lossy decodes aren't bit-reproducible: decoded output can differ between machines, especially with floating-point decoders. — [FFmpeg-user 2013](https://ffmpeg.org/pipermail/ffmpeg-user/2013-November/018542.html); [audiodiff docs](https://audiodiff.readthedocs.io/)
- FFmpeg `adelay` accepts per-channel delays in ms, or in exact samples with the `S` suffix (e.g. `adelay=delays=64S:all=1`). You can apply an alignment offset in an FFmpeg pipeline with it. — [ffmpeg-filters docs](https://ffmpeg.org/ffmpeg-filters.html)
- Existing projects:
  - `smart-audio-diff` (TypeScript) normalizes volume and aligns before comparing two WAVs. — [pkgstats smart-audio-diff](https://www.pkgstats.com/pkg:smart-audio-diff) (snippet)
  - `sounddiff` (Python CLI, "git diff, but for audio") supports WAV, FLAC, OGG and AIFF, with colored terminal output. — [sounddiff on PyPI](https://pypi.org/project/sounddiff/) (snippet; the page didn't render when fetched)
  - FourierRocks (GPLv2 GUI) diffs in both the time and frequency domains, but only for 16-bit PCM WAV, one channel at a time. — [FourierRocks on SourceForge](https://sourceforge.net/projects/fourierrocks/) (snippet)
  - WavChecker checks that the PCM in QT/MXF matches a WAV. — [KengoSawa2/WavChecker](https://www.github.com/KengoSawa2/WavChecker) (snippet)

### Inferences
- Suggested pipeline (general DSP knowledge; not sourced in full):
  1. Decode both files to float32 at the higher of the two sample rates. Resample with a high-quality resampler (soxr/libsamplerate). For MP3/AAC, use a gapless-aware decoder (FFmpeg or Symphonia) and record which decoder was used.
  2. Run coarse alignment on a downsampled mono mix (e.g. 8 kHz) with FFT xcorr over a bounded lag window (±2 s). Refine at full rate over ±N samples; parabolic peak interpolation gives sub-sample accuracy.
  3. Estimate gain with least squares, g = ⟨a,b⟩/⟨b,b⟩, so a pure level change shows up as "gain −1.5 dB" and not as a full-file diff.
  4. Compute the residual r = a − g·b. Report the null depth: 20·log10(RMS(r)/RMS(a)).
     - A bit-identical file gives −∞ (exact zero).
     - A dither or bit-depth change typically leaves a residual near the noise floor of the lower bit depth: about −96 dBFS for 16-bit, −144 dBFS for 24-bit (theoretical).
     - Lossy re-encodes leave much shallower nulls.
  5. Changed regions: compute residual RMS in windows of about 50–100 ms, threshold relative to the local signal RMS (e.g. >−60 dB rel.), merge windows closer than ~250 ms, and emit `[start, end, peak_residual_dB]` ranges for the UI.
- Edits that shift timing in the middle of a file (an inserted bar, for example) break global alignment. Detect this by doing piecewise alignment per chunk (e.g. 5 s) and watching for step changes in the lag. DTW on chroma or MFCC frames (`librosa.sequence.dtw`) is the robust fallback for tempo or arrangement edits.
- Bit-depth differences: compare in float. Quantizing 24→16 adds dither noise, so a "same content, different bit depth" result should be reported as a format change with the residual near the expected dither floor, not as a content change.
- Treat DC offset, polarity inversion (correlation peak negative) and L/R swap as named, detectable transforms rather than generic diffs.

### Gaps
- I found no maintained, widely used open-source tool that does decode + align + gain-match + null + region-report end to end. Maturity and maintenance of sounddiff and smart-audio-diff weren't verified.
- I didn't fetch Gearspace or KVR null-test threads. Practitioner guidance (e.g. the effect of plugin latency compensation and of nondeterministic plugins such as analog-modelled noise on null tests) isn't sourced here.
- I didn't confirm Symphonia's MP3 gapless status in the 0.6.x line.

## Q3. Perceptual and feature diffs (spectrogram, MFCC/chroma, PEAQ/ViSQOL/PESQ, fingerprinting)

### Takeaway
For music, ViSQOL v3 in audio mode (48 kHz, SVR model, MOS up to about 4.75) is the most practical open-source full-reference quality metric. GstPEAQ is the open PEAQ option, but it is non-conformant and the patent and license position needs checking. PESQ is speech-only. For "is this the same song/take?" checks, Chromaprint raw fingerprints compared by bit error rate after offset search are cheap and robust to re-encoding.

### Cited Findings
- ViSQOL v3 (Chinen et al., QoMEX 2020, arXiv 2004.09584) is Google's open-source, full-reference objective metric for speech and audio. It made incremental improvements, not a move to end-to-end DNN modelling. — [arXiv 2004.09584](https://arxiv.org/abs/2004.09584); [Google Research page](https://research.google/pubs/visqol-v3-an-open-source-production-ready-objective-speech-and-audio-metric/)
- ViSQOL modes:
  - Audio mode needs the `similarity_to_quality_model` SVR model. Its maximum MOS is about 4.75.
  - Speech mode uses a wideband model and expects 16 kHz input.
  - Guidance is to use clips of about 8–10 s with about 0.5 s of silence around them, and a clean reference.
  - [google/visqol GitHub](https://github.com/google/visqol) (snippet)
- `visqol-python` (community, not Google) is a pure-Python port supporting audio mode at 48 kHz. It reports 9/10 tests identical to the C++ MOS output. — [visqol-python on PyPI](https://pypi.org/project/visqol-python/) (snippet)
- GstPEAQ (Holters & Zölzer, DAFx-15) implements PEAQ basic (11 MOVs, 3 hidden nodes) and advanced (5 MOVs, 5 hidden nodes). It is "non-conforming" to ITU-R BS.1387-1 but close. It is a GStreamer `peaq` element plus a CLI with an `--advanced` flag, licensed LGPL. Reported RMSE on the ITU test vectors is 0.2063 according to Wikipedia and 0.2329 according to another copy; the sources disagree. — [DAFx paper](https://dafx.de/paper-archive/details/cHfHWwcZJB8k3KwAtdoigw); [Wikipedia PEAQ](https://en.wikipedia.org/wiki/Perceptual_Evaluation_of_Audio_Quality)
- One blog says PEAQ may be patent-protected and the bundled test data restricted. That is a secondary, unverified claim. — [aimastering blog](https://aimastering.com/blog/en/?p=931) (snippet)
- Chromaprint comparison:
  - The base64 fingerprints can't be compared directly. Use `fpcalc -raw` (32-bit ints; `-signed` gives pg_acoustid compatibility) and compute XOR + popcount → bit error rate.
  - This assumes the fingerprints are aligned, so search over offsets first.
  - The Go library treats a score of 0.95 or higher as effectively identical.
  - fpcalc can't fingerprint very short clips (under about 2–3 s).
  - [AcoustID group: comparing fingerprints](https://groups.google.com/g/acoustid/c/mkZ_eQ0vaFQ); [How does Chromaprint work?](https://oxygene.sk/2011/01/how-does-chromaprint-work/); [go-fingerprint](https://pkg.go.dev/github.com/go-fingerprint/fingerprint) (snippets)
- Chromaprint fingerprints change somewhat across encoding qualities. — [AcoustID group thread](https://groups.google.com/g/acoustid/c/Uq_ASjaq3bw/m/kLreyQgxKmgJ) (snippet)

### Inferences
- Feature diffs, from general knowledge of librosa and Essentia (not fetched this session):
  - The spectrogram difference |STFT_a| − |STFT_b| in dB (after alignment) shows *where in frequency* a change happened: an EQ change looks like a band, a new instrument like a localized blob. It's the most informative visual for mix revisions.
  - Chroma (`librosa.feature.chroma_cqt`) distance flags harmonic or arrangement changes.
  - MFCC distance flags timbre changes.
  - Per-frame cosine distance curves can be plotted under the waveform as a "change intensity" lane.
- Choosing a tier:
  - Bit-identical → done.
  - Deep null (< −90 dB) → "inaudible / format-only".
  - Moderate null → run the spectral diff and show regions.
  - No null and fingerprint similar → "re-recorded/re-arranged version of same material".
  - Fingerprint dissimilar → "different content".
- PEAQ and ViSQOL are designed for codec degradation against a reference. They are less meaningful for creative mix changes, where neither version is "the reference". Use them mainly to answer "did this bounce or transcode degrade quality?" (e.g. comparing an MP3 preview against its WAV master).
- PESQ (ITU-T P.862) is a speech-only, narrow/wideband metric with licensing restrictions (general knowledge). It isn't suitable for music.
- dejavu (a Python fingerprinting project) is an alternative to Chromaprint for "find this clip within that file" (general knowledge; not verified this session).

### Gaps
- I didn't fetch the librosa, Essentia or dejavu docs. Function names above come from general knowledge.
- I found no published thresholds that map ViSQOL MOS or null depth to "audible difference" for music mixes.
- I didn't verify the PEAQ patent status.

## Q4. Visual diff UI (waveforms, overlays, spectrograms, A/B playback, existing apps)

### Takeaway
Generate peak files server-side or at commit time with BBC audiowaveform (`.dat`/JSON, multi-resolution, per-channel) and cache them by LFS oid. Render with peaks.js or wavesurfer.js using precomputed peaks, so the browser never decodes whole multi-GB files. Show changed regions with the wavesurfer Regions plugin or peaks.js segments, and use a windowed spectrogram for the frequency-domain diff. Few products actually ship audio A/B diffing. Splice Studio, the closest precedent, has been shut down, and Anchorpoint offers LFS management but no documented audio diff.

### Cited Findings
- audiowaveform (BBC, C++):
  - Reads MP3, WAV, FLAC, Ogg Vorbis and Opus (Opus needs libsndfile ≥1.0.29), plus raw audio. Piped input works only for MP3, WAV and raw.
  - Outputs binary `.dat`, JSON, PNG and WAV.
  - Options: `--zoom` (samples per point, default 256) or `--pixels-per-second` (default 100), `--bits 8|16`, `--split-channels`.
  - GPL-3.0. Install examples reference v1.10.2. Development has moved to codeberg.org/chrisn/audiowaveform.
  - [bbc/audiowaveform](https://github.com/bbc/audiowaveform)
- AIFF isn't in audiowaveform's listed inputs, so AIFF would need converting first (e.g. pipe WAV from ffmpeg). — [bbc/audiowaveform](https://github.com/bbc/audiowaveform)
- wavesurfer.js:
  - Decodes with Web Audio by default, so large files can fail on memory limits. The recommended approach is pre-decoded peaks (e.g. from audiowaveform). Streaming is supported only with pre-decoded peaks plus duration.
  - Plugins: Regions, Timeline, Minimap, Envelope, Record, Spectrogram (with `rendering: 'windowed'` for long files), Hover.
  - The docs mix v7 and v8 references.
  - [wavesurfer.js docs](https://wavesurfer.xyz/docs/)
- wavesurfer.js v8 was released. Its public API and plugin constructors are said to be unchanged, with breaking changes in edge cases, TS subclasses and deep `dist/` imports. — [newreleases: wavesurfer.js 8.0.0](https://newreleases.io/project/github/katspaugh/wavesurfer.js/release/8.0.0)
- `wavesurfer-multitrack` (katspaugh) is a BSD-3 "multi-track super-plugin" with per-track volume (`setTrackVolume`) and a WebAudioPlayer. The author calls it commercial, with no free support. It has low activity (about 22 stars, one maintainer, last release about a year ago per an index). — [ecosyste.ms entry](https://awesome.ecosyste.ms/projects/github.com%2Fkatspaugh%2Fwavesurfer-multitrack); [author gist](https://gist.github.com/katspaugh/868654c2209973d0a56975c5341eb6f4) (snippets)
- peaks.js (BBC):
  - Uses audiowaveform data through waveform-data.js.
  - Supports multi-channel waveforms, a zoomview plus an overview, `segments.add({id,startTime,endTime,labelText})` and `points.add({time,labelText})`.
  - Has `enablePoints`/`enableSegments` options.
  - A gist by chrisn demonstrates multiple audio tracks.
  - [peaks.js API doc](https://github-redirect.dependabot.com/bbc/peaks.js/blob/master/doc/API.md); [waveform-data.js](https://github.com/bbc/waveform-data.js); [chrisn gist: multiple tracks](https://gist.github.com/chrisn/376c73de59b1bcacb6f44bba45ea2811) (snippets)
- Splice Studio (cloud DAW-project backup and versioning, launched 2013/2014) was wound down. New Studio project creation was disabled on April 7, 2023, and Splice help now calls it "no longer active". — [MusicRadar: Splice Studio closing](https://musicradar.com/news/splice-studio-closing); [Splice CEO letter](https://splice.com/blog/learn-with-splice/) (snippets)
- Anchorpoint is a Git client with Git LFS preconfigured: it auto-writes .gitattributes and says it "understands" audio formats. It uses its own locking server, which is separate from `git lfs lock`. I found no evidence that it has audio waveform or A/B diff features; the sources are vendor marketing. — [Anchorpoint: version control for binary files](https://www.anchorpoint.app/blog/version-control-for-binary-files); [Anchorpoint: Git for artists](https://www.anchorpoint.app/features/git-for-artists); [Anchorpoint file locking](https://www.anchorpoint.app/blog/using-file-locking-in-git)
- Other visual comparison tools: Sonic Lineup (aligns and compares multiple recordings, from the Sonic Visualiser authors), browser spectrogram-compare tools, and audio_analyzer (a notebook-based visual comparison). — [0xdevalias gist on audio diffing](https://gist.github.com/0xdevalias/91ae33e0c9290e69b457ce5034956fb7); [Spectrogram Compare](https://soniqtools.pages.dev/compare/); [jcarlosneto/audio_analyzer](https://github.com/jcarlosneto/audio_analyzer) (snippets)

### Inferences
- UI pattern:
  - Two stacked lanes, or one overlaid lane (A in one color, B semi-transparent in another), sharing a single time axis and zoom state.
  - A third "difference" lane shows the residual-envelope peaks, generated by running audiowaveform on the residual WAV.
  - Changed-region bands come from the Q2 region list and are rendered as Regions or segments. Clicking a band seeks both players.
- A/B playback:
  - Use the Web Audio API with two `AudioBufferSourceNode`s (or two `MediaElementSource`s for streaming), started at the same `AudioContext.currentTime`, each routed through its own `GainNode`.
  - Switch A/B with a 5–20 ms gain crossfade so the switch doesn't click.
  - Loudness-match by applying the gain offset (B_I − A_I in LUFS from ebur128) to the louder side. Otherwise "louder sounds better" bias spoils the comparison.
  - Apply the alignment offset from Q2 so the switch is sample-synced.
  - (General Web Audio knowledge; not fetched this session.)
- For long multitrack files, stream audio through HTTP range requests (an LFS object served via the app's own blob endpoint or a signed URL). Render only from precomputed multi-zoom peaks; audiowaveform `.dat` at 2–3 zoom levels covers overview and detail. Compute spectrogram tiles server-side as images per zoom level and time tile, rather than in the browser.
- The licenses differ: audiowaveform is GPL-3.0 (fine when invoked as a separate CLI process), wavesurfer.js is BSD-3, and peaks.js and waveform-data.js are LGPL-3.0 (general knowledge). This matters for a closed-source desktop app that bundles them.

### Gaps
- I didn't verify the peaks.js license or current version, or the wavesurfer.js peaks/duration option names, from primary docs this session.
- I found no current commercial or open product that offers git-integrated audio A/B diff with null-test regions. Avid Pro Tools cloud collaboration and other DAW-vendor versioning features weren't researched.
- I didn't check whether `wavesurfer-multitrack` is compatible with v8.

## Q5. Libraries, performance and cheap change detection (decoded-PCM hashing, chunk hashing)

### Takeaway
Every version should get a hash of the decoded PCM in a canonical sample format, stored alongside the LFS oid. A metadata-only edit changes the file hash (LFS oid) but not the PCM hash, so the app can say "tags changed, audio identical" without a full diff. Fixed-duration chunk hashes (for example per second, or per FFmpeg `framemd5` packet) find the changed regions in O(n) with no DSP. These are exact only for lossless sources, since lossy decodes can differ across platforms.

### Cited Findings
- FFmpeg hashing:
  - The `framemd5`/`framehash` muxers hash each decoded packet.
  - The `hash` muxer (`-f hash -hash md5|sha256…`) gives one hash over all decoded frames and ignores timestamps.
  - Audio defaults to `pcm_s16le` for hashing unless you specify `-c:a pcm_s24le` (or similar).
  - [FFmpeg wiki: framemd5 Intro and HowTo](https://fftrac-bg.ffmpeg.org/wiki/framemd5%20Intro%20and%20HowTo); [FFmpeg-user 2019](https://lists.ffmpeg.org/pipermail/ffmpeg-user/2019-July/044913.html); [hashenc patch](https://lists.mplayerhq.hu/pipermail/ffmpeg-devel/2016-March/190592.html)
- `f32le` sources get hashed as `pcm_s16le` unless the codec is set explicitly, which silently truncates and can make different files collide. Always set the hash sample format. — [FFmpeg-devel 2022-11](https://ffmpeg.org/pipermail/ffmpeg-devel/2022-November/304066.html)
- A framemd5 file doesn't replace a whole-file checksum, because files can change in ways decoded-frame hashes don't capture. — [FFmpeg wiki: framemd5](https://fftrac-bg.ffmpeg.org/wiki/framemd5%20Intro%20and%20HowTo)
- audiodiff uses SHA1 of 24-bit PCM as its audio-identity checksum, and warns that lossy formats can hash differently across platforms. — [audiodiff docs](https://audiodiff.readthedocs.io/)
- The foobar2000 `foo_audiomd5` component computes MD5s of decoded audio. It is prior art for "audio-only" hashes in player tooling. — [foo_audiomd5](https://foobar2000.org/components/view/foo_audiomd5) (snippet)
- Streaming reads: `soundfile.blocks(blocksize, overlap)` gives chunked processing without loading whole files. — [python-soundfile docs](https://python-soundfile.readthedocs.io/en/latest/)
- Rust: Symphonia is a pure-Rust demux/decode stack (WAV/AIFF/FLAC/MP3/ALAC/AAC, …) with per-format feature crates. By default it enables only royalty-free formats. — [Symphonia README](https://docs.rs/crate/symphonia/latest/source/README.md)

### Inferences
- Canonical PCM hash: decode to interleaved little-endian at the native bit depth (int16/int24 for PCM/FLAC; float32 for float WAV), using the native sample rate and channel count with no resampling. Hash with SHA-256 or BLAKE3. Store `{lfs_oid, pcm_hash, format_fingerprint, chunk_hashes[]}` in an app-side index (SQLite), or as git notes keyed by blob.
- Same PCM bits stored as WAV and as FLAC give the same PCM hash, so a "converted WAV→FLAC losslessly" change can be detected exactly.
- FLAC already embeds an MD5 of the unencoded audio in its STREAMINFO block, which gives a free PCM hash without decoding. WAV/AIFF data chunks can be hashed straight from the file bytes by skipping the header and metadata chunks: parse RIFF/AIFF chunks and hash only `data`/`SSND`. This is much faster than decoding. (General knowledge of the FLAC and RIFF specs; not fetched this session.)
- Chunk hashing for region detection: hash fixed sample-count chunks (e.g. 48,000 frames), and the mismatched chunk indices are the changed seconds. An inserted or deleted segment shifts every later chunk, so fall back to cross-correlation alignment, or to content-defined chunking (a rolling hash over quantized PCM), when mismatches run to the end of the file.
- Performance for long multitrack sessions:
  - Do every analysis in one streaming pass per file (decode once; feed hasher, peaks, ebur128 and the fingerprint at the same time).
  - Run it at commit or push time (a git hook, or the app's commit flow), or lazily on first view.
  - Cache artifacts (peaks `.dat`, spectrogram tiles, analysis JSON) keyed by LFS oid, so they never need recomputing and can be shared across clones through the app server rather than stored in git.
  - Pairwise diffs (residual, regions) are keyed by (oidA, oidB) and computed on demand.
- Language choice: Python (numpy/scipy/soundfile/librosa) is fastest to prototype the analysis service. Rust (Symphonia + hound for WAV writing + rustfft) suits an embedded desktop or Tauri app with no Python runtime. FFmpeg CLI is the universal fallback decoder for odd formats. pydub is a convenience wrapper around FFmpeg and adds little over calling FFmpeg directly (general knowledge).

### Gaps
- I didn't check the hound crate, the librosa/essentia performance characteristics, or pydub docs this session.
- I didn't verify whether `ffmpeg -f hash` honours the LAME gapless trim. If it doesn't, MP3 PCM hashes would include encoder delay and padding.
- I found no benchmark data on decode and analysis throughput for long multitrack sessions.
