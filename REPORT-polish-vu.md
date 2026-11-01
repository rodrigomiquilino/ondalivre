# OndaLivre — polish + VU metering report

Date: 2026-11-01 (America/Sao_Paulo)

## Root cause (VU)

**Before:** The on-air L/R “VU” was a cosmetic animation (`sin`-based fake levels), unrelated to decoded samples or the audible mix. It could not match music dynamics, silence, or clip.

**After:** Real metering taps the **output stage**:
- `MeterTap` wraps every rodio `Source` **after** per-player gain (sink volume stays 1.0), so primary + jingles/time/aux share one `MeterBus`.
- Per-frame `take_instant` → L/R **peak + RMS** → dBFS (−60…0).
- `MeterBallistics`: fast attack, ~300 ms release, peak hold ~1.5 s, clip latch ~800 ms.
- Optional ~25 ms delay queue aligns meter with output buffer latency (playback, not decode cursor).
- UI: segmented G/Y/R bars, ticks −60/−40/−20/−10/−6/−3/0, clip LEDs; `request_repaint_after(16 ms)` (~60 FPS) while live; meters cleared/zeroed when stopped/paused.

## Tests

`cargo test --lib` → **57 passed** (incl. 5 meter tests):
- `minus_6_dbfs_peak_and_rms` — peak −6 ±0.5, RMS ≈ −9.03 ±0.5
- `silence_is_floor`
- `clip_flag_on_full_scale`
- `gain_scales_peak`
- `dbfs_meter_mapping`

Synthetic WAV: `dist/windows/music/vu-sine-m6.wav` (−6 dBFS stereo sine).

## Evidence (Wine / Xvfb)

Under `zararadio-study/evidence/screenshots/phaseD2/`:
- `FINAL-VU-playing.png` — playing `VU sine -6dBFS`; peak hold on **−6**; G/Y/R segments + scale visible
- `crop-VU-meters.png` — close-up of meters
- Other polish shots: `FINAL-00`…`FINAL-06`, `FINAL-Estacao.png`

## UI polish (this cycle)

- Localizar inline on playlist; fade/overlap clamped; classic menu in title bar; slim InfoBar/session strip; compact transport off No ar; Sweepers empty state; Estação footer height raised (72 px) so label/name are not clipped.
- Win7 workflow/updater/README work **cancelled** per user; **SAPI TTS** kept (no hard WinRT).

## Honest critique

- Estação is a pinned footer (name only), not a full nav page — easy to miss vs. Settings “Estação” fields.
- Meter latency compensation is a fixed 25 ms default, not measured WASAPI buffer size (good enough under Wine; refine on Windows if buffer sizes vary).
- Multi-voice max-merge of RMS is peak-of-RMS, not true energy sum across buses (acceptable for broadcast-style meters).
- Wine screenshots can briefly show Time announce (HRS/MIN) if the clock control is hit — VU proof used resume of the sine file.

## Git

Local commits only — **no push, no tag.**
