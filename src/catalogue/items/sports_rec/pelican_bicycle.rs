//! Pelican on a bicycle — a Sports/Recreation prop, and #972's deliberate
//! out-of-distribution test: a real bicycle, and a real pelican riding it.
//!
//! **The bicycle** is a lugged steel frame whose every tube is a
//! [`strut`] between two NAMED joints — bottom bracket, seat cluster, head
//! top, fork crown, hub ends — so no tube carries a hand-rolled rotation
//! and the guard can ask, of every built tube, which two joints its ends
//! land on. Three lugs (bottom bracket, seat cluster, fork crown) are the
//! axis-aligned sub-roots the tubes meet at, which is what a lugged frame
//! looks like and what #972 lesson 22 needs: a turned member is a leaf.
//! The wheels are the handcart's recipe — a hub turned onto the axle line
//! carrying rim, tyre and spokes AT ITS OWN ORIGIN — with both tyres'
//! bottoms on the ground. Cranks are struts 180° apart on the crank axis,
//! with pedals, a chainring, a cog and a chain; the saddle sits on a seat
//! post sunk into the seat tube, which runs down into the bottom bracket:
//! an unbroken chain of solids (lesson 33), guarded as a chain.
//!
//! **The pelican** is one [`blob_group`] — body, breast, shoulder hump,
//! tail, an S-curved neck of three capsules, the head, the pouch, and both
//! wings reaching forward as two capsules apiece to the handlebar grips —
//! meshed as a single skin (guarded with `blob_components == 1`, and with
//! the cell-size arithmetic that keeps it that way). The bill is two
//! tapered superellipsoids and a hooked nail, all aimed with [`aim_y`] on
//! one bill vector, so the head faces where the bill points (lesson 39);
//! the legs are struts from the hips to the pedals and the feet are
//! flat tapered slabs — a webbed foot is a triangle in plan, so the slab is
//! quarter-turned to put its taper along the foot's length, wide at the toes
//! — standing on the pedals with three toe ridges on top.
//!
//! Forward is `+X`. The hero face is the broadside toward `-Z`, so the
//! render front and the settlement placer both see the bird in profile,
//! looking along the direction of travel.
//!
//! **Nesting** (lesson 3): bottom bracket lug → [seat lug → saddle →
//! pelican], [fork crown → steering], [rear hub → wheel], and the crank set.
//! Every sub-root is axis-aligned; the only turned parents are the two hubs
//! and their children sit at their origin.
//!
//! **Every guard is proven to bite.** The tree is built through
//! [`Faults`], a set of deliberate defects each the shape of what one guard
//! exists to catch (a floating saddle, a sunk tyre, a rim off its hub, a
//! crank off 180°, a foot above its pedal, a wing that stops short, a bill
//! aimed backward, a neck out of blend range, a tube that misses its joint,
//! a spindle end flush with the pedal face, a glazed saddle, a stray part
//! past the clearance). Every test runs its guard on the shipped build and
//! then on the broken variant, and checks WHICH assertion fired (lesson 34's
//! selector note): a guard that bites on a count is not a guard.
//!
//! #972 lesson 40 — see the issue: an organic rider on a mechanical mount
//! is two vocabularies meeting at four contacts (saddle, two grips, two
//! pedals), and every one of those contacts is a guardable relation between
//! a BUILT blob element or strut end and a BUILT machine part.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::catalogue::items::util::{
    aim_y, blob_capsule, blob_ellipsoid, blob_group, cuboid_tapered, cuboid_tapered_xz,
    cylinder_tapered, id_quat, nest, prim, quat_x, quat_y, quat_z, solid, sphere, strut,
    superellipsoid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::BlobElement;
