//! Rotting Hulk — a broken-backed ship on the strand, re-roofed as a shelter.
//!
//! A hull driven ashore and left: broken across the middle, her forward half
//! open to the sky with the frames bare where the planking has gone, her after
//! half roofed over with salvaged boards and a canvas patch and lived in. A
//! stovepipe smokes out of the quarterdeck. Witchfire burns in the open hold
//! where nothing should be burning at all.
//!
//! # Why this is the cursed register's Landmark and not a Secondary
//!
//! It was planned as a Secondary, and that was a mistake worth correcting
//! before it shipped. The settlement deriver's landmark pool falls back to the
//! whole theme when a prosperity band has no landmark of its own
//! (`tiered_pool`), so a Poor pirate room would have taken the
//! [`super::harbour_battery`] — a garrisoned, colours-flying, gun-deck-lit
//! fortress — as the biggest thing in it, and then scattered a gibbet and some
//! bones around it. The register the user asked for ("the Poor register turns
//! eerie rather than merely poor") cannot survive that: whatever is largest
//! sets the reading of a room, and a working fort with bones near it reads as a
//! working fort.
//!
//! A wrecked ship is a perfectly good hero object — it is sixteen metres of
//! silhouette — and making it the landmark leaves the Secondary pool to fall
//! back to the working kit, which is the right way round. A destitute harbour
//! still has a tavern; what it does not have is a garrison.
//!
//! # The break is the subject
//!
//! Everything here is arranged around one fact: she is broken *across*, and the
//! two halves have settled at different angles. That is what separates a wreck
//! from a beached ship, and it is the reason the file is organised the way it
//! is. Each half is one leaf [`blob_group`] carrying its own single rotation,
//! and everything fixed to a half is placed in the world by that half's own
//! frame function ([`fore`] / [`aft`]) — so nothing is ever nested under a
//! rotated node (#972 lesson 22) and no fitting can drift out of agreement with
//! the hull it is fitted to. That is the careening slip's arrangement, and the
//! reason it is repeated here is that the slip's first build got the *sign* of
//! its rotation wrong and the symmetric hull hid it (#1030).
//!
//! # The frames read because they belong to one hull
//!
//! Her frames are arcs cut out of tori and **scaled** into ellipses, and their
//! heights and half-beams come from the same interpolated lines plan the
//! [`super::longboat`] uses — so they describe one continuous sheer instead of
//! being a row of hoops that happen to stand near each other. A wreck's ribs
//! are the most recognisable thing about it, and they are only recognisable if
//! they agree.
//!
//! Two things about them had to be got right before she read as a ship at all,
//! and both are now recorded on the shared [`hull_frame`]: a frame is as tall as
//! her sheer and as wide as her beam, which are different numbers and so need a
//! scale rather than a radius; and the `U` open to the sky is reached by
//! *translating* a hanging arc up by its own semi-height, because no rotation of
//! a half-ring will give it (a semicircle is congruent to its own reflection).
//! The first render came out as a black slug under a covered wagon; the second
//! hung its frames through the beach; the third flipped them back into a covered
//! wagon.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::catalogue::items::util::{
    attach, blob_box, blob_capsule, blob_ellipsoid, blob_group, cuboid_tapered, cylinder_tapered,
    face_uv_offset, footing, glow, id_quat, lit_interior, nest, prim, quat_mul, quat_x, quat_y,
    quat_z, solid, strut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Fp4, Generator};
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BONE_PALE, CANVAS_BONE, CANVAS_SHADE, DECK_HOLY, HULL_OAK, HULL_TAR, IRON_BLACK, PORT_POOR,
    ROPE_HEMP, STRAND_SHINGLE, WITCHFIRE, board, bone, fx, hemp, hull_frame, iron, lantern,
    sailcloth, strake, strand, tar,
};

/// The strand she lies on — the sub-root every footprint guard measures
/// against (#972 lesson 19). Shingle, not paving: this is the tide line.
///
/// Deep in `Z` because she is, which is the careening slip's convention for a
/// big hull. The first build had the pad's long axis crossing her and the
/// footprint guard caught her bow three metres out over open ground.
const PAD: [f32; 3] = [11.0, 0.3, 20.0];
const GROUND: f32 = PAD[1];

/// Her lines: `(z, half-beam, height of the sheer above the keel)`, bow first.
///
/// Three quantities rather than the longboat's two, because a wreck shows its
/// frames and a frame's *height* is as visible as its spread. She lies along
/// `Z` — bow toward the water at `-Z`, which is the hero side, so the approach
/// looks down her length past the broken frames into the lit shelter aft.
const STATIONS: [(f32, f32, f32); 5] = [
    (-7.6, 0.9, 2.5), // stem
    (-4.2, 2.3, 2.9), // fore quarter
    (-0.4, 2.7, 3.0), // amidships — where she is broken
    (3.4, 2.5, 3.2),  // after quarter
    (7.0, 1.7, 3.4),  // transom (a ship's sheer rises aft)
];

/// Where she is broken across, and how the two halves have settled.
///
/// The forward half has gone over further and dug her bow in; the after half
/// sits nearly level because that is the end still resting on the beach. Two
/// different angles, because two *equal* angles read as one bent object rather
/// than as something that failed.
const BREAK_Z: f32 = -0.4;
const FORE_HEEL: f32 = 0.4;
const FORE_TRIM: f32 = 0.16;
const AFT_HEEL: f32 = -0.12;
const AFT_TRIM: f32 = -0.05;

