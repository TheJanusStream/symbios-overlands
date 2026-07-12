//! Airship defaults: envelope forms, gondola, and the tail fin. Built in each slot's local attachment frame — see the module
//! docstring on [`super::super`] (`parts`).

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::pds::avatar::default_visuals::common::{
    cone, cuboid, cylinder, id_quat, lathe, prim, quat_x, quat_xyzw, quat_z, sphere, spine, torus,
    with_cut, with_shape, with_torture,
};
use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::{Fp, Fp3};
use crate::seeded_defaults::{OrnatenessTier, unit_f32};

use super::super::PartCtx;
use super::common::{ensure_delta, floor_value, luma, saturate, shade, to_value};

/// Salt for the gondola-dressing sub-stream (kept distinct from the palette /
/// outfit streams so the ornamentation varies independently per ship).
const GONDOLA_DRESS_SALT: u64 = 0x60_D0_1A_DE_55_00_00_01;

// ---------------------------------------------------------------------------
// Airship colour scheme (#789)
// ---------------------------------------------------------------------------
//
// Airships read as flat monochrome blobs when the whole envelope is a single
// `body(primary)` panel — worse still, the metal finish family bakes a glossy
// brushed-panel look onto the huge gas bag, so a battered industrial ship reads
// as chocolate plastic (the survey's "materials look weird"). And the fins used
// the *tertiary* accent — an independent third draw that clashes (chartreuse
// fins on a magenta envelope). The scheme below spends only the envelope's
// two-hue palette: the envelope wears the primary (value-floored so a dark ship
// keeps a body), while the fins, gondola, and frame all derive from the
// *complement* (secondary), value-separated from the envelope so they read as
// distinct parts without a third hue. The tertiary survives only as small
// disciplined pops — the registry stripe, nose finial, and the *normalized*
// interior-light window colour (so every gondola reads lit without a blowout).

/// The seeded airship two-hue scheme, value-floored + value-separated.
#[derive(Clone, Copy)]
pub(crate) struct AirshipColors {
    /// Envelope canvas (primary accent, value-floored — the huge surface never
    /// collapses to a near-black or sky-grey blob).
    pub(crate) envelope: [f32; 3],
    /// Fins + gondola cabin — the envelope's complement (secondary),
    /// value-separated from the envelope so parts read apart *without* pulling
    /// the clashing tertiary third draw (fixes chartreuse-fins-on-magenta).
    pub(crate) accent: [f32; 3],
    /// Structural metal — frame rings, gore battens, keel beam, cross-struts:
    /// a darker shade of `accent` so the rigging reads against both the
    /// envelope and the gondola.
    pub(crate) frame: [f32; 3],
    /// Registry stripe band + nose finial — a bright tertiary small-area pop,
    /// value-floored so a dark tertiary still registers.
    pub(crate) stripe: [f32; 3],
    /// Normalized interior-light window colour (see [`window_light`]).
    pub(crate) window: [f32; 3],
}

/// Normalize a raw accent into an interior-light window colour: saturate it to
/// a jewel (a greyed accent still reads as *coloured* light), floor its value
/// (a dark accent lights up instead of reading as a dead pane), and cap it
/// below white (a near-white accent doesn't blow the pane out to a featureless
/// slab). Standardizes the gondola glazing that used to inherit the raw
/// tertiary at a fixed glow strength — dead on dark seeds, blown out on pale
/// ones, only right when the tertiary happened to be cyan (#789, absorbing the
/// #781 window item; seed 12 is the target look).
pub(crate) fn window_light(accent: [f32; 3]) -> [f32; 3] {
    // Floor the value so a dark accent lights up, saturate to a jewel so even a
    // pale low-chroma tertiary reads as *coloured* light, then cap well below
    // white (pulling a light pane back down *raises* its chroma) so a pale
    // accent doesn't wash to a featureless slab (#789 review: seeds 45/48).
    let c = saturate(floor_value(accent, 0.44));
    if luma(c) > 0.7 { to_value(c, 0.7) } else { c }
}

