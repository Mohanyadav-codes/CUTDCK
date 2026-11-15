# Cutdeck: product and build plan

Name: **Cutdeck**.
Part of the mohanbuilds studio. Landing page: `cutdeck.mohanbuilds.com`.

## 1. What it does

Cutdeck turns a long video (podcast, talking head, stream, tutorial) into ready-to-post vertical
shorts on your own computer. No upload, no account, no credits, no watermark.

1. Drop a long video onto the corner tray.
2. Cutdeck transcribes it, finds the moments worth posting, crops them to 9:16 around the speaker,
   cuts silences and filler words, and adds animated word-by-word captions.
3. Clips slide out from the screen edge as cards. Hover to play, flick right to keep, flick left to
   discard.
4. Kept clips export as 1080x1920 MP4s, ready for YouTube Shorts, Reels and TikTok.

### Version 1 inputs

- Talking head (one speaker on camera).
- Podcast or interview (two speakers, one or two cameras).
- Screen recording with a facecam (split layout: screen on top, face below).

### Out of scope for version 1

- Downloading from YouTube URLs (add later, for the user's own videos only).
- B-roll, emoji, music, sound effects, translation.
- Cloud processing of any kind.

## 2. Where it has to stand out

Local "Opus Clip alternatives" already exist, mostly as GitHub projects: HotClip, OpenCutdeck
(Tauri), OpenClip Studio, Chopify, ClipForge, CapShorts, Shorts Studio and others. Most need Python,
Ollama or an API key, and all of them look like editors. Cutdeck wins on four points:

1. **The interaction.** The corner tray, the progress ring and swipe review. No other Cutdeck works
   like this, and it makes the content.
2. **One-click install.** A single installer. Models download on first run with a progress bar.
   No Python, no Ollama, no terminal.
3. **Pick quality.** Clips that start on a hook and end on a payoff, not random 30-second windows.
4. **Hinglish and Indian languages (to decide).** Captions in romanized Hinglish, which almost no tool
   handles well. A clear audience and a strong content angle.

## 3. How it looks

Two surfaces: the **Tray** (always there, ambient) and the **Studio** window (opens only for fine
editing).

### Visual language (shared across mohanbuilds apps)

- Dark, near-black glass panels with a soft blur, 14 px corner radius, one accent color per app.
  Cutdeck's accent: hot coral (`#FF5A4E`), with a warm amber for progress.
- Motion carries the meaning: cards slide out of the edge, rings fill, flicks have physics
  (spring with slight overshoot). Every animation under 300 ms except the "develop" moments.
- Type: Inter or Geist for UI, a bold condensed face for caption presets.
- Quiet by default. Nothing moves unless work is happening or the user hovers.

### The Tray (signature interaction)

```
                                            screen edge ->|
                                                          |
   idle:     a small coral pill in the bottom-right corner  [ â— ]
                                                          |
   dragging a video anywhere on screen: the pill grows    [ Drop to clip ]
   into a drop target with a soft glow                    |
                                                          |
   working:  a progress ring around the pill,             [ â—” 42% ]
   hover shows the stage ("Finding moments...")           |
                                                          |
   done:     clip cards fan out from the edge             [â–®][â–®][â–®][â–®]
```

- **Clip card:** a 9:16 thumbnail (about 120x213 px) with the hook line as a caption, the length,
  and a score dot (green, amber, grey).
- **Hover:** the card scales up and plays muted with captions.
- **Flick right:** keep. The card flies into the "Kept" stack and starts exporting.
- **Flick left:** discard. The card drops off screen.
- **Double-click:** opens the clip in the Studio.
- **Kept stack:** hover to fan out. Drag a finished clip straight into a browser upload page,
  WhatsApp or Explorer/Finder.

### The Studio window

```
â”Œâ”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”
â”‚  podcast-ep12.mp4 Â· 58 min Â· 14 clips found                 [Export] â”‚
â”œâ”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”¬â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”¬â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”¤
â”‚ Clips        â”‚                               â”‚ Captions              â”‚
â”‚ â–® 0:42  â—    â”‚      â”Œâ”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”          â”‚  Style  [Pop][Clean]  â”‚
â”‚ â–® 0:31  â—    â”‚      â”‚   9:16      â”‚          â”‚         [Bold][Mono]  â”‚
â”‚ â–® 0:55  â—    â”‚      â”‚   preview   â”‚          â”‚  Position  â–â–‚â–ƒ        â”‚
â”‚ â–® 0:27  â—    â”‚      â”‚  with live  â”‚          â”‚  Highlight color â–     â”‚
â”‚ ...          â”‚      â”‚  captions   â”‚          â”‚                       â”‚
â”‚              â”‚      â””â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”˜          â”‚ Framing               â”‚
â”‚              â”‚                               â”‚  [Auto][Face][Split]  â”‚
â”‚              â”‚                               â”‚  [Fit + blur]         â”‚
â”œâ”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”´â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”´â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”¤
â”‚ Transcript timeline: click words to trim, strike words to cut them   â”‚
â”‚ â”€â”€[â– â– â– â– â– â– â– â– â– â– â– â– â– â– â– â– â– â– â– â– â– â– â– ]â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€  â”‚
â””â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”˜
```

- Editing is text-based: trim a clip by selecting transcript words, delete a word to cut it.
- Framing can be adjusted by dragging the crop box on the preview, which adds a keyframe.

## 4. How it works (engine)

All processing runs on the user's machine. Bundled binaries run as sidecars of the app.

| Step | Tool | Notes |
|---|---|---|
| Decode, cut, encode | FFmpeg | Hardware encoders when available (NVENC, QuickSync, VideoToolbox). |
| Transcription with word timestamps | whisper.cpp | GPU through CUDA, Vulkan or Metal. Model chosen by hardware (base, small, large-v3-turbo). |
| Silence and filler removal | Silero VAD + transcript filler list | "um", "uh", long pauses; Hindi fillers too if Hinglish is in scope. |
| Finding moments | Small local LLM through llama.cpp (Qwen or Gemma, about 3-4B, quantized) | Scores transcript windows for hook, payoff, standalone sense. Falls back to heuristics (energy, laughter, speech rate) on weak machines. Optional: user's own API key. |
| Speaker and face tracking | ONNX face detector (YuNet or similar) + audio-to-mouth activity | Smoothed camera path so the crop never jitters. |
| Captions | ASS subtitles burned in with libass (inside FFmpeg) | Word-level highlight and pop animations. Preview draws the same style in the UI. |

Models download from Hugging Face on first run, so the installer stays small and there is no server.

### Stack

- **App shell:** Tauri 2 (Rust) with a React or Svelte front end. Small installers, transparent
  always-on-top windows for the tray, and an updater that works without OS code signing.
- **Engine:** Rust orchestration that calls the sidecars, with a job queue and resumable stages
  (a crash at 80% resumes, it doesn't restart).
- **Updates:** GitHub Releases + `updates.mohanbuilds.com/cutdeck/latest.json`, per the studio rule.

### Minimum hardware target

- 8 GB RAM, any 64-bit Windows 10/11 or macOS 12+ (Apple Silicon first-class).
- No GPU required. A GPU or Apple Silicon makes it several times faster.
- Goal: a 60-minute video produces its first clip cards within 10 minutes on an M1 or an
  RTX 3060-class PC. Treat this as a target to measure in Phase 0, not a promise.

## 5. Phase-wise plan

Times assume part-time solo work and are estimates.

### Phase 0: Prove the engine (1 week)

Goal: decide if pick quality and crop quality are good enough before building UI.

- Gather 5 test videos: solo talking head, 2-person podcast, screen + facecam, one noisy, one Hinglish.
- Scripted pipeline: FFmpeg â†’ whisper.cpp â†’ LLM picks â†’ face crop â†’ ASS captions â†’ MP4.
- Measure speed on your machine and on one weaker machine.
- Bundle FFmpeg, whisper.cpp and llama.cpp as Tauri sidecars and confirm they run on Windows and macOS.

Exit criteria: at least half of the top 5 picks per video are clips you would actually post.

Content: "Can a laptop replace Opus Clip?" with the first raw results.

### Phase 1: Engine as a real module (2 weeks)

- Rust job runner with stages, progress events and resume after a crash.
- Hardware detection that picks model sizes automatically.
- First-run model download with progress, checksum and resume.
- Smooth reframing (camera path smoothing, speaker switching with hold time).
- Silence and filler cuts with crossfades so cuts don't click.
- Caption presets: Pop, Clean, Bold, Mono.

Exit criteria: one command turns any test video into finished clips with no manual steps.

### Phase 2: The Tray (2 weeks)

- Transparent always-on-top corner window, idle pill, drop target on drag.
- Progress ring with stage names on hover.
- Clip cards fan out from the edge, hover to play, flick right or left with spring physics.
- Kept stack, export in the background, drag a finished clip out into other apps.
- Tray menu: open Studio, settings, quit.

Exit criteria: the full flow works from drop to posting a clip without opening any other window.

Content: the tray demo video. This is the launch hook.

### Phase 3: The Studio (2 weeks)

- Clip list, 9:16 preview with live captions, transcript timeline.
- Text-based trimming and word deletion.
- Framing modes: Auto, Face, Split (screen + face), Fit with blurred background.
- Caption style editing and saved presets.
- Batch export with a naming pattern.

Exit criteria: you can fix any bad auto clip in under a minute.

### Phase 4: Ship (1 week)

- Installers for Windows (NSIS) and macOS (DMG, ad-hoc signed for Apple Silicon).
- Auto-update through the studio manifest, in-app changelog.
- First-run walkthrough (one screen) and a "how to open an unsigned app" note on the landing page.
- Local crash log the user can choose to send by email. No telemetry by default.
- Landing page at `cutdeck.mohanbuilds.com` with the tray demo, before/after clips and download
  buttons. Add the card to the mohanbuilds portfolio.

Exit criteria: a friend on a fresh Windows PC and a fresh Mac installs it and gets clips without help.

### Phase 5: After launch (ongoing, pick by feedback)

- YouTube URL import for your own videos.
- Hinglish and Hindi caption modes (if not in v1).
- Titles, descriptions and hashtags per clip.
- Emoji and keyword highlights in captions, background music with auto-ducking.
- Watch folder: new recordings get clipped automatically.
- Optional own-API-key mode for higher-quality picks.
- Possible paid "Pro" later (pay once): batch folders, custom caption fonts, 4K export.

Total to public launch: about 8 weeks part-time.

## 6. Risks

| Risk | Mitigation |
|---|---|
| Local LLM picks feel random | Phase 0 gate. Combine LLM score with heuristics. Optional own-API-key mode. |
| Slow on weak laptops | Smaller models chosen automatically, heuristic picks, clear time estimate before starting. |
| Large first download (1-3 GB of models) | Download only what the hardware needs. Show size and time up front. |
| Unsigned app warnings scare users | Short "how to open" guide with screenshots on the landing page. Signing later if downloads grow. |
| Caption preview differs from export | Keep presets simple in v1. Test each preset against the exported MP4. |
| Crowded category | Lead every piece of content with the tray interaction and the one-click install. |

## 7. Open decisions

1. Final name.
2. Hinglish captions in v1, or after launch?
3. React or Svelte for the front end.
4. Open source or closed source. Open source gets GitHub reach; closed keeps a paid Pro option simpler.

