# Phase D6 — Restante hero, jingles 12+pager, Nível, VU track

**Date:** 2026-11-01 (America/Sao_Paulo)  
**Scope:** User review of `D5-1366-playing` → four fixes; verify 1366×768 + 1024×640 in `phaseD6/`.  
**No push.**

## Defect checklist

| # | Defect | Status | Notes |
|---|--------|--------|-------|
| 1 | Restante not dominant | **Fixed** | Monospace **52px** hero; Decorrido **22px** secondary grey. Color: amber `<20s`, red `<10s` (seen amber at `00:18`). |
| 2 | Jingles only 6 carts / pager gone @1366 | **Fixed** | Empty Agenda → `events_h = 78`. Always **4×3 = 12** carts; `pad_h` clamps `1…38` without stealing pager; margins `SPACE_2`. Pager `◀ Pág. 1 ▶` visible at **1366 and 1024**. |
| 3 | “Nível RMS · pico” | **Fixed** | Plain **“Nível”** + hover legend (RMS / pico / hold / zone meanings). |
| 4 | Unlit VU looks like signal | **Fixed** | Zone color washes removed. Unlit = neutral dim track (`gray 42`); faint white hairlines at −18/−6 only. Pixel check: `redish_unlitish = 0` in VU band. |

## Rect math (delta from D5)

- `top_h`: short window `0.34` clamp 210–244; tall `0.40` clamp 260–320 (room for 52px Restante).
- Empty agenda: fixed **78px**; jingles_min **280** (≥360 body) / **220**.
- Jingles: `pad_h = floor((pad_area − 3·gap)/4).clamp(1, 38)` — never force min height that clips the pager.

## Evidence (`evidence/screenshots/phaseD6/`)

- `D6-1366-playing.png` / `OL-1366-playing.png`
- `D6-1024-playing.png` / `OL-1024-playing.png`
- `crop-counters-1366.png`, `crop-VU-1366.png`, `crop-jingles-1366.png`, `crop-agenda-1366.png`
- `crop-counters-1024.png`, `crop-VU-1024.png`, `crop-jingles-1024.png`

## Tests

`cargo test --lib` → **57 passed**, 0 failed.

## Critique

**Wins**
- Restante reads as the on-air clock again (size + urgency color).
- All 12 carts + pager fit at both target sizes with empty Agenda collapsed.
- VU unlit no longer competes with lit fill; “Nível” is quieter copy.

**Residual**
- Restante cell (~78px) still needs the taller top card; at 1024 the VU bars are shorter than at 1366 (expected).
- Agenda empty strip still shows checkbox + short “Livre · sem eventos” — could collapse to header-only if more jingles room is ever needed.
- Hover legend on “Nível” is discoverability-only (no visible “?” affordance).

## Git

Dirty unpublished tree (`src/ui/mod.rs`, `src/ui/theme.rs`, reports, evidence). **No push** per brief.