/// Depth of the blended bottom shell, as a fraction of the local sheer height.
///
/// Shallow on purpose, and this is the whole difference between a wreck and a
/// black slug. The first build carried the mass up to half her height and let
/// the frames rise out of what was left: the mass swallowed them, and fifteen
/// metres of rounded blob with a pallet on top read as neither ship nor
/// building. A wreck is recognisable because it is mostly *open frames* — so
/// the blob is only the part that is rotting into the shingle, and the hull
/// above it is structure you can see through.
const MASS_FRAC: f32 = 0.3;

/// Where the after half's remaining planking sits, as a fraction of the sheer
/// height, and how thick it is.
const PLANK_FRAC: f32 = 0.62;
const PLANK_T: f32 = 0.1;

/// Blend radius between hull elements, and the sample resolution.
///
/// She is fifteen metres long, so at the sanitiser's practical ceiling the
/// cells are still a third of a metre and nothing thinner than 700 mm survives
/// — see the shared `blob_cell_size` note in `items::util`, and #1026 for what
/// happens when that is ignored.
const BLEND: f32 = 0.3;
const HULL_RES: u32 = 44;

/// How far the two halves overlap at the break, so the wound has torn timber
/// in it rather than being a clean saw cut across a boat.
const BREAK_LAP: f32 = 0.5;

/// How many frames stand along her, and the timber's section.
///
/// They run her whole length, not just the open half: a ship's frames do, and
/// the ones aft are what the salvaged roof is resting on. Forward they are bare,
/// which is why they read.
const FRAMES: usize = 11;
const FRAME_R: f32 = 0.13;

/// Hero side — the render tool and the settlement placer both look down `-Z`.
const FRONT: f32 = -1.0;

const _: () = assert!(
    FORE_HEEL != AFT_HEEL,
    "both halves have settled at the same angle — she reads as bent, not broken"
);
const _: () = assert!(
    STATIONS[4].0 - STATIONS[0].0 < PAD[2] * 0.85,
    "she is longer than the strand she is lying on"
);

pub struct RottingHulk;

impl CatalogueEntry for RottingHulk {
    fn slug(&self) -> &'static str {
        "rotting_hulk"
    }
    fn name(&self) -> &'static str {
        "Rotting Hulk"
    }
    fn description(&self) -> &'static str {
        "A broken-backed hull driven ashore, her frames bare forward and her after half roofed \
         over with salvage and lived in."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Pirate]
    }
    fn prosperity_band(&self) -> ProsperityBand {
        PORT_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 13.0,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, local_did: &str) -> Generator {
        build_tree(local_did)
    }
}

/// Half-beam and sheer height at station `z`, interpolated along [`STATIONS`].
///
/// The one place that answers "how big is she here". A wreck's frames, her
/// roof, her deck and the shores propping her all have to ask it, and the
/// careening slip's shores are what happens when one of them asks something
/// else instead (#1030).
fn lines_at(z: f32) -> (f32, f32) {
    if z <= STATIONS[0].0 {
        return (STATIONS[0].1, STATIONS[0].2);
    }
    let last = STATIONS[STATIONS.len() - 1];
    if z >= last.0 {
        return (last.1, last.2);
    }
    for w in STATIONS.windows(2) {
        let ((z0, b0, h0), (z1, b1, h1)) = (w[0], w[1]);
        if z >= z0 && z <= z1 {
            let t = (z - z0) / (z1 - z0);
            return (b0 + (b1 - b0) * t, h0 + (h1 - h0) * t);
        }
    }
    (STATIONS[2].1, STATIONS[2].2)
}

/// The rotation one half carries: a heel about `Z` after a trim about `X`.
///
/// `quat_mul(a, b)` composes as `a · b`, so this applies the trim first and the
/// heel second — and [`tilted`] applies its two plane rotations in exactly that
/// order. Getting the order wrong is a smaller version of getting the sign
/// wrong, and the sign is what cost the careening slip a whole revision
/// (#1030): a hull's section is symmetric athwartships, so a mirrored or
/// misordered heel is invisible on the hull itself and catastrophic on
/// everything placed by its frame.
fn half_rotation(heel: f32, trim: f32) -> Fp4 {
    quat_mul(quat_z(heel), quat_x(trim))
}

/// Turn a point in one half's own frame — `y` up from the keel, `z` along her
/// from that half's pivot — into the world.
///
/// The same trick as the careening slip's `heeled`: anything fixed to a tilted
/// hull is naturally described where it sits on the *upright* vessel and then
/// has to be placed where the tilt actually puts it. Doing it here, once per
/// half, is what keeps this file translation-only and what stops a stovepipe or
/// a frame drifting out of agreement with the timber it is fixed to.
///
/// Trim about `X` first, then heel about `Z`, matching [`half_rotation`]'s
/// composition exactly — both are the standard plane rotation by PLUS the
/// angle.
fn tilted(pivot_z: f32, heel: f32, trim: f32, x: f32, y: f32, z: f32) -> [f32; 3] {
    let (ts, tc) = trim.sin_cos();
    let (y1, z1) = (y * tc - z * ts, y * ts + z * tc);
    let (hs, hc) = heel.sin_cos();
    let (x2, y2) = (x * hc - y1 * hs, x * hs + y1 * hc);
    [x2, GROUND + keel_lift() + y2, pivot_z + z1]
}

/// How far the keel is lifted off the shingle where she is bedded in.
///
/// She has settled INTO the beach, not onto it: a wreck sitting on top of the
/// strand with daylight under her keel is a model of a ship, so the lift is
/// negative — the mass sinks and the shingle closes over it. Small, because
/// too much buries the frames that are the whole point.
fn keel_lift() -> f32 {
    -0.35
}

fn fore(x: f32, y: f32, z: f32) -> [f32; 3] {
    tilted(BREAK_Z, FORE_HEEL, FORE_TRIM, x, y, z)
}

