//! Boat defaults: the four hull forms, deck, and mast. Built in each slot's local attachment frame — see the module
//! docstring on [`super::super`] (`parts`).

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::default_visuals::common::{
    blob_cone, blob_ellipsoid, blob_group, cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw,
    quat_z, spine, with_shape,
};
use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;

use super::super::PartCtx;
use super::common::shade;

/// A hidden structural core for a boat hull at the waterline origin. The boat
/// assembler overwrites the root transform (travel yaw + hover drop) and mounts
/// the deck / mast / bow to it, so the root must stay an **unscaled** cuboid;
/// the visible hull is built from its children.
pub(super) fn boat_root(body: &SovereignMaterialSettings) -> Generator {
    prim(
        cuboid([0.2, 0.14, 0.9], body.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    )
}

/// Placement + dimensions for one boat hull built by [`boat_hull_body`].
#[derive(Clone, Copy)]
pub(super) struct HullSpec {
    /// Lateral offset of this hull from the centreline.
    x: f32,
    /// Beam (full width).
    beam: f32,
    /// Overall hull length.
    length: f32,
    /// Above-waterline height (freeboard).
    freeboard: f32,
    /// Whether to sweep the deck-edge rub-strake + waterline boot rails down
    /// this hull's flanks. Set on the visible main hull(s); the trimaran's tiny
    /// amas skip them (a rail on a stub outrigger just reads as clutter).
    rails: bool,
}

/// Sample-grid resolution for a hull BlobGroup (cells along its longest axis —
/// the hull length). Smooth enough for a clean sheer without faceting; near the
/// sanitiser's 48 ceiling but a trimaran's three hulls still bake in a few ms.
const HULL_BLOB_RES: u32 = 44;

/// Waterline boot-stripe stations as `(z_fraction, half_beam_fraction)`
/// bow→stern. A rail swept through these hugs the hull's plan outline — fine at
/// the bow, full at midships, tucked in at the transom — instead of a straight
/// cuboid whose ends poke past the narrowing blob (the monohull's "unanchored
/// side rod", #785). Near-full length + width, since the hull is fullest at the
/// waterline.
const BOOT_STATIONS: [(f32, f32); 7] = [
    (0.50, 0.06),
    (0.36, 0.26),
    (0.20, 0.40),
    (0.00, 0.45),
    (-0.20, 0.42),
    (-0.34, 0.31),
    (-0.44, 0.18),
];

/// Deck-edge rub-strake stations. Shorter and narrower than [`BOOT_STATIONS`]:
/// up at the sheer the hull is far slimmer than at the waterline, and a
/// slender blob's iso-surface pulls *inboard* of its analytic ellipsoid, so a
/// full-length sheer rail spikes past the fine bow/stern (worst on the
/// catamaran's thin pontoons, #785). Confining it to the fuller midships keeps
/// it on the topsides at every hull fullness.
const STRAKE_STATIONS: [(f32, f32); 5] = [
    (0.34, 0.20),
    (0.17, 0.30),
    (0.00, 0.33),
    (-0.20, 0.29),
    (-0.34, 0.18),
];

/// Sweep one rubbing rail down each flank of a hull at height `y`, tracing the
/// plan outline in `stations` (scaled by the spec's beam/length) so the band
/// follows the curved topsides. A thin [`spine`] tube (round section) reads as
/// a toe-rail at the sheer or a boot stripe at the waterline. Offset by the
/// hull's own `x` so a catamaran's two pontoons each carry their own.
fn hull_rail(
    parent: &mut Generator,
    spec: HullSpec,
    y: f32,
    radius: f32,
    stations: &[(f32, f32)],
    material: SovereignMaterialSettings,
) {
    let HullSpec {
        x, beam, length, ..
    } = spec;
    for s in [-1.0f32, 1.0] {
        let pts: Vec<([f32; 3], f32)> = stations
            .iter()
            .map(|(zf, xf)| ([s * xf * beam, y, zf * length], radius))
            .collect();
        parent.children.push(prim(
            spine(&pts, 6, material.clone()),
            [x, 0.0, 0.0],
            id_quat(),
        ));
    }
}

/// Build one swept boat hull into `parent` at the spec's lateral offset: a
/// single smooth BlobGroup — a full amidships mass pulled to a fine point at
/// the bow (+Z) and rounded to a transom aft — plus a thin waterline boot
/// stripe. Replaces the old topsides-box + squashed-cone-prow idiom (which
/// read as a flat-walled APC head-on, its round cone base unable to cap the
/// square section); the blob is watertight by construction and reads pointed
/// from every angle. Shared by the monohull, the catamaran's two pontoons, and
/// the trimaran's hulls so every form is the same vessel, just arranged
/// differently. The dark below-waterline two-tone rides the materials pass
/// (#786); this pass owns the shape.
pub(super) fn boat_hull_body(parent: &mut Generator, ctx: &PartCtx, spec: HullSpec) {
    let HullSpec {
        x,
        beam,
        length,
        freeboard,
        ..
    } = spec;
    // A smooth matte painted skin — the blob's curvature carries the form, so
    // the busy brushed/woven `body` normal-map (which read as scaly bumps on
    // the round hull) is dropped here; a proper hull material + wrap-mapped
    // texture is the materials pass's job (#786).
    let skin = ctx.materials.cloth(ctx.palette.primary_accent);
    let stripe = ctx.materials.accent(ctx.palette.secondary_accent);
    let hull = blob_group(
        vec![
            // Amidships mass: beam wide, freeboard tall (its top sits at the
            // deck line ≈ freeboard·0.48 so the cockpit seats on it), biased a
            // touch aft so the fullest section is behind midships.
            blob_ellipsoid(
                [0.0, -freeboard * 0.26, -length * 0.05],
                [beam * 0.5, freeboard * 0.74, length * 0.40],
                id_quat(),
                freeboard * 0.5,
            ),
            // Bow: a cone blended forward from inside the mass to a fine point
            // at ≈ +0.54·length, raised toward the deck line so the stem lifts
            // out of the water like a real sheer. quat_x(+90°) aims the cone's
            // +Y axis along +Z (the authored bow direction); the tighter blend
            // keeps the point crisp rather than bulbous.
            blob_cone(
                [0.0, -freeboard * 0.10, length * 0.34],
                beam * 0.44,
                length * 0.20,
                0.025,
                quat_xyzw(quat_x(FRAC_PI_2)),
                freeboard * 0.40,
            ),
        ],
        HULL_BLOB_RES,
        skin,
    );
    parent.children.push(prim(hull, [x, 0.0, 0.0], id_quat()));
    if spec.rails {
        // Deck-edge rub-strake just below the sheer + a waterline boot stripe,
        // both swept so they hug the curved topsides instead of a straight
        // cuboid poking past the narrowing ends (#785). The strake is a dark
        // toe-rail; the boot stripe keeps the accent band (its two-tone /
        // below-waterline refinement rides the materials pass, #786).
        let strake = ctx.materials.trim(ctx.palette.tertiary_accent);
        hull_rail(
            parent,
            spec,
            freeboard * 0.32,
            0.011,
            &STRAKE_STATIONS,
            strake,
        );
        hull_rail(
            parent,
            spec,
            freeboard * 0.02,
            0.012,
            &BOOT_STATIONS,
            stripe,
        );
    }
}

/// The seeded monohull reference dimensions `(beam, length, freeboard)` from
/// the boat blueprint — the shared proportion contract every hull *form* scales
/// from — falling back to the pre-blueprint nominal if a boat part is ever built
/// without a blueprint (defensive; a boat ctx always carries one).
fn hull_dims(ctx: &PartCtx) -> (f32, f32, f32) {
    ctx.boat()
        .map_or((0.5, 1.32, 0.26), |b| (b.beam, b.hull_len, b.freeboard))
}

pub(super) fn hull(ctx: &PartCtx) -> Generator {
    // Monohull — a single sleek smooth launch hull. Deck-edge trim (gunwales,
    // rubbing strake) rides the deck-furniture pass (#785); the box-hull's
    // straight gunwale rails don't sit on a rounded sheer.
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let (beam, length, freeboard) = hull_dims(ctx);

    let mut root = boat_root(&body);
    boat_hull_body(
        &mut root,
        ctx,
        HullSpec {
            x: 0.0,
            beam,
            length,
            freeboard,
            rails: true,
        },
    );
    root
}

pub(super) fn hull_catamaran(ctx: &PartCtx) -> Generator {
    // Catamaran — two slim pontoon hulls under a connecting deck bridge.
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let bridge = ctx.materials.body(shade(ctx.palette.primary_accent, 0.8));

    let (beam, length, freeboard) = hull_dims(ctx);
    // Pontoon dims + spread as ratios of the monohull reference, so a
    // catamaran scales with the same seeded proportions.
    let spread = beam * 0.66;
    let mut root = boat_root(&body);
    // Two slim pontoon hulls set well apart so the twin-hull tunnel reads.
    for s in [-1.0f32, 1.0] {
        boat_hull_body(
            &mut root,
            ctx,
            HullSpec {
                x: s * spread,
                beam: beam * 0.48,
                length: length * 0.94,
                freeboard: freeboard * 0.77,
                rails: true,
            },
        );
    }
    // An *open* bridge — a narrow centre deck spanning the tunnel plus two
    // cross-beams reaching the outer hulls — rather than a slab that buries the
    // catamaran's defining gap.
    root.children.push(prim(
        cuboid([spread * 1.03, 0.07, length * 0.5], bridge.clone()),
        [0.0, freeboard * 0.5, -0.05],
        id_quat(),
    ));
    for z in [length * 0.24, -length * 0.32] {
        root.children.push(prim(
            // Reach from pontoon centre to pontoon centre plus a small overhang.
            cuboid([spread * 2.18, 0.05, 0.09], bridge.clone()),
            [0.0, freeboard * 0.38, z],
            id_quat(),
        ));
    }
    root
}

pub(super) fn hull_trimaran(ctx: &PartCtx) -> Generator {
    // Trimaran — a central main hull flanked by two small outrigger amas on
    // cross-beams.
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let beam_mat = ctx.materials.metal(ctx.palette.tertiary_accent);

    let (beam, length, freeboard) = hull_dims(ctx);
    let ama_x = beam * 0.88;
    let mut root = boat_root(&body);
    boat_hull_body(
        &mut root,
        ctx,
        HullSpec {
            x: 0.0,
            beam: beam * 0.84,
            length,
            freeboard,
            rails: true,
        },
    );
    for s in [-1.0f32, 1.0] {
        boat_hull_body(
            &mut root,
            ctx,
            HullSpec {
                x: s * ama_x,
                beam: beam * 0.28,
                length: length * 0.62,
                freeboard: freeboard * 0.5,
                rails: false,
            },
        );
        // Cross-beam (aka) tying the ama to the main hull.
        root.children.push(prim(
            cuboid([beam * 0.8, 0.04, 0.08], beam_mat.clone()),
            [s * ama_x * 0.55, freeboard * 0.38, 0.06],
            id_quat(),
        ));
    }
    root
}

pub(super) fn hull_barge(ctx: &PartCtx) -> Generator {
    // Barge — a wide, flat, boxy hull with raked punt ends and gunwale walls.
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let below = ctx.materials.metal(shade(ctx.palette.primary_accent, 0.4));
    let wall = ctx.materials.body(shade(ctx.palette.primary_accent, 0.85));
    let rail = ctx.materials.metal(ctx.palette.secondary_accent);

    // The barge is a custom box (no HullSpec); scale its dimensions by the
    // seeded reference so a barge varies with the same knobs as the other
    // forms. `bw` widths (barge runs beamier than the monohull), `bl` lengths,
    // `fh` freeboard heights — each a ratio of the blueprint to the nominal.
    let (beam, length, freeboard) = hull_dims(ctx);
    let (bw, bl, fh) = (beam / 0.5, length / 1.32, freeboard / 0.26);

    let mut root = boat_root(&body);
    // Wide flat hull box.
    root.children.push(prim(
        cuboid([0.72 * bw, 0.22 * fh, 1.2 * bl], body.clone()),
        [0.0, 0.03 * fh, 0.0],
        id_quat(),
    ));
    // Dark flat bottom.
    root.children.push(prim(
        cuboid([0.66 * bw, 0.1 * fh, 1.12 * bl], below.clone()),
        [0.0, -0.12 * fh, 0.0],
        id_quat(),
    ));
    // Raked punt ends (bow lifts forward, stern lifts aft).
    for (z, ang) in [(0.66 * bl, -0.5f32), (-0.62 * bl, 0.5)] {
        root.children.push(prim(
            cuboid([0.7 * bw, 0.04, 0.34 * bl], body.clone()),
            [0.0, 0.08 * fh, z],
            quat_xyzw(quat_x(ang)),
        ));
    }
    // Gunwale walls around the deck perimeter.
    for s in [-1.0f32, 1.0] {
        root.children.push(prim(
            cuboid([0.04, 0.1 * fh, 1.16 * bl], wall.clone()),
            [s * 0.34 * bw, 0.13 * fh, 0.0],
            id_quat(),
        ));
    }
    root.children.push(prim(
        cuboid([0.66 * bw, 0.1 * fh, 0.04], wall),
        [0.0, 0.13 * fh, -0.6 * bl],
        id_quat(),
    ));
    // Rubbing rail down each flank.
    for s in [-1.0f32, 1.0] {
        root.children.push(prim(
            cuboid([0.03, 0.04, 1.18 * bl], rail.clone()),
            [s * 0.37 * bw, 0.04 * fh, 0.0],
            id_quat(),
        ));
    }
    root
}

pub(super) fn deck(ctx: &PartCtx) -> Generator {
    // A low cockpit + a shaped cabin trunk that hunkers *down* on the smooth
    // deck — the old tall boxy tub was the boxiest thing on the rounded hull
    // (#785). The cabin is a tapered wedge with a raked wrap windscreen and
    // portholes; the open cockpit aft carries a bench inside a low coaming,
    // clear of the boom that sweeps over it.
    let house = ctx.materials.body(shade(ctx.palette.primary_accent, 0.72));
    let dark = ctx.materials.metal(shade(ctx.palette.primary_accent, 0.42));
    let glass = ctx.materials.glass(ctx.palette.secondary_accent);
    // Cockpit footprint tracks the seeded hull (a narrow sleek hull would
    // otherwise wear a fixed-width tub poking over its gunwales).
    let (beam, length, _) = hull_dims(ctx);
    let (dw, dl) = (beam / 0.5, length / 1.32);

    // Low cockpit sole — the flat deck the rest sits on.
    let mut deck = prim(
        cuboid([0.34 * dw, 0.045, 0.62 * dl], house.clone()),
        [0.0, 0.0, -0.04 * dl],
        id_quat(),
    );
    // Cabin trunk forward — a low shaped wedge (top drawn in across + fore-aft)
    // rather than a slab, so it reads as a deckhouse, not a box.
    let ch = 0.10;
    deck.children.push(prim(
        with_shape(
            cuboid([0.30 * dw, ch, 0.36 * dl], house.clone()),
            [0.20, 0.34],
            [0.0, 0.0, 0.0],
            [0.0, 0.0],
        ),
        [0.0, ch * 0.5, 0.22 * dl],
        id_quat(),
    ));
    // Raked wrap windscreen across the cabin's aft face (the helm looks forward
    // over the low trunk): tapered in and leaned back.
    deck.children.push(prim(
        with_shape(
            cuboid([0.27 * dw, 0.085, 0.02], glass),
            [0.22, 0.0],
            [0.0, 0.0, -0.07],
            [0.0, 0.0],
        ),
        [0.0, ch + 0.02, 0.05 * dl],
        id_quat(),
    ));
    // Two portholes per side on the cabin trunk (short discs facing outboard).
    for s in [-1.0f32, 1.0] {
        for z in [0.16 * dl, 0.30 * dl] {
            deck.children.push(prim(
                cylinder(0.026, 0.02, 10, dark.clone()),
                [s * 0.15 * dw, ch * 0.55, z],
                quat_xyzw(quat_z(FRAC_PI_2)),
            ));
        }
    }
    // Low coaming down each side of the open cockpit (inboard of the sheer, so
    // it doesn't fight the curved hull edge).
    for s in [-1.0f32, 1.0] {
        deck.children.push(prim(
            cuboid([0.02, 0.05, 0.34 * dl], dark.clone()),
            [s * 0.16 * dw, 0.03, -0.14 * dl],
            id_quat(),
        ));
    }
    // Bench across the after end of the cockpit.
    deck.children.push(prim(
        cuboid([0.26 * dw, 0.07, 0.06], house),
        [0.0, 0.035, -0.30 * dl],
        id_quat(),
    ));
    deck
}

pub(super) fn mast(ctx: &PartCtx) -> Generator {
    // A fore-and-aft sloop rig: a raked pole carrying a triangular mainsail
    // slung from a boom, topped by a streaming pennant. The always-bright cloth
    // sail (secondary accent) breaks the old bare-crossbar-plus-lollipop
    // "crucifix" read — a fore-and-aft sail is edge-on from dead ahead, so the
    // front tile now shows a clean pole, never a cross — and gives the
    // near-monochrome hull its contrast element. Height comes from the
    // blueprint so a tall-rigged and a stubby seed differ.
    let spar = ctx.materials.metal(ctx.palette.secondary_accent);
    let canvas = ctx.materials.cloth(ctx.palette.secondary_accent);
    let flag = ctx.materials.accent(ctx.palette.tertiary_accent);
    let h = ctx.boat().map_or(0.42, |b| b.mast_h);

    // Raked pole from the deck pivot (origin) to the masthead.
    let mut root = prim(
        cylinder(0.016, h, 8, spar.clone()),
        [0.0, h * 0.5, 0.0],
        quat_xyzw(quat_x(-0.06)),
    );
    // The boom + sail hang in the pole's local frame (pole-local Y = 0 sits at
    // mid-mast; the deck is at −h/2). The sail extends aft (−Z) over the
    // cockpit; the mast is authored at the bow-forward +Z, so the boat's travel
    // yaw carries the rig aft as expected.
    let foot = h * 0.68; // sail foot / boom length aft of the mast
    let boom_y = -h * 0.34; // just above the deck, in pole-local Y
    // Boom laid along Z, from the mast (z = 0) reaching aft.
    root.children.push(prim(
        cylinder(0.012, foot, 6, spar.clone()),
        [0.0, boom_y, -foot * 0.5],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    // Triangular mainsail: luff up the mast (front, +Z), foot along the boom,
    // head near the masthead. taper.y collapses the head to a point; shear.y
    // pins the luff vertical at the mast so the leech slopes aft-and-down.
    let sail_h = h * 0.74;
    root.children.push(prim(
        with_shape(
            cuboid([0.012, sail_h, foot], canvas),
            [0.0, 0.96],
            [0.0, 0.0, 0.0],
            [0.0, foot * 0.5],
        ),
        [0.0, boom_y + sail_h * 0.5, -foot * 0.5],
        id_quat(),
    ));
    // Masthead pennant streaming aft — replaces the old lollipop nav sphere.
    // (0.012 keeps the flag thin without dropping under the sanitiser's 0.01
    // minimum cuboid dimension, which would rewrite it and break the parts'
    // survive-sanitise-unchanged round-trip.)
    root.children.push(prim(
        cuboid([0.012, 0.05, 0.14], flag),
        [0.0, h * 0.44, -0.08],
        id_quat(),
    ));
    root
}

// ---------------------------------------------------------------------------
// Airship
// ---------------------------------------------------------------------------