pub(crate) fn airship_colors(ctx: &PartCtx) -> AirshipColors {
    let p = &ctx.palette;
    let envelope = floor_value(p.primary_accent, 0.30);
    let el = luma(envelope);
    // Value-separate the complement from the envelope, then *absolutely* floor
    // + saturate it: a dark or near-neutral secondary (compounded by a battered
    // body-grime pass) otherwise collapses the fins/gondola to a near-black,
    // near-grey silhouette instead of a readable coloured stabiliser (#789
    // review, seeds 0/29/45). The floor only lifts accents a light envelope
    // pushed down, so the ≥0.16 separation still holds on dark ships.
    let accent = saturate(floor_value(
        ensure_delta(p.secondary_accent, el, 0.16),
        0.30,
    ));
    AirshipColors {
        envelope,
        accent,
        frame: floor_value(shade(accent, 0.62), 0.12),
        stripe: floor_value(p.tertiary_accent, 0.5),
        window: window_light(p.tertiary_accent),
    }
}

/// A doped-fabric / painted-canvas gas-bag material: matte, so the huge
/// envelope reads as taut cloth rather than the glossy chocolate plastic the
/// metal finish family baked onto it (the "weird materials" survey note). No
/// aggressive normal map (the #784 scaly-bump gotcha on curved surfaces); the
/// surface interest comes from the two-tone frame, gore battens, rings, and
/// registry stripe, not a texture.
pub(crate) fn envelope_material(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        metallic: Fp(0.04),
        roughness: Fp(0.72),
        ..Default::default()
    }
}

