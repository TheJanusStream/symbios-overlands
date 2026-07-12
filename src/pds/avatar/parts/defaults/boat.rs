//! Boat defaults: the four hull forms, deck, and mast. Built in each slot's local attachment frame — see the module
//! docstring on [`super::super`] (`parts`).

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::default_visuals::common::{
    cone, cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw, sphere, with_torture,
};
use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::Fp3;

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
}

/// Build one boat hull into `parent` at the spec's lateral offset: an
/// above-waterline topsides box, a pointed cone prow at the bow (+Z) flattened
/// to the hull's section, a dark below-waterline belly, and a waterline boot
/// stripe. Shared by the monohull, the catamaran's two pontoons, and the
/// trimaran's hulls so every form reads as the same vessel, just arranged
/// differently.
pub(super) fn boat_hull_body(
    parent: &mut Generator,
    body: &SovereignMaterialSettings,
    below: &SovereignMaterialSettings,
    stripe: &SovereignMaterialSettings,
    spec: HullSpec,
) {
    let HullSpec {
        x,
        beam,
        length,
        freeboard,
    } = spec;
    // A short aft topsides box leaves the forward ~40 % of the hull to the prow,
    // so the pointed bow — not a flat full-beam box wall — is what's seen
    // head-on. A gentle flare (negative taper) keeps the deck off a plain slab.
    let box_len = length * 0.58;
    let z_off = -length * 0.12;
    parent.children.push(prim(
        with_torture(
            cuboid([beam, freeboard, box_len], body.clone()),
            0.0,
            -0.1,
            [0.0, 0.0, 0.0],
        ),
        [x, freeboard * 0.15, z_off],
        id_quat(),
    ));
    // Pointed cone prow (apex +Z) forming the forward hull: its base meets the
    // box front (a touch wider, so it caps the box rather than leaving a flat
    // wall) and it tapers to the bow tip, so the craft reads pointed from
    // head-on. quat_x(+90°) sends the cone apex (+Y) to +Z; the node Z-scale
    // squashes the round section to the hull's freeboard.
    let mut prow = prim(
        cone(beam * 0.52, length * 0.52, 14, body.clone()),
        [x, freeboard * 0.22, length * 0.43],
        quat_xyzw(quat_x(FRAC_PI_2)),
    );
    prow.transform.scale = Fp3([0.96, 1.0, freeboard / (beam * 1.04)]);
    parent.children.push(prow);
    // Dark below-waterline belly — a shallow V-keel (negative taper flares the
    // waterline + narrows the keel) so the underbody reads as a hull bottom,
    // not a flat skid plate.
    parent.children.push(prim(
        with_torture(
            cuboid([beam * 0.8, freeboard * 0.78, box_len], below.clone()),
            0.0,
            -0.35,
            [0.0, 0.0, 0.0],
        ),
        [x, -freeboard * 0.42, z_off],
        id_quat(),
    ));
    // Waterline boot stripe down each flank.
    for s in [-1.0f32, 1.0] {
        parent.children.push(prim(
            cuboid([0.016, freeboard * 0.18, box_len * 0.85], stripe.clone()),
            [x + s * beam * 0.5, -freeboard * 0.05, z_off],
            id_quat(),
        ));
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
    // Monohull — a single sleek launch hull with gunwale rails.
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let below = ctx.materials.metal(shade(ctx.palette.primary_accent, 0.4));
    let stripe = ctx.materials.accent(ctx.palette.secondary_accent);
    let rail = ctx.materials.metal(ctx.palette.tertiary_accent);
    let (beam, length, freeboard) = hull_dims(ctx);

    let mut root = boat_root(&body);
    boat_hull_body(
        &mut root,
        &body,
        &below,
        &stripe,
        HullSpec {
            x: 0.0,
            beam,
            length,
            freeboard,
        },
    );
    // Gunwale rails along each deck edge (tracking the seeded beam / length).
    for s in [-1.0f32, 1.0] {
        root.children.push(prim(
            cuboid([0.03, 0.035, length * 0.74], rail.clone()),
            [s * beam * 0.48, freeboard * 0.65, -0.06],
            id_quat(),
        ));
    }
    root
}

pub(super) fn hull_catamaran(ctx: &PartCtx) -> Generator {
    // Catamaran — two slim pontoon hulls under a connecting deck bridge.
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let below = ctx.materials.metal(shade(ctx.palette.primary_accent, 0.4));
    let stripe = ctx.materials.accent(ctx.palette.secondary_accent);
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
            &body,
            &below,
            &stripe,
            HullSpec {
                x: s * spread,
                beam: beam * 0.48,
                length: length * 0.94,
                freeboard: freeboard * 0.77,
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
    let below = ctx.materials.metal(shade(ctx.palette.primary_accent, 0.4));
    let stripe = ctx.materials.accent(ctx.palette.secondary_accent);
    let beam_mat = ctx.materials.metal(ctx.palette.tertiary_accent);

    let (beam, length, freeboard) = hull_dims(ctx);
    let ama_x = beam * 0.88;
    let mut root = boat_root(&body);
    boat_hull_body(
        &mut root,
        &body,
        &below,
        &stripe,
        HullSpec {
            x: 0.0,
            beam: beam * 0.84,
            length,
            freeboard,
        },
    );
    for s in [-1.0f32, 1.0] {
        boat_hull_body(
            &mut root,
            &body,
            &below,
            &stripe,
            HullSpec {
                x: s * ama_x,
                beam: beam * 0.28,
                length: length * 0.62,
                freeboard: freeboard * 0.5,
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
    let shell = ctx.materials.body(shade(ctx.palette.primary_accent, 0.75));
    let dash = ctx.materials.metal(ctx.palette.secondary_accent);
    let glass = ctx.materials.glass(ctx.palette.secondary_accent);
    // Cockpit footprint tracks the seeded hull (a narrow sleek hull would
    // otherwise wear a fixed-width tub poking over its gunwales).
    let (beam, length, _) = hull_dims(ctx);
    let (dw, dl) = (beam / 0.5, length / 1.32);

    // Cockpit tub recessed into the pod deck.
    let mut deck = prim(
        cuboid([0.38 * dw, 0.06, 0.66 * dl], shell.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Seat back toward the stern.
    deck.children.push(prim(
        cuboid([0.26 * dw, 0.16, 0.07], shell),
        [0.0, 0.1, -0.22 * dl],
        id_quat(),
    ));
    // Dashboard fairing at the front of the cockpit.
    deck.children.push(prim(
        cuboid([0.34 * dw, 0.08, 0.06], dash),
        [0.0, 0.05, 0.24 * dl],
        id_quat(),
    ));
    // Wraparound windscreen, raked back over the cockpit.
    deck.children.push(prim(
        with_torture(
            cuboid([0.36 * dw, 0.16, 0.03], glass),
            0.0,
            0.25,
            [0.0, 0.0, -0.12],
        ),
        [0.0, 0.12, 0.24 * dl],
        id_quat(),
    ));
    deck
}

pub(super) fn mast(ctx: &PartCtx) -> Generator {
    // A short boat mast: a slightly aft-raked pole rising from the deck pivot
    // (origin) with a spreader crossbar and a masthead nav light. Height comes
    // from the blueprint so a tall-rigged and a stubby seed differ.
    let pole = ctx.materials.metal(ctx.palette.secondary_accent);
    let light = ctx.materials.glow(ctx.palette.tertiary_accent);
    let h = ctx.boat().map_or(0.42, |b| b.mast_h);

    let mut root = prim(
        cylinder(0.018, h, 8, pole.clone()),
        [0.0, h * 0.5, 0.0],
        quat_xyzw(quat_x(-0.05)),
    );
    // Spreader crossbar near the top.
    root.children.push(prim(
        cuboid([0.26, 0.02, 0.02], pole),
        [0.0, h * 0.29, 0.0],
        id_quat(),
    ));
    // Masthead nav light.
    root.children.push(prim(
        sphere(0.03, 2, light),
        [0.0, h * 0.55, 0.0],
        id_quat(),
    ));
    root
}

// ---------------------------------------------------------------------------
// Airship
// ---------------------------------------------------------------------------