fn aft(x: f32, y: f32, z: f32) -> [f32; 3] {
    tilted(BREAK_Z, AFT_HEEL, AFT_TRIM, x, y, z)
}

/// One half of the hull, as a single blended mass.
///
/// `z0..z1` is the run of stations this half covers *in her own length*, so the
/// two halves are cut from one set of lines and lap at the break rather than
/// each being drawn to look about right. Every element overlaps its neighbours
/// structurally: the slip's stern floated clear of her own hull because a
/// 0.55 m gap was left for the blend to bridge and the blend did not bridge it.
fn hull_half(
    z0: f32,
    z1: f32,
    pivot: fn(f32, f32, f32) -> [f32; 3],
    heel: f32,
    trim: f32,
) -> Generator {
    let mut elements = Vec::new();
    // Five sections along this half, each a slab of the local beam and mass
    // depth, generously overlapping.
    let steps = 4;
    let span = z1 - z0;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let z = z0 + span * t;
        let (beam, height) = lines_at(z);
        let mass = height * MASS_FRAC;
        // Overlap: each slab is a third of the span long, at a quarter pitch.
        let half_len = (span.abs() / steps as f32) * 0.72;
        let local_z = z - BREAK_Z;
        if i == 0 || i == steps {
            // The ends draw in — a stem forward, a transom aft.
            elements.push(blob_ellipsoid(
                [0.0, mass * 0.5, local_z],
                [beam, mass * 0.5, half_len],
                BLEND,
            ));
        } else {
            elements.push(blob_box(
                [0.0, mass * 0.5, local_z],
                [beam, mass * 0.5, half_len],
                BLEND,
            ));
        }
    }
    // The keel, running her length under the garboards — the member that tells
    // the eye these two masses were one ship.
    let (mid_z0, mid_z1) = (z0 - BREAK_Z, z1 - BREAK_Z);
    elements.push(blob_capsule(
        [0.0, 0.18, (mid_z0 + mid_z1) * 0.5],
        0.3,
        (mid_z1 - mid_z0).abs() * 0.46,
        quat_x(FRAC_PI_2),
        0.16,
    ));
    prim(
        blob_group(elements, HULL_RES, strake(HULL_TAR)),
        pivot(0.0, 0.0, 0.0),
        half_rotation(heel, trim),
    )
}

/// Where frame `i` of [`FRAMES`] stands, in her own length.
fn frame_z(i: usize) -> f32 {
    let (z0, z1) = (STATIONS[0].0 + 1.0, STATIONS[4].0 - 0.8);
    z0 + (z1 - z0) * (i as f32 / (FRAMES - 1) as f32)
}

/// One of her frames, drawn from her own lines at station `z`.
///
/// The geometry — and the three renders it took to arrive at it — now lives in
/// the shared [`hull_frame`], because [`super::tideline_bones`] is built out of
/// the same shape and the kit cannot afford two spellings of the most
/// recognisable thing about a wreck (#972 lesson 5).
///
/// What stays here is the part that is hers: which half she belongs to, and the
/// fact that the ellipse centre is placed by *her* frame function at one
/// `height` above the keel, so the raise happens before the tilt.
fn frame(z: f32) -> Generator {
    let (beam, height) = lines_at(z);
    let is_fore = z < BREAK_Z;
    let (heel, trim) = if is_fore {
        (FORE_HEEL, FORE_TRIM)
    } else {
        (AFT_HEEL, AFT_TRIM)
    };
    let at = if is_fore {
        fore(0.0, height, z - BREAK_Z)
    } else {
        aft(0.0, height, z - BREAK_Z)
    };
    hull_frame(
        at,
        beam,
        height,
        half_rotation(heel, trim),
        FRAME_R,
        board(HULL_OAK),
    )
}

/// A run of planking between two stations on one side, at `y_frac` of the local
/// sheer height.
///
/// Computed entirely in **her own frame** and then carried into the world by
/// [`fore`] / [`aft`], so the board leans with the half it is fastened to. The
/// first build took the two endpoints into the world first and then gave the
/// board only a plan rotation, which left every plank standing bolt upright on
/// a hull heeled twenty-three degrees — the same class of error as the
/// careening slip's mirrored heel (#1030), just quieter.
///
/// The direction-to-rotation conversion is the file's only one, and it is here
/// rather than at the call sites for the reason six of them were wrong in this
/// kit before `strut` existed (#1028). A board's long axis is its local `+Z`,
/// and `quat_y(θ)` carries `+Z` to `(sin θ, 0, cos θ)`.
fn plank_run(z0: f32, z1: f32, sx: f32, y_frac: f32, aft_half: bool) -> Generator {
    let local = |z: f32| {
        let (beam, height) = lines_at(z);
        [sx * beam * 0.97, height * y_frac, z - BREAK_Z]
    };
    let (p0, p1) = (local(z0), local(z1));
    let d = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-4);
    let mid = [
        (p0[0] + p1[0]) * 0.5,
        (p0[1] + p1[1]) * 0.5,
        (p0[2] + p1[2]) * 0.5,
    ];
    let (heel, trim) = if aft_half {
        (AFT_HEEL, AFT_TRIM)
    } else {
        (FORE_HEEL, FORE_TRIM)
    };
    let at = if aft_half {
        aft(mid[0], mid[1], mid[2])
    } else {
        fore(mid[0], mid[1], mid[2])
    };
    let (_, height) = lines_at((z0 + z1) * 0.5);
    prim(
        solid(cuboid_tapered(
            [PLANK_T, height * 0.34, len],
            0.0,
            strake(HULL_TAR),
        )),
        at,
        quat_mul(half_rotation(heel, trim), quat_y(d[0].atan2(d[2]))),
    )
}