/// A disciplined self-lit gondola-window material toned to a pre-[`window_light`]
/// -normalized colour: emissive, but at a running-light strength (not the
/// fixed `glow` 5.0 that blew pale panes out), so the cabin reads lit and warm
/// at any seed.
pub(crate) fn window_material(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        metallic: Fp(0.3),
        roughness: Fp(0.35),
        emission_color: Fp3(color),
        emission_strength: Fp(3.6),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Lathe envelope continuum (#791)
// ---------------------------------------------------------------------------
//
// The envelopes were scaled spheres with bolted-on nose/tail cones — the sphere
// ↔ cone junction left a visible crease, and the handful of hardcoded
// `(half-extents, cone)` tuples gave the population only ~5 fixed silhouettes.
// Each form is now a single smooth Lathe body of revolution whose profile
// radius `r(t)` is a seeded function: the four templates (zeppelin / blimp /
// lobed / twin) set the shape knobs, the blueprint's `len_mult` / `radius_mult`
// perturb them per seed, and the rings, gore battens, and mount landmarks all
// derive from `r(t)` instead of a table — a continuum, watertight by
// construction.

/// A seeded airship-envelope silhouette: the radius profile `r(t)` for a single
/// Lathe body of revolution, `t ∈ [0, 1]` running tail (0) → nose (1). The
/// power knobs shape the ends (`> 1` pointed, `< 1` blunt/rounded); the ripple
/// pinches the profile into lobes (the caterpillar); `length` / `max_r` are
/// already blueprint-scaled.
#[derive(Clone, Copy)]
pub(crate) struct EnvProfile {
    pub(crate) length: f32,
    pub(crate) max_r: f32,
    nose_p: f32,
    tail_p: f32,
    waist: f32,
    ripple_freq: f32,
    ripple_amp: f32,
}

impl EnvProfile {
    /// Surface radius at station `t ∈ [0, 1]` (0 = tail, 1 = nose).
    pub(crate) fn radius(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        let w = self.waist.clamp(0.05, 0.95);
        let base = if t <= w {
            self.max_r * (t / w).powf(self.tail_p)
        } else {
            self.max_r * ((1.0 - t) / (1.0 - w)).powf(self.nose_p)
        };
        if self.ripple_freq > 0.5 {
            // Pinch the profile into lobes: dips to `1 − amp` at the waists
            // between beads, full radius at the beads.
            base * (1.0 - self.ripple_amp * (0.5 - 0.5 * (TAU * self.ripple_freq * t).cos()))
        } else {
            base
        }
    }
    /// Z station (envelope centred at the origin) for `t`.
    pub(crate) fn height(&self, t: f32) -> f32 {
        (t - 0.5) * self.length
    }
    pub(crate) fn nose_z(&self) -> f32 {
        0.5 * self.length
    }
}

/// The seeded profile for an envelope `slug`, its length + girth scaled by the
/// blueprint multipliers. The single source of truth shared by the envelope
/// *part* (which laths it) and the *assembler* (which seats the gondola / fins /
/// pods on landmarks derived from it) — see [`crate::pds::avatar::default_visuals`].
pub(crate) fn airship_profile(slug: &str, len_mult: f32, radius_mult: f32) -> EnvProfile {
    // `(length, max_r, nose_power, tail_power, waist, ripple_freq, ripple_amp)`
    // per form. Zeppelin: long + slender, sharpish nose. Blimp: short + fat,
    // blunt rounded ends. Lobed: a rippled profile that pinches into beads.
    // Twin: a slim single-hull profile the part laths twice. Teardrop: a sharp
    // nose over a full rounded tail, waist biased forward.
    // Power knobs are < 1 so the body stays full across the middle and only
    // rounds off near the ends (a power of ~1 gives a pointed diamond, not a
    // gas bag); a smaller power = blunter/fuller, larger = sharper.
    let (l, r, np, tp, w, rf, ra) = match slug {
        "default_envelope_blimp" => (2.4, 0.9, 0.42, 0.44, 0.5, 0.0, 0.0),
        "default_envelope_lobed" => (2.9, 0.64, 0.5, 0.52, 0.5, 2.0, 0.3),
        "default_envelope_twin" => (2.5, 0.44, 0.55, 0.55, 0.5, 0.0, 0.0),
        "airship_envelope_teardrop" => (3.0, 0.72, 0.9, 0.38, 0.6, 0.0, 0.0),
        _ => (3.15, 0.62, 0.62, 0.55, 0.5, 0.0, 0.0),
    };
    let length = l * len_mult;
    // Floor the length:diameter aspect so the shortest+fattest clamp corner
    // (len_mult 0.85 × radius_mult 1.2) can't render a wider-than-long balloon —
    // only the already-fat blimp base ever approaches 1:1; the slimmer forms
    // stay well clear, so the cap never touches them (#791 review).
    const MIN_ASPECT: f32 = 1.18;
    let max_r = (r * radius_mult).min(length / (2.0 * MIN_ASPECT));
    EnvProfile {
        length,
        max_r,
        nose_p: np,
        tail_p: tp,
        waist: w,
        ripple_freq: rf,
        ripple_amp: ra,
    }
}

/// The envelope profile for the seed being built (blueprint mults, or the
/// nominal `1.0` when a non-airship ctx exercises the part — the sanitiser
/// round-trip test).
pub(crate) fn ctx_profile(ctx: &PartCtx, slug: &str) -> EnvProfile {
    let (lm, rm) = ctx
        .airship()
        .map_or((1.0, 1.0), |bp| (bp.len_mult, bp.radius_mult));
    airship_profile(slug, lm, rm)
}

/// Number of profile stations sampled for the Lathe (a smooth spline needs
/// only a handful; must stay ≤ the sanitiser's `MAX_SWEEP_POINTS` = 16 or the
/// profile would round-trip truncated).
const ENV_STATIONS: usize = 13;

/// Build a single smooth Lathe gas-bag from a profile, laid along Z (nose +Z)
/// via `quat_x(90°)` — the pole radii pinch to a point so there are no cone
/// junctions. `x` offsets it from the centreline (the twin's two hulls).
pub(crate) fn lathe_spindle(
    p: &EnvProfile,
    x: f32,
    material: SovereignMaterialSettings,
) -> Generator {
    let pts: Vec<(f32, f32)> = (0..ENV_STATIONS)
        .map(|i| {
            let t = i as f32 / (ENV_STATIONS - 1) as f32;
            (p.radius(t), p.height(t))
        })
        .collect();
    prim(
        lathe(&pts, 22, true, material),
        [x, 0.0, 0.0],
        quat_xyzw(quat_x(FRAC_PI_2)),
    )
}

/// Segment rings seated proud of the Lathe surface at `n` interior stations,
/// their radius read from the profile (a flush band at every girth, not a
/// hardcoded `(z, r)` table). `x` matches the hull offset.
pub(crate) fn push_env_rings(
    env: &mut Generator,
    p: &EnvProfile,
    x: f32,
    n: u32,
    material: &SovereignMaterialSettings,
) {
    for i in 1..=n {
        let t = i as f32 / (n + 1) as f32;
        let r = p.radius(t);
        if r < 0.06 {
            continue;
        }
        let mut ring = env_ring(material, p.height(t), r);
        ring.transform.translation.0[0] += x;
        env.children.push(ring);
    }
}

/// Longitudinal gore battens tracing the Lathe profile (`n` meridians) + an
/// optional registry band down each flank. Seated a full standoff proud of the
/// skin (`tube + PROUD`) and kept inboard of the pinching poles (`T_LO..T_HI`),
/// so the thin batten never runs *along* the receding silhouette where it
/// z-fights into a dashed stipple (#791 review; the same class #789 fixed for
/// the fatter rings). `x` matches the hull offset.
pub(crate) fn push_env_gores(
    env: &mut Generator,
    p: &EnvProfile,
    x: f32,
    n: u32,
    seam: &SovereignMaterialSettings,
    stripe: Option<&SovereignMaterialSettings>,
) {
    const SAMPLES: u32 = 11;
    const T_LO: f32 = 0.13;
    const T_HI: f32 = 0.87;
    const PROUD: f32 = 0.02;
    let meridian = |theta: f32, tube: f32| -> Vec<([f32; 3], f32)> {
        let (ct, st) = (theta.cos(), theta.sin());
        (0..=SAMPLES)
            .map(|k| {
                let t = T_LO + (T_HI - T_LO) * (k as f32 / SAMPLES as f32);
                let r = p.radius(t) + tube + PROUD;
                ([x + r * ct, r * st, p.height(t)], tube)
            })
            .collect()
    };
    for i in 0..n {
        let theta = TAU * (i as f32 + 0.5) / n as f32;
        if stripe.is_some() && theta.sin().abs() < 1e-3 {
            continue;
        }
        env.children.push(prim(
            spine(&meridian(theta, 0.02), 5, seam.clone()),
            [0.0, 0.0, 0.0],
            id_quat(),
        ));
    }
    if let Some(stripe) = stripe {
        for theta in [0.0f32, PI] {
            env.children.push(prim(
                spine(&meridian(theta, 0.035), 6, stripe.clone()),
                [0.0, 0.0, 0.0],
                id_quat(),
            ));
        }
    }
}

/// A structural frame ring (torus in the plane ⟂ Z) at `z`, major radius `r`.
/// `r` should be ≈ the bag radius at `z`; the tube is seated one minor-radius
/// PROUD of it (major radius `r + 0.024`) so it reads as a raised frame ring
/// hugging the surface without going coplanar/tangent at the silhouette — a
/// tube straddling the skin z-fights there into a dashed stipple (#789 review).
pub(crate) fn env_ring(material: &SovereignMaterialSettings, z: f32, r: f32) -> Generator {
    prim(
        torus(0.024, r + 0.024, material.clone()),
        [0.0, 0.0, z],
        quat_xyzw(quat_x(FRAC_PI_2)),
    )
}

/// Hidden structural core for an airship envelope at the origin — the unscaled
/// root the assembler mounts the gondola / fins / pods to (a root scale would
/// stretch and fling them), with the visible Lathe spindle as its child.
pub(crate) fn env_core(body: &SovereignMaterialSettings) -> Generator {
    prim(
        cuboid([0.3, 0.3, 1.3], body.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    )
}

pub(super) fn envelope(ctx: &PartCtx) -> Generator {
    // Zeppelin — a long, slender rigid dirigible: a single smooth Lathe spindle
    // (no sphere↔cone junction crease) with a sharpish nose, prominent segment
    // rings, and full-length gore seams.
    let c = airship_colors(ctx);
    let skin = envelope_material(c.envelope);
    let frame = ctx.materials.metal(c.frame);
    let stripe = ctx.materials.trim(c.stripe);
    let p = ctx_profile(ctx, "default_envelope");

    let mut env = env_core(&skin);
    env.children.push(lathe_spindle(&p, 0.0, skin));
    // Longitudinal gore battens + a registry band down each flank, then the
    // rigid segment rings — all seated from the profile radius.
    push_env_gores(&mut env, &p, 0.0, 8, &frame, Some(&stripe));
    push_env_rings(&mut env, &p, 0.0, 5, &frame);
    // Pointed nose finial just past the profile nose.
    env.children.push(prim(
        sphere(0.1, 3, stripe),
        [0.0, 0.0, p.nose_z() + 0.06],
        id_quat(),
    ));
    env
}

pub(super) fn envelope_blimp(ctx: &PartCtx) -> Generator {
    // Blimp — a short, fat, soft non-rigid envelope: a full Lathe spindle with
    // blunt rounded ends, only a couple of soft bands (fewer gores than the
    // rigid zeppelin), a stubbier silhouette.
    let c = airship_colors(ctx);
    let skin = envelope_material(c.envelope);
    let band = ctx.materials.metal(c.frame);
    let stripe = ctx.materials.trim(c.stripe);
    let p = ctx_profile(ctx, "default_envelope_blimp");

    let mut env = env_core(&skin);
    env.children.push(lathe_spindle(&p, 0.0, skin));
    push_env_gores(&mut env, &p, 0.0, 6, &band, Some(&stripe));
    push_env_rings(&mut env, &p, 0.0, 2, &band);
    // Rounded nose finial.
    env.children.push(prim(
        sphere(0.14, 3, stripe),
        [0.0, 0.0, p.nose_z() + 0.04],
        id_quat(),
    ));
    env
}

pub(super) fn envelope_lobed(ctx: &PartCtx) -> Generator {
    // Lobed — a multi-cell caterpillar of three gas bags decreasing toward the
    // tail, jointed by rings; a deliberately segmented, knobbly silhouette.
    let c = airship_colors(ctx);
    let skin = envelope_material(c.envelope);
    let ring = ctx.materials.metal(c.frame);
    let stripe = ctx.materials.trim(c.stripe);

    // A single Lathe spindle whose profile RIPPLE pinches it into a string of
    // beads (the caterpillar) — one watertight surface instead of three bolted
    // spheres joined by neck cylinders. The lobing reads from the profile
    // outline; rings cinch the pinched waists.
    let p = ctx_profile(ctx, "default_envelope_lobed");
    let mut env = env_core(&skin);
    env.children.push(lathe_spindle(&p, 0.0, skin));
    push_env_gores(&mut env, &p, 0.0, 6, &ring, None);
    // Rings cinching the pinched waists (the ripple dips at t = 0.25, 0.75).
    for t in [0.25f32, 0.75] {
        env.children.push(env_ring(&ring, p.height(t), p.radius(t)));
    }
    // Pointed nose finial.
    env.children.push(prim(
        sphere(0.12, 3, stripe),
        [0.0, 0.0, p.nose_z() + 0.04],
        id_quat(),
    ));
    env
}

pub(super) fn envelope_twin(ctx: &PartCtx) -> Generator {
    // Twin — a catamaran dirigible: two parallel Lathe spindles joined by a
    // braced centre truss that carries the cruciform tail. Its defining feature
    // is the pair of side-by-side hulls seen head-on.
    let c = airship_colors(ctx);
    let skin = envelope_material(c.envelope);
    let frame = ctx.materials.metal(c.frame);
    let stripe = ctx.materials.trim(c.stripe);
    let strut = ctx.materials.metal(c.accent);
    let p = ctx_profile(ctx, "default_envelope_twin");
    // Hulls set a hair apart so the twin tunnel reads (scales with girth).
    let hull_x = p.max_r + 0.02;

    let mut env = env_core(&skin);
    for s in [-1.0f32, 1.0] {
        let x = s * hull_x;
        env.children.push(lathe_spindle(&p, x, skin.clone()));
        env.children.push(prim(
            sphere(0.07, 3, stripe.clone()),
            [x, 0.0, p.nose_z() + 0.03],
            id_quat(),
        ));
        // A few gore battens per hull (no flank stripe — the inner flanks face
        // the narrow tunnel where a band would just be hidden).
        push_env_gores(&mut env, &p, x, 4, &frame, None);
    }
    // Fin station: the tail-inboard point the cruciform fins + empennage share
    // with `airship_mounts` (both use −0.4·length).
    let tail = -0.4 * p.length;
    // Centre truss (#789): an exposed airframe — the two hulls nearly touch at
    // the centreline, so a truss at hull-centre height buries itself in their
    // shadow. It drops LOW so its crossings dip below the hull bottoms into
    // clear air, in the brighter `accent` metal so the bracing reads.
    env.children.push(prim(
        cuboid([0.07, 0.09, p.length * 0.92], strut.clone()),
        [0.0, -0.36, 0.0],
        id_quat(),
    ));
    // X cross-struts at three stations, spanning the hull centres (±hull_x),
    // raked so the crossings sit in the exposed tunnel slot.
    for z in [0.24 * p.length, 0.0, -0.24 * p.length] {
        for sign in [1.0f32, -1.0] {
            env.children.push(prim(
                cuboid([2.0 * hull_x, 0.055, 0.06], strut.clone()),
                [0.0, -0.32, z],
                quat_xyzw(quat_z(sign * 0.42)),
            ));
        }
    }
    // Central empennage at the fin station, so the dorsal / ventral fins grip a
    // body at the centreline between the hulls. Tapered + aft-raked vertical
    // stabiliser (not a flat slab, #781) + a thin horizontal spar.
    env.children.push(prim(
        with_shape(
            cuboid([0.12, 1.1, 0.46], skin.clone()),
            [0.3, 0.7],
            [0.0, 0.0, 0.0],
            [0.0, -0.12],
        ),
        [0.0, 0.0, tail],
        id_quat(),
    ));
    env.children.push(prim(
        cuboid([2.0 * hull_x + 0.1, 0.12, 0.34], skin),
        [0.0, 0.0, tail],
        id_quat(),
    ));
    env
}

/// A gondola car's cabin bounding box (half-extents) + underside line, so the
/// shared dressing + glazing seat railings / lanterns / windows / an
/// observation bubble relative to whatever archetype built the car (#790).
#[derive(Clone, Copy)]
pub(crate) struct GondolaDims {
    pub(crate) hw: f32,
    pub(crate) hh: f32,
    pub(crate) hl: f32,
    /// The car's lowest surface (keel bottom for the enclosed cabin, tub/deck
    /// floor for the open archetypes) — where the observation bubble seats
    /// flush. A fixed offset would dangle it below the shallow open cars (#790
    /// review), so each archetype supplies its own underside.
    pub(crate) keel_y: f32,
}

/// Open the gondola's per-part stochastic sub-stream (salted off the seed so
/// the dressing varies per ship without disturbing the palette/outfit streams).
/// `tweak` forks an independent stream for a second roll (glazing vs dressing).
fn dress_rng(ctx: &PartCtx, tweak: u64) -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(ctx.seed ^ GONDOLA_DRESS_SALT ^ tweak)
}

/// Ornateness → hanging-lantern count: plain gondolas stay spare, ornate ones
/// are festooned — so the tier finally reads on the geometry (#790).
fn lantern_count(ctx: &PartCtx) -> usize {
    match ctx.ornateness {
        OrnatenessTier::Plain => 0,
        OrnatenessTier::Adorned => 2,
        OrnatenessTier::Ornate => 4,
    }
}

/// Draw the gondola's lit glazing in one of two seeded styles — a continuous
/// mullioned window band (the salon look) or a row of round portholes (dark rim
/// + glowing lens) — both toned to the normalized interior-light colour (#789).
pub(crate) fn gondola_windows(g: &mut Generator, ctx: &PartCtx, dims: GondolaDims) {
    let c = airship_colors(ctx);
    let frame = ctx.materials.metal(c.frame);
    let window = window_material(c.window);
    let GondolaDims { hw, hh, hl, .. } = dims;
    let y = hh * 0.3;
    let mut rng = dress_rng(ctx, 0x9E);
    if unit_f32(&mut rng) < 0.5 {
        // Round portholes: a dark rim ring around a bright glowing lens.
        for s in [-1.0f32, 1.0] {
            for zf in [-0.62f32, -0.21, 0.21, 0.62] {
                let x = s * hw * 1.02;
                g.children.push(prim(
                    cylinder(0.05, 0.02, 10, frame.clone()),
                    [x, y, zf * hl],
                    quat_xyzw(quat_z(FRAC_PI_2)),
                ));
                g.children.push(prim(
                    cylinder(0.036, 0.03, 10, window.clone()),
                    [x, y, zf * hl],
                    quat_xyzw(quat_z(FRAC_PI_2)),
                ));
            }
        }
    } else {
        // Continuous lit window band broken into panes by mullions.
        for s in [-1.0f32, 1.0] {
            g.children.push(prim(
                cuboid([0.02, 0.09, hl * 1.6], window.clone()),
                [s * hw * 1.02, y, 0.0],
                id_quat(),
            ));
            for zf in [-0.52f32, 0.0, 0.52] {
                g.children.push(prim(
                    cuboid([0.03, 0.11, 0.03], frame.clone()),
                    [s * hw * 1.05, y, zf * hl],
                    id_quat(),
                ));
            }
        }
    }
}

/// Dress a built gondola car with seeded ornamentation scaled by ornateness: a
/// promenade railing round the roof, hanging lanterns at the keel corners, and
/// an observation bubble (a profile-cut bottom half-dome view port) at the bow
/// underside. Shared by every gondola archetype so the tier reads on all of
/// them (#790).
pub(crate) fn dress_gondola(g: &mut Generator, ctx: &PartCtx, dims: GondolaDims) {
    let c = airship_colors(ctx);
    let rail = ctx.materials.metal(c.frame);
    let lantern = ctx.materials.glow(c.window);
    let GondolaDims { hw, hh, hl, keel_y } = dims;
    let mut rng = dress_rng(ctx, 0x00);

    // Promenade railing round the roof: corner posts under a top rail. Every
    // ornate ship, most adorned ones, the odd plain one.
    let railed = match ctx.ornateness {
        OrnatenessTier::Ornate => true,
        OrnatenessTier::Adorned => unit_f32(&mut rng) < 0.7,
        OrnatenessTier::Plain => unit_f32(&mut rng) < 0.25,
    };
    if railed {
        let top = hh + 0.07;
        for sx in [-1.0f32, 1.0] {
            g.children.push(prim(
                cuboid([0.014, 0.014, hl * 1.9], rail.clone()),
                [sx * hw * 0.92, top, 0.0],
                id_quat(),
            ));
            for zf in [-0.85f32, -0.28, 0.28, 0.85] {
                g.children.push(prim(
                    cuboid([0.014, 0.07, 0.014], rail.clone()),
                    [sx * hw * 0.92, hh + 0.035, zf * hl],
                    id_quat(),
                ));
            }
        }
        for sz in [-1.0f32, 1.0] {
            g.children.push(prim(
                cuboid([hw * 1.84, 0.014, 0.014], rail.clone()),
                [0.0, top, sz * hl * 0.9],
                id_quat(),
            ));
        }
    }

    // Hanging lanterns at the keel corners — a dark yoke + a glowing bulb.
    let n = lantern_count(ctx);
    for spot in [[-1.0f32, 0.72], [1.0, 0.72], [-1.0, -0.72], [1.0, -0.72]]
        .iter()
        .take(n)
    {
        let (x, z) = (spot[0] * hw * 0.8, spot[1] * hl);
        g.children.push(prim(
            cylinder(0.01, 0.05, 6, rail.clone()),
            [x, -hh - 0.05, z],
            id_quat(),
        ));
        g.children.push(prim(
            sphere(0.028, 3, lantern.clone()),
            [x, -hh - 0.11, z],
            id_quat(),
        ));
    }

    // Observation bubble: a profile-cut bottom half-dome at the bow underside —
    // a downward view port. Ornate ships, or a lucky adorned one. Seated FLUSH
    // at the car's own underside (`keel_y`), not a fixed cabin-depth offset that
    // dangled it below the shallow open cars (#790 review). Glassy (not the
    // lanterns' opaque glow) + a metal rim ring so it reads as a framed port,
    // not another hanging light.
    let bubble = ctx.ornateness == OrnatenessTier::Ornate
        || (ctx.ornateness == OrnatenessTier::Adorned && unit_f32(&mut rng) < 0.5);
    if bubble {
        let z = hl * 0.55;
        // profile_cut [0, 0.5] keeps the southern (bottom) hemisphere — a dome
        // bulging downward; the flat cut face seats flush at the underside.
        let dome = with_cut(
            sphere(0.13, 4, ctx.materials.glass(c.window)),
            [0.0, 1.0],
            [0.0, 0.5],
            0.0,
        );
        g.children.push(prim(dome, [0.0, keel_y, z], id_quat()));
        // Rim ring framing the port where it meets the hull.
        g.children.push(prim(
            torus(0.014, 0.12, rail.clone()),
            [0.0, keel_y, z],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
    }
}

pub(super) fn gondola(ctx: &PartCtx) -> Generator {
    // Enclosed-cabin gondola. Wears the envelope's complement (`accent`),
    // value-separated from the bag, so cabin and envelope read as two
    // coordinated colours (#789). Glazing style + ornamentation are seeded from
    // the gondola sub-stream, scaled by ornateness (#790).
    let c = airship_colors(ctx);
    let body = ctx.materials.body(c.accent);
    let keel = ctx.materials.body(shade(c.accent, 0.7));
    let frame = ctx.materials.metal(c.frame);
    let dims = GondolaDims {
        hw: 0.22,
        hh: 0.14,
        hl: 0.46,
        // The rounded keel drops to ≈ −0.24; seat the view port at its bottom.
        keel_y: -0.24,
    };
    // Main cabin hull.
    let mut g = prim(
        cuboid([0.44, 0.28, 0.92], body.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Rounded nose + tail end caps.
    for sz in [-1.0f32, 1.0] {
        let mut cap = prim(
            sphere(0.22, 3, body.clone()),
            [0.0, -0.02, sz * 0.46],
            id_quat(),
        );
        cap.transform.scale = Fp3([0.95, 0.62, 0.55]);
        g.children.push(cap);
    }
    gondola_windows(&mut g, ctx, dims);
    // Rounded keel underneath.
    g.children.push(prim(
        cuboid([0.38, 0.12, 0.84], keel),
        [0.0, -0.18, 0.0],
        id_quat(),
    ));
    // Bridge cockpit bump at the bow (+Z).
    g.children.push(prim(
        cuboid([0.3, 0.14, 0.18], frame),
        [0.0, 0.14, 0.4],
        id_quat(),
    ));
    dress_gondola(&mut g, ctx, dims);
    g
}

// ---------------------------------------------------------------------------
// Fin + engine pod
// ---------------------------------------------------------------------------

pub(super) fn fin(ctx: &PartCtx) -> Generator {
    // A thin tapered, aft-swept fin centred on its mount; the assembler rotates
    // each copy into a cruciform tail. Centred at the origin (not pre-raised) so
    // the assembler's rotation spins it about its own centre cleanly. Tapered +
    // swept so it reads as a stabiliser, with a glowing trailing edge.
    // The blade wears the envelope's complement (`accent`, value-separated) —
    // NOT the tertiary third draw that put chartreuse fins on a magenta ship
    // (#789) — and the trailing edge uses the ship's normalized running-light
    // colour so it reads as a nav light without a blowout.
    let c = airship_colors(ctx);
    let mut f = prim(
        with_torture(
            cuboid([0.05, 0.62, 0.62], ctx.materials.body(c.accent)),
            0.0,
            0.5,
            [0.0, 0.0, -0.28],
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Glowing trailing edge along the aft side (-Z).
    f.children.push(prim(
        cuboid([0.06, 0.5, 0.04], ctx.materials.glow(c.window)),
        [0.0, 0.0, -0.28],
        id_quat(),
    ));
    f
}

/// Push a vertical pylon strut into `pod` reaching up (+Y) from the nacelle
/// into the envelope's lower flank, so the pod reads as slung under the hull
/// rather than floating. Shared by every pod variant (the assembler mounts the
/// pod X-symmetrically, so the strut stays on the centreline — no mirror flip).
pub(crate) fn pod_pylon(pod: &mut Generator, material: &SovereignMaterialSettings) {
    pod.children.push(prim(
        cuboid([0.05, 0.5, 0.09], material.clone()),
        [0.0, 0.33, 0.0],
        id_quat(),
    ));
}

pub(super) fn pod(ctx: &PartCtx) -> Generator {
    // The default engine pod: a nacelle laid along the travel axis (+Z front)
    // with a nose spinner, a torus prop-guard ring, a simple two-blade airscrew,
    // and a tapered tail — the airship's visible propulsion (a flying family
    // that had none). Wears the ship's `accent` metal so the pods read as one
    // mechanical set with the gondola / fins; a glowing hub gives a running
    // light. Authored X-symmetric (pylon up the centreline) so the assembler's
    // mirrored pair needs no flip.
    let c = airship_colors(ctx);
    let body = ctx.materials.metal(c.accent);
    let dark = ctx.materials.metal(c.frame);
    let hub = ctx.materials.glow(c.window);

    // Nacelle: a cylinder laid along Z (quat_x(90°) aims the barrel's +Y along
    // +Z, the authored travel-forward direction).
    let mut p = prim(
        cylinder(0.13, 0.52, 12, body.clone()),
        [0.0, 0.0, 0.0],
        quat_xyzw(quat_x(FRAC_PI_2)),
    );
    // Nose spinner cone (apex +Z) + a glowing hub cap at its tip.
    p.children.push(prim(
        cone(0.12, 0.16, 12, dark.clone()),
        [0.0, 0.0, 0.3],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    p.children
        .push(prim(sphere(0.045, 3, hub), [0.0, 0.0, 0.42], id_quat()));
    // Two-blade airscrew at the front (a thin vertical pair, X-symmetric).
    for sy in [-1.0f32, 1.0] {
        p.children.push(prim(
            cuboid([0.03, 0.2, 0.02], dark.clone()),
            [0.0, sy * 0.13, 0.36],
            id_quat(),
        ));
    }
    // Prop-guard ring around the airscrew (torus in the plane ⟂ Z).
    p.children.push(prim(
        torus(0.02, 0.2, dark.clone()),
        [0.0, 0.0, 0.34],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    // Tapered tail cone (apex -Z).
    p.children.push(prim(
        cone(0.1, 0.14, 12, body),
        [0.0, 0.0, -0.3],
        quat_xyzw(quat_x(-FRAC_PI_2)),
    ));
    pod_pylon(&mut p, &dark);
    p
}
