//! Airship defaults: envelope forms, gondola, and the tail fin. Built in each slot's local attachment frame — see the module
//! docstring on [`super::super`] (`parts`).

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use crate::pds::avatar::default_visuals::common::{
    cone, cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw, quat_z, sphere, spine, torus,
    with_shape, with_torture,
};
use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::{Fp, Fp3};

use super::super::PartCtx;
use super::common::{ensure_delta, floor_value, luma, saturate, shade, to_value};

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

/// Lay `n` longitudinal gore-seam battens over a scaled-ellipsoid gas bag
/// (`center` + `half`-extents), plus — when `stripe` is `Some` — a wider
/// registry band down each flank. Each batten is a thin [`spine`] tube tracing
/// one meridian of the ellipsoid from shoulder to shoulder, so the seams run
/// the length of the bag like the gores of a real fabric envelope: the surface
/// interest that stops the largest surface in the avatar set reading as a flat
/// monochrome blob (#789). The polar arc is inset from the poles so the battens
/// stop short of the nose/tail cones + finials instead of converging on them.
/// The gore ring is offset half a step so no batten lands on a flank — the
/// registry stripe owns the flanks (θ = 0, π).
pub(crate) fn push_gore_seams(
    parent: &mut Generator,
    center: [f32; 3],
    half: [f32; 3],
    n: u32,
    seam: &SovereignMaterialSettings,
    stripe: Option<&SovereignMaterialSettings>,
) {
    const SAMPLES: u32 = 9;
    const PHI_LO: f32 = 0.17; // inset from the +Z pole (fraction of PI)
    const PHI_HI: f32 = 0.83; // inset from the -Z pole
    // Trace each batten one tube-radius PROUD of the surface (inflate the
    // half-extents by `r`) so the whole tube sits *outside* the skin and never
    // goes coplanar/tangent with it at the silhouette — a straddling tube
    // z-fights there into a dashed stipple (#789 review). Reads as a raised
    // batten, not a floating hoop, since the offset is one thin tube radius.
    let meridian = |theta: f32, r: f32| -> Vec<([f32; 3], f32)> {
        let (ct, st) = (theta.cos(), theta.sin());
        let h = [half[0] + r, half[1] + r, half[2] + r];
        (0..=SAMPLES)
            .map(|k| {
                let phi = PI * (PHI_LO + (PHI_HI - PHI_LO) * (k as f32 / SAMPLES as f32));
                let (sp, cp) = phi.sin_cos();
                (
                    [
                        center[0] + h[0] * sp * ct,
                        center[1] + h[1] * sp * st,
                        center[2] + h[2] * cp,
                    ],
                    r,
                )
            })
            .collect()
    };
    for i in 0..n {
        let theta = TAU * (i as f32 + 0.5) / n as f32;
        // When a registry stripe owns the flanks (θ = 0, π), skip a batten that
        // lands on one — happens for odd `n` (e.g. n = 7), and its thin tube
        // would just bury inside the wider stripe tube (invariant hole caught
        // by the #789 review). `sin θ ≈ 0` only at the flanks here.
        if stripe.is_some() && theta.sin().abs() < 1e-3 {
            continue;
        }
        parent.children.push(prim(
            spine(&meridian(theta, 0.02), 5, seam.clone()),
            [0.0, 0.0, 0.0],
            id_quat(),
        ));
    }
    if let Some(stripe) = stripe {
        for theta in [0.0f32, PI] {
            parent.children.push(prim(
                spine(&meridian(theta, 0.035), 6, stripe.clone()),
                [0.0, 0.0, 0.0],
                id_quat(),
            ));
        }
    }
}