use crate::pds::{Fp2, Generator, GeneratorKind, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{STEEL_GREY, enamel, painted, steel};

// ---- palette (sRGB). Distinct constants for distinct parts: a shared
// colour constant is a shared selector (#972 lesson 32).

/// Frame enamel — the ten frame tubes and the three lugs.
const FRAME_RED: [f32; 3] = [0.70, 0.14, 0.12];
/// Tyre rubber.
const TYRE_BLACK: [f32; 3] = [0.09, 0.09, 0.10];
/// Saddle vinyl.
const SADDLE_BLACK: [f32; 3] = [0.13, 0.12, 0.12];
/// Handlebar grips.
const GRIP_BLACK: [f32; 3] = [0.16, 0.16, 0.17];
/// Pedal bodies.
const PEDAL_DARK: [f32; 3] = [0.22, 0.22, 0.24];
/// Race number plate.
const PLATE_WHITE: [f32; 3] = [0.94, 0.94, 0.92];
/// The band across the plate.
const PLATE_RED: [f32; 3] = [0.80, 0.16, 0.14];
/// Plumage — white-grey, one skin, one colour.
const PLUMAGE: [f32; 3] = [0.91, 0.90, 0.86];
/// The bill.
const BILL_YELLOW: [f32; 3] = [0.96, 0.66, 0.20];
/// The nail hooking the bill's tip.
const NAIL_ORANGE: [f32; 3] = [0.86, 0.46, 0.14];
/// Legs and feet.
const LEG_ORANGE: [f32; 3] = [0.93, 0.55, 0.20];
/// Eyes.
const EYE_DARK: [f32; 3] = [0.08, 0.07, 0.07];

// ---- the bicycle. Forward is +X; the wheels turn about Z.

/// Tyre outer radius — the hubs sit at this height so the tyres touch the
/// ground.
const WHEEL_R: f32 = 0.30;
const TYRE_T: f32 = 0.032;
const RIM_T: f32 = 0.010;
/// Rim centreline: the rim's outer edge overlaps the tyre's inner edge.
const RIM_R: f32 = WHEEL_R - TYRE_T - 0.018;
const HUB_R: f32 = 0.030;
const HUB_L: f32 = 0.080;
/// Spokes are diameter bars in the wheel plane; nine of them at an odd
/// pitch, so no two are a quarter turn apart (crossed spokes of one stock
/// tie for depth — the tractor's lesson).
const SPOKE_PAIRS: u32 = 9;
const SPOKE_T: f32 = 0.010;
const SPOKE_LEN: f32 = 2.0 * (RIM_R - RIM_T);

const HUB_Y: f32 = WHEEL_R;
const REAR_HUB: [f32; 3] = [-0.50, HUB_Y, 0.0];
const FRONT_HUB: [f32; 3] = [0.50, HUB_Y, 0.0];
/// Where the stays and blades meet the hub ends.
const HUB_END_Z: f32 = 0.050;

/// Bottom bracket — the root, and the joint the seat tube, down tube and
/// chain stays all run to.
const BB: [f32; 3] = [-0.10, 0.27, 0.0];
/// Half-extents of the three lugs (rolled boxes).
const LUG_HALF: [f32; 3] = [0.045, 0.045, 0.048];
const SEAT_LUG_HALF: [f32; 3] = [0.034, 0.034, 0.034];
const CROWN_HALF: [f32; 3] = [0.034, 0.030, 0.050];
const LUG_EXP: f32 = 0.35;

/// Seat tube: leans back at `SEAT_ANGLE` from horizontal.
const SEAT_ANGLE: f32 = 72.0 * PI / 180.0;
const SEAT_TUBE_LEN: f32 = 0.27;
/// Head tube: leans forward at `HEAD_ANGLE`, its top at `HEAD_TOP`.
const HEAD_TOP: [f32; 3] = [0.36, 0.66, 0.0];
const HEAD_ANGLE: f32 = 70.0 * PI / 180.0;
const HEAD_TUBE_LEN: f32 = 0.12;
/// Chain stays and seat stays leave their joint at `±STAY_Z`; fork blades
/// leave the crown at `±FORK_Z`.
const STAY_Z: f32 = 0.045;
const FORK_Z: f32 = 0.040;
const FRAME_R: f32 = 0.016;
const STAY_R: f32 = 0.011;

/// Stem from the head top to the bar centre; the bar lies along Z.
const STEM_RISE: [f32; 3] = [0.08, 0.06, 0.0];
const STEM_R: f32 = 0.014;
const BAR_R: f32 = 0.011;
/// The bar ends INSIDE the grips (a bar exactly as long as the grips would
/// put its end discs on the grips' end discs — lesson 37's tie).
const BAR_HALF: f32 = 0.275;
const GRIP_R: f32 = 0.016;
const GRIP_L: f32 = 0.11;
const GRIP_Z: f32 = 0.225;

/// The seat post is sunk `POST_SINK` into the seat tube and rises
/// `POST_RISE` above the cluster to the saddle.
const POST_SINK: f32 = 0.04;
const POST_RISE: f32 = 0.07;
const POST_R: f32 = 0.013;
const SADDLE_HALF: [f32; 3] = [0.13, 0.025, 0.075];
/// Saddle centre relative to the post's top: set back a little and up so the
/// post's top is inside the saddle.
const SADDLE_SET: [f32; 3] = [-0.03, 0.02, 0.0];

/// Cranks: `CRANK_L` from the axle, in the plane `±CRANK_Z`, the drive-side
/// crank at `CRANK_ANGLE` from forward and the other a half-turn on.
const CRANK_L: f32 = 0.14;
const CRANK_ANGLE: f32 = -15.0 * PI / 180.0;
const CRANK_Z: f32 = 0.060;
const CRANK_R: f32 = 0.012;
const AXLE_R: f32 = 0.012;
const AXLE_L: f32 = 0.14;
const SPINDLE_R: f32 = 0.010;
/// The spindle ends inside the pedal body, short of its outer face.
const SPINDLE_END_Z: f32 = 0.150;
const PEDAL: [f32; 3] = [0.08, 0.018, 0.09];
const PEDAL_Z: f32 = 0.115;
/// Drive-side discs: chainring at the bottom bracket, cog at the rear hub.
const DRIVE_Z: f32 = 0.050;
const DISC_T: f32 = 0.010;
const CHAINRING_R: f32 = 0.085;
const COG_R: f32 = 0.035;
const CHAIN_R: f32 = 0.010;

/// Race plate on the head tube's front.
const PLATE: [f32; 3] = [0.01, 0.11, 0.13];
const PLATE_BAND: [f32; 3] = [0.01, 0.028, 0.13];

// ---- the pelican. Body-local coordinates are relative to the blob
// group's origin at the body centre.

const BODY_HALF: [f32; 3] = [0.28, 0.16, 0.15];
/// The body is sunk this far into the saddle so the meshed underside sits on
/// it rather than floating.
const BODY_SINK: f32 = 0.02;
/// The body centre sits this far forward of the saddle centre.
const BODY_FWD: f32 = 0.05;
const BREAST_C: [f32; 3] = [0.14, -0.02, 0.0];
const BREAST_HALF: [f32; 3] = [0.18, 0.14, 0.13];
const HUMP_C: [f32; 3] = [0.05, 0.08, 0.0];
const HUMP_HALF: [f32; 3] = [0.16, 0.10, 0.13];
const TAIL_C: [f32; 3] = [-0.36, 0.05, 0.0];
const TAIL_HALF: [f32; 3] = [0.16, 0.035, 0.09];
/// The neck: base inside the shoulders, back and up, up, then forward to
/// the head — the S.
const NECK: [[f32; 3]; 3] = [[0.20, 0.10, 0.0], [0.14, 0.28, 0.0], [0.19, 0.42, 0.0]];
const HEAD_C: [f32; 3] = [0.36, 0.52, 0.0];
const HEAD_HALF: [f32; 3] = [0.095, 0.078, 0.072];
const NECK_R: f32 = 0.060;
/// The bill's line from the head centre: forward and down.
const BILL_DIR_RAW: [f32; 2] = [0.86, -0.51];
const BILL_LEN: f32 = 0.40;
/// The bill's root is sunk this far into the head.
const BILL_SINK: f32 = 0.03;
const UPPER_HALF: [f32; 3] = [0.026, BILL_LEN * 0.5, 0.050];
/// The lower mandible's thickness and width; its length is derived from
/// the converging line it runs on.
const LOWER_THICK: f32 = 0.014;
const LOWER_WIDE: f32 = 0.047;
/// The lower mandible's centreline sits `LOWER_DROP_ROOT` below the upper's
/// at the head and `LOWER_DROP_TIP` below it at the tip: two tapered
/// mandibles on PARALLEL lines open into a V toward the tip, because the
/// taper thins them faster than a constant drop closes. Converging lines
/// keep the bill shut along its length.
const LOWER_DROP_ROOT: f32 = 0.034;
const LOWER_DROP_TIP: f32 = 0.018;
/// Taper per axis: thickness to 65 %, width to 45 % at the tip.
const BILL_TAPER: [f32; 2] = [0.35, 0.55];
const NAIL_R: f32 = 0.016;
const NAIL_L: f32 = 0.045;
/// Pouch: a flat ellipsoid hung along the bill line from the throat —
/// `POUCH_HALF` is [depth below the bill, half-length along it, half-width]
/// and it is centred `POUCH_ALONG` down the bill and `POUCH_DROP` below it.
/// Narrower than the lower mandible, so the mandible shows along its top
/// edge.
const POUCH_HALF: [f32; 3] = [0.095, 0.18, 0.036];
const POUCH_ALONG: f32 = 0.20;
const POUCH_DROP: f32 = 0.090;
const EYE_R: f32 = 0.014;
const EYE_AT: [f32; 3] = [0.41, 0.555, 0.066];
/// Wings: shoulder → elbow, elbow → the grip (body-local, mirrored in Z).
const SHOULDER: [f32; 3] = [0.14, 0.09, 0.11];
const ELBOW: [f32; 3] = [0.36, 0.06, 0.20];
const FOREWING_R: f32 = 0.042;
/// Each wing is a flattened panel from the shoulder to the elbow and a
/// second from the elbow to the grip, with a capsule inside the forewing
/// ending in the hand on the grip — `[depth, half-length, half-width]`
/// aimed along the segment.
const WING_PANEL_HALF: [f32; 3] = [0.085, 0.15, 0.032];
const FOREWING_PANEL_HALF: [f32; 3] = [0.055, 0.14, 0.030];
const WING_PANEL_LIFT: f32 = 0.02;
const BLEND_BODY: f32 = 0.08;
const BLEND_NECK: f32 = 0.05;
/// The sanitiser's ceiling, and the smallest that keeps a 42 mm forewing
/// over two cells across a 1.2 m span.
const BODY_RES: u32 = 48;
/// Hips, body-local, mirrored in Z.
const HIP: [f32; 3] = [0.02, -0.06, 0.09];
const LEG_R: f32 = 0.020;
/// A foot: thickness, length (toes to heel), width at the toes; the heel is
/// pinched to `1 - FOOT_HEEL` of the toe width.
const FOOT_T: f32 = 0.020;
const FOOT_L: f32 = 0.17;
const FOOT_W: f32 = 0.13;
const FOOT_HEEL: f32 = 0.80;
/// The foot's centre sits this far forward of the pedal's; the ankle this
/// far behind the foot's centre.
const FOOT_FWD: f32 = 0.02;
const ANKLE_BACK: f32 = 0.04;
const TOE_R: f32 = 0.010;
const TOE_SINK: f32 = 0.006;
const TOE_SPREAD: f32 = 0.05;

/// Deliberate defects, one per guard, so every test can prove its guard
/// bites on the fault it exists for — and on nothing else. The shipped
/// prop is `Faults::default()`.
#[derive(Clone, Copy, Default)]
struct Faults {
    /// Raise the saddle (and the bird on it) off the seat post.
    saddle_lift: f32,
    /// Sink both hubs so the tyres run through the ground.
    tyre_drop: f32,
    /// Offset the front rim from its hub — a child offset under a turn.
    rim_offset: f32,
    /// Turn the second crank by this much off the half-turn.
    pedal_skew: f32,
    /// Hold the feet this far above the pedals.
    foot_lift: f32,
    /// Shorten the forewings by this fraction.
    wing_short: f32,
    /// Aim the bill backward (over the tail).
    bill_back: bool,
    /// Push the head and the top of the neck up out of blend range.
    neck_gap: f32,
    /// Slide the down tube's head end off the fork crown.
    strut_miss: f32,
    /// End the pedal spindles ON the pedals' outer faces.
    spindle_flush: bool,
    /// Dress the saddle in the kit's Window glass.
    glazed_saddle: bool,
    /// Park a stray part this far out along X.
    stray_part: f32,
    /// Ask the blob for more resolution than the sanitiser allows.
    hot_resolution: bool,
}

pub struct PelicanBicycle;

impl CatalogueEntry for PelicanBicycle {
    fn slug(&self) -> &'static str {
        "pelican_bicycle"
    }
    fn name(&self) -> &'static str {
        "Pelican on a Bicycle"
    }
    fn description(&self) -> &'static str {
        "A pelican riding a red steel bicycle, wings on the bars and webbed feet on the pedals."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SportsRec]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SPORTS_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

