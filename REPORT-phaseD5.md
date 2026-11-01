# Phase D5 — layout polish (Zara proportions + defect pass)

**Date:** 2026-11-01 (America/Sao_Paulo)  
**Scope:** User defects on `OL-1366-playing` → Rect-measured top card, horizontal stereo VU, single transport, command-bar spacing, verify 1366×768 + 1024×640.  
**No push.**

## Defect checklist

| # | Defect | Status | Notes |
|---|--------|--------|-------|
| 1 | Decorrido clipped | **Fixed** | 64px top-aligned counter cells, expanded clip_rect, monospace 28. Digit ink fully inside cell (pixel-measured). |
| 2 | VU tiny / empty card center | **Fixed** | Wide horizontal E/D bars fill remaining top-card height (~165px green-fill span at 1366). −60…0 scale + peak readout (−6/−7 dB on sine). |
| 3 | Agenda empty-state clipped | **Fixed** (1366); **OK** (1024) | Empty hint outside ScrollArea; single 11px line + pad; `events_h ≥ 148`. |
| 4 | Duplicate Play/Stop/Próximo | **Fixed** | Studio command bar: mode + Vol + Abrir lista + Anunciar hora + clock. Transport only on playlist footer. |
| 5 | Anunciar hora cramped vs clock | **Fixed** | Measured gap Anunciar→clock ≈ **133 px** (RTL allocate 96px). |

## Rect math (studio)

- `top_h`: if `content_h < 560` → `0.32` clamp 200–236; else `0.38` clamp 248–310.
- Body: playlist **70%** / right **30%**.
- Right: `events_h = (body − gap − jingles_min).clamp(148, 200)`, `jingles_h = max(140, remainder)`.
- VU: `draw_vu_lr` packs top→bottom; `bar_h` from exact remaining height (cap 72); never over-allocates past `available_height`.

## Evidence (`evidence/screenshots/phaseD5/`)

- `D5-1366-playing.png` / `OL-1366-playing.png`
- `D5-1024-playing.png` / `OL-1024-playing.png`
- `crop-VU-1366.png`, `crop-counters-1366.png`, `crop-cmd-1366.png`, `crop-agenda-1366.png`, `crop-right-1024.png`

## Tests

`cargo test --lib` → **57 passed**, 0 failed.

## Critique

**Wins**
- Horizontal stereo VU with readable dB scale + peak readout owns the lower half of the top card (Zara-like).
- Single transport under playlist; command bar no longer duplicates Play/Stop/Próximo.
- Clock clearly separated from Anunciar hora (~133px).
- Decorrido/Restante no longer hard-clipped (pixel check).

**Residual**
- Visual band between track header and VU scale still reads a bit empty (Nível caption + scale ticks before thick bars).
- At **1024×640**, Jingles often shows ~2 cart rows fully; a sliver of row 3 may clip. Mitigations left: 2-row cart mode under ~180px pad area, or collapse empty Agenda further.
- Counter font (28) is smaller than Zara hero timers; bumping needs more `top_h` on large windows only.

## Git

Dirty unpublished tree (`src/ui/mod.rs`, `src/ui/theme.rs`, …). **No push** per brief.
