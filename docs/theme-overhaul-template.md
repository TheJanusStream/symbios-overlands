# Deep theme-overhaul template

Cyberpunk was the **pilot** for taking a theme from "has a catalogue kit" to a
fully-realized visual identity. This is the repeatable process to do the same
for any of the other 22 `ThemeArchetype` variants.

It is **screenshot-driven**: each step's judgement depends on the one before it,
so render the **same seed** and look before moving to the next step. Pick one
seed per theme up front (a DID whose `SceneCharacter::for_seed` lands the target
theme) and reuse it the whole pass.

> The mechanism (theme axis, kits, accents, voices, socio bands) and the
> infrastructure below are **already done and theme-agnostic** — an overhaul is
> tuning + per-theme arms, not new systems.

## Already universal — no per-theme work

These were built during the cyberpunk arc and apply to every theme automatically:

- **Asset dedup** — settlement props and lot buildings share one generator per
  distinct catalogue entry/slug (many `Placement`s reference it). Nothing to do.
- **Lot-based buildings** — road-growing themes auto-populate the road network's
  lots from the theme's catalogue (`terrain/lots.rs`). Just make sure the theme's
  `Landmark`/`Secondary`/`Prop` pools are rich enough (see step 4).
- **Socio finish + ruin** — `material_finish::apply_socio_finish` +
  `ruin::apply_ruin` walk every built tree by the room's prosperity/escalation.
  Per-theme work is only authoring `Poor`/`Rich` band variants (already shipped).
- **Road mesh pipeline** — the draped ribbon, deck/structure/neon split, and
  reactive rebuild are theme-agnostic; per-theme is just the palette arm (step 4).

## The overhaul, in order

### 1. Backdrop lighting (do first)
A self-lit/mood theme needs the right darkness to read — tuning emissives against
the wrong backdrop is wasted effort.
- `theme_luminosity(theme)` in [`seeded_defaults/room/accent.rs`](../src/seeded_defaults/room/accent.rs)
  — `1.0` = full day, lower = darker. Feeds `apply_nightfall()` in
  [`pds/room.rs`](../src/pds/room.rs)`::default_for_seed`, which scales the sun +
  ambient down and darkens sky/fog/cloud together. Identity at `1.0`, so daylight
  themes stay byte-for-byte untouched.
- `ThemeAccent::for_theme(theme)` (same file) — bounded fog/sky `tint` +
  `tint_strength`, additive `haze`, multiplicative `brightness`, optional
  `particle_mood`. The *light* nudge that makes the surroundings echo the theme.
- Pick the theme's luminosity from the **staging table** below, apply, screenshot.

### 2. Emissive color-hold (second)
HDR + bloom (`Bloom::NATURAL`, `camera.rs`) clips a `glow(color, strength)`
surface to white once `color × strength` pushes any channel past `1.0`.
- A **broad face** (panel, screen, sign) hits that point at far lower strength
  than a **thin tube** (band, edge strip, ring): faces ~`1.5–3.5`, tubes ~`5–9`.
- Frame a bright face with a hot thin-tube border so it reads as a framed sign,
  not a white slab. Reference: [`catalogue/items/cyberpunk/mod.rs`](../src/catalogue/items/cyberpunk/mod.rs).

### 3. Geometry / cohesion (last)
Per-item silhouette/material fixes, once lighting + emissive read true:
- A dark body + lit window-bands beats a pale full-`window_wall` mass; two
  drooping panels read as a draped tarp where a flat slab doesn't.
- Watch coplanar **z-fighting** on every geometry change: never size trim/
  foundation to exactly meet the structure (use `FOUNDATION_INSET`; keep edge
  strips proud). See the `zfight-coplanar` rule.

### 4. Roads + lots (road-growing themes only)
Gate: `theme_grows_roads(theme)` in `pds/room.rs` (Cyberpunk, ModernCity,
IndustrialPark, Roadside, CivicCampus, Suburban, SportsRec).
- **Road palette** — add/tune the theme's arm in
  [`terrain/roads.rs`](../src/terrain/roads.rs)`::road_palette`: `deck` +
  `structure` base colours, `edge` emissive (via `glow`), `edge_unlit`
  (true = neon tube, false = lit painted line).
