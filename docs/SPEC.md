# Cutdeck: technical spec (for approval)

Product behavior and design are in `PLAN.md`. This file covers exactly what gets built, with what,
and how it is verified. Nothing here is built until it is approved.

## 1. Version 1 scope

**In:**

- Inputs: MP4, MOV, MKV, WebM. Talking head, two-person podcast, screen recording with facecam.
- Languages: English. Hinglish captions as a beta toggle (see decision D2).
- Outputs: 1080x1920 H.264/AAC MP4, 15-90 s clips. Captions burned in.
- Windows 10/11 x64 and macOS 12+ (Apple Silicon and Intel).
- Surfaces: corner Tray (drop, progress, swipe review, kept stack, drag out) and Studio window
  (clip list, preview, transcript trim, framing, caption style, export).
- First-run model download, auto-update, local crash log.

**Out (later):** URL import, B-roll, emoji, music, translation, watch folders, batch folders,
any cloud processing, telemetry.

## 2. Stack

| Layer | Choice | Why |
|---|---|---|
| App shell | Tauri 2 (Rust) | Small installer, transparent always-on-top windows, updater that works unsigned. |
| Front end | React 19 + TypeScript + Vite | Largest ecosystem; you know it. |
| Styling | Tailwind CSS v4 + CSS variables for the studio theme | Fast to build, one theme file shared by later mohanbuilds apps. |
| Animation | Motion (formerly Framer Motion) | Spring physics, drag gestures and flick velocity built in. |
| State | Zustand | Small, no boilerplate. |
| Caption preview | JASSUB (libass compiled to WebAssembly) | Renders the same `.ass` file that FFmpeg burns in, so preview and export match. |
| Engine | Rust crate `cutdeck-engine` + a CLI `cutdeck-cli` | Same code for the app and for automated tests. |
| Video | FFmpeg 7+ (bundled binary) | Decode, cut, crop, captions (libass), encode. |
| Speech to text | whisper.cpp `whisper-cli` (bundled binary) | Word timestamps, built-in Silero VAD, no Python. |
| Moment picking | llama.cpp `llama-server` (bundled binary) + Qwen3 4B Instruct Q4_K_M GGUF | Local, JSON-schema constrained output. 1.7B model on machines with under 12 GB RAM. |
| Face detection | YuNet ONNX (OpenCV Zoo, MIT) through the `ort` crate (ONNX Runtime) | About 1 MB model, fast on CPU, gives eyes, nose and mouth points for speaker detection. |
| Downloads | `reqwest` with HTTP Range resume + SHA-256 check | Models come from Hugging Face and GitHub, no server of ours. |
| Hardware info | `sysinfo` crate + `llama-server --list-devices` | Picks model sizes and GPU backend. |
| Drag out to other apps | `tauri-plugin-drag` | Native file drag from the web view into a browser, WhatsApp or Explorer/Finder. |
| Updates | `tauri-plugin-updater` + GitHub Releases + `updates.mohanbuilds.com/cutdeck/latest.json` | Studio rule; Tauri's own update signature. |
| CI | GitHub Actions (Windows + macOS runners) | Builds both platforms; builds whisper.cpp for Metal and Vulkan. |

### Binaries per platform

Checked against the current releases (whisper.cpp b5454, llama.cpp b12413):

| Binary | Windows x64 | macOS |
|---|---|---|
| llama.cpp | prebuilt `win-vulkan-x64` (GPU, works on Intel Arc, AMD, NVIDIA) + `win-cpu-x64` fallback | prebuilt `macos-arm64` (Metal) and `macos-x64` |
| whisper.cpp | prebuilt `whisper-blas-bin-x64` (CPU) for v1; Vulkan build in CI if Phase 0 shows CPU is too slow | no prebuilt; built in CI with Metal |
| FFmpeg | GPL static build with libass, x264, QSV/NVENC/AMF | static build with libass, x264, VideoToolbox |
| ONNX Runtime | DLL loaded by `ort` | dylib loaded by `ort` |

FFmpeg ships as a separate executable with its GPL licence and a source link in the About screen.
Cutdeck's own code stays under whatever licence you choose (decision D4).

## 3. Engine pipeline

Each stage writes its output to the project folder. A stage is skipped when its output exists and
its input hash matches, which makes jobs resumable after a crash or quit.

