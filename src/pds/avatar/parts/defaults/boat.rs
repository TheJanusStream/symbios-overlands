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

pub(super) fn hull(ctx: &PartCtx) -> Generator {
    // Monohull — a single sleek launch hull with gunwale rails.
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let below = ctx.materials.metal(shade(ctx.palette.primary_accent, 0.4));
    let stripe = ctx.materials.accent(ctx.palette.secondary_accent);
    let rail = ctx.materials.metal(ctx.palette.tertiary_accent);

    let mut root = boat_root(&body);
    boat_hull_body(
        &mut root,
        &body,
        &below,
        &stripe,
        HullSpec {
            x: 0.0,
            beam: 0.5,
            length: 1.32,
            freeboard: 0.26,
        },
    );
    // Gunwale rails along each deck edge.
    for s in [-1.0f32, 1.0] {
        root.children.push(prim(
            cuboid([0.03, 0.035, 0.98], rail.clone()),
            [s * 0.24, 0.17, -0.06],
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

    let mut root = boat_root(&body);
    // Two slim pontoon hulls set well apart so the twin-hull tunnel reads.
    for s in [-1.0f32, 1.0] {
        boat_hull_body(
            &mut root,
            &body,
            &below,
            &stripe,
            HullSpec {
                x: s * 0.33,
                beam: 0.24,
                length: 1.24,
                freeboard: 0.2,
            },
        );
    }
    // An *open* bridge — a narrow centre deck spanning the tunnel plus two
    // cross-beams reaching the outer hulls — rather than a slab that buries the
    // catamaran's defining gap.
    root.children.push(prim(
        cuboid([0.34, 0.07, 0.62], bridge.clone()),
        [0.0, 0.13, -0.05],
        id_quat(),
    ));
    for z in [0.3f32, -0.4] {
        root.children.push(prim(
            cuboid([0.72, 0.05, 0.09], bridge.clone()),
            [0.0, 0.1, z],
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

    let mut root = boat_root(&body);
    boat_hull_body(
        &mut root,
        &body,
        &below,
        &stripe,
        HullSpec {
            x: 0.0,
            beam: 0.42,
            length: 1.32,
            freeboard: 0.26,
        },
    );
    for s in [-1.0f32, 1.0] {
        boat_hull_body(
            &mut root,
            &body,
            &below,
            &stripe,
            HullSpec {
                x: s * 0.44,
                beam: 0.14,
                length: 0.82,
                freeboard: 0.13,
            },
        );
        // Cross-beam (aka) tying the ama to the main hull.
        root.children.push(prim(
            cuboid([0.4, 0.04, 0.08], beam_mat.clone()),
            [s * 0.24, 0.1, 0.06],
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

    let mut root = boat_root(&body);
    // Wide flat hull box.
    root.children.push(prim(
        cuboid([0.72, 0.22, 1.2], body.clone()),
        [0.0, 0.03, 0.0],
        id_quat(),
    ));
    // Dark flat bottom.
    root.children.push(prim(
        cuboid([0.66, 0.1, 1.12], below.clone()),
        [0.0, -0.12, 0.0],
        id_quat(),
    ));
    // Raked punt ends (bow lifts forward, stern lifts aft).
    for (z, ang) in [(0.66f32, -0.5f32), (-0.62, 0.5)] {
        root.children.push(prim(
            cuboid([0.7, 0.04, 0.34], body.clone()),
            [0.0, 0.08, z],
            quat_xyzw(quat_x(ang)),
        ));
    }
    // Gunwale walls around the deck perimeter.
    for s in [-1.0f32, 1.0] {
        root.children.push(prim(
            cuboid([0.04, 0.1, 1.16], wall.clone()),
            [s * 0.34, 0.13, 0.0],
            id_quat(),
        ));
    }
    root.children.push(prim(
        cuboid([0.66, 0.1, 0.04], wall),
        [0.0, 0.13, -0.6],
        id_quat(),
    ));
    // Rubbing rail down each flank.
    for s in [-1.0f32, 1.0] {
        root.children.push(prim(
            cuboid([0.03, 0.04, 1.18], rail.clone()),
            [s * 0.37, 0.04, 0.0],
            id_quat(),
        ));
    }
    root
}

pub(super) fn deck(ctx: &PartCtx) -> Generator {
    let shell = ctx.materials.body(shade(ctx.palette.primary_accent, 0.75));
    let dash = ctx.materials.metal(ctx.palette.secondary_accent);
    let glass = ctx.materials.glass(ctx.palette.secondary_accent);

    // Cockpit tub recessed into the pod deck.
    let mut deck = prim(
        cuboid([0.38, 0.06, 0.66], shell.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Seat back toward the stern.
    deck.children.push(prim(
        cuboid([0.26, 0.16, 0.07], shell),
        [0.0, 0.1, -0.22],
        id_quat(),
    ));
    // Dashboard fairing at the front of the cockpit.
    deck.children.push(prim(
        cuboid([0.34, 0.08, 0.06], dash),
        [0.0, 0.05, 0.24],
        id_quat(),
    ));
    // Wraparound windscreen, raked back over the cockpit.
    deck.children.push(prim(
        with_torture(
            cuboid([0.36, 0.16, 0.03], glass),
            0.0,
            0.25,
            [0.0, 0.0, -0.12],
        ),
        [0.0, 0.12, 0.24],
        id_quat(),
    ));
    deck
}

pub(super) fn mast(ctx: &PartCtx) -> Generator {
    // A short boat mast: a slightly aft-raked pole rising from the deck pivot
    // (origin) with a spreader crossbar and a masthead nav light.
    let pole = ctx.materials.metal(ctx.palette.secondary_accent);
    let light = ctx.materials.glow(ctx.palette.tertiary_accent);

    let mut root = prim(
        cylinder(0.018, 0.42, 8, pole.clone()),
        [0.0, 0.21, 0.0],
        quat_xyzw(quat_x(-0.05)),
    );
    // Spreader crossbar near the top.
    root.children.push(prim(
        cuboid([0.26, 0.02, 0.02], pole),
        [0.0, 0.12, 0.0],
        id_quat(),
    ));
    // Masthead nav light.
    root.children
        .push(prim(sphere(0.03, 2, light), [0.0, 0.23, 0.0], id_quat()));
    root
}

// ---------------------------------------------------------------------------
// Airship
// ---------------------------------------------------------------------------