- **Density** — add/tune the theme's arm in `pds/room.rs::road_config_from_scene`
  (district ½-extent + major/minor spacing by how built-up the theme is;
  prosperity grows + tightens it). Keep derived values inside the GUI slider
  ranges (a test enforces this).
- **Lot buildings** — automatic. Verify the catalogue pools are deep enough that
  a filled district doesn't read repetitive.

### 5. Accent + voice
- `ThemeAccent::for_theme` arm (step 1) — tune during the pass.
- `ThemeVoice` arm in `room/audio/theme_music.rs::voice_for` (**exhaustive** match
  — a new theme won't compile without one).

### 6. Verification gates
- `cargo test --lib` — per-theme `sanitize`-stable + emissive-survives +
  settlement-resolves tests must stay green.
- `cargo clippy --lib` + `cargo fmt --check` + `cargo doc --no-deps
  --document-private-items` (0 warnings).
- Screenshot the **same seed** after each step above.

## Per-theme lighting staging

Recommended starting `theme_luminosity` for each theme (apply, then screenshot to
confirm — these are calibrated guesses, not final). Only Cyberpunk is set today
(`0.12`); every other theme is `1.0`. Mood themes that *feature self-illumination
or gloom* are the candidates to drop; sun-lit themes stay at `1.0`.

| Theme            | Luminosity | Backdrop rationale                                  |
|------------------|-----------:|-----------------------------------------------------|
| Cyberpunk        | 0.12       | Neon-noir night; emissive trim carries (shipped).   |
| GothicHorror     | 0.25       | Moonlit gloom; lanterns/windows should glow.        |
| AlienOrganic     | 0.30       | Biolume motes only read against darkness.           |
| SpaceOutpost     | 0.35       | Void/night; panel + window lights feature.          |
| AlienMonolithic  | 0.50       | Eerie dim so glyph/seam glows register.             |
| ModernCity       | 0.55       | Evening city — LED streetlights + lit windows read. |
| IndustrialPark   | 0.70       | Sodium-lit yard at dusk; hazard amber reads.        |
| PostApoc         | 0.75       | Ashen overcast (mostly via accent haze, not night). |
| Steampunk        | 0.80       | Sooty dusk; furnace/gaslight glow.                  |
| Medieval         | 1.00       | Sun-lit; no self-illumination to feature.           |
| Nordic           | 1.00       | Sun-lit.                                            |
| FeudalJapan      | 1.00       | Sun-lit.                                            |
| Mesoamerican     | 1.00       | Sun-lit.                                            |
| RuralFarmland    | 1.00       | Sun-lit.                                            |
| CoastalResort    | 1.00       | Sun-lit, bright.                                    |
| CivicCampus      | 1.00       | Sun-lit civic daytime.                              |
| SportsRec        | 1.00       | Sun-lit daytime.                                    |
| Roadside         | 1.00       | Daytime highway; lane lines are paint, not glow.    |
| Suburban         | 1.00       | Daytime residential.                                |
| Solarpunk        | 1.00       | Lush daylight; warm accent rather than darkness.    |
| Fantasy          | 1.00       | Daylight default (a magical-dusk variant is later). |
| WildWest         | 1.00       | Harsh daylight.                                     |
| AncientClassical | 1.00       | Sun-lit (also the settlement fallback theme).       |

When a theme drops below `1.0`, add its arm to `theme_luminosity` and confirm its
kit's emissives (step 2) carry the now-darker scene before tuning geometry.

## Suggested rollout order

1. **Urban themes first** (ModernCity, IndustrialPark) — they exercise the
   already-generalized roads + lots, so the most reuse and the fastest signal
   that the generalization holds.
2. **Mood/self-lit themes** (GothicHorror, AlienOrganic, SpaceOutpost) — highest
   payoff from the lighting + emissive steps.
3. **Daylight themes** — mostly geometry/cohesion + accent polish; lowest risk.
