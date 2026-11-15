# Cutdeck: build tasks

Status: **waiting for approval of SPEC.md**. Each task is done only when its check passes
(see the loop in SPEC.md section 6). Progress notes go in `PROGRESS.md`.

## Phase 0: Prove the engine

- [ ] 0.1 Repo scaffold (Cargo workspace, pnpm workspace, Tauri 2 app shell that opens a window).
  Check: `cargo build` and `pnpm tauri dev` run on Windows.
- [ ] 0.2 `scripts/fetch-binaries` downloads pinned FFmpeg, whisper.cpp, llama.cpp, ONNX Runtime.
  Check: each binary runs `--version` (or equivalent) from the app's resource folder.
- [ ] 0.3 Model downloader (Range resume, SHA-256). Check: interrupted download resumes; bad hash rejected.
- [ ] 0.4 probe + audio + transcribe stages. Check: transcript of a fixture has word timings within
  150 ms of known speech onsets.
- [ ] 0.5 segment + pick stages (LLM + heuristic fallback). Check: valid JSON every run; picks are
  15-90 s and start/end on sentence boundaries.
- [ ] 0.6 Minimal render: center crop + plain captions → 1080x1920 MP4. Check: ffprobe size, duration,
  audio present.
- [ ] 0.7 Benchmark on your test videos; write results to `PROGRESS.md`.
- [ ] **0.8 Quality gate (you):** review top 5 picks per video. Continue if at least half are postable.

## Phase 1: Engine complete

- [ ] 1.1 Edit list: silence and filler cuts, source↔output time mapping. Check: unit tests on mapping;
  A/V drift under 40 ms after cuts; no audible clicks (short fades at cuts).
- [ ] 1.2 Face tracking on picked ranges (YuNet via `ort`). Check: face found in more than 90% of sampled
  frames of the solo fixture.
- [ ] 1.3 Framing: Face mode with smoothing and dead zone. Check: crop stays inside the frame; no pans
  under the dead zone threshold.
- [ ] 1.4 Speaker switching for two-person video. Check: switch count and timing match a hand-labeled clip.
- [ ] 1.5 Split and Fit + blur modes. Check: render and visual check from screenshots.
- [ ] 1.6 Caption presets (Pop, Clean, Bold, Mono) as `.ass`, safe area. Check: rendered frames inspected;
  words appear within one frame of their timestamp.
- [ ] 1.7 Hinglish beta (if D2 = v1): romanized captions keep word timing. Check: Hinglish fixture.
- [ ] 1.8 Preview renders + thumbnails + hover loops, emitted clip by clip.
- [ ] 1.9 Final render: hardware encoder detection with x264 fallback, -14 LUFS.
- [ ] 1.10 Job runner: stages resumable, cancel works, `llama-server` stops after picking.
  Check: kill mid-job, restart, job resumes at the right stage.
- [ ] 1.11 Hardware detection picks model sizes. Check: forced low-RAM profile chooses the small models.

## Phase 2: Tray

- [ ] 2.1 Shared theme package (`studio-ui`): colors, type, radii, motion tokens.
- [ ] 2.2 Transparent always-on-top corner window, mouse pass-through outside the hit area,
  follows the active monitor.
- [ ] 2.3 Idle pill → drop target during a file drag → accepts video files only.
- [ ] 2.4 Progress ring + stage label on hover; cancel from the pill menu.
- [ ] 2.5 Clip cards fan out as previews finish; hover plays the loop.
- [ ] 2.6 Flick right to keep (starts final render), flick left to discard, spring physics.
- [ ] 2.7 Kept stack: hover to fan out, drag a finished MP4 out into other apps.
- [ ] 2.8 System tray menu: open Studio, settings, launch at login, quit.
- [ ] 2.9 First-run flow: hardware check → model download with size and time estimate.

Check for the phase: screenshot sequence of the full flow from drop to drag-out, compared with
`PLAN.md` section 3.

## Phase 3: Studio

- [ ] 3.1 Window layout: clip list, 9:16 preview, side panel, transcript timeline.
- [ ] 3.2 Preview player with JASSUB captions matching the export.
- [ ] 3.3 Text-based trim (select words) and word deletion; edit list updates.
- [ ] 3.4 Framing modes switch + drag crop box to add a keyframe.
- [ ] 3.5 Caption style panel, saved presets.
- [ ] 3.6 Batch export with naming pattern; settings screen.

## Phase 4: Ship

- [ ] 4.1 Windows NSIS installer, bundled binaries, size under 150 MB.
- [ ] 4.2 GitHub Actions: CI tests on Windows + macOS; release workflow builds both platforms
  (whisper.cpp Metal build, ad-hoc mac signing).
- [ ] 4.3 Updater: signing key, `latest.json`, in-app update prompt and changelog.
- [ ] 4.4 Crash log + "send by email" option; About screen with licences (FFmpeg GPL notice).
- [ ] 4.5 Clean-machine test on Windows (fresh user profile) and a real Mac (needs a tester).
- [ ] 4.6 Release v1.0.0.

## Phase 5: Landing page and studio (next task after Cutdeck)

- [ ] 5.1 `mohanbuilds` site skeleton with product content files.
- [ ] 5.2 `cutdeck.mohanbuilds.com` landing page: tray demo, before/after, downloads, unsigned-app guide.
- [ ] 5.3 Cloudflare Pages deploy + DNS + `updates.mohanbuilds.com`.