// ---- small vector helpers (arrays, no glam — the record is arrays).

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn with_z(p: [f32; 3], z: f32) -> [f32; 3] {
    [p[0], p[1], z]
}
fn unit(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    scale(v, 1.0 / l)
}
fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

// ---- derived joints. Every tube runs between two of these.

/// Unit direction up the seat tube (back and up).
fn seat_dir() -> [f32; 3] {
    [-SEAT_ANGLE.cos(), SEAT_ANGLE.sin(), 0.0]
}
fn seat_cluster() -> [f32; 3] {
    add(BB, scale(seat_dir(), SEAT_TUBE_LEN))
}
/// Unit direction down the head tube (forward and down).
fn head_dir() -> [f32; 3] {
    [HEAD_ANGLE.cos(), -HEAD_ANGLE.sin(), 0.0]
}
/// The fork crown — the head tube's bottom end.
fn head_bot() -> [f32; 3] {
    add(HEAD_TOP, scale(head_dir(), HEAD_TUBE_LEN))
}
fn bar_centre() -> [f32; 3] {
    add(HEAD_TOP, STEM_RISE)
}
fn grip(sz: f32) -> [f32; 3] {
    with_z(bar_centre(), sz * GRIP_Z)
}
/// The seat post's top — what the saddle is set on.
fn saddle_mount() -> [f32; 3] {
    add(seat_cluster(), scale(seat_dir(), POST_RISE))
}
fn saddle_centre(f: &Faults) -> [f32; 3] {
    add(add(saddle_mount(), SADDLE_SET), [0.0, f.saddle_lift, 0.0])
}
fn saddle_top(f: &Faults) -> f32 {
    saddle_centre(f)[1] + SADDLE_HALF[1]
}
/// Pedal spindle centre on side `sz` (`+1` is the drive side).
fn pedal_at(sz: f32, f: &Faults) -> [f32; 3] {
    let a = if sz > 0.0 {
        CRANK_ANGLE
    } else {
        CRANK_ANGLE + PI + f.pedal_skew
    };
    [
        BB[0] + CRANK_L * a.cos(),
        BB[1] + CRANK_L * a.sin(),
        sz * PEDAL_Z,
    ]
}
fn pedal_top(sz: f32, f: &Faults) -> f32 {
    pedal_at(sz, f)[1] + PEDAL[1] * 0.5
}
/// The body centre: on the saddle, a little forward of it.
fn body_centre(f: &Faults) -> [f32; 3] {
    let s = saddle_centre(f);
    [
        s[0] + BODY_FWD,
        saddle_top(f) + BODY_HALF[1] - BODY_SINK,
        0.0,
    ]
}
/// The bill's unit direction: forward and down, or backward when broken.
fn bill_dir(f: &Faults) -> [f32; 3] {
    let x = if f.bill_back {
        -BILL_DIR_RAW[0]
    } else {
        BILL_DIR_RAW[0]
    };
    unit([x, BILL_DIR_RAW[1], 0.0])
}
/// Perpendicular to the bill in the pitch plane, pointing below it.
fn bill_below(f: &Faults) -> [f32; 3] {
    let d = bill_dir(f);
    [d[1], -d[0], 0.0]
}

// ---- materials

fn frame() -> SovereignMaterialSettings {
    enamel(FRAME_RED)
}
fn chrome() -> SovereignMaterialSettings {
    steel(STEEL_GREY)
}
/// Set a per-axis taper on any primitive (the bill's superellipsoids
/// narrow to the tip the way [`cuboid_tapered_xz`] does).
fn tapered(mut kind: GeneratorKind, taper: [f32; 2]) -> GeneratorKind {
    if let Some(t) = kind.torture_mut() {
        t.taper = Fp2(taper);
    }
    kind
}

// ---- assemblies

/// A spoked wheel at `hub`, axle along Z: the hub is the turned sub-root and
/// rim, tyre and spokes are its children at its own origin (#972 lesson 22,
/// the handcart's recipe). Under `quat_x(π/2)` the hub's local `XZ` plane is
/// the world `XY` wheel plane, so the spoke bars turn about local `Y`.
fn wheel(hub: [f32; 3], rim_offset: f32) -> Generator {
    let mut w = prim(
        solid(cylinder_tapered(HUB_R, HUB_L, 12, 0.0, chrome())),
        hub,
        quat_x(FRAC_PI_2),
    );
    w.children.push(prim(
        torus(RIM_T, RIM_R, chrome()),
        [rim_offset, 0.0, 0.0],
        id_quat(),
    ));
    w.children.push(prim(
        solid(torus(TYRE_T, WHEEL_R - TYRE_T, painted(TYRE_BLACK))),
        [0.0, 0.0, 0.0],
        id_quat(),
    ));
    for k in 0..SPOKE_PAIRS {
        w.children.push(prim(
            cuboid_tapered([SPOKE_LEN, SPOKE_T, SPOKE_T], 0.0, chrome()),
            [0.0, 0.0, 0.0],
            quat_y(k as f32 * PI / SPOKE_PAIRS as f32),
        ));
    }
    w
}

/// The crank set: axle through the bottom bracket, two crank arms a
/// half-turn apart, spindles, pedals, the chainring, and the chain running
/// back to the cog.
fn crank_set(f: &Faults) -> Vec<Generator> {
    let mut out = vec![prim(
        cylinder_tapered(AXLE_R, AXLE_L, 10, 0.0, chrome()),
        BB,
        quat_x(FRAC_PI_2),
    )];
    for sz in [1.0_f32, -1.0] {
        let p = pedal_at(sz, f);
        let crank_end = with_z(p, sz * CRANK_Z);
        out.push(strut(
            with_z(BB, sz * CRANK_Z),
            crank_end,
            CRANK_R,
            8,
            chrome(),
        ));
        let spindle_end_z = if f.spindle_flush {
            PEDAL_Z + PEDAL[2] * 0.5
        } else {
            SPINDLE_END_Z
        };
        out.push(strut(
            crank_end,
            with_z(p, sz * spindle_end_z),
            SPINDLE_R,
            8,
            chrome(),
        ));
        out.push(prim(
            solid(cuboid_tapered(PEDAL, 0.0, painted(PEDAL_DARK))),
            p,
            id_quat(),
        ));
    }
    // Chainring and cog on the drive side, and the chain's two runs between
    // their tops and bottoms.
    out.push(prim(
        cylinder_tapered(CHAINRING_R, DISC_T, 24, 0.0, chrome()),
        with_z(BB, DRIVE_Z),
        quat_x(FRAC_PI_2),
    ));
    out.push(prim(
        cylinder_tapered(COG_R, DISC_T, 16, 0.0, chrome()),
        with_z(REAR_HUB, DRIVE_Z),
        quat_x(FRAC_PI_2),
    ));
    for sy in [1.0_f32, -1.0] {
        out.push(strut(
            [BB[0], BB[1] + sy * CHAINRING_R, DRIVE_Z],
            [REAR_HUB[0], REAR_HUB[1] + sy * COG_R, DRIVE_Z],
            CHAIN_R,
            6,
            chrome(),
        ));
    }
    out
}