```
probe ─► audio ─► transcribe ─► segment ─► pick ─► (per clip) faces ─► edit ─► frame ─► captions ─► preview
                                                                                                   │
                                                                         keep (flick right) ──► final render
```

1. **probe**: `ffprobe` for duration, fps, resolution, rotation, audio streams.
2. **audio**: 16 kHz mono WAV.
3. **transcribe**: `whisper-cli` with VAD on, JSON-full output, token timestamps (DTW) → words with
   start, end, confidence. Model by hardware: `large-v3-turbo-q5_0` on GPU/Apple Silicon,
   `small.en-q5_1` on CPU (English), `small-q5_1` for Hinglish.
4. **segment**: words → sentences (punctuation + pauses over 600 ms) → numbered sentence list.
5. **pick**: transcript split into 8-minute windows with 1-minute overlap. For each window,
   `llama-server` returns JSON matching a schema:
   `{clips: [{start_sentence, end_sentence, hook, title, score 0-100, reason}]}`.
   Rules in the prompt: starts on a hook, makes sense alone, ends on a payoff, 15-90 s.
   Then dedupe overlapping picks, add heuristic points (speech energy, words per second, questions
   answered inside the clip), keep the top N (default 10).
   Fallback without LLM (low RAM or model missing): heuristics only.
6. **faces** (only for picked ranges, not the whole video): FFmpeg decodes 5 fps at 640 px wide as
   raw frames piped to Rust → YuNet → face boxes + mouth points → tracks matched by overlap.
7. **edit**: builds the **edit list**, the core data structure. Removes silences over 400 ms
   (keeps 120 ms padding) and filler words ("um", "uh", "you know"; Hindi fillers for Hinglish).
   Maps source time to output time. Every later stage works in output time.