/// Her frames, and the planking that is still on aft.
///
/// Forward the frames are bare, which is the read: you see through her. Aft they
/// carry two runs of strake a side, so the shelter has walls and the salvaged
/// roof has something to sit on. Two strakes have also sprung away forward — the
/// detail that says the planking came *off* rather than never existing.
fn hull_structure() -> Vec<Generator> {
    let mut out = Vec::new();
    for i in 0..FRAMES {
        out.push(frame(frame_z(i)));
    }
    // Aft planking: between consecutive frames abaft the break, both sides, at
    // two heights. Each run is placed on the stations its own ends stand at, so
    // planking cannot float clear of the frames it is fastened to.
    for i in 0..FRAMES - 1 {
        let (za, zb) = (frame_z(i), frame_z(i + 1));
        if za < BREAK_Z {
            continue;
        }
        for sx in [-1.0_f32, 1.0] {
            for y_frac in [PLANK_FRAC - 0.22, PLANK_FRAC] {
                out.push(plank_run(za, zb, sx, y_frac, true));
            }
        }
    }
    // Two strakes sprung off the bare frames forward, hanging by one end.
    for (a, b, sx) in [(0_usize, 2_usize, 1.0_f32), (3, 5, -1.0)] {
        let (za, zb) = (frame_z(a), frame_z(b));
        let (ba, ha) = lines_at(za);
        let (bb, hb) = lines_at(zb);
        out.push(strut(
            fore(sx * ba * 0.94, ha * 0.66, za - BREAK_Z),
            fore(sx * bb * 1.24, hb * 0.34, zb - BREAK_Z),
            0.09,
            5,
            strake(HULL_TAR),
        ));
    }
    out
}

/// The after half's shelter: salvaged decking, a canvas patch, a stovepipe, a
/// hanging canvas door, and the lit hold behind it.
fn shelter() -> Vec<Generator> {
    let (z0, z1) = (BREAK_Z + 1.1, STATIONS[4].0 - 0.6);
    let mut out = Vec::new();

    // Roof: four courses of salvaged board laid across her, each sized to the
    // beam at its own station so the roof follows her lines in rather than
    // overhanging her quarters.
    let courses = 4;
    for i in 0..courses {
        let t = (i as f32 + 0.5) / courses as f32;
        let z = z0 + (z1 - z0) * t;
        let (beam, height) = lines_at(z);
        let len = (z1 - z0) / courses as f32 + 0.12;
        out.push(prim(
            solid(cuboid_tapered(
                [beam * 1.78, 0.09, len],
                0.0,
                board(DECK_HOLY),
            )),
            aft(0.0, height * 0.94, z - BREAK_Z),
            half_rotation(AFT_HEEL, AFT_TRIM),
        ));
    }
    // One course is gone and a canvas has been lashed over the gap instead.
    let patch_z = z0 + (z1 - z0) * 0.38;
    let (patch_beam, patch_h) = lines_at(patch_z);
    out.push(prim(
        solid(cuboid_tapered(
            [patch_beam * 1.5, 0.05, (z1 - z0) * 0.3],
            0.0,
            sailcloth(CANVAS_BONE, CANVAS_SHADE),
        )),
        aft(0.0, patch_h * 1.0, patch_z - BREAK_Z),
        half_rotation(AFT_HEEL, AFT_TRIM),
    ));
    // Lashings holding the canvas down, over the side to the sheer.
    for sx in [-1.0_f32, 1.0] {
        out.push(strut(
            aft(sx * patch_beam * 0.6, patch_h * 1.02, patch_z - BREAK_Z),
            aft(sx * patch_beam * 1.02, patch_h * 0.5, patch_z - BREAK_Z),
            0.035,
            5,
            hemp(ROPE_HEMP),
        ));
    }

    // Stovepipe out through the roof, aft — the one unambiguous sign that
    // somebody lives here. Smoke comes off its head, not off the hull.
    let pipe_z = z0 + (z1 - z0) * 0.78;
    let (_, pipe_h) = lines_at(pipe_z);
    let pipe_foot = aft(0.6, pipe_h * 0.9, pipe_z - BREAK_Z);
    let pipe_head = aft(0.6, pipe_h * 0.9 + 1.35, pipe_z - BREAK_Z);
    out.push(strut(pipe_foot, pipe_head, 0.11, 8, iron(IRON_BLACK, 0xD1)));
    out.push(prim(
        solid(cylinder_tapered(0.17, 0.12, 8, 0.0, iron(IRON_BLACK, 0xD2))),
        pipe_head,
        half_rotation(AFT_HEEL, AFT_TRIM),
    ));

    // The way in: a gap under the roof's forward edge, with a canvas hung
    // across half of it and a lit floor running back into the hold. The lit
    // surface is what makes a dark hole read as a shelter rather than as damage
    // — the same reason the magazine's store is lit through its one door.
    let door_z = z0 + 0.2;
    let (door_beam, door_h) = lines_at(door_z);
    out.push(prim(
        solid(cuboid_tapered(
            [door_beam * 1.4, 0.08, (z1 - z0) * 0.55],
            0.0,
            lit_interior([0.34, 0.27, 0.2], 0.24),
        )),
        aft(
            0.0,
            door_h * MASS_FRAC + 0.06,
            door_z - BREAK_Z + (z1 - z0) * 0.26,
        ),
        half_rotation(AFT_HEEL, AFT_TRIM),
    ));
    out.push(prim(
        solid(cuboid_tapered(
            [door_beam * 0.62, door_h * 0.4, 0.05],
            0.0,
            sailcloth(CANVAS_SHADE, CANVAS_BONE),
        )),
        aft(
            -door_beam * 0.42,
            door_h * MASS_FRAC + door_h * 0.2,
            door_z - BREAK_Z,
        ),
        half_rotation(AFT_HEEL, AFT_TRIM),
    ));
    out
}