/// The steering: fork crown (sub-root) carrying the head tube, the stem,
/// the bar and its grips, the race plate, the fork blades and the front
/// wheel.
fn steering(f: &Faults) -> Generator {
    let crown = head_bot();
    let mut parts = vec![strut(crown, HEAD_TOP, FRAME_R, 10, frame())];
    parts.push(strut(HEAD_TOP, bar_centre(), STEM_R, 8, chrome()));
    parts.push(prim(
        cylinder_tapered(BAR_R, BAR_HALF * 2.0, 10, 0.0, chrome()),
        bar_centre(),
        quat_x(FRAC_PI_2),
    ));
    for sz in [1.0_f32, -1.0] {
        parts.push(prim(
            cylinder_tapered(GRIP_R, GRIP_L, 10, 0.0, painted(GRIP_BLACK)),
            grip(sz),
            quat_x(FRAC_PI_2),
        ));
        parts.push(strut(
            with_z(crown, sz * FORK_Z),
            with_z(FRONT_HUB, sz * HUB_END_Z),
            STAY_R,
            8,
            frame(),
        ));
    }
    // Race plate proud of the head tube's front face, aimed along the
    // tube; a red band across its top, proud of the plate.
    let hd = head_dir();
    let fwd = [-hd[1], hd[0], 0.0];
    let mid = add(HEAD_TOP, scale(hd, HEAD_TUBE_LEN * 0.5));
    let plate_c = add(mid, scale(fwd, FRAME_R + PLATE[0] * 0.5 + 0.004));
    parts.push(prim(
        cuboid_tapered(PLATE, 0.0, painted(PLATE_WHITE)),
        plate_c,
        aim_y(hd),
    ));
    let band_c = add(
        add(plate_c, scale(fwd, PLATE[0] * 0.5 + PLATE_BAND[0] * 0.5)),
        scale(hd, -(PLATE[1] * 0.5 - PLATE_BAND[1] * 0.5 - 0.01)),
    );
    parts.push(prim(
        cuboid_tapered(PLATE_BAND, 0.0, painted(PLATE_RED)),
        band_c,
        aim_y(hd),
    ));
    parts.push(wheel(FRONT_HUB, f.rim_offset));
    nest(
        prim(
            solid(superellipsoid(CROWN_HALF, LUG_EXP, LUG_EXP, frame())),
            crown,
            id_quat(),
        ),
        parts,
    )
}

/// The pelican: one blob skin at the body centre, with the bill, eyes,
/// legs and feet as its children.
fn pelican(f: &Faults) -> Generator {
    let bc = body_centre(f);
    let local = |w: [f32; 3]| add(w, scale(bc, -1.0));
    let capsule_between = |a: [f32; 3], b: [f32; 3], r: f32, blend: f32| -> BlobElement {
        let mid = scale(add(a, b), 0.5);
        blob_capsule(
            mid,
            r,
            dist(a, b) * 0.5,
            aim_y(unit(add(b, scale(a, -1.0)))),
            blend,
        )
    };
    let d = bill_dir(f);
    let below = bill_below(f);
    let gap = [0.0, f.neck_gap, 0.0];
    let head_c = add(HEAD_C, gap);
    let neck_top = add(NECK[2], gap);

    let mut elements = vec![
        blob_ellipsoid([0.0, 0.0, 0.0], BODY_HALF, BLEND_BODY),
        blob_ellipsoid(BREAST_C, BREAST_HALF, BLEND_BODY),
        blob_ellipsoid(HUMP_C, HUMP_HALF, BLEND_BODY),
        blob_ellipsoid(TAIL_C, TAIL_HALF, BLEND_BODY),
        capsule_between(NECK[0], NECK[1], NECK_R, BLEND_NECK),
        capsule_between(NECK[1], NECK[2], NECK_R, BLEND_NECK),
        capsule_between(neck_top, head_c, NECK_R, BLEND_NECK),
        blob_ellipsoid(head_c, HEAD_HALF, BLEND_NECK),
    ];
    // The pouch hangs along the bill line: an ellipsoid aimed down the bill,
    // its rear end inside the throat.
    let bill_root = add(head_c, scale(d, BILL_SINK));
    let mut pouch = blob_ellipsoid(
        add(
            add(bill_root, scale(d, POUCH_ALONG)),
            scale(below, POUCH_DROP),
        ),
        POUCH_HALF,
        BLEND_NECK,
    );
    pouch.rotation = aim_y(d);
    elements.push(pouch);
    for sz in [1.0_f32, -1.0] {
        let shoulder = [SHOULDER[0], SHOULDER[1], sz * SHOULDER[2]];
        let elbow = [ELBOW[0], ELBOW[1], sz * ELBOW[2]];
        let tip = local(grip(sz));
        let tip = add(
            elbow,
            scale(add(tip, scale(elbow, -1.0)), 1.0 - f.wing_short),
        );
        let panel_along = |a: [f32; 3], b: [f32; 3], half: [f32; 3]| -> BlobElement {
            let mut e = blob_ellipsoid(
                add(scale(add(a, b), 0.5), [0.0, WING_PANEL_LIFT, 0.0]),
                half,
                BLEND_BODY,
            );
            e.rotation = aim_y(unit(add(b, scale(a, -1.0))));
            e
        };
        elements.push(panel_along(shoulder, elbow, WING_PANEL_HALF));
        elements.push(capsule_between(elbow, tip, FOREWING_R, BLEND_BODY));
        elements.push(panel_along(elbow, tip, FOREWING_PANEL_HALF));
    }
    let res = if f.hot_resolution { 64 } else { BODY_RES };
    let body = prim(blob_group(elements, res, painted(PLUMAGE)), bc, id_quat());

    // Bill: two tapered mandibles and the nail, all aimed on the bill vector.
    let head_w = add(bc, head_c);
    let root = add(head_w, scale(d, BILL_SINK));
    let tip = add(root, scale(d, BILL_LEN));
    let mut parts = vec![prim(
        tapered(
            superellipsoid(UPPER_HALF, 0.9, 0.5, enamel(BILL_YELLOW)),
            BILL_TAPER,
        ),
        add(root, scale(d, UPPER_HALF[1])),
        aim_y(d),
    )];
    // The lower mandible runs on a line that converges on the upper's at
    // the tip, so the tapered pair stays shut.
    let lower_root = add(root, scale(below, LOWER_DROP_ROOT));
    let lower_tip = add(tip, scale(below, LOWER_DROP_TIP));
    parts.push(prim(
        tapered(
            superellipsoid(
                [LOWER_THICK, dist(lower_root, lower_tip) * 0.5, LOWER_WIDE],
                0.9,
                0.5,
                enamel(BILL_YELLOW),
            ),
            BILL_TAPER,
        ),
        scale(add(lower_root, lower_tip), 0.5),
        aim_y(unit(add(lower_tip, scale(lower_root, -1.0)))),
    ));
    // The nail hooks down off the tip: a cone whose apex (+Y) points down
    // and forward, seated with its base inside the tip.
    let nail_dir = unit(add(d, scale(below, 1.4)));
    parts.push(prim(
        cylinder_tapered(NAIL_R, NAIL_L, 8, 0.9, enamel(NAIL_ORANGE)),
        add(
            add(tip, scale(d, -NAIL_R)),
            scale(nail_dir, NAIL_L * 0.5 - 0.012),
        ),
        aim_y(nail_dir),
    ));
    for sz in [1.0_f32, -1.0] {
        parts.push(prim(
            sphere(EYE_R, 2, painted(EYE_DARK)),
            add(bc, add([EYE_AT[0], EYE_AT[1], sz * EYE_AT[2]], gap)),
            id_quat(),
        ));
    }

    // Legs and feet, one per pedal.
    for sz in [1.0_f32, -1.0] {
        let p = pedal_at(sz, f);
        let sole = pedal_top(sz, f) + f.foot_lift;
        let foot_c = [p[0] + FOOT_FWD, sole + FOOT_T * 0.5, p[2]];
        let foot_top = sole + FOOT_T;
        let ankle = [foot_c[0] - ANKLE_BACK, foot_top, p[2]];
        let hip = add(bc, [HIP[0], HIP[1], sz * HIP[2]]);
        parts.push(strut(
            hip,
            [ankle[0], ankle[1] - 0.005, ankle[2]],
            LEG_R,
            8,
            enamel(LEG_ORANGE),
        ));
        // The web: a slab whose local +Y (the pinched end) is turned onto
        // world -X, so the heel is narrow and the toes are wide.
        parts.push(prim(
            solid(cuboid_tapered_xz(
                [FOOT_T, FOOT_L, FOOT_W],
                [0.0, FOOT_HEEL],
                enamel(LEG_ORANGE),
            )),
            foot_c,
            quat_z(FRAC_PI_2),
        ));
        for k in [-1.0_f32, 0.0, 1.0] {
            parts.push(strut(
                [ankle[0], ankle[1] - TOE_SINK, ankle[2]],
                [
                    foot_c[0] + FOOT_L * 0.5 - TOE_R,
                    foot_top - TOE_SINK,
                    foot_c[2] + k * TOE_SPREAD,
                ],
                TOE_R,
                6,
                enamel(LEG_ORANGE),
            ));
        }
    }
    nest(body, parts)
}

