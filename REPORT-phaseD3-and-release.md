# Release v1.0.2 + phaseD3 polish (unpublished)

## Release (published)

- Tag: `v1.0.2` @ commit `c39ff19` (“Versao 1.0.2”)
- Workflow: **success** — https://github.com/rodrigomiquilino/ondalivre/actions/runs/41359092806
- Release page: https://github.com/rodrigomiquilino/ondalivre/releases/tag/v1.0.2
- Assets:
  - `ondalivre.exe` (9 762 816 bytes)
  - `ondalivre.exe.sha256`
- Base content: VU metering + prior polish from `f5117ee` (not the phaseD3 WIP below).

## phaseD3 polish (local only — not pushed)

Working tree changes after unstash (stay unpublished):

- **VU:** wider/taller bars, RMS fill vs peak needle + hold ticks, numeric peak readout, scale labels −60/−40/−20/−10/−6/0 (no −3 to reduce collision).
- **Sidebar:** fully scrollable; Estação at end of scroll (no pinned clip); Ferramentas hidden when classic menu on.
- **Announce status:** `announce_active` clears “Anunciando hora…” when queue/TTS/jingle done → “No ar: …” or “Pronto”.
- **Session InfoBar:** auto-dismiss ~12s; still slim Retomar/Ignorar.
- **Settings:** fade/overlap clamped to current track length with warning (per-track Cue/fade already clamped).
- Compact transport on non–No ar + Sweepers empty state already present; verified in shots.

### Evidence (`zararadio-study/evidence/screenshots/phaseD3/`)

- `D3-1366-VU-playing.png` + `crop-VU.png` — playing sine; peak readout ~−7/−8 at vol 0.9 (post-gain).
- `D3-1024-NoAr.png`, `D3-1024-Estacao-scrolled.png`, `crop-sidebar-1024.png` — 1024×640.
- `D3-1024-Sweepers-compact.png`, `D3-1366-Settings-compact.png`, earlier Sweepers empty.

### Critique

- Top of dBFS scale is still dense physically (−6 sits near 0 on a linear −60…0 map); dropping −3 helps but −6 vs 0 remain close.
- At vol 0.9 a −6 dBFS file reads ~−7 peak (correct post-gain); calibration demos should use vol=1.0.
- 1024×640 Estação needs a scroll gesture when Gestão fills the pane — expected with full-scroll nav.
- Wine window chrome/DPI can make “exact” client size slightly off; shots used outer 1024×640 / ~fullscreen.

Tests: `cargo test --lib` → 57 passed. No push/tag of polish.