/// The shores propping her up, and the ladder somebody climbs aboard by.
///
/// Each shore's head is placed on the hull's own sheer at its own station, so a
/// shore cannot bear on air where she narrows — which is exactly what the
/// careening slip's did before its per-station half-beam went in (#1028).
///
/// **Which side** each shore stands on is derived from its half's heel rather
/// than chosen. [`tilted`] raises the `+X` side for a positive heel, so the
/// raised side is `signum(heel)` — and that is the side a shore belongs on,
/// because propping the side a hull has already rolled onto props nothing. The
/// first build put the forward shore on the low side and the guard found its
/// head at 1.3 m, down where the sheer had come to meet the beach.
fn shores() -> Vec<Generator> {
    let mut out = Vec::new();
    for z in [2.2_f32, 4.6, -3.0] {
        let (beam, height) = lines_at(z);
        let is_fore = z < BREAK_Z;
        let heel = if is_fore { FORE_HEEL } else { AFT_HEEL };
        let sx = if heel >= 0.0 { 1.0 } else { -1.0 };
        let head = if is_fore {
            fore(sx * beam * 0.98, height * 0.72, z - BREAK_Z)
        } else {
            aft(sx * beam * 0.98, height * 0.78, z - BREAK_Z)
        };
        // The foot stands out from the hull by its own height's worth of batter,
        // on the shingle.
        let foot = [
            head[0] + sx * (head[1] - GROUND) * 0.42,
            GROUND + 0.05,
            head[2] + 0.3,
        ];
        out.push(strut(head, foot, 0.14, 6, board(HULL_OAK)));
    }
    // A ladder up to the shelter's door, from the shingle to the sheer. On the
    // after half's LOW side, which is the side anybody would actually climb.
    let (door_beam, door_h) = lines_at(BREAK_Z + 1.3);
    let ladder_sx = if AFT_HEEL >= 0.0 { -1.0 } else { 1.0 };
    let top = aft(ladder_sx * door_beam * 0.9, door_h * MASS_FRAC + 0.6, 1.3);
    let base = [top[0] + ladder_sx * 1.1, GROUND + 0.05, top[2] + 0.35];
    for dz in [-0.2_f32, 0.2] {
        out.push(strut(
            [top[0], top[1], top[2] + dz],
            [base[0], base[1], base[2] + dz],
            0.055,
            5,
            board(DECK_HOLY),
        ));
    }
    let rungs = 5;
    for i in 1..rungs {
        let t = i as f32 / rungs as f32;
        let p = [
            top[0] + (base[0] - top[0]) * t,
            top[1] + (base[1] - top[1]) * t,
            top[2] + (base[2] - top[2]) * t,
        ];
        out.push(prim(
            solid(cylinder_tapered(0.035, 0.44, 5, 0.0, board(DECK_HOLY))),
            p,
            quat_x(FRAC_PI_2),
        ));
    }
    out
}