/// The seat course: seat lug (sub-root) carrying the top tube, the seat
/// stays, the seat post, and the saddle with the pelican on it.
fn seat_course(f: &Faults) -> Generator {
    let cluster = seat_cluster();
    let mut parts = vec![strut(cluster, HEAD_TOP, FRAME_R, 10, frame())];
    for sz in [1.0_f32, -1.0] {
        parts.push(strut(
            with_z(cluster, sz * STAY_Z),
            with_z(REAR_HUB, sz * HUB_END_Z),
            STAY_R,
            8,
            frame(),
        ));
    }
    parts.push(strut(
        add(cluster, scale(seat_dir(), -POST_SINK)),
        saddle_mount(),
        POST_R,
        8,
        chrome(),
    ));
    let saddle_mat = if f.glazed_saddle {
        super::glass([0.4, 0.5, 0.54], 0.5)
    } else {
        painted(SADDLE_BLACK)
    };
    let saddle = nest(
        prim(
            solid(superellipsoid(SADDLE_HALF, 0.7, 0.6, saddle_mat)),
            saddle_centre(f),
            id_quat(),
        ),
        vec![pelican(f)],
    );
    parts.push(saddle);
    nest(
        prim(
            solid(superellipsoid(SEAT_LUG_HALF, LUG_EXP, LUG_EXP, frame())),
            cluster,
            id_quat(),
        ),
        parts,
    )
}

fn build_with(f: &Faults) -> Generator {
    let hub_drop = [0.0, -f.tyre_drop, 0.0];
    let mut parts = vec![
        strut(BB, seat_cluster(), FRAME_R, 10, frame()),
        strut(
            BB,
            add(head_bot(), [f.strut_miss, 0.0, 0.0]),
            FRAME_R,
            10,
            frame(),
        ),
    ];
    for sz in [1.0_f32, -1.0] {
        parts.push(strut(
            with_z(BB, sz * STAY_Z),
            with_z(REAR_HUB, sz * HUB_END_Z),
            STAY_R,
            8,
            frame(),
        ));
    }
    parts.extend(crank_set(f));
    parts.push(wheel(add(REAR_HUB, hub_drop), 0.0));
    parts.push(seat_course(f));
    let mut st = steering(f);
    if f.tyre_drop != 0.0 {
        // The front wheel is the crown's child; drop it there.
        for c in st.children.iter_mut() {
            if matches!(c.kind, GeneratorKind::Cylinder { .. }) && !c.children.is_empty() {
                c.transform.translation.0[1] -= f.tyre_drop;
            }
        }
    }
    parts.push(st);
    if f.stray_part > 0.0 {
        parts.push(prim(
            cuboid_tapered([0.2, 0.2, 0.2], 0.0, chrome()),
            [f.stray_part, 0.1, 0.0],
            id_quat(),
        ));
    }
    nest(
        prim(
            solid(superellipsoid(LUG_HALF, LUG_EXP, LUG_EXP, frame())),
            BB,
            id_quat(),
        ),
        parts,
    )
}