8. **frame**: per mode.
   - *Face*: crop center follows the active face, smoothed (critically damped spring, dead zone 6%
     of width so small head moves don't pan).
   - *Speaker switching* (podcast): active speaker = face whose mouth points move most while VAD says
     speech; switch only after 1.5 s of the other speaker. Hard cut on switch, not a pan.
   - *Split*: screen region on top, face crop below, `vstack`.
   - *Fit + blur*: full frame centered over a blurred, zoomed copy.
   Crop positions go to FFmpeg as a `sendcmd` file driving `crop` x/y over time.
9. **captions**: `.ass` file with word-level karaoke and pop presets (Pop, Clean, Bold, Mono).
   Lines of at most 3 words / 18 characters, kept inside a safe area above the platform UI.
   Hinglish mode: Devanagari transcript → romanized by the LLM, keeping word timings.
10. **preview**: 540x960 fast-encode MP4 per clip, plus a thumbnail and a 1.5 s hover loop.
    Clip cards appear in the Tray as each preview finishes, not at the end.
11. **final render**: on keep. 1080x1920, hardware encoder if available (QSV, NVENC, AMF,
    VideoToolbox), otherwise x264 `veryfast` CRF 20. AAC 160 kbps, loudness normalized to -14 LUFS.

### Project folder

`<app data>/projects/<source hash>/`:
`meta.json`, `audio.wav`, `transcript.json`, `sentences.json`, `picks.json`,
`clips/<id>/{clip.json, faces.json, edit.json, crop.cmd, captions.ass, preview.mp4, thumb.jpg}`.
Exports go to `Videos/Cutdeck/<source name>/` by default.

## 4. App architecture

```
┌────────────────────────── Tauri app (one process) ───────────────────────────┐
│  Windows:  tray (transparent, always on top, no taskbar)   studio (normal)   │
│  Front end: React, talks to Rust only through typed commands + events        │
│  Rust:  commands ─► job queue ─► cutdeck-engine ─► sidecars (ffmpeg, whisper,│
│         llama-server, onnxruntime)                                           │
│         events ◄─ progress, clip_ready, export_done, error                   │
└──────────────────────────────────────────────────────────────────────────────┘
```

- **One job at a time**, queued. `llama-server` starts only for the pick stage and stops after,
  so it doesn't hold 3 GB of RAM while idle.
- **Tray window**: a transparent window sized for the fan-out, anchored to the bottom-right of the
  monitor under the cursor. It ignores mouse input except over visible elements: Rust polls the
  cursor position every 50 ms and turns mouse input on only inside the hit area. During a file
  drag the hit area grows to the whole corner zone so drops are caught.
- **Typed bridge**: Rust types exported to TypeScript with `specta`, so front and back can't drift.
- **Settings**: JSON in app config folder (export folder, caption preset, language, max clips,
  model choice, launch at login).

## 5. Repository

```
mohanbuilds/                 (git repo; later also holds the landing sites)
  apps/cutdeck/
    src/                     React front end (tray/, studio/, shared/)
    src-tauri/               Tauri app (commands, windows, tray hit-testing, updater)
  crates/cutdeck-engine/     pipeline, edit list, framing, captions, sidecar runners
  crates/cutdeck-cli/        `cutdeck run <video> --out <dir>` for tests and debugging
  packages/studio-ui/        shared theme + components for future mohanbuilds apps
  scripts/fetch-binaries.*   downloads pinned sidecar binaries by SHA-256
  tests/fixtures/            small generated test media (big real videos stay outside git)
  .github/workflows/         ci.yml (test), release.yml (build + publish)
```

Recommended location: `C:\dev\mohanbuilds`, **outside OneDrive**. Rust build output and
`node_modules` reach several GB and OneDrive syncing them slows builds and can lock files
(decision D1).

## 6. How it gets built: the loop

Every task in `TASKS.md` goes through the same loop until its checks pass:

1. **Plan**: pick the next unchecked task, write down its acceptance check first.
2. **Build**: smallest change that meets the check.
3. **Verify**:
   - `cargo test` + `cargo clippy -D warnings` for Rust, `tsc --noEmit` + Vitest for the front end.
   - Engine tasks: run `cutdeck-cli` on the fixture videos and assert on the output with `ffprobe`
     (size 1080x1920, duration in range, audio present, A/V drift under 40 ms, captions present).
   - UI tasks: run the app, capture screenshots, and inspect them against the design in `PLAN.md`.
4. **Fix** failures and repeat 3 (max 5 rounds, then stop and report the blocker).
5. **Record**: tick the task, add one line to `PROGRESS.md` (what changed, how it was verified),
   commit.

Stops to ask you only at: the Phase 0 quality gate (you judge if picks are postable), any decision
that changes scope, and anything costing money.

### What can and can't be verified here

- **Windows:** fully, on this machine (Core Ultra 5 125H, Intel Arc, 15 GB RAM).
- **macOS:** builds in GitHub Actions, but I can't run it. Someone needs to test the Mac build on a
  real Mac before launch.
- **Pick quality:** automated checks catch broken clips, not boring ones. You review the Phase 0
  output.

## 7. Performance targets (to measure in Phase 0)

On this machine, for a 60-minute video:

| Stage | Target |
|---|---|
| Transcribe (CPU, small.en) | under 15 min; if slower, add the Vulkan whisper build |
| Pick (Vulkan, 4B) | under 4 min |
| Faces + edit + preview, 10 clips | under 3 min |
| First clip card appears | under 20 min from drop |
| Final render per 45 s clip | under 30 s |

Installer target under 150 MB. Models download separately: about 0.5 GB (CPU path) to 3.5 GB
(GPU path).

## 8. Security and privacy

- No network calls except model downloads (pinned URLs + SHA-256) and the update check.
- Tauri capabilities limited to the commands the app uses; no shell access from the front end.
- Sidecars are called with argument lists, never through a shell string, so file names can't
  inject commands.
- File access goes through Rust commands that only touch the chosen video, the project folder and
  the export folder.

## 9. Decisions needed before starting

- **D1. Repo location:** `C:\dev\mohanbuilds` (recommended) or keep it on OneDrive Desktop.
- **D2. Hinglish:** beta toggle in v1 (recommended, it's the strongest differentiator), or after launch.
- **D3. GitHub:** a GitHub account and an empty repo (private is fine) for CI and Mac builds.
  GitHub CLI isn't installed here; I can work with plain `git` + a remote URL.
- **D4. Licence:** closed source for now (recommended; decide on open source at launch) or open now.
- **D5. Test videos:** 3-5 of your own videos (solo, podcast, screen + facecam, ideally one
  Hinglish), 5-60 min each. They stay outside git.