/// A scaled-ellipsoid gas bag (a unit sphere scaled to `half`-extents) — the
/// building block of every airship envelope form. The envelope root carries no
/// scale (the assembler mounts gondola / fins to it and a root scale would
/// stretch + fling them), so every bag is a scaled child of a hidden core.
pub(crate) fn gas_bag(
    material: &SovereignMaterialSettings,
    center: [f32; 3],
    half: [f32; 3],
) -> Generator {
    let mut bag = prim(sphere(1.0, 4, material.clone()), center, id_quat());
    bag.transform.scale = Fp3(half);
    bag
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

/// Hidden structural core for an airship envelope at the origin.
pub(super) fn env_core(body: &SovereignMaterialSettings) -> Generator {
    prim(
        cuboid([0.3, 0.3, 1.3], body.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    )
}

pub(super) fn envelope(ctx: &PartCtx) -> Generator {
    // Zeppelin — a long, rigid dirigible: a sleek matte gas bag with tapered
    // nose + tail cones, prominent segment rings, and full-length gore seams.
    let c = airship_colors(ctx);
    let skin = envelope_material(c.envelope);
    let frame = ctx.materials.metal(c.frame);
    let stripe = ctx.materials.trim(c.stripe);

    let mut env = env_core(&skin);
    // Slim + long (clearly slimmer than the fat blimp).
    let (center, half) = ([0.0, 0.0, 0.0], [0.66, 0.68, 1.55]);
    env.children.push(gas_bag(&skin, center, half));
    // Tapered nose cone (apex +Z) and tail cone (apex -Z) for the rigid points.
    env.children.push(prim(
        cone(0.4, 0.5, 12, skin.clone()),
        [0.0, 0.0, 1.42],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    env.children.push(prim(
        cone(0.44, 0.55, 12, skin.clone()),
        [0.0, 0.0, -1.4],
        quat_xyzw(quat_x(-FRAC_PI_2)),
    ));
    // Longitudinal gore battens over the bag + a registry band down each flank.
    push_gore_seams(&mut env, center, half, 8, &frame, Some(&stripe));
    // Segment rings (rigid frame) seated at the bag radius so the band
    // straddles the surface — flush, not a hoop floating proud.
    for (z, r) in [
        (-0.92f32, 0.53),
        (-0.46, 0.63),
        (0.0, 0.66),
        (0.46, 0.63),
        (0.92, 0.53),
    ] {
        env.children.push(env_ring(&frame, z, r));
    }
    // Pointed nose finial.
    env.children
        .push(prim(sphere(0.1, 3, stripe), [0.0, 0.0, 1.7], id_quat()));
    env
}

pub(super) fn envelope_blimp(ctx: &PartCtx) -> Generator {
    // Blimp — a short, fat, soft non-rigid envelope: rounded ends, only a
    // couple of soft bands, a stubbier silhouette than the zeppelin.
    let c = airship_colors(ctx);
    let skin = envelope_material(c.envelope);
    let band = ctx.materials.metal(c.frame);
    let stripe = ctx.materials.trim(c.stripe);

    let mut env = env_core(&skin);
    let (center, half) = ([0.0, 0.0, 0.0], [0.92, 0.88, 1.24]);
    env.children.push(gas_bag(&skin, center, half));
    // A short rounded tail bulb so the fins at z=-1.0 have a body to grip.
    env.children
        .push(gas_bag(&skin, [0.0, 0.0, -1.0], [0.5, 0.5, 0.5]));
    // Longitudinal gore battens + a registry band down each flank (fewer gores
    // than the zeppelin — a soft blimp is smoother, not a panelled rigid hull).
    push_gore_seams(&mut env, center, half, 6, &band, Some(&stripe));
    // Two soft bands. The bag's cross-section at z=±0.45 runs ≈0.82 (Y) to
    // ≈0.86 (X); seat the circular band near the larger radius so it straddles
    // the surface instead of sinking to a top-crescent (#781).
    for z in [-0.45f32, 0.45] {
        env.children.push(env_ring(&band, z, 0.85));
    }
    // Rounded nose finial.
    env.children
        .push(prim(sphere(0.14, 3, stripe), [0.0, 0.0, 1.2], id_quat()));
    env
}

pub(super) fn envelope_lobed(ctx: &PartCtx) -> Generator {
    // Lobed — a multi-cell caterpillar of three gas bags decreasing toward the
    // tail, jointed by rings; a deliberately segmented, knobbly silhouette.
    let c = airship_colors(ctx);
    let skin = envelope_material(c.envelope);
    let ring = ctx.materials.metal(c.frame);
    let stripe = ctx.materials.trim(c.stripe);

    // Three distinct round beads (decreasing toward the tail) set far enough
    // apart that the silhouette PINCHES to a narrow waist between them, joined
    // by thin neck cylinders — a true string-of-beads caterpillar rather than a
    // smooth ovoid with wrap-bands. The lobing reads from the profile outline.
    let mut env = env_core(&skin);
    env.children
        .push(gas_bag(&skin, [0.0, 0.0, 0.92], [0.5, 0.52, 0.46]));
    let (mid_c, mid_h) = ([0.0, 0.0, 0.0], [0.62, 0.64, 0.5]);
    env.children.push(gas_bag(&skin, mid_c, mid_h));
    env.children
        .push(gas_bag(&skin, [0.0, 0.0, -0.92], [0.48, 0.5, 0.46]));
    // Gore battens over the fat centre bead — the caterpillar's lobing carries
    // the fore/aft cells, so this just keeps the largest one from reading flat.
    push_gore_seams(&mut env, mid_c, mid_h, 6, &ring, None);
    // Thin necks bridging the pinched waists (laid along Z).
    for z in [0.46f32, -0.46] {
        env.children.push(prim(
            cylinder(0.32, 0.5, 10, skin.clone()),
            [0.0, 0.0, z],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
        // A ring cinching each neck.
        env.children.push(env_ring(&ring, z, 0.33));
    }
    // Tail cone (apex -Z) past the tail bead so the cruciform fins at z=-1.0
    // sit on a pointed tail.
    env.children.push(prim(
        cone(0.4, 0.5, 12, skin.clone()),
        [0.0, 0.0, -1.32],
        quat_xyzw(quat_x(-FRAC_PI_2)),
    ));
    // Pointed nose finial.
    env.children
        .push(prim(sphere(0.12, 3, stripe), [0.0, 0.0, 1.32], id_quat()));
    env
}

pub(super) fn envelope_twin(ctx: &PartCtx) -> Generator {
    // Twin — a catamaran dirigible: two parallel gas bags joined by a braced
    // centre truss that carries the cruciform tail. Its defining feature is the
    // pair of side-by-side hulls seen head-on.
    let c = airship_colors(ctx);
    let skin = envelope_material(c.envelope);
    let frame = ctx.materials.metal(c.frame);
    let stripe = ctx.materials.trim(c.stripe);

    let mut env = env_core(&skin);
    for s in [-1.0f32, 1.0] {
        let hull = [s * 0.46, 0.04, 0.0];
        env.children.push(gas_bag(&skin, hull, [0.4, 0.46, 1.22]));
        // Per-bag nose + tail cones.
        env.children.push(prim(
            cone(0.26, 0.4, 10, skin.clone()),
            [s * 0.46, 0.04, 1.1],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
        env.children.push(prim(
            cone(0.28, 0.42, 10, skin.clone()),
            [s * 0.46, 0.04, -1.08],
            quat_xyzw(quat_x(-FRAC_PI_2)),
        ));
        env.children.push(prim(
            sphere(0.07, 3, stripe.clone()),
            [s * 0.46, 0.04, 1.32],
            id_quat(),
        ));
        // A few gore battens per hull so the twin's bags read as taut fabric
        // like the other forms (no flank stripe — the inner flanks face the
        // narrow tunnel between the hulls where a band would just be hidden).
        push_gore_seams(&mut env, hull, [0.4, 0.46, 1.22], 4, &frame, None);
    }
    // Centre truss (#789): the bare connecting slabs become an exposed airframe.
    // The two hulls nearly touch at the centreline (a ~0.12 tunnel between
    // ±0.46 hulls of 0.4 half-width), so a truss spanning at hull-centre height
    // buries ~85 % of itself inside the hulls and — in the dark `frame` tone —
    // vanished into their shadow (#789 review). The fix drops the truss LOW so
    // its crossings dip below the hull bottoms (y = −0.42) into open air where
    // they read, and wears the brighter `accent` metal so the bracing pops
    // against the shadowed inner hulls instead of merging with them.
    let strut = ctx.materials.metal(c.accent);
    // Longitudinal keel beam slung under the tunnel — the member the gondola
    // cables meet and the fins seat behind.
    env.children.push(prim(
        cuboid([0.07, 0.09, 1.2], strut.clone()),
        [0.0, -0.36, 0.0],
        id_quat(),
    ));
    // X cross-struts at three stations: each is a pair of diagonals whose
    // crossing sits in the exposed tunnel slot and whose lower arms splay down
    // past the hull bottoms into clear air (rake ≈ 0.42 rad over a ±0.45 × ±0.19
    // diagonal), so the bracing reads as an X rather than a buried blob.
    for z in [0.6f32, 0.0, -0.6] {
        for sign in [1.0f32, -1.0] {
            env.children.push(prim(
                cuboid([0.9, 0.055, 0.06], strut.clone()),
                [0.0, -0.32, z],
                quat_xyzw(quat_z(sign * 0.42)),
            ));
        }
    }
    // Central empennage at the cruciform-fin mount (z = -1.0), so the dorsal /
    // ventral fins have a body to grip at the centreline between the two hulls.
    // The vertical stabiliser is tapered + raked aft (not a flat slab) so the
    // tail reads as a shaped fin rather than the bare rectangle a plain cuboid
    // showed broadside (#781); the horizontal spar stays a thin plate (it never
    // read as a slab) but is trimmed shallower so the swept fins overhang it.
    env.children.push(prim(
        with_shape(
            cuboid([0.12, 1.1, 0.46], skin.clone()),
            [0.3, 0.7], // draw the top in — full chord at the root, thin aloft
            [0.0, 0.0, 0.0],
            [0.0, -0.12], // rake the tip aft
        ),
        [0.0, 0.0, -1.0],
        id_quat(),
    ));
    env.children.push(prim(
        cuboid([1.0, 0.12, 0.34], skin),
        [0.0, 0.0, -1.0],
        id_quat(),
    ));
    env
}

pub(super) fn gondola(ctx: &PartCtx) -> Generator {
    // The gondola wears the envelope's complement (`accent`), value-separated
    // from the bag, so cabin and envelope read as two coordinated colours (not
    // an independent third draw). Its lit window band uses the *normalized*
    // interior-light colour + a disciplined emission (#789): every seed reads
    // lit and warm, where the raw-tertiary glow used to be dead or blown out.
    let c = airship_colors(ctx);
    let body = ctx.materials.body(c.accent);
    let keel = ctx.materials.body(shade(c.accent, 0.7));
    let frame = ctx.materials.metal(c.frame);
    let window = window_material(c.window);
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
    // A continuous lit window band along each flank, broken into panes by
    // mullions, instead of a sparse row of portholes.
    for s in [-1.0f32, 1.0] {
        g.children.push(prim(
            cuboid([0.02, 0.09, 0.74], window.clone()),
            [s * 0.225, 0.04, 0.0],
            id_quat(),
        ));
        for z in [-0.24f32, 0.0, 0.24] {
            g.children.push(prim(
                cuboid([0.03, 0.11, 0.03], frame.clone()),
                [s * 0.23, 0.04, z],
                id_quat(),
            ));
        }
    }
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
    g
}

// ---------------------------------------------------------------------------
// Skiff
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
