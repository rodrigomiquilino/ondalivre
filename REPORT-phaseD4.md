# phaseD4 — ZaraRadio proportion pass (unpublished)

## Goal

Use ZaraRadio’s **main-window information hierarchy** as the layout reference, reinterpreted in Fluent Design (not a visual copy). Apply the steered proportions, then compare side-by-side and critique fidelity.

## ZaraRadio reference (live Wine capture)

Source: `ZaraRadio.exe` Free Edition under Wine, empty untitled list.

Saved as:

- `evidence/screenshots/phaseD4/zara-main-1366.png`
- mirrored under `zararadio-study/evidence/screenshots/phaseD4/`

Observed Zara geometry (classic Win32):

| Region | Zara placement |
|--------|----------------|
| Program output strip | Top: now-playing title, Remaining, thin **horizontal** L/R VU, End time, Next + clock/TTH |
| Playlist | Dominant **center-right** (~60–70% width) |
| Upcoming events + file tree | **Left** column |
| Jingles | **Bottom** horizontal row of 9 carts |
| Transport | Top toolbar icons + vertical arrows between events and playlist |

Manual (UserManual_en § “Program output display”) matches the strip: Remaining + L/R bars + End time; Next + clock.

## OndaLivre changes (local WIP only — not pushed)

`src/ui/mod.rs` + `src/ui/theme.rs`:

1. **Top strip** ~22% height (clamp 128–168): track + next + progress · big **Restante** + **Decorrido** · **tall vertical** stereo VU spanning strip height (E/D, RMS fill, peak needle, hold, collision-aware −60/−40/−20/−10/−6/0).
2. **Body 2-column**: playlist **~70% left** · right **~30%** stacked **Agenda** (~36%) + **Jingles** (compact carts, pad_h clamp 40–56).
3. **Transport near playlist**: Play / Stop / Próximo duplicated on the playlist footer next to edit actions (Zara bottom-toolbar adjacency), keeping the global command bar for always-visible ops.

## Evidence

| File | What |
|------|------|
| `phaseD4/zara-main-1366.png` | Live ZaraRadio reference |
| `phaseD4/OL-1366-playing.png` | OndaLivre 1366×768 playing VU sine / tracks |
| `phaseD4/OL-1024-playing.png` | OndaLivre 1024×640 same session |
| `phaseD4/crop-VU-1366.png` | Tall VU + Restante detail |
| `phaseD4/side-by-side-1366.png` | Zara \| OndaLivre comparison |

Tests: `cargo test --lib` → **57 passed**. Zigbuild Windows exe refreshed locally. **No push / no tag.**

## Hierarchy critique (faithfulness)

**Faithful (steered reinterpretation):**

- On-air primacy: one top strip carries title, next, remaining/elapsed, and level — operator glance target matches Zara’s program-output strip.
- Playlist dominates horizontal area (~70%) so the queue stays the “main element” (Zara manual wording).
- Events + carts stay secondary and always visible on the studio page (no hunt through menus for the day-to-day surface).
- Transport lives beside the playlist (footer), echoing Zara’s transport/jingle adjacency to the list.

**Intentional Fluent divergences (not bugs):**

- **VU orientation:** Zara uses thin **horizontal** L/R bars under Remaining; OndaLivre uses **tall vertical** meters spanning the strip (steering + readable dBFS scale / G-Y-R zones). Same stereo program-level job, different meter grammar.
- **Column mirroring:** Zara puts events+browser **left** and playlist **right**; OndaLivre puts playlist **left** and events+carts **right** (steered “playlist dominant on the left”). Hierarchy weight is preserved; reading order is LTR-friendly for the queue.
- **Carts:** Zara’s 9 bottom pads → OndaLivre’s compact 3×4 grid in the right column (F1–F12 pages). Density is higher; individual pads are smaller by design (`pad_h` ≤ 56).
- **Chrome:** Fluent nav rail + dark mica cards vs Win32 toolbars; classic menu kept when enabled. Not a skin of Zara.

**Gaps / watch items:**

- At **1024×640** the right column is tight: Agenda empty-state + 12 carts compete; lower carts can feel cramped vs Zara’s always-visible bottom row of 9.
- Global command-bar Play **and** playlist-footer Play both exist (operator muscle memory + Zara adjacency). Slight redundancy; acceptable until a single home for transport is chosen.
- Zara’s file-tree under events is not on the studio page (Biblioteca nav instead) — by prior “no redundancy” preference.
- Vertical VU scale still drops mid ticks when short (collision filter); −10/−6 may vanish on the shortest strips — prefer height ≥ ~100 px inside the strip.

## Verdict

Information hierarchy tracks Zara’s studio logic (on-air strip → dominant playlist → secondary events/carts → transport by the list) while the Fluent shell and steered L/R / vertical-VU choices make it a reinterpretation, not a clone. Proportion pass complete for review; keep unpublished until explicit publish approval.