fn build_tree() -> Generator {
    build_with(&Faults::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_no_coplanar_faces, assert_no_glazing_on_solids, assert_no_tilted_parents,
        assert_sanitize_stable, blob_cell_size, blob_components, rotate_by,
    };
    use crate::pds::Fp3;

    const SLUG: &str = "pelican_bicycle";

    fn walk(g: &Generator, at: [f32; 3], f: &mut dyn FnMut(&Generator, [f32; 3])) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        f(g, here);
        for c in &g.children {
            walk(c, here, f);
        }
    }

    /// A cylinder's two BUILT ends, from its own quaternion.
    fn ends(g: &Generator, at: [f32; 3]) -> Option<([f32; 3], [f32; 3], f32)> {
        let GeneratorKind::Cylinder { height, radius, .. } = &g.kind else {
            return None;
        };
        let tip = rotate_by(g.transform.rotation.0, [0.0, height.0 * 0.5, 0.0]);
        Some((add(at, scale(tip, -1.0)), add(at, tip), radius.0))
    }

    /// A prim's axis-aligned box from its BUILT rotation (the handcart's
    /// plan box, extended to the organic kinds by their element extents).
    fn aabb(g: &Generator, at: [f32; 3]) -> Option<([f32; 3], [f32; 3])> {
        let mut half = match &g.kind {
            GeneratorKind::Cuboid { size, .. } => {
                [size.0[0] * 0.5, size.0[1] * 0.5, size.0[2] * 0.5]
            }
            GeneratorKind::Cylinder { radius, height, .. } => [radius.0, height.0 * 0.5, radius.0],
            GeneratorKind::Torus {
                major_radius,
                minor_radius,
                ..
            } => {
                let r = major_radius.0 + minor_radius.0;
                [r, minor_radius.0, r]
            }
            GeneratorKind::Sphere { radius, .. } => [radius.0; 3],
            GeneratorKind::Superellipsoid { half_extents, .. } => half_extents.0,
            GeneratorKind::BlobGroup { elements, .. } => {
                let mut lo = [f32::MAX; 3];
                let mut hi = [f32::MIN; 3];
                for e in elements {
                    let r = e.radii.0[0].max(e.radii.0[1]).max(e.radii.0[2]);
                    for i in 0..3 {
                        lo[i] = lo[i].min(e.position.0[i] - r);
                        hi[i] = hi[i].max(e.position.0[i] + r);
                    }
                }
                let c = scale(add(lo, hi), 0.5);
                let h = scale(add(hi, scale(lo, -1.0)), 0.5);
                let at = add(at, c);
                return Some((add(at, scale(h, -1.0)), add(at, h)));
            }
            _ => return None,
        };
        // Taper reads to the widest end.
        if let GeneratorKind::Cuboid { common, .. } = &g.kind {
            let t = common.torture.taper.0;
            if t[0] < 0.0 {
                half[0] *= 1.0 - t[0];
            }
            if t[1] < 0.0 {
                half[2] *= 1.0 - t[1];
            }
        }
        let q = g.transform.rotation.0;
        let mut ext = [0.0_f32; 3];
        for (axis, h) in half.iter().enumerate() {
            let mut v = [0.0; 3];
            v[axis] = *h;
            for (e, w) in ext.iter_mut().zip(rotate_by(q, v)) {
                *e += w.abs();
            }
        }
        Some((add(at, scale(ext, -1.0)), add(at, ext)))
    }

    fn colour(g: &Generator) -> Option<[f32; 3]> {
        g.kind.material().map(|m| m.base_color.0)
    }
    fn is(c: Option<[f32; 3]>, want: [f32; 3]) -> bool {
        c == Some(want)
    }

    /// Run `guard` on the broken variant `faults` and require that it
    /// panics WITH `needle` in its message — proof it bit on the fault, not
    /// on a selector count.
    fn bites(faults: Faults, guard: &dyn Fn(&Generator), needle: &str) {
        let built = build_with(&faults);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| guard(&built)));
        let msg = match r {
            Ok(()) => panic!("the guard passed on the broken variant ({needle})"),
            Err(e) => e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default(),
        };
        assert!(
            msg.contains(needle),
            "the guard bit on the broken variant, but not on the fault: wanted {needle:?} in\n{msg}"
        );
    }

    // ---- 1. sanitize round trip

    fn guard_round_trip(root: &Generator) {
        assert_sanitize_stable(root, SLUG);
    }
    #[test]
    fn build_round_trips_through_sanitize() {
        guard_round_trip(&build_tree());
        bites(
            Faults {
                hot_resolution: true,
                ..Default::default()
            },
            &guard_round_trip,
            "rewritten by sanitiser",
        );
    }

    // ---- 2. tilted parents

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        let g = |r: &Generator| assert_no_tilted_parents(r, SLUG);
        g(&build_tree());
        bites(
            Faults {
                rim_offset: 0.05,
                ..Default::default()
            },
            &g,
            "carries a child offset",
        );
    }

    // ---- 3. coplanar faces

    #[test]
    fn no_two_faces_tie_for_depth() {
        let g = |r: &Generator| assert_no_coplanar_faces(r, SLUG);
        g(&build_tree());
        bites(
            Faults {
                spindle_flush: true,
                ..Default::default()
            },
            &g,
            "share a plane",
        );
    }

    // ---- 4. glazing on solids

    #[test]
    fn no_solid_wears_a_window_card() {
        let g = |r: &Generator| assert_no_glazing_on_solids(r, SLUG);
        g(&build_tree());
        bites(
            Faults {
                glazed_saddle: true,
                ..Default::default()
            },
            &g,
            "wears a Window texture",
        );
    }

    // ---- 5. every frame tube runs joint to joint

    /// The named joints a frame tube may end on.
    fn joints() -> Vec<(&'static str, [f32; 3])> {
        let mut j = vec![
            ("bottom bracket", BB),
            ("seat cluster", seat_cluster()),
            ("head top", HEAD_TOP),
            ("fork crown", head_bot()),
        ];
        for sz in [1.0_f32, -1.0] {
            j.push(("bottom bracket end", with_z(BB, sz * STAY_Z)));
            j.push(("seat cluster end", with_z(seat_cluster(), sz * STAY_Z)));
            j.push(("fork crown end", with_z(head_bot(), sz * FORK_Z)));
            j.push(("rear hub end", with_z(REAR_HUB, sz * HUB_END_Z)));
            j.push(("front hub end", with_z(FRONT_HUB, sz * HUB_END_Z)));
        }
        j
    }

    /// **Every frame tube's two BUILT ends land on a named joint.** The
    /// tubes are selected by what they are — enamelled cylinders — and the
    /// count is exact: head, top, down, seat, two chain stays, two seat
    /// stays, two fork blades.
    fn guard_frame_joints(root: &Generator) {
        let joints = joints();
        let mut tubes = 0;
        walk(root, [0.0; 3], &mut |g, at| {
            if !is(colour(g), FRAME_RED) {
                return;
            }
            let Some((a, b, _)) = ends(g, at) else {
                return;
            };
            tubes += 1;
            for end in [a, b] {
                let hit = joints.iter().find(|(_, j)| dist(*j, end) < 1e-3);
                assert!(
                    hit.is_some(),
                    "{SLUG}: a frame tube ends at {end:?}, which is no named joint \
                     (its other end is at {:?})",
                    if end == a { b } else { a }
                );
            }
        });
        assert_eq!(tubes, 10, "ten frame tubes");
    }
    #[test]
    fn every_frame_tube_runs_from_joint_to_joint() {
        guard_frame_joints(&build_tree());
        bites(
            Faults {
                strut_miss: 0.05,
                ..Default::default()
            },
            &guard_frame_joints,
            "no named joint",
        );
    }

    // ---- 6. wheels: on the ground, and concentric

    /// The two wheels as built: hub centre, hub axis, and the children's
    /// offsets and radii.
    struct Wheel {
        hub: [f32; 3],
        axis: [f32; 3],
        hub_r: f32,
        rim: ([f32; 3], f32),
        tyre: ([f32; 3], f32, f32),
        spokes: usize,
    }
    fn wheels(root: &Generator) -> Vec<Wheel> {
        let mut out = Vec::new();
        walk(root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cylinder { radius, .. } = &g.kind else {
                return;
            };
            if g.children.is_empty() {
                return;
            }
            let mut rim = None;
            let mut tyre = None;
            let mut spokes = 0;
            for c in &g.children {
                match &c.kind {
                    GeneratorKind::Torus {
                        major_radius,
                        minor_radius,
                        ..
                    } if is(colour(c), TYRE_BLACK) => {
                        tyre = Some((c.transform.translation.0, major_radius.0, minor_radius.0));
                    }
                    GeneratorKind::Torus { major_radius, .. } => {
                        rim = Some((c.transform.translation.0, major_radius.0));
                    }
                    GeneratorKind::Cuboid { .. } => spokes += 1,
                    _ => {}
                }
            }
            out.push(Wheel {
                hub: at,
                axis: rotate_by(g.transform.rotation.0, [0.0, 1.0, 0.0]),
                hub_r: radius.0,
                rim: rim.expect("a rim"),
                tyre: tyre.expect("a tyre"),
                spokes,
            });
        });
        out
    }
    /// **Both tyres touch the ground, both wheels turn about Z, and each
    /// wheel's rim, tyre and spokes are concentric with its hub.**
    fn guard_wheels(root: &Generator) {
        let ws = wheels(root);
        assert_eq!(ws.len(), 2, "two wheels");
        for w in &ws {
            let (tc, tmaj, tmin) = w.tyre;
            let bottom = w.hub[1] - (tmaj + tmin);
            assert!(
                bottom.abs() < 1e-3,
                "{SLUG}: a tyre at {:?} has its bottom at {bottom:.3}, not on the ground",
                w.hub
            );
            assert!(
                w.axis[2].abs() > 0.999,
                "{SLUG}: a wheel's axle runs along {:?}, not Z",
                w.axis
            );
            let (rc, rmaj) = w.rim;
            for (what, c) in [("rim", rc), ("tyre", tc)] {
                assert!(
                    c.iter().all(|v| v.abs() < 1e-4),
                    "{SLUG}: the {what} of the wheel at {:?} is offset {c:?} from its hub",
                    w.hub
                );
            }
            assert!(
                rmaj > w.hub_r && rmaj < tmaj + tmin && rmaj + RIM_T > tmaj - tmin,
                "{SLUG}: the rim (r {rmaj}) does not sit between hub and tyre"
            );
            assert_eq!(w.spokes, SPOKE_PAIRS as usize, "spoke bars");
        }
    }
    #[test]
    fn both_wheels_stand_on_the_ground_and_are_concentric() {
        guard_wheels(&build_tree());
        bites(
            Faults {
                tyre_drop: 0.03,
                ..Default::default()
            },
            &guard_wheels,
            "not on the ground",
        );
        bites(
            Faults {
                rim_offset: 0.05,
                ..Default::default()
            },
            &guard_wheels,
            "offset",
        );
    }

    // ---- 7. pedals opposite on the crank axis

    fn pedals(root: &Generator) -> Vec<[f32; 3]> {
        let mut out = Vec::new();
        walk(root, [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Cuboid { size, .. } = &g.kind
                && is(colour(g), PEDAL_DARK)
                && size.0[1] < 0.03
            {
                out.push(at);
            }
        });
        out
    }
    /// **The two pedals are a half-turn apart about the crank axle**: their
    /// midpoint is the axle in `XY`, they are equidistant from it, and they
    /// sit on opposite sides in Z.
    fn guard_pedals(root: &Generator) {
        let ps = pedals(root);
        assert_eq!(ps.len(), 2, "two pedals");
        let (a, b) = (ps[0], ps[1]);
        let mid = scale(add(a, b), 0.5);
        assert!(
            (mid[0] - BB[0]).abs() < 1e-3 && (mid[1] - BB[1]).abs() < 1e-3,
            "{SLUG}: the pedals at {a:?} and {b:?} are not opposite about the crank axle at \
             {BB:?} — their midpoint is {mid:?}"
        );
        assert!(
            (a[2] + b[2]).abs() < 1e-4 && a[2].abs() > 0.05,
            "{SLUG}: the pedals are not on opposite sides"
        );
    }
    #[test]
    fn the_pedals_are_opposite_on_the_crank_axis() {
        guard_pedals(&build_tree());
        bites(
            Faults {
                pedal_skew: 0.3,
                ..Default::default()
            },
            &guard_pedals,
            "not opposite",
        );
    }

    // ---- 8. each foot's sole on its pedal

    /// **Each foot's sole rests on its pedal.** Feet are the orange
    /// tapered slabs; each is matched to the pedal its plan box overlaps and
    /// its underside must be on that pedal's top.
    fn guard_feet(root: &Generator) {
        let mut feet = Vec::new();
        let mut pedal_boxes = Vec::new();
        walk(root, [0.0; 3], &mut |g, at| match &g.kind {
            GeneratorKind::Cuboid { common, .. }
                if is(colour(g), LEG_ORANGE) && common.torture.taper.0[1] > 0.5 =>
            {
                feet.push(aabb(g, at).unwrap());
            }
            GeneratorKind::Cuboid { .. } if is(colour(g), PEDAL_DARK) => {
                pedal_boxes.push(aabb(g, at).unwrap());
            }
            _ => {}
        });
        assert_eq!(feet.len(), 2, "two feet");
        assert_eq!(pedal_boxes.len(), 2, "two pedals");
        for (lo, hi) in &feet {
            let over = pedal_boxes.iter().find(|(plo, phi)| {
                lo[0] < phi[0] && hi[0] > plo[0] && lo[2] < phi[2] && hi[2] > plo[2]
            });
            let (_, phi) = over.unwrap_or_else(|| {
                panic!("{SLUG}: a foot spanning {lo:?}..{hi:?} stands over no pedal")
            });
            assert!(
                (lo[1] - phi[1]).abs() < 5e-3,
                "{SLUG}: a foot's sole is at {:.3} over a pedal whose top is at {:.3} — it \
                 does not touch its pedal",
                lo[1],
                phi[1]
            );
        }
    }
    #[test]
    fn each_foot_stands_on_its_pedal() {
        guard_feet(&build_tree());
        bites(
            Faults {
                foot_lift: 0.03,
                ..Default::default()
            },
            &guard_feet,
            "does not touch its pedal",
        );
    }

    // ---- 9. the wing tips reach the bars

    /// The single blob skin and where it is built.
    fn skin(root: &Generator) -> (Vec<BlobElement>, [f32; 3], u32) {
        let mut found = Vec::new();
        walk(root, [0.0; 3], &mut |g, at| {
            if let GeneratorKind::BlobGroup {
                elements,
                resolution,
                ..
            } = &g.kind
            {
                found.push((elements.clone(), at, *resolution));
            }
        });
        assert_eq!(found.len(), 1, "one skin");
        found.remove(0)
    }
    /// **Each wing's tip lands on a grip.** The two most forward capsule
    /// ends in the BUILT skin must each be within a centimetre of a grip's
    /// centre — a hand wrapped round the bar.
    fn guard_wings(root: &Generator) {
        let (elements, at, _) = skin(root);
        let mut grips = Vec::new();
        walk(root, [0.0; 3], &mut |g, here| {
            if matches!(g.kind, GeneratorKind::Cylinder { .. }) && is(colour(g), GRIP_BLACK) {
                grips.push(here);
            }
        });
        assert_eq!(grips.len(), 2, "two grips");
        let mut tips: Vec<[f32; 3]> = elements
            .iter()
            .filter(|e| matches!(e.shape, crate::pds::generator::BlobShape::Capsule))
            .flat_map(|e| {
                let half = rotate_by(e.rotation.0, [0.0, e.radii.0[1], 0.0]);
                let c = add(at, e.position.0);
                [add(c, half), add(c, scale(half, -1.0))]
            })
            .collect();
        tips.sort_by(|a, b| b[0].total_cmp(&a[0]));
        for tip in &tips[..2] {
            let near = grips
                .iter()
                .map(|g| dist(*g, *tip))
                .fold(f32::MAX, f32::min);
            assert!(
                near < 0.01,
                "{SLUG}: a wing tip at {tip:?} stops {near:.3} m short of the nearest grip"
            );
        }
    }
    #[test]
    fn the_wing_tips_reach_the_bars() {
        guard_wings(&build_tree());
        bites(
            Faults {
                wing_short: 0.3,
                ..Default::default()
            },
            &guard_wings,
            "short of the nearest grip",
        );
    }

    // ---- 10. the bill points forward and down

    /// **The bill's built `+Y` points along the direction of travel and
    /// down**, and the direction of travel is toward the wheel under the
    /// handlebar. Reads the built quaternion of the yellow superellipsoids.
    fn guard_bill(root: &Generator) {
        let mut bills = Vec::new();
        let mut bar = None;
        walk(root, [0.0; 3], &mut |g, at| match &g.kind {
            GeneratorKind::Superellipsoid { .. } if is(colour(g), BILL_YELLOW) => {
                bills.push(rotate_by(g.transform.rotation.0, [0.0, 1.0, 0.0]));
            }
            GeneratorKind::Cylinder { height, .. }
                if is(colour(g), STEEL_GREY) && height.0 > 0.5 =>
            {
                bar = Some(at);
            }
            _ => {}
        });
        assert_eq!(bills.len(), 2, "two mandibles");
        let bar = bar.expect("a handlebar");
        let ws = wheels(root);
        let front = ws
            .iter()
            .min_by(|a, b| {
                (a.hub[0] - bar[0])
                    .abs()
                    .total_cmp(&(b.hub[0] - bar[0]).abs())
            })
            .unwrap();
        let rear = ws
            .iter()
            .max_by(|a, b| dist(a.hub, bar).total_cmp(&dist(b.hub, bar)))
            .unwrap();
        let travel = unit(add(front.hub, scale(rear.hub, -1.0)));
        for d in &bills {
            let along = d[0] * travel[0] + d[2] * travel[2];
            assert!(
                along > 0.6 && d[1] < -0.3,
                "{SLUG}: the bill points {d:?}; travel is {travel:?} — the pelican is not \
                 looking forward and down"
            );
        }
    }
    #[test]
    fn the_bill_points_forward_and_down() {
        guard_bill(&build_tree());
        bites(
            Faults {
                bill_back: true,
                ..Default::default()
            },
            &guard_bill,
            "not looking forward",
        );
    }

    // ---- 11. saddle → seat post → seat tube → bottom bracket

    /// **The saddle is carried by an unbroken chain of solids** down to the
    /// bottom bracket: the post's top is inside the saddle, the post's
    /// bottom is inside the seat tube, and the seat tube's bottom is inside
    /// the bottom bracket lug — each read from the BUILT strut ends.
    fn guard_seat_chain(root: &Generator) {
        let mut saddle = None;
        let mut lug = None;
        let mut post = None;
        let mut tube = None;
        walk(root, [0.0; 3], &mut |g, at| match &g.kind {
            GeneratorKind::Superellipsoid { half_extents, .. } if is(colour(g), SADDLE_BLACK) => {
                saddle = Some((at, half_extents.0));
            }
            // The bottom bracket is the lowest of the three lugs.
            GeneratorKind::Superellipsoid { half_extents, .. }
                if is(colour(g), FRAME_RED)
                    && lug.is_none_or(|(l, _): ([f32; 3], [f32; 3])| at[1] < l[1]) =>
            {
                lug = Some((at, half_extents.0));
            }
            // The post is the one chrome member of its stock.
            GeneratorKind::Cylinder { radius, .. }
                if is(colour(g), STEEL_GREY) && (radius.0 - POST_R).abs() < 1e-4 =>
            {
                post = ends(g, at);
            }
            // The seat tube is the main-tube stock at the seat tube's length.
            GeneratorKind::Cylinder { radius, height, .. }
                if is(colour(g), FRAME_RED)
                    && (radius.0 - FRAME_R).abs() < 1e-4
                    && (height.0 - SEAT_TUBE_LEN).abs() < 1e-3 =>
            {
                tube = ends(g, at);
            }
            _ => {}
        });
        let (sc, sh) = saddle.expect("a saddle");
        let (lc, lh) = lug.expect("a bottom bracket lug");
        let (pa, pb, pr) = post.expect("a seat post");
        let (ta, tb, tr) = tube.expect("a seat tube");
        let (post_top, post_bot) = if pa[1] > pb[1] { (pa, pb) } else { (pb, pa) };
        let (tube_top, tube_bot) = if ta[1] > tb[1] { (ta, tb) } else { (tb, ta) };
        assert!(
            (0..3).all(|i| (post_top[i] - sc[i]).abs() < sh[i]),
            "{SLUG}: the seat post tops out at {post_top:?}, outside the saddle at {sc:?} \
             (half {sh:?}) — the saddle floats"
        );
        // The post's bottom lies on the tube's segment, inside its radius.
        let seg = add(tube_top, scale(tube_bot, -1.0));
        let len = dist(tube_top, tube_bot);
        let rel = add(post_bot, scale(tube_bot, -1.0));
        let t = (rel[0] * seg[0] + rel[1] * seg[1] + rel[2] * seg[2]) / (len * len);
        let foot = add(tube_bot, scale(seg, t.clamp(0.0, 1.0)));
        assert!(
            dist(foot, post_bot) < tr + pr && (0.0..=1.0).contains(&t),
            "{SLUG}: the seat post's bottom at {post_bot:?} is not inside the seat tube"
        );
        assert!(
            (0..3).all(|i| (tube_bot[i] - lc[i]).abs() < lh[i]),
            "{SLUG}: the seat tube's bottom at {tube_bot:?} is not inside the bottom bracket"
        );
    }
    #[test]
    fn the_saddle_is_carried_down_to_the_bottom_bracket() {
        guard_seat_chain(&build_tree());
        bites(
            Faults {
                saddle_lift: 0.05,
                ..Default::default()
            },
            &guard_seat_chain,
            "the saddle floats",
        );
    }

    // ---- 12. one skin

    /// **The bird is one skin**, and the arithmetic that keeps it so: the
    /// thinnest element is over two sample cells across the skin's span.
    fn guard_skin(root: &Generator) {
        let (elements, _, res) = skin(root);
        let kind = blob_group(elements.clone(), res, painted(PLUMAGE));
        assert_eq!(
            blob_components(&kind),
            1,
            "{SLUG}: the bird polygonised into more than one piece — an element has \
             drifted out of blend range, or is thinner than the sample grid"
        );
        let (lo, hi) = aabb(&prim(kind, [0.0; 3], id_quat()), [0.0; 3]).unwrap();
        let span = (0..3).map(|i| hi[i] - lo[i]).fold(0.0_f32, f32::max);
        let cell = blob_cell_size(span, res);
        let thinnest = elements
            .iter()
            .map(|e| match e.shape {
                crate::pds::generator::BlobShape::Capsule => e.radii.0[0] * 2.0,
                _ => e.radii.0[0].min(e.radii.0[1]).min(e.radii.0[2]) * 2.0,
            })
            .fold(f32::MAX, f32::min);
        assert!(
            thinnest > cell * 2.0,
            "{SLUG}: the thinnest element is {thinnest} m across a {cell} m cell"
        );
    }
    #[test]
    fn the_bird_is_one_skin() {
        guard_skin(&build_tree());
        bites(
            Faults {
                neck_gap: 0.25,
                ..Default::default()
            },
            &guard_skin,
            "more than one piece",
        );
    }

    // ---- 13. footprint inside the declared clearance

    fn guard_footprint(root: &Generator) {
        let clearance = PelicanBicycle.footprint().clearance;
        walk(root, [0.0; 3], &mut |g, at| {
            if let Some((lo, hi)) = aabb(g, at) {
                let reach = lo[0]
                    .abs()
                    .max(hi[0].abs())
                    .max(lo[2].abs())
                    .max(hi[2].abs());
                assert!(
                    reach <= clearance + 1e-3,
                    "{SLUG}: a {} reaches {reach:.3} m, past the declared clearance {clearance}",
                    g.kind.kind_tag()
                );
            }
        });
    }
    #[test]
    fn the_footprint_is_inside_the_declared_clearance() {
        guard_footprint(&build_tree());
        bites(
            Faults {
                stray_part: 1.4,
                ..Default::default()
            },
            &guard_footprint,
            "past the declared clearance",
        );
    }

    // ---- 14. subtree sizes — the editability contract

    fn count(g: &Generator) -> usize {
        1 + g.children.iter().map(count).sum::<usize>()
    }
    #[test]
    fn subtree_sizes_are_the_editability_contract() {
        let root = build_tree();
        let wheel_n = 1 + 2 + SPOKE_PAIRS as usize;
        let mut sizes = Vec::new();
        walk(&root, [0.0; 3], &mut |g, _| {
            if !g.children.is_empty() {
                sizes.push((g.kind.kind_tag(), colour(g), count(g)));
            }
        });
        // Root, seat lug, saddle, bird, fork crown, two hubs.
        assert_eq!(sizes.len(), 7, "{sizes:?}");
        let of = |tag: &str, col: [f32; 3]| -> Vec<usize> {
            sizes
                .iter()
                .filter(|(t, c, _)| *t == tag && is(*c, col))
                .map(|(_, _, n)| *n)
                .collect()
        };
        assert_eq!(of("Cylinder", STEEL_GREY), vec![wheel_n, wheel_n]);
        // The bird: skin + 2 mandibles + nail + 2 eyes + 2 legs + 2 feet + 6 toes.
        assert_eq!(of("BlobGroup", PLUMAGE), vec![16]);
        assert_eq!(of("Superellipsoid", SADDLE_BLACK), vec![17]);
        // Fork crown: head tube, stem, bar, 2 grips, 2 blades, plate, band, wheel.
        assert_eq!(count(&root), 72, "whole prop");
        let mut lugs = of("Superellipsoid", FRAME_RED);
        lugs.sort_unstable();
        // Seat lug: top tube, 2 stays, post, saddle subtree. Fork crown:
        // head tube, stem, bar, 2 grips, 2 blades, plate, band, wheel.
        assert_eq!(lugs, vec![1 + 4 + 17, 1 + 9 + wheel_n, 72]);
    }

    /// The material fixture the tests select on is what the kit hands out —
    /// a kit palette change would silently re-target every selector above.
    #[test]
    fn selectors_read_the_kit_colours_they_expect() {
        assert_eq!(chrome().base_color, Fp3(STEEL_GREY));
        assert_eq!(frame().base_color, Fp3(FRAME_RED));
    }
}