fn build_tree(_local_did: &str) -> Generator {
    let pad_c = [0.0, GROUND * 0.5, 0.0];
    let mut shingle = strand(STRAND_SHINGLE);
    shingle.uv_offset = face_uv_offset(FaceKey::Top, pad_c);

    let mut carried = vec![footing(PAD[0] * 0.6, PAD[2] * 0.5, [0.0, 0.0], 13.0)];

    // The two halves, cut from one set of lines and lapping at the break.
    carried.push(hull_half(
        STATIONS[0].0,
        BREAK_Z + BREAK_LAP,
        fore,
        FORE_HEEL,
        FORE_TRIM,
    ));
    carried.push(hull_half(
        BREAK_Z - BREAK_LAP,
        STATIONS[4].0,
        aft,
        AFT_HEEL,
        AFT_TRIM,
    ));
    carried.extend(hull_structure());
    carried.extend(shelter());
    carried.extend(shores());

    // Witchfire in the open hold, where the break is — the register's whole
    // point, and it is placed at the WOUND rather than anywhere convenient,
    // because what is burning is what came apart.
    let (break_beam, break_h) = lines_at(BREAK_Z);
    let fire_at = fore(0.0, break_h * MASS_FRAC + 0.3, 0.1);
    carried.push(prim(
        solid(cylinder_tapered(
            break_beam * 0.5,
            0.12,
            10,
            0.2,
            glow(WITCHFIRE, 0.5),
        )),
        fire_at,
        half_rotation(FORE_HEEL, FORE_TRIM),
    ));
    // ...and one lantern still hanging aft, burning tallow. Two lights of
    // opposite temperature in one prop: the shelter is warm and lived in, the
    // wreck is cold and is not.
    let (lamp_beam, lamp_h) = lines_at(5.4);
    carried.push(lantern(
        aft(lamp_beam * 0.8, lamp_h * 1.05, 5.8),
        0.62,
        0xD4,
    ));

    // Wrack along the tide line, a bone or two in it, and the stump of a mast
    // lying where it fell. The tide line is derived from the PAD's own edge, so
    // a retuned strand keeps its wrack on the shingle (#972 lesson 8).
    let tide_x = FRONT * (PAD[0] * 0.5 - 1.1);
    for (i, dz) in [-6.2_f32, -2.4, 1.6, 5.4].into_iter().enumerate() {
        carried.push(prim(
            solid(cuboid_tapered(
                [0.34, 0.07, 1.9],
                0.4,
                tar([0.24, 0.25, 0.19]),
            )),
            [tide_x + (i as f32 * 0.28 - 0.4), GROUND + 0.035, dz],
            quat_y(0.14 * i as f32),
        ));
    }
    // Her fallen mast, lying along the tide line clear of the hull. Along her,
    // not across her — the wrack line of a beach runs parallel to the water,
    // and so does anything the sea left on it.
    carried.push(strut(
        [tide_x + 0.5, GROUND + 0.28, -7.1],
        [tide_x - 0.2, GROUND + 0.22, -0.9],
        0.26,
        8,
        board(HULL_OAK),
    ));
    // A long bone and a skull in the wrack — stated once and small, because the
    // register's horror is the hulk, and a beach strewn with skulls reads as a
    // joke rather than as a warning.
    carried.push(prim(
        solid(cylinder_tapered(0.07, 0.72, 6, 0.12, bone(BONE_PALE))),
        [tide_x + 0.7, GROUND + 0.07, 3.2],
        quat_x(FRAC_PI_2),
    ));
    carried.push(prim(
        solid(cylinder_tapered(0.19, 0.24, 8, 0.22, bone(BONE_PALE))),
        [tide_x + 0.35, GROUND + 0.16, 3.9],
        quat_x(PI * 0.55),
    ));

    let mut root = nest(
        prim(solid(cuboid_tapered(PAD, 0.0, shingle)), pad_c, id_quat()),
        carried,
    );
    // The hulk's voice is the cursed hiss, not the harbour's swell — and the
    // smoke comes off the stovepipe's head, which is a position the shelter
    // already knows.
    root.audio = fx::witchfire_hiss();
    attach(&mut root, fx::witchfire(fire_at, 0xD5));
    let pipe_z = BREAK_Z + 1.1 + (STATIONS[4].0 - 0.6 - BREAK_Z - 1.1) * 0.78;
    let (_, pipe_h) = lines_at(pipe_z);
    attach(
        &mut root,
        fx::hearth_smoke(aft(0.6, pipe_h * 0.9 + 1.5, pipe_z - BREAK_Z), 0xD6),
    );
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::measure;
    use crate::catalogue::items::util::{
        assert_no_glazing_on_solids, assert_no_tilted_parents, assert_sanitize_stable,
        blob_cell_size, blob_components, rotate_by, window_cards,
    };
    use crate::pds::GeneratorKind as K;

    fn built() -> Generator {
        RottingHulk.build("")
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&built(), "rotting_hulk");
    }

    /// #972 lesson 22, on the entry in this kit most likely to break it.
    ///
    /// A broken ship *wants* to be two rotated parents with a roof and a rig
    /// hanging off each, and that is precisely the shape that spins its
    /// children's offsets out of the geometry while every guard here walks
    /// translations only. Both halves are leaf `BlobGroup`s and everything
    /// fixed to a half is placed in the world by [`fore`] or [`aft`].
    #[test]
    fn no_rotated_node_carries_an_offset_child() {
        assert_no_tilted_parents(&built(), "rotting_hulk");
    }

    /// A wreck has no windows. The kit has a `Window` card and the reflex to
    /// reach for it is what the ledger keeps catching, so the prohibition is
    /// stated rather than assumed (#972 lesson 24).
    #[test]
    fn the_hulk_carries_no_glazing() {
        let g = built();
        assert_no_glazing_on_solids(&g, "rotting_hulk");
        assert!(window_cards(&g).is_empty(), "a wreck has grown a window");
    }

    /// Both halves polygonise whole, and are thick enough for the grid.
    #[test]
    fn each_half_is_one_continuous_mass() {
        fn blobs(g: &Generator, out: &mut Vec<K>) {
            if matches!(g.kind, K::BlobGroup { .. }) {
                out.push(g.kind.clone());
            }
            for c in &g.children {
                blobs(c, out);
            }
        }
        let mut found = Vec::new();
        blobs(&built(), &mut found);
        assert_eq!(found.len(), 2, "she is broken into exactly two halves");
        for (i, half) in found.iter().enumerate() {
            assert_eq!(
                blob_components(half),
                1,
                "half {i} polygonised into more than one piece — her sections \
                 have drifted out of blend range, or she is finer than the \
                 sample grid can resolve"
            );
        }
        // The thinnest thing authored: the mass depth at the stem, which is the
        // shallowest station.
        let thinnest = STATIONS[0].2 * MASS_FRAC;
        let cell = blob_cell_size(STATIONS[4].0 - STATIONS[0].0, HULL_RES);
        assert!(
            thinnest > cell * 2.0,
            "the shallowest section is {thinnest} m across a {cell} m sample \
             cell — under two cells it comes out full of holes"
        );
    }

    /// She is broken, not bent — the two halves lie at genuinely different
    /// angles, and the difference shows up as a step in the sheer at the break.
    ///
    /// Read by asking each half's own frame where the same point on the ship
    /// ends up: if the two answers agree, she is one object with a seam in it,
    /// which is the failure this entry exists to avoid.
    #[test]
    fn the_two_halves_lie_at_different_angles() {
        let (beam, height) = lines_at(BREAK_Z);
        let f = fore(beam, height, 0.0);
        let a = aft(beam, height, 0.0);
        let step = ((f[0] - a[0]).powi(2) + (f[1] - a[1]).powi(2) + (f[2] - a[2]).powi(2)).sqrt();
        assert!(
            step > 0.7,
            "the sheer at the break steps by only {step} m between the two \
             halves — she reads as a bent ship, not a broken one"
        );
        // ...and the forward half has gone over further, which is the story:
        // that is the end that struck.
        assert!(
            FORE_HEEL.abs() > AFT_HEEL.abs(),
            "the after half has heeled further than the bow that struck"
        );
    }

    /// Every frame rises out of the hull mass, and only the after half is
    /// planked.
    ///
    /// Frames are arcs cut from tori and stood up by a negative quarter turn
    /// about `X`, because `path_cut` keeps the primitive's local `+Z` half — a
    /// positive turn sends the kept arc to world `−Y` and hangs the frame below
    /// its own keel. That is the signal mast's vault fault (#1021), and this is
    /// the guard that would have caught it there.
    ///
    /// The planking count is the other half of the read: forward she is open,
    /// and if planking ever appears there she stops being a wreck.
    #[test]
    fn the_frames_rise_out_of_the_hull_and_only_the_stern_is_planked() {
        // Selected on the frame timber's own section, not on being a torus: the
        // shared `lantern` helper hangs one too, and the first version of this
        // guard counted seven frames where there were six. #972 lesson 24,
        // ninth instance in this kit.
        let mut frames = Vec::new();
        fn frames_of(g: &Generator, at: [f32; 3], out: &mut Vec<[f32; 3]>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let K::Torus { minor_radius, .. } = &g.kind
                && (minor_radius.0 - FRAME_R).abs() < 0.005
            {
                out.push(here);
            }
            for c in &g.children {
                frames_of(c, here, out);
            }
        }
        frames_of(&built(), [0.0; 3], &mut frames);
        assert_eq!(
            frames.len(),
            FRAMES,
            "expected {FRAMES} frames, found {}",
            frames.len()
        );

        // They rise clear of the mass rotting into the shingle — which is the
        // whole reason the mass is only `MASS_FRAC` deep. Measured off the
        // built meshes, mass and frames both.
        let mass = blob_bounds(&built());
        let structure = structure_bounds(&built());
        assert!(
            structure.max.y > mass.max.y + 1.0,
            "her frames top out at {} against a mass topping out at {} — they \
             are swallowed by the very thing they are supposed to stand out of",
            structure.max.y,
            mass.max.y
        );

        // Planking exists abaft the break and nowhere forward of it. Counted off
        // the AUTHORED section rather than off a bounding box: a board leaning
        // with a heeled hull and running in to a taper has an AABB whose
        // smallest side is three times its own thickness, and the first version
        // of this guard found three planks where there were twenty.
        let mut planks = Vec::new();
        fn planks_of(g: &Generator, at: [f32; 3], out: &mut Vec<[f32; 3]>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let K::Cuboid { size, .. } = &g.kind
                && (size.0[0] - PLANK_T).abs() < 1e-4
            {
                out.push(here);
            }
            for c in &g.children {
                planks_of(c, here, out);
            }
        }
        planks_of(&built(), [0.0; 3], &mut planks);
        assert!(
            planks.len() >= 8,
            "only {} runs of planking left on her — the shelter has no walls",
            planks.len()
        );
        for p in &planks {
            assert!(
                p[2] > BREAK_Z - 0.6,
                "a run of planking at z = {} is forward of the break, where she \
                 is supposed to be open to the sky",
                p[2]
            );
        }

        // The frames describe one hull's rising sheer, not a row of equal
        // hoops: the tallest and shortest differ by a real amount, and the
        // difference comes out of the lines rather than out of a jitter.
        let hi = (0..FRAMES)
            .map(|i| lines_at(frame_z(i)).1)
            .fold(f32::MIN, f32::max);
        let lo = (0..FRAMES)
            .map(|i| lines_at(frame_z(i)).1)
            .fold(f32::MAX, f32::min);
        assert!(
            hi - lo > 0.6,
            "the frames vary by only {} m in height — they do not describe one \
             hull's lines",
            hi - lo
        );
    }

    /// World bounds of the hull's whole structure — the blended masses AND the
    /// frames standing out of them.
    ///
    /// The distinction matters: since #1023's second pass the mass is only the
    /// part rotting into the shingle, so "the hull" as a shore or a roof sees it
    /// is the frames, not the blob.
    fn structure_bounds(root: &Generator) -> measure::Bounds {
        let mut out: Option<measure::Bounds> = None;
        for p in measure::solids(root) {
            if p.kind_tag != "Torus" && p.kind_tag != "BlobGroup" {
                continue;
            }
            out = Some(match out.take() {
                None => p.bounds,
                Some(prev) => measure::Bounds {
                    min: prev.min.min(p.bounds.min),
                    max: prev.max.max(p.bounds.max),
                },
            });
        }
        out.expect("the hulk carries a hull")
    }

    /// Bounds of the forward half's mesh — the lowest-lying `BlobGroup`.
    fn blob_bounds(root: &Generator) -> measure::Bounds {
        let mut out: Option<measure::Bounds> = None;
        fn walk(g: &Generator, at: [f32; 3], out: &mut Option<measure::Bounds>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if matches!(g.kind, K::BlobGroup { .. }) {
                let mut xf = bevy::prelude::Transform::from_xyz(here[0], here[1], here[2]);
                xf.rotation = bevy::prelude::Quat::from_array(g.transform.rotation.0);
                if let Some(b) = measure::mesh_bounds(&g.kind, &xf) {
                    *out = Some(match out.take() {
                        None => b,
                        Some(prev) => measure::Bounds {
                            min: prev.min.min(b.min),
                            max: prev.max.max(b.max),
                        },
                    });
                }
            }
            for c in &g.children {
                walk(c, here, out);
            }
        }
        walk(root, [0.0; 3], &mut out);
        out.expect("the hulk carries a meshed hull")
    }

    /// She is bedded into the shingle, not standing on it.
    #[test]
    fn she_is_bedded_into_the_strand() {
        let hull = blob_bounds(&built());
        assert!(
            hull.min.y < GROUND,
            "the hull's underside is at {} and the strand is at {GROUND} — \
             there is daylight under her keel, which reads as a model of a ship",
            hull.min.y
        );
        assert!(
            hull.min.y > GROUND - 1.2,
            "the hull's underside is at {} — she has sunk far enough to bury \
             the frames that are the whole point",
            hull.min.y
        );
    }

    /// Every shore bears on the hull at one end and on the shingle at the
    /// other.
    ///
    /// Read from the built struts through [`rotate_by`]: a shore of the right
    /// length pointing *near* the hull looks correct from three of four angles,
    /// and that is the fault class this kit retired (#1028, #1030).
    #[test]
    fn every_shore_bears_on_the_hull_and_the_beach() {
        let mut found = Vec::new();
        fn shores_of(g: &Generator, at: [f32; 3], out: &mut Vec<([f32; 3], [f32; 3])>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let K::Cylinder { radius, height, .. } = &g.kind
                && (radius.0 - 0.14).abs() < 0.005
            {
                let tip = rotate_by(g.transform.rotation.0, [0.0, height.0 * 0.5, 0.0]);
                out.push((
                    [here[0] + tip[0], here[1] + tip[1], here[2] + tip[2]],
                    [here[0] - tip[0], here[1] - tip[1], here[2] - tip[2]],
                ));
            }
            for c in &g.children {
                shores_of(c, here, out);
            }
        }
        shores_of(&built(), [0.0; 3], &mut found);
        assert_eq!(
            found.len(),
            3,
            "expected three shores, found {}",
            found.len()
        );
        let mass = blob_bounds(&built());
        let structure = structure_bounds(&built());
        for (a, b) in &found {
            let (hi, lo) = if a[1] > b[1] { (a, b) } else { (b, a) };
            assert!(
                lo[1] < GROUND + 0.2,
                "a shore's foot is at {} — it never reaches the shingle",
                lo[1]
            );
            // A shore props her SIDE, so its head belongs above the mass and
            // below the sheer. Checking it against the blob alone was the first
            // version of this guard, and it failed the moment the mass was
            // shallowed to let the frames show: it was measuring the part of the
            // hull a shore does not touch.
            assert!(
                hi[1] > mass.max.y - 0.5 && hi[1] < structure.max.y + 0.3,
                "a shore's head is at {} against a mass topping out at {} and a \
                 sheer at {} — it bears on nothing",
                hi[1],
                mass.max.y,
                structure.max.y
            );
            // The head is against her side, the foot is out from it: a shore
            // that stands vertically is a post and props nothing.
            assert!(
                (hi[0].abs() - lo[0].abs()) < -0.2,
                "a shore runs from {hi:?} to {lo:?} — its foot is not out from \
                 the hull, so it has no batter and cannot be propping her"
            );
        }
    }

    /// The shelter is lit and the wreck is not — two lights of opposite
    /// temperature, which is what makes the cursed register read as cursed.
    #[test]
    fn the_shelter_burns_warm_and_the_wound_burns_cold() {
        // Read on `emission_strength > 0`, not through `has_emissive`, which
        // only counts surfaces over strength 1.0 — everything in this kit is
        // deliberately below that, because a lit face at strength blooms white
        // and loses the very hue this test is about.
        fn lights(g: &Generator, at: [f32; 3], out: &mut Vec<([f32; 3], [f32; 3])>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let Some(m) = g.kind.material()
                && m.emission_strength.0 > 0.0
            {
                out.push((here, m.emission_color.0));
            }
            for c in &g.children {
                lights(c, here, out);
            }
        }
        let g = built();
        let mut found = Vec::new();
        lights(&g, [0.0; 3], &mut found);
        assert!(
            found.len() >= 3,
            "only {} lit surfaces — the shelter should be lit, the wound \
             should burn, and the lantern should be alight",
            found.len()
        );
        let cold = found
            .iter()
            .filter(|(_, c)| c[1] > c[0] * 1.5 && c[1] > c[2] * 1.4)
            .count();
        let warm = found.iter().filter(|(_, c)| c[0] >= c[1]).count();
        assert!(
            cold >= 1,
            "nothing here burns cold — the cursed register's whole read is one \
             green light among the amber"
        );
        assert!(
            warm >= 2,
            "only {warm} warm lights — the shelter has to be lived in, or the \
             green has nothing to be cold against"
        );
        // The cold light is at the break, and the warm ones are aft in the
        // shelter. Getting that the wrong way round would put a hearth in the
        // wreckage and a corpse-light in somebody's home.
        let cold_z = found
            .iter()
            .find(|(_, c)| c[1] > c[0] * 1.5 && c[1] > c[2] * 1.4)
            .map(|(p, _)| p[2])
            .expect("checked above");
        assert!(
            cold_z < BREAK_Z + 2.0,
            "the witchfire is at z = {cold_z}, aft of the break — it is \
             burning inside the shelter instead of in the wreckage"
        );
    }

    /// Everything stands on the strand it is nested under (#972 lessons 8, 19).
    #[test]
    fn every_part_lies_on_the_strand() {
        let half = [PAD[0] * 0.5, PAD[2] * 0.5];
        let mut checked = 0;
        for p in measure::solids(&built()) {
            checked += 1;
            assert!(
                p.bounds.min.x >= -half[0] - 1e-3 && p.bounds.max.x <= half[0] + 1e-3,
                "{} at {:?} overhangs the strand in X ({} .. {})",
                p.kind_tag,
                p.bounds.center(),
                p.bounds.min.x,
                p.bounds.max.x
            );
            assert!(
                p.bounds.min.z >= -half[1] - 1e-3 && p.bounds.max.z <= half[1] + 1e-3,
                "{} at {:?} overhangs the strand in Z ({} .. {})",
                p.kind_tag,
                p.bounds.center(),
                p.bounds.min.z,
                p.bounds.max.z
            );
        }
        assert!(checked > 30, "only {checked} parts examined");
    }
}
