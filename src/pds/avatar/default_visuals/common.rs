//! Shared primitive vocabulary for the avatar assemblers and parts.
//!
//! Pure geometry plumbing: axis-rotation quaternion helpers, primitive-kind
//! constructors (torture triple zeroed), and the assembler placement
//! helpers ([`offset`] / [`offset_rot`]). Material
//! *finish* lives in [`crate::seeded_defaults::MaterialKit`]; both the
//! assemblers ([`super`]) and the part catalogue
//! ([`crate::pds::avatar::parts`]) build from this bin so geometry plumbing
//! lives in exactly one place.

use crate::pds::PrimCommon;
use crate::pds::generator::{Generator, GeneratorKind, LathePoint, SpinePoint};
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::{Fp, Fp2, Fp3, Fp4, TransformData};

// ---------------------------------------------------------------------------
// Quaternion helpers
// ---------------------------------------------------------------------------

/// Rotation around X as a normalised `[x, y, z, w]` quaternion - points a
/// cone apex (local +Y) along ±Z, e.g. a forward-pointing prow ram.
pub(crate) fn quat_x(angle_rad: f32) -> [f32; 4] {
    let half = angle_rad * 0.5;
    [half.sin(), 0.0, 0.0, half.cos()]
}

/// Rotation around Y as a normalised `[x, y, z, w]` quaternion - yaws a part
/// in plan view, e.g. the root flip that turns a chassis to face −Z.
pub(crate) fn quat_y(angle_rad: f32) -> [f32; 4] {
    let half = angle_rad * 0.5;
    [0.0, half.sin(), 0.0, half.cos()]
}

/// Rotation around Z - lays wheel cylinders onto their axle and rolls hair
/// tufts / stabiliser fins off vertical.
pub(crate) fn quat_z(angle_rad: f32) -> [f32; 4] {
    let half = angle_rad * 0.5;
    [0.0, 0.0, half.sin(), half.cos()]
}

/// Hamilton product of two `[x, y, z, w]` quaternions: the rotation that
/// applies `b` first and then `a`.
///
/// Test-only since #1364. It composed the two axis rotations that laid a
/// legacy skiff wheel on its axle; the redesigned family lays a turned wheel
/// with one [`quat_z`], so the only caller left is the airship's fin-cluster
/// test, which uses it to rotate a vector by hand.
#[cfg(test)]
pub(crate) fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

pub(crate) fn quat_xyzw(q: [f32; 4]) -> Fp4 {
    Fp4(q)
}

/// Identity rotation for transforms that don't turn their child.
pub(crate) fn id_quat() -> Fp4 {
    Fp4([0.0, 0.0, 0.0, 1.0])
}

// ---------------------------------------------------------------------------
// Node assembly
// ---------------------------------------------------------------------------

/// Wrap a [`GeneratorKind`] into a childless [`Generator`] node at
/// `translation` with `rotation`. Children are pushed onto the returned node
/// by the caller where needed.
pub(crate) fn prim(kind: GeneratorKind, translation: [f32; 3], rotation: Fp4) -> Generator {
    Generator {
        kind,
        transform: TransformData {
            translation: Fp3(translation),
            rotation,
            scale: Fp3([1.0, 1.0, 1.0]),
        },
        children: Vec::new(),
        audio: crate::pds::SovereignAudioConfig::None,
        spin: None,
    }
}

// ---------------------------------------------------------------------------
// Primitive-kind constructors (torture triple zeroed)
// ---------------------------------------------------------------------------

pub(crate) fn cuboid(size: [f32; 3], material: SovereignMaterialSettings) -> GeneratorKind {
    GeneratorKind::Cuboid {
        size: Fp3(size),
        common: PrimCommon::with_material(material),
    }
}

/// A right-triangular prism in its `size` box: the slope rises from the
/// front-bottom edge (`+Z`, `-Y`) to the back-top one (`-Z`, `+Y`) across
/// the full width, so the `-Z` face is the upright one. The steam tug's
/// forefoot and stem are two (#1370): a sweep's end cap tilts with its path,
/// and a keel line rising as steeply as a bow's tilted it into a ram.
pub(crate) fn wedge(size: [f32; 3], material: SovereignMaterialSettings) -> GeneratorKind {
    GeneratorKind::Wedge {
        size: Fp3(size),
        common: PrimCommon::with_material(material),
    }
}

pub(crate) fn sphere(
    radius: f32,
    resolution: u32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Sphere {
        radius: Fp(radius),
        resolution,
        common: PrimCommon::with_material(material),
    }
}

pub(crate) fn cylinder(
    radius: f32,
    height: f32,
    resolution: u32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Cylinder {
        radius: Fp(radius),
        height: Fp(height),
        resolution,
        common: PrimCommon::with_material(material),
    }
}

// The smooth-blend SDF vocabulary (`blob_group` and its elements) lived here
// until #1363. It existed for the boat hull, which was a blob iso-surface
// nobody could predict - which is exactly why rails floated off it and every
// mount needed an embed fudge factor - and the redesign's owner decision 4 is
// that machines are built from swept and turned shapes, never blobs, because a
// blob reads as organic. Nothing left in this crate assembles one, so it is
// gone rather than kept warm; git history has it if a genuinely organic avatar
// part ever wants it back.

pub(crate) fn spine(
    points: &[([f32; 3], f32)],
    resolution: u32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Spine {
        points: points
            .iter()
            .map(|(p, r)| SpinePoint {
                position: Fp3(*p),
                radius: Fp(*r),
            })
            .collect(),
        resolution,
        samples_per_segment: 8,
        common: PrimCommon::with_material(material),
    }
}

/// Box with rounded **vertical** edges - a pressed panel. Its chamfer runs
/// parallel to the Y extrusion axis, so `size` is `[width, depth, height]` and
/// a radiator shell standing across the nose wants a quarter turn about X;
/// authored the other way it lies flat like a tray (#1359's owner-steer
/// comment, found by render).
pub(crate) fn bevel(
    size: [f32; 3],
    bevel_radius: f32,
    segments: u32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Bevel {
        size: Fp3(size),
        bevel: Fp(bevel_radius),
        bevel_segments: segments,
        common: PrimCommon::with_material(material),
    }
}

/// Profile-revolve prim (#689): `points` are `(radius, height)` silhouette
/// stations bottom-to-top; `smooth` splines them.
pub(crate) fn lathe(
    points: &[(f32, f32)],
    resolution: u32,
    smooth: bool,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Lathe {
        points: points
            .iter()
            .map(|(r, h)| LathePoint {
                radius: Fp(*r),
                height: Fp(*h),
            })
            .collect(),
        resolution,
        smooth,
        common: PrimCommon::with_material(material),
    }
}

/// A Barr superellipsoid - the one prim that is a pressed panel with rounded
/// edges all round. `exponent_ns` shapes the vertical profile and
/// `exponent_ew` the horizontal section: small exponents are boxy (a flat roof
/// on upright sides), `1.0` is an ellipsoid. The roadster's hardtop cabin is
/// one (#1367), because a swept dome has no upright side for a window band to
/// face the chase camera from.
pub(crate) fn superellipsoid(
    half_extents: [f32; 3],
    exponent_ns: f32,
    exponent_ew: f32,
    latitudes: u32,
    longitudes: u32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Superellipsoid {
        half_extents: Fp3(half_extents),
        exponent_ns: Fp(exponent_ns),
        exponent_ew: Fp(exponent_ew),
        latitudes,
        longitudes,
        common: PrimCommon::with_material(material),
    }
}

pub(crate) fn cone(
    radius: f32,
    height: f32,
    resolution: u32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Cone {
        radius: Fp(radius),
        height: Fp(height),
        resolution,
        common: PrimCommon::with_material(material),
    }
}

pub(crate) fn torus(
    minor_radius: f32,
    major_radius: f32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Torus {
        minor_radius: Fp(minor_radius),
        major_radius: Fp(major_radius),
        minor_resolution: 12,
        major_resolution: 24,
        common: PrimCommon::with_material(material),
    }
}

/// A helical tube - spring / screw / spiral (`Helix` prim, #527). `radius` is
/// the helix radius, `tube` the wire thickness, `pitch` the vertical rise per
/// full turn, `turns` the revolution count. Laid along +Y; rotate to lay it
/// along the travel axis for a screw propeller.
pub(crate) fn helix(
    radius: f32,
    tube: f32,
    pitch: f32,
    turns: f32,
    resolution: u32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Helix {
        radius: Fp(radius),
        tube_radius: Fp(tube),
        pitch: Fp(pitch),
        turns: Fp(turns),
        resolution,
        common: PrimCommon::with_material(material),
    }
}

/// Stamp a torture triple onto a parametric primitive kind for organic
/// shaping. Semantics live in `crate::world_builder::prim`: `twist` is
/// radians of Y-rotation across the height, `taper` scales X/Z toward the
/// top (`0.5` → half-width crown, negative flares outward), `bend` displaces
/// the top quadratically on world X/Z. The scalar `new_taper` sets a uniform
/// (X == Z) taper; author per-axis taper or an S-bend by building
/// [`TortureParams`](crate::pds::TortureParams) directly. Non-primitive
/// kinds pass through.
pub(crate) fn with_torture(
    mut kind: GeneratorKind,
    new_twist: f32,
    new_taper: f32,
    new_bend: [f32; 3],
) -> GeneratorKind {
    if let Some(t) = kind.torture_mut() {
        t.twist = Fp(new_twist);
        t.taper = Fp2([new_taper, new_taper]);
        t.bend = Fp3(new_bend);
    }
    kind
}

/// Per-axis shaping for box bodies - [`with_torture`]'s sibling for when the
/// X and Z taper differ (a cabin greenhouse that narrows more across than
/// fore-aft) or a top-shear lean is wanted. `taper` scales `[x, z]` toward the
/// top (`1 - taper·t`, so `0.0` = straight, positive draws the top in,
/// negative flares it); `bend` is the quadratic top displacement; `shear`
/// slides the top linearly in `[x, z]` (a parallelepiped, edges stay straight).
/// On an 8-corner cuboid this turns the box into a frustum / wedge / leaning
/// prism - the cheapest de-blocking deform. Non-primitive kinds pass through.
pub(crate) fn with_shape(
    mut kind: GeneratorKind,
    taper: [f32; 2],
    bend: [f32; 3],
    shear: [f32; 2],
) -> GeneratorKind {
    if let Some(t) = kind.torture_mut() {
        t.taper = Fp2(taper);
        t.bend = Fp3(bend);
        t.shear = Fp2(shear);
    }
    kind
}

/// Per-axis taper at BOTH ends - [`with_shape`]'s sibling for a form that
/// narrows toward its top AND toward its base independently. `taper` scales
/// `[x, z]` toward the top (`1 - taper·t`) and `taper_bottom` toward the base
/// (`1 - taper_bottom·(1 - t)`), and the two compose, so one prim can be a
/// frustum, a lens or a spearhead.
///
/// The armoured car's hull plates are the first caller (#1375): a Bevel with
/// one bevel segment is an octagonal prism, and these two sliders cut it to
/// the hexagonal section its [`BodyPlan`](super::skiffs::BodyPlan)
/// publishes (`taper` 0.5 over the datum and `taper_bottom` 0.5 under it),
/// so an armour plate is one node with flat faces and hard edges. Nothing in
/// the family had ever written `taper_bottom`, which is why this sits beside
/// [`with_shape`] rather than inside it: adding it there would rewrite the
/// field on every part that has ever asked for a plain taper, and those dumps
/// are pinned.
pub(crate) fn with_taper(
    mut kind: GeneratorKind,
    taper: [f32; 2],
    taper_bottom: [f32; 2],
) -> GeneratorKind {
    if let Some(t) = kind.torture_mut() {
        t.taper = Fp2(taper);
        t.taper_bottom = Fp2(taper_bottom);
    }
    kind
}

/// Stamp the SL-style topology cuts onto a swept primitive (Sphere / Cylinder /
/// Cone / Torus / Tube): `path_cut` (`[begin, end]` kept angular fraction),
/// `profile_cut` (`[begin, end]` kept latitude band - domes / bowls), and
/// `hollow` (bore fraction). Non-swept kinds pass through unchanged. Honoured
/// by the unified sweep mesher in `crate::world_builder::prim`.
pub(crate) fn with_cut(
    mut kind: GeneratorKind,
    path_cut: [f32; 2],
    profile_cut: [f32; 2],
    hollow: f32,
) -> GeneratorKind {
    if let Some(t) = kind.torture_mut() {
        t.path_cut = Fp2(path_cut);
        t.profile_cut = Fp2(profile_cut);
        t.hollow = Fp(hollow);
    }
    kind
}

// ---------------------------------------------------------------------------
// Assembler placement
// ---------------------------------------------------------------------------

/// Offset a built part to a joint anchor by adding the anchor to the part
/// root's intrinsic translation (which carries the part's own offset from its
/// attachment pivot - e.g. an arm hanging below the shoulder). Rotation and
/// scale on the part root are preserved.
pub(crate) fn offset(mut part: Generator, anchor: [f32; 3]) -> Generator {
    let t = part.transform.translation.0;
    part.transform.translation = Fp3([anchor[0] + t[0], anchor[1] + t[1], anchor[2] + t[2]]);
    part
}

/// [`offset`] plus a rotation set on the part root - for slots the assembler
/// orients (a wheel laid on its axle, an airship fin). Parts build at identity
/// rotation in their local frame, so setting it here is safe.
pub(crate) fn offset_rot(part: Generator, anchor: [f32; 3], rotation: Fp4) -> Generator {
    let mut p = offset(part, anchor);
    p.transform.rotation = rotation;
    p
}

// ---------------------------------------------------------------------------
// Connectedness (test-only)
// ---------------------------------------------------------------------------

/// Does every part of an assembled craft actually meet another one?
///
/// The owner's complaint on the roadster prototype was that "the parts are not
/// all properly connected. Some kind of axles are missing" - and the reason
/// that could be true of a design already judged by render is that **the chase
/// camera looks DOWN**, at `ORBIT_PITCH` (0.4 rad). Nothing under a craft is
/// ever in frame at play distance, so a part floating there is invisible
/// exactly where it matters most, and a zoomed sheet only catches it from the
/// elevations somebody thought to render. This answers it by arithmetic
/// instead (#1364).
///
/// It resolves every node of a tree into the root's frame, samples each one's
/// surface, and asks whether that sample is inside another node's solid. Two
/// nodes touch when either holds one of the other's samples; a craft is sound
/// when the touch graph is a **single connected component**, because a machine
/// in two halves passes a per-node check and is still not one machine.
///
/// It is deliberately approximate in one direction only: `path_cut` and
/// `hollow` are ignored when testing containment, so a half-pipe counts as the
/// whole tube. That over-counts contact for cut-away bodywork, which is the
/// safe way round here - the bodywork is what everything else is supposed to
/// touch, and the parts this exists to catch are the small ones hanging off
/// it. Spines and lathes are resampled with the same Catmull-Rom the mesher
/// uses, so a curved tube is tested where it is actually drawn.
///
/// Test-only: it exists to guard the geometry, not to ship with it.
#[cfg(test)]
/// Where two trees first disagree, as a path plus what moved - so a
/// sanitiser rewrite names the part it touched instead of printing two
/// whole machines.
pub(crate) fn first_difference(a: &Generator, b: &Generator, path: &str) -> Option<String> {
    if a.kind != b.kind {
        return Some(format!(
            "{path} ({}): kind\n  {:?}\n  {:?}",
            a.kind.kind_tag(),
            a.kind,
            b.kind
        ));
    }
    // Rotations are compared with an epsilon: the sanitiser renormalises
    // every quaternion, which moves the last ulp of an already-normalised
    // one. `quat_x(FRAC_PI_2)` is exactly that case - sin and cos of a
    // quarter turn are both 0.70710677 and the pair's norm is a hair under
    // one - where a car is nothing but rotated nodes, and a runabout has her
    // wheel, her seat backs and her pods (#1372). The SLOOP met it for the
    // first time in #1379: unkitted she still authors no rotated node and
    // round-trips bit for bit, but a Pirate's gunports lie on the skin's own
    // normal and her roger's bones are laid over. Shared by both families'
    // round-trip guards.
    let turned =
        (0..4).any(|i| (a.transform.rotation.0[i] - b.transform.rotation.0[i]).abs() > 1e-5);
    if turned
        || a.transform.translation != b.transform.translation
        || a.transform.scale != b.transform.scale
    {
        return Some(format!(
            "{path} ({}): transform {:?} -> {:?}",
            a.kind.kind_tag(),
            a.transform,
            b.transform
        ));
    }
    if a.children.len() != b.children.len() {
        return Some(format!("{path}: child count"));
    }
    a.children
        .iter()
        .zip(b.children.iter())
        .enumerate()
        .find_map(|(i, (ca, cb))| first_difference(ca, cb, &format!("{path}/{i}")))
}

/// What a vehicle DRAWS, read off its generator tree: whether every part
/// meets another ([`report`], [`assert_one_machine`]) and how far the whole
/// reaches ([`highest`], [`lowest`], [`widest`]).
///
/// A guard asks this rather than the arithmetic that placed a part, because
/// a part placed by a formula is exactly what the formula cannot check -
/// and it cannot be judged by eye either: a 2 cm gap between a strut and a
/// plate is invisible at the chase camera's distance and still a part that
/// floats.
///
/// Each node is read as the solid its mesher draws, from the mesher's own
/// code where that matters (#1393). Exact: a box; a tube's flat end caps and
/// its rings' own planes; the polygon every round prim is drawn as - a
/// cylinder, cone, torus, sweep and lathe of resolution `n` is an `n`-gon,
/// its faces as far in as `cos(pi / n)` of its radius, and a path cut is the
/// open wedge it draws - with a sweep's rings in the mesher's own stations
/// and frames; a cone tapering to its apex; and a vertex deform (taper,
/// bend, shear, twist, bulge, s-bend) through the mesher's own map and its
/// inverse - exact for a taper alone or a shear, whose faces stay flat, and
/// within a row's sag for a curving one (a bend, a bulge, a taper at both
/// ends), which the mesher draws in straight rows.
///
/// Read GENEROUSLY, so a part can seem to meet what it misses: a hollow's
/// bore is read as solid; a profile cut as the whole prim, except on a
/// sweep, whose trimmed band is read as the band it is; a torus's path cut
/// as the whole ring; a cut sphere as the whole ball; a Bevel as its box,
/// chamfer and all; a Superellipsoid as its box; a Wedge as its box, slope
/// and all; an icosphere as the round ball it is within half a percent of.
/// A bend that LIFTS a prim's top (`bend.y`) is refused outright - its
/// inverse is a root solve no craft has needed (#1393).
///
/// Two parts meet when a sample of one - a point of its drawn surface - lies
/// inside the other, or when a witness does: where two of its sweeps,
/// cylinders or cones, or a block and one of those, face each other across
/// the line their axes pass closest on (see [`meet`]). The control is
/// `touch_reads_each_prim_as_the_mesher_draws_it`, against
/// `build_primitive_mesh` itself.
#[cfg(test)]
pub(crate) mod touch {
    use std::f32::consts::{FRAC_PI_2, TAU};

    use bevy::math::{Quat, Vec2, Vec3};

    use crate::pds::generator::{Generator, GeneratorKind};
    use crate::world_builder::for_touch::{
        DEFORM_ROWS, SpineStation, Torture, deform_vertex, kept_stations, lathe_stations,
        spine_stations, torture_of, undeform_vertex,
    };

    /// Stride through a resampled centreline, a profile or a ring of rings
    /// when sampling it for contact - every station would be far more samples
    /// than the question needs. The extents read every one.
    const STRIDE: usize = 2;
    /// Bands a deformed block is sampled in for contact (its extents read the
    /// mesher's own [`DEFORM_ROWS`]): a bend curves a block between its ends,
    /// so its corners alone would miss a side it meets mid-height.
    const SAMPLE_ROWS: u32 = 4;
    /// Windows of a tube under one chunk box: a point skips a chunk whole
    /// before it is asked about any window in it (#1393).
    const CHUNK: usize = 8;

    /// The solid of one node, in its own local frame.
    enum Shape {
        /// Half-extents. Also stands in for a Bevel, a Superellipsoid and a
        /// Wedge, none of which is ever fatter than its box.
        Block(Vec3),
        Ball(f32),
        /// Axis along local Y: Bevy's cylinder, a prism on its [`Section`].
        Cylinder {
            radius: f32,
            half_h: f32,
            section: Section,
        },
        /// Bevy's cone: its [`Section`] of `radius` at `-half_h`, its apex at
        /// `+half_h`. It was read as a cylinder before #1393 - a whole base
        /// radius of phantom solid round its tip.
        Cone {
            radius: f32,
            half_h: f32,
            section: Section,
        },
        /// Bevy's torus: a `minor_sides`-gon swept round a `major_sides`-gon.
        Ring {
            minor: f32,
            major: f32,
            minor_sides: u32,
            major_sides: u32,
        },
        /// The mesher's own stations (`sweeps::spine_stations`), each drawn
        /// as its [`Section`] in its parallel-transported frame, and each
        /// window's box - the segment's own, grown by its larger radius - so
        /// a point is tested only against the windows it could be inside, and
        /// the box round each [`CHUNK`] of windows, so it skips most of them
        /// in a few comparisons (#1393: a long hull sweep's far windows were
        /// most of the containment cost).
        Tube {
            stations: Vec<SpineStation>,
            section: Section,
            boxes: Vec<(Vec3, Vec3)>,
            chunks: Vec<(Vec3, Vec3)>,
        },
        /// The mesher's own `(radius, height)` profile stations
        /// (`sweeps::lathe_stations`), revolved about local Y on its
        /// [`Section`]. `dense` when they are a smooth profile's resample,
        /// whose stations the contact samples may stride; a raw profile's
        /// are every one a corner, and the one a stride skipped was the rim
        /// a roadster's tail lamp is set into its tail by.
        Revolve {
            profile: Vec<Vec2>,
            section: Section,
            dense: bool,
        },
    }

    /// One node, resolved into the root's frame.
    struct Part {
        name: String,
        translation: Vec3,
        rotation: Quat,
        scale: Vec3,
        shape: Shape,
        /// The deform the mesher moves this node's vertices with, and the
        /// undeformed height band it reads each vertex's `t` over - `None`
        /// when it draws the shape as authored (#1393 T1).
        deform: Option<(Torture, f32, f32)>,
        samples: Vec<Vec3>,
        /// A round prim's axis, window by window, for [`meet`]'s witnesses -
        /// empty for any other node, and for a round one under a deform or a
        /// non-uniform scale, whose sections are not the polygons drawn.
        axes: Vec<Axis>,
        /// The bounds of every vertex the node is drawn through - its extents.
        lo: Vec3,
        hi: Vec3,
        /// Those bounds grown past [`Part::contains`]' tolerance and a deformed
        /// row's sag, and for a tube the same box round each [`CHUNK`] of its
        /// windows: [`holds`] turns a point away on these before carrying it
        /// into the node's frame.
        reach: (Vec3, Vec3),
        chunk_reach: Vec<(Vec3, Vec3)>,
    }

    /// A stretch of a round prim's axis in the root's frame - one window of
    /// a tube, or a cylinder's or a cone's whole length - with the radius,
    /// the section and the section's frame at each end, and its box.
    struct Axis {
        ends: [Vec3; 2],
        radii: [f32; 2],
        frames: [(Vec3, Vec3); 2],
        section: Section,
        lo: Vec3,
        hi: Vec3,
    }

    impl Part {
        /// A world point in this node's own frame.
        fn local(&self, p: Vec3) -> Vec3 {
            (self.rotation.inverse() * (p - self.translation)) / self.scale
        }

        /// A point in this node's own frame, in the world's.
        fn world(&self, q: Vec3) -> Vec3 {
            self.translation + self.rotation * (self.scale * q)
        }

        fn contains(&self, p: Vec3) -> bool {
            const EPS: f32 = 1e-4;
            let q = self.local(p);
            // A deformed node holds a point exactly when its shape as
            // authored holds the point the deform carries there.
            let q = match self.deform {
                Some((torture, y_min, y_range)) => undeform_vertex(q, y_min, y_range, torture)
                    .expect("walk refuses a deform it cannot undo"),
                None => q,
            };
            match &self.shape {
                Shape::Block(h) => {
                    q.x.abs() <= h.x + EPS && q.y.abs() <= h.y + EPS && q.z.abs() <= h.z + EPS
                }
                Shape::Ball(r) => q.length() <= r + EPS,
                Shape::Cylinder {
                    radius,
                    half_h,
                    section,
                } => q.y.abs() <= half_h + EPS && section.reach(q.x, q.z, EPS) <= radius + EPS,
                Shape::Cone {
                    radius,
                    half_h,
                    section,
                } => {
                    // Its section at any height is the base ring shrunk toward
                    // the apex: every lateral face runs from the tip to a base
                    // edge.
                    let k = ((half_h - q.y) / (2.0 * half_h)).clamp(0.0, 1.0);
                    q.y.abs() <= half_h + EPS && section.reach(q.x, q.z, EPS) <= radius * k + EPS
                }
                Shape::Ring {
                    minor,
                    major,
                    minor_sides,
                    major_sides,
                } => {
                    // Between two vertices of the major ring every minor
                    // vertex runs along a chord, so in that face's own
                    // measure - its reach against the major ring - the
                    // section is the minor ring laid at the major radius.
                    let u = Section::whole(*major_sides).reach(q.x, q.z, EPS);
                    Section::whole(*minor_sides).reach(u - major, q.y, EPS) <= minor + EPS
                }
                Shape::Tube {
                    stations,
                    section,
                    boxes,
                    chunks,
                } => {
                    let last = boxes.len().saturating_sub(1);
                    let window = |k: usize| {
                        let (sa, sb) = (&stations[k], &stations[k + 1]);
                        // Where between its two rings the point lies: on the
                        // plane through the centre, square to the tangent,
                        // both carried linearly from one ring to the next.
                        // The mesher joins the rings vertex to vertex, so
                        // that plane sweeps its wall, and a ring vertex lies
                        // on its own ring's plane and no other - where the
                        // chord's square, which a bend tilts off a ring's,
                        // read the rings of a bend half outside the tube.
                        let (dc, dt) = (sb.pos - sa.pos, sb.tangent - sa.tangent);
                        let rel = q - sa.pos;
                        let (qa, qb, qc) = (
                            -dc.dot(dt),
                            rel.dot(dt) - dc.dot(sa.tangent),
                            rel.dot(sa.tangent),
                        );
                        let disc = qb * qb - 4.0 * qa * qc;
                        let u = if qa.abs() < 1e-9 || disc < 0.0 {
                            if qb.abs() > 1e-12 { -qc / qb } else { 0.0 }
                        } else {
                            2.0 * qc / (-qb - qb.signum() * disc.sqrt())
                        };
                        // Behind this window's first ring or past its second
                        // the neighbouring window holds the point - and past
                        // a tube's first or last ring nothing does: the
                        // mesher closes a spine with FLAT caps on those rings
                        // (`sweeps.rs`). Clamped into a round cap, an end
                        // window read a whole end radius past the drawn one -
                        // over 100 mm on 13.8 % of the fleet's sweep ends,
                        // 856 mm at worst (#1393 T2b).
                        // The caps are asked whatever `u` came out as: a window
                        // too short to place a point in is no way past one.
                        let behind = qc < -EPS;
                        let beyond = (q - sb.pos).dot(sb.tangent) > EPS;
                        if (k == 0 && behind)
                            || (k == last && beyond)
                            || (u < 0.0 && behind)
                            || (u > 1.0 && beyond)
                        {
                            return false;
                        }
                        let u = u.clamp(0.0, 1.0);
                        let d = q - (sa.pos + dc * u);
                        let (n, b) = blended(
                            (sa.normal, sa.binormal, sa.radius),
                            (sb.normal, sb.binormal, sb.radius),
                            u,
                        );
                        // The section in the rings' own frames (#1393 T2a), in
                        // the units of a unit ring.
                        let r = sa.radius + (sb.radius - sa.radius) * u;
                        let tol = EPS / r.max(EPS);
                        match on_ring(d, n, b) {
                            Some((x, y)) => section.reach(x, y, tol) <= 1.0 + tol,
                            None => d.length() <= EPS,
                        }
                    };
                    // Boxes first, chunk then window: a point outside a box is
                    // outside every window under it, so the far reaches of a
                    // long sweep cost a few comparisons rather than a segment
                    // distance each.
                    let outside = |(lo, hi): &(Vec3, Vec3)| outside(q, *lo, *hi);
                    chunks.iter().enumerate().any(|(c, chunk)| {
                        !outside(chunk)
                            && (c * CHUNK..((c + 1) * CHUNK).min(boxes.len()))
                                .any(|k| !outside(&boxes[k]) && window(k))
                    })
                }
                // Point in the silhouette polygon, closed down the axis: the
                // lathe's caps are exactly that closure. Every face between two
                // profile stations is flat, so the section at any height is
                // the ring at the profile's radius there, and the point's
                // distance out is its reach. On the silhouette counts as in,
                // as on every other shape's surface: a crossing count alone
                // says nothing there, and every vertex the lathe is drawn
                // through lies on it.
                Shape::Revolve {
                    profile, section, ..
                } => {
                    let pt = Vec2::new(section.reach(q.x, q.z, EPS), q.y);
                    let mut poly: Vec<Vec2> = profile.clone();
                    poly.push(Vec2::new(0.0, profile[profile.len() - 1].y));
                    poly.push(Vec2::new(0.0, profile[0].y));
                    let mut inside = false;
                    let n = poly.len();
                    for i in 0..n {
                        let (a, b) = (poly[i], poly[(i + 1) % n]);
                        let ab = b - a;
                        let t = ((pt - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
                        if (pt - (a + ab * t)).length() <= EPS {
                            return true;
                        }
                        if (a.y > pt.y) != (b.y > pt.y) {
                            let x = a.x + (pt.y - a.y) / (b.y - a.y) * (b.x - a.x);
                            if pt.x < x {
                                inside = !inside;
                            }
                        }
                    }
                    inside
                }
            }
        }
    }

    /// How a mesher lays one ring of a round prim: `sides` faces across
    /// `span` radians from `a0` - the whole turn, or a path cut's kept arc,
    /// closed through the axis - with vertex `j` at `a0 + span * j / sides`.
    /// Every mesher starts a ring there: `sweeps.rs` from a station's normal
    /// toward its binormal, Bevy's cylinder, cone and torus and `cuts.rs`
    /// from `+X` toward `+Z`. Read round, a ring of resolution `n` stands
    /// `1 - cos(pi / n)` of its radius proud of the faces it is drawn with -
    /// half its radius at resolution 3 - and 60 % of the fleet's sweeps and
    /// 43 % of its cylinders are under resolution 8 (#1393 T2a). A path cut
    /// is part of the layout, not a trim of it: a half-pipe of three faces
    /// is half a hexagon, which no whole ring of resolution 3 draws.
    #[derive(Clone, Copy)]
    struct Section {
        a0: f32,
        span: f32,
        sides: u32,
    }

    impl Section {
        fn whole(sides: u32) -> Self {
            Self {
                a0: 0.0,
                span: TAU,
                sides,
            }
        }

        /// The ring a prim's resolution and path cut lay, the cut read as
        /// `cuts::path_cut_angles` reads it.
        fn of(sides: u32, kind: &GeneratorKind) -> Self {
            let [lo, hi] = kind.torture().map_or([0.0, 1.0], |t| t.path_cut.0);
            let (a0, span) = if hi >= lo {
                (lo * TAU, (hi - lo) * TAU)
            } else {
                (hi * TAU, (lo - hi) * TAU)
            };
            if span >= TAU - 1e-3 {
                Self::whole(sides)
            } else {
                Self { a0, span, sides }
            }
        }

        fn is_whole(&self) -> bool {
            self.span >= TAU - 1e-3
        }

        /// The circumradius of the ring whose boundary passes through
        /// `(x, y)`: a point lies inside the ring of circumradius `r` exactly
        /// when this is at most `r`, and the round reading, `hypot(x, y)`, is
        /// this with infinitely many sides. Infinite in a path cut's open
        /// wedge, except within `eps` of either cut face, which bounds it.
        fn reach(&self, x: f32, y: f32, eps: f32) -> f32 {
            let step = self.span / self.sides as f32;
            let mut phi = (y.atan2(x) - self.a0).rem_euclid(TAU);
            if phi > self.span {
                let (past, back) = (phi - self.span, TAU - phi);
                if x.hypot(y) * past.min(back).min(FRAC_PI_2).sin() > eps {
                    return f32::INFINITY;
                }
                phi = if past < back { self.span } else { 0.0 };
            }
            let face = self.a0 + ((phi / step).floor().min(self.sides as f32 - 1.0) + 0.5) * step;
            (x * face.cos() + y * face.sin()) / (step * 0.5).cos()
        }

        /// Every vertex the ring is drawn through, as `(cos, sin)` on the unit
        /// ring: the whole ring's `sides`, or a cut ring's `sides + 1` and the
        /// axis its cut faces run to.
        fn corners(&self) -> Vec<(f32, f32)> {
            let last = if self.is_whole() {
                self.sides - 1
            } else {
                self.sides
            };
            let mut out: Vec<(f32, f32)> = (0..=last)
                .map(|j| {
                    let (s, c) = (self.a0 + self.span * (j as f32 / self.sides as f32)).sin_cos();
                    (c, s)
                })
                .collect();
            if !self.is_whole() {
                out.push((0.0, 0.0));
            }
            out
        }
    }

    /// The ring a window draws at `u` between two rings, each given as its
    /// frame's normal and binormal and its radius: the images of the unit
    /// ring's two axes. The mesher joins two rings vertex to vertex, so the
    /// ring between them is their linear blend, radius and frame together and
    /// never renormalised - a bent window's blended axes are SHORTER than
    /// either ring's, and its wall stands that much in from the radius.
    fn blended(a: (Vec3, Vec3, f32), b: (Vec3, Vec3, f32), u: f32) -> (Vec3, Vec3) {
        (
            (a.0 * a.2).lerp(b.0 * b.2, u),
            (a.1 * a.2).lerp(b.1 * b.2, u),
        )
    }

    /// The coordinates `(x, y)` that put `x * n + y * b` nearest `d`, on the
    /// ring whose axes are `n` and `b` - `None` for a ring collapsed to a point.
    fn on_ring(d: Vec3, n: Vec3, b: Vec3) -> Option<(f32, f32)> {
        let (nn, nb, bb) = (n.dot(n), n.dot(b), b.dot(b));
        let det = nn * bb - nb * nb;
        if det <= 1e-12 {
            return None;
        }
        let (dn, db) = (d.dot(n), d.dot(b));
        Some(((dn * bb - db * nb) / det, (db * nn - dn * nb) / det))
    }

    /// The points a shape is drawn through, in its local frame: every ring
    /// vertex of every `stride`-th station, profile station or major vertex
    /// (and the last station, so a tube's far cap is always read); a block's
    /// corners at `rows + 1` heights, its side faces' centres between them
    /// and its end faces' centres; a cylinder's or a cone's rings at `rows +
    /// 1` heights; a ball's six poles.
    fn vertices(shape: &Shape, stride: usize, rows: u32) -> Vec<Vec3> {
        let ring =
            |c: Vec3, u: Vec3, v: Vec3, r: f32, corners: &[(f32, f32)], out: &mut Vec<Vec3>| {
                for &(k, s) in corners {
                    out.push(c + (u * k + v * s) * r);
                }
            };
        let taken = |i: usize, n: usize| i.is_multiple_of(stride) || i + 1 == n;
        let mut out = Vec::new();
        match shape {
            Shape::Block(h) => {
                let at = |k: f32| -h.y + 2.0 * h.y * k / rows as f32;
                for k in 0..=rows {
                    for (sx, sz) in [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
                        out.push(Vec3::new(sx * h.x, at(k as f32), sz * h.z));
                    }
                }
                for k in 0..rows {
                    let y = at(k as f32 + 0.5);
                    for (x, z) in [(h.x, 0.0), (-h.x, 0.0), (0.0, h.z), (0.0, -h.z)] {
                        out.push(Vec3::new(x, y, z));
                    }
                }
                out.push(Vec3::Y * h.y);
                out.push(-Vec3::Y * h.y);
            }
            Shape::Ball(r) => {
                for a in [Vec3::X, Vec3::Y, Vec3::Z] {
                    out.push(a * *r);
                    out.push(-a * *r);
                }
            }
            // In `rows` bands when deformed, as their meshers draw them, so a
            // bulge or an s-bend reaches the extents.
            Shape::Cylinder {
                radius,
                half_h,
                section,
            } => {
                let corners = section.corners();
                for k in 0..=rows {
                    let y = -half_h + 2.0 * half_h * k as f32 / rows as f32;
                    ring(Vec3::Y * y, Vec3::X, Vec3::Z, *radius, &corners, &mut out);
                }
            }
            Shape::Cone {
                radius,
                half_h,
                section,
            } => {
                let corners = section.corners();
                for k in 0..rows {
                    let f = k as f32 / rows as f32;
                    let y = -half_h + 2.0 * half_h * f;
                    ring(
                        Vec3::Y * y,
                        Vec3::X,
                        Vec3::Z,
                        radius * (1.0 - f),
                        &corners,
                        &mut out,
                    );
                }
                out.push(Vec3::Y * *half_h);
            }
            Shape::Ring {
                minor,
                major,
                minor_sides,
                major_sides,
            } => {
                let minor_ring = Section::whole(*minor_sides).corners();
                for (j, (k, s)) in Section::whole(*major_sides)
                    .corners()
                    .into_iter()
                    .enumerate()
                {
                    if taken(j, *major_sides as usize) {
                        let dir = Vec3::new(k, 0.0, s);
                        ring(dir * *major, dir, Vec3::Y, *minor, &minor_ring, &mut out);
                    }
                }
            }
            Shape::Tube {
                stations, section, ..
            } => {
                let corners = section.corners();
                for (i, st) in stations.iter().enumerate() {
                    if taken(i, stations.len()) {
                        ring(
                            st.pos,
                            st.normal,
                            st.binormal,
                            st.radius,
                            &corners,
                            &mut out,
                        );
                    }
                }
            }
            Shape::Revolve {
                profile,
                section,
                dense,
            } => {
                let corners = section.corners();
                for (i, s) in profile.iter().enumerate() {
                    if !dense || taken(i, profile.len()) {
                        ring(Vec3::Y * s.y, Vec3::X, Vec3::Z, s.x, &corners, &mut out);
                    }
                }
            }
        }
        out
    }

    fn shape_of(kind: &GeneratorKind) -> Shape {
        match kind {
            GeneratorKind::Cuboid { size, .. } | GeneratorKind::Bevel { size, .. } => {
                Shape::Block(Vec3::from(size.0) * 0.5)
            }
            GeneratorKind::Wedge { size, .. } => Shape::Block(Vec3::from(size.0) * 0.5),
            GeneratorKind::Superellipsoid { half_extents, .. } => {
                Shape::Block(Vec3::from(half_extents.0))
            }
            GeneratorKind::Sphere { radius, .. } => Shape::Ball(radius.0),
            GeneratorKind::Cylinder {
                radius,
                height,
                resolution,
                ..
            } => Shape::Cylinder {
                radius: radius.0,
                half_h: height.0 * 0.5,
                section: Section::of((*resolution).max(3), kind),
            },
            GeneratorKind::Cone {
                radius,
                height,
                resolution,
                ..
            } => Shape::Cone {
                radius: radius.0,
                half_h: height.0 * 0.5,
                section: Section::of((*resolution).max(3), kind),
            },
            GeneratorKind::Torus {
                minor_radius,
                major_radius,
                minor_resolution,
                major_resolution,
                ..
            } => Shape::Ring {
                minor: minor_radius.0,
                major: major_radius.0,
                minor_sides: (*minor_resolution).max(3),
                major_sides: (*major_resolution).max(3),
            },
            GeneratorKind::Spine {
                points,
                resolution,
                samples_per_segment,
                ..
            } => {
                // Only the stations the mesher keeps under the path trim: a
                // sail drawn in bands is a band each, not the whole sail.
                let all = spine_stations(
                    &points
                        .iter()
                        .map(|p| (Vec3::from(p.position.0), p.radius.0))
                        .collect::<Vec<_>>(),
                    *samples_per_segment,
                );
                let [t0, t1] = kind.torture().map_or([0.0, 1.0], |t| t.profile_cut.0);
                let kept = kept_stations(&all, t0, t1);
                let stations: Vec<SpineStation> = all
                    .into_iter()
                    .skip(*kept.start())
                    .take(kept.end() - kept.start() + 1)
                    .collect();
                // Grown by the contains test's own EPS too, so the box never
                // turns away a point the test would take.
                let boxes: Vec<(Vec3, Vec3)> = stations
                    .windows(2)
                    .map(|w| {
                        let reach = Vec3::splat(w[0].radius.max(w[1].radius) + 1e-3);
                        (
                            w[0].pos.min(w[1].pos) - reach,
                            w[0].pos.max(w[1].pos) + reach,
                        )
                    })
                    .collect();
                let chunks = boxes
                    .chunks(CHUNK)
                    .map(|run| {
                        run.iter()
                            .fold(run[0], |(lo, hi), (l, h)| (lo.min(*l), hi.max(*h)))
                    })
                    .collect();
                Shape::Tube {
                    stations,
                    section: Section::of((*resolution).clamp(3, 64), kind),
                    boxes,
                    chunks,
                }
            }
            GeneratorKind::Lathe {
                points,
                resolution,
                smooth,
                ..
            } => Shape::Revolve {
                profile: lathe_stations(
                    &points
                        .iter()
                        .map(|p| (p.radius.0, p.height.0))
                        .collect::<Vec<_>>(),
                    *smooth,
                ),
                section: Section::of((*resolution).clamp(3, 128), kind),
                dense: *smooth,
            },
            other => panic!(
                "touch: no solid for {} - teach this helper the prim before \
                 a craft uses it",
                other.kind_tag()
            ),
        }
    }

    /// `node` and every node under it, resolved into the root's frame - with
    /// what [`meet`] asks of a node (its samples, its chunk boxes, its axes)
    /// only when `contact` wants it: the extents read nothing but a node's
    /// vertices, and the air-draft and gateway sweeps read nothing but those.
    fn walk(
        node: &Generator,
        t: Vec3,
        r: Quat,
        s: Vec3,
        path: String,
        out: &mut Vec<Part>,
        contact: bool,
    ) {
        let lt = Vec3::from(node.transform.translation.0);
        let lr = Quat::from_array(node.transform.rotation.0).normalize();
        let ls = Vec3::from(node.transform.scale.0);
        let wt = t + r * (s * lt);
        let wr = r * lr;
        let ws = s * ls;
        debug_assert!(
            (ws.x - ws.y).abs() < 1e-4 && (ws.y - ws.z).abs() < 1e-4 || lr.is_near_identity(),
            "{path}: a non-uniform scale under a rotation is not a similarity, \
             so this helper cannot resolve it exactly"
        );
        let shape = shape_of(&node.kind);
        // The mesher deforms a prim over the height band its undeformed
        // vertices span (`torture::apply_vertex_torture`).
        let torture = torture_of(&node.kind);
        let deform = (!torture.is_identity()).then(|| {
            let (y_min, y_max) = vertices(&shape, 1, 1)
                .iter()
                .fold((f32::MAX, f32::MIN), |(lo, hi), p| {
                    (lo.min(p.y), hi.max(p.y))
                });
            let y_range = (y_max - y_min).max(1e-6);
            assert!(
                undeform_vertex(Vec3::ZERO, y_min, y_range, torture).is_some(),
                "{path}: touch cannot undo a bend that lifts a prim's top \
                 (bend.y) - teach it the root solve before a craft authors one"
            );
            assert!(
                !matches!(shape, Shape::Ball(_)),
                "{path}: touch reads a ball by its six poles, which a deform \
                 does not carry to its extents - teach it the icosphere first"
            );
            (torture, y_min, y_range)
        });
        let to_root = |p: Vec3| {
            let p = match deform {
                Some((torture, y_min, y_range)) => deform_vertex(p, y_min, y_range, torture),
                None => p,
            };
            wt + wr * (ws * p)
        };
        let (rows, sample_rows) = match deform {
            Some(_) => (DEFORM_ROWS, SAMPLE_ROWS),
            None => (1, 1),
        };
        // The extents read every vertex the mesher draws, not the samples:
        // a polygon reaches furthest at a vertex, and the station a sweep
        // reaches highest at need not be one the contact samples take.
        let outline: Vec<Vec3> = match (&shape, deform) {
            // Undeformed, a tube's rings are carried into the root's frame by
            // their stations: the frame is affine, so a ring point is its
            // station's carried centre plus its carried axes, three
            // transforms a station rather than one a vertex - most of the
            // walk's cost in an unoptimised build (#1393).
            (
                Shape::Tube {
                    stations, section, ..
                },
                None,
            ) => {
                let corners = section.corners();
                let carry = |v: Vec3| wr * (ws * v);
                let mut out = Vec::with_capacity(stations.len() * corners.len());
                for st in stations {
                    let (c, n, b) = (
                        wt + carry(st.pos),
                        carry(st.normal * st.radius),
                        carry(st.binormal * st.radius),
                    );
                    out.extend(corners.iter().map(|&(k, s)| c + n * k + b * s));
                }
                out
            }
            _ => vertices(&shape, 1, rows).into_iter().map(to_root).collect(),
        };
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for &p in &outline {
            lo = lo.min(p);
            hi = hi.max(p);
        }
        // A tube's outline is its rings in station order, so its samples -
        // every `STRIDE`-th ring and the last - and the box round each chunk
        // of windows are read off it rather than built again. A window is the
        // solid between two rings, inside the hull of their vertices, so the
        // box round a chunk's rings bounds the chunk.
        const MARGIN: Vec3 = Vec3::splat(1e-3);
        let (samples, chunk_reach) = match &shape {
            _ if !contact => (Vec::new(), Vec::new()),
            Shape::Tube {
                stations, section, ..
            } => {
                let m = section.corners().len();
                let rings: Vec<&[Vec3]> = outline.chunks(m).collect();
                debug_assert_eq!(rings.len(), stations.len(), "{path}: a ring per station");
                let samples = rings
                    .iter()
                    .enumerate()
                    .filter(|&(i, _)| i.is_multiple_of(STRIDE) || i + 1 == rings.len())
                    .flat_map(|(_, r)| r.iter().copied())
                    .collect();
                let chunks = (0..rings.len() - 1)
                    .collect::<Vec<_>>()
                    .chunks(CHUNK)
                    .map(|run| {
                        let (mut clo, mut chi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
                        for &k in run {
                            for &p in rings[k].iter().chain(rings[k + 1]) {
                                clo = clo.min(p);
                                chi = chi.max(p);
                            }
                        }
                        (clo - MARGIN, chi + MARGIN)
                    })
                    .collect();
                (samples, chunks)
            }
            _ => (
                vertices(&shape, STRIDE, sample_rows)
                    .into_iter()
                    .map(to_root)
                    .collect(),
                Vec::new(),
            ),
        };
        let uniform = (ws.x - ws.y).abs() < 1e-4 && (ws.y - ws.z).abs() < 1e-4;
        let axes = match deform {
            None if uniform && contact => axes_of(&shape, |p| wt + wr * (ws * p), wr, ws.x),
            _ => Vec::new(),
        };
        out.push(Part {
            name: format!("{path}:{}", node.kind.kind_tag()),
            translation: wt,
            rotation: wr,
            scale: ws,
            shape,
            deform,
            samples,
            axes,
            lo,
            hi,
            reach: (lo - MARGIN, hi + MARGIN),
            chunk_reach,
        });
        for (i, child) in node.children.iter().enumerate() {
            walk(child, wt, wr, ws, format!("{path}/{i}"), out, contact);
        }
    }

    /// The axis of a round shape, window by window, carried into the root's
    /// frame by `to_root`, its rotation `rot` and its uniform scale `s`.
    fn axes_of(shape: &Shape, to_root: impl Fn(Vec3) -> Vec3, rot: Quat, s: f32) -> Vec<Axis> {
        let axis =
            |ends: [Vec3; 2], radii: [f32; 2], frames: [(Vec3, Vec3); 2], section: Section| {
                let reach = Vec3::splat(radii[0].max(radii[1]) + 1e-3);
                Axis {
                    ends,
                    radii,
                    frames,
                    section,
                    lo: ends[0].min(ends[1]) - reach,
                    hi: ends[0].max(ends[1]) + reach,
                }
            };
        let upright = (rot * Vec3::X, rot * Vec3::Z);
        match shape {
            Shape::Cylinder {
                radius,
                half_h,
                section,
            } => vec![axis(
                [to_root(-Vec3::Y * *half_h), to_root(Vec3::Y * *half_h)],
                [radius * s, radius * s],
                [upright, upright],
                *section,
            )],
            Shape::Cone {
                radius,
                half_h,
                section,
            } => vec![axis(
                [to_root(-Vec3::Y * *half_h), to_root(Vec3::Y * *half_h)],
                [radius * s, 0.0],
                [upright, upright],
                *section,
            )],
            Shape::Tube {
                stations, section, ..
            } => stations
                .windows(2)
                .map(|w| {
                    axis(
                        [to_root(w[0].pos), to_root(w[1].pos)],
                        [w[0].radius * s, w[1].radius * s],
                        [
                            (rot * w[0].normal, rot * w[0].binormal),
                            (rot * w[1].normal, rot * w[1].binormal),
                        ],
                        *section,
                    )
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Where two segments pass closest, as each one's parameter along
    /// itself (Ericson, Real-Time Collision Detection 5.1.9).
    fn closest_params([p1, q1]: [Vec3; 2], [p2, q2]: [Vec3; 2]) -> (f32, f32) {
        let (d1, d2, r) = (q1 - p1, q2 - p2, p1 - p2);
        let (a, e, f) = (d1.dot(d1), d2.dot(d2), d2.dot(r));
        if a <= 1e-12 && e <= 1e-12 {
            return (0.0, 0.0);
        }
        if a <= 1e-12 {
            return (0.0, (f / e).clamp(0.0, 1.0));
        }
        let c = d1.dot(r);
        if e <= 1e-12 {
            return ((-c / a).clamp(0.0, 1.0), 0.0);
        }
        let b = d1.dot(d2);
        let denom = a * e - b * b;
        let mut s = if denom > 1e-12 {
            ((b * f - c * e) / denom).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mut t = (b * s + f) / e;
        if t < 0.0 {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else if t > 1.0 {
            t = 1.0;
            s = ((b - c) / a).clamp(0.0, 1.0);
        }
        (s, t)
    }

    /// The point of `w`'s drawn surface at `s` along it, out from its axis
    /// toward `toward` taken in the section's own plane - `None` when that
    /// direction runs along the axis or into a path cut's open wedge.
    fn facing(w: &Axis, s: f32, toward: Vec3) -> Option<Vec3> {
        let (n, b) = blended(
            (w.frames[0].0, w.frames[0].1, w.radii[0]),
            (w.frames[1].0, w.frames[1].1, w.radii[1]),
            s,
        );
        let (x, y) = on_ring(toward, n, b)?;
        let reach = w.section.reach(x, y, 0.0);
        if !reach.is_finite() || reach < 1e-9 {
            return None;
        }
        Some(w.ends[0].lerp(w.ends[1], s) + (n * x + b * y) / reach)
    }

    /// Whether `q` lies outside the box `lo..hi`. Spelt out lane by lane: in
    /// an unoptimised build - CI's `cargo test` is one - glam's lane compares
    /// are function calls of their own, and box tests were the largest part
    /// of the vehicle sweeps' cost there (#1393).
    fn outside(q: Vec3, lo: Vec3, hi: Vec3) -> bool {
        q.x < lo.x || q.y < lo.y || q.z < lo.z || q.x > hi.x || q.y > hi.y || q.z > hi.z
    }

    /// Whether two boxes are disjoint, spelt out as [`outside`] is.
    fn apart(a_lo: Vec3, a_hi: Vec3, b_lo: Vec3, b_hi: Vec3) -> bool {
        a_lo.x > b_hi.x
            || a_lo.y > b_hi.y
            || a_lo.z > b_hi.z
            || b_lo.x > a_hi.x
            || b_lo.y > a_hi.y
            || b_lo.z > a_hi.z
    }

    /// Whether `part` holds `p`, a point of another node: first against the
    /// box round every vertex `part` is drawn through and, for a tube, round
    /// each chunk of its windows (both grown past [`Part::contains`]' own
    /// tolerance and a deformed row's sag), which turn away most of another
    /// node's samples before any of them is carried into `part`'s frame.
    fn holds(part: &Part, p: Vec3) -> bool {
        if outside(p, part.reach.0, part.reach.1) {
            return false;
        }
        if !part.chunk_reach.is_empty()
            && part.chunk_reach.iter().all(|&(lo, hi)| outside(p, lo, hi))
        {
            return false;
        }
        part.contains(p)
    }

    /// Whether two nodes meet: a sample of either inside the other, or a
    /// witness where they face each other.
    ///
    /// The samples are vertices, and two solids can overlap by millimetres
    /// with no vertex of either inside the other - a junk's batten crossing
    /// her mast, both drawn as polygons, overlaps in a lens no ring vertex
    /// reaches (#1393: it read as a second piece of the junk once the round
    /// tubes, which put a sample there by luck, were gone). So two sweeps,
    /// cylinders or cones are also asked where their axes pass closest,
    /// window against window: the point of each one's surface facing the
    /// other along that line, which the overlap holds if the polygons overlap
    /// there at all. And a block against one of those, at the axis point
    /// nearest the block: the block's own point nearest it, and the surface
    /// facing the block. A witness is a point ON one drawn surface, so it
    /// can find only contact that is there. A torus and a lathe carry no
    /// axis, and meet by their samples alone.
    fn meet(a: &Part, b: &Part) -> bool {
        if a.samples.iter().any(|&p| holds(b, p)) || b.samples.iter().any(|&p| holds(a, p)) {
            return true;
        }
        // Only a window inside the box the two parts share can face the other.
        let (o_lo, o_hi) = (a.reach.0.max(b.reach.0), a.reach.1.min(b.reach.1));
        let near = |axes: &'_ [Axis]| -> Vec<usize> {
            (0..axes.len())
                .filter(|&k| !apart(axes[k].lo, axes[k].hi, o_lo, o_hi))
                .collect()
        };
        let (near_a, near_b) = (near(&a.axes), near(&b.axes));
        for wa in near_a.iter().map(|&k| &a.axes[k]) {
            for wb in near_b.iter().map(|&k| &b.axes[k]) {
                if apart(wa.lo, wa.hi, wb.lo, wb.hi) {
                    continue;
                }
                let (s, t) = closest_params(wa.ends, wb.ends);
                let (ca, cb) = (
                    wa.ends[0].lerp(wa.ends[1], s),
                    wb.ends[0].lerp(wb.ends[1], t),
                );
                if facing(wa, s, cb - ca).is_some_and(|p| holds(b, p))
                    || facing(wb, t, ca - cb).is_some_and(|p| holds(a, p))
                {
                    return true;
                }
            }
        }
        block_meets_axes(a, b) || block_meets_axes(b, a)
    }

    /// [`meet`]'s block witness: at the point of each of `other`'s windows
    /// nearest `block`'s centre, the block's own point nearest it - found in
    /// the block as authored, so a deformed block is asked at a point on its
    /// drawn surface - and the window's surface facing that point.
    fn block_meets_axes(block: &Part, other: &Part) -> bool {
        let Shape::Block(h) = block.shape else {
            return false;
        };
        other.axes.iter().any(|w| {
            if apart(w.lo, w.hi, block.lo, block.hi) {
                return false;
            }
            let (s, _) = closest_params(w.ends, [block.translation; 2]);
            let c = w.ends[0].lerp(w.ends[1], s);
            let near = match block.deform {
                Some((torture, y_min, y_range)) => {
                    let q = undeform_vertex(block.local(c), y_min, y_range, torture)
                        .expect("walk refuses a deform it cannot undo");
                    deform_vertex(q.clamp(-h, h), y_min, y_range, torture)
                }
                None => block.local(c).clamp(-h, h),
            };
            let p = block.world(near);
            holds(other, p) || facing(w, s, p - c).is_some_and(|f| holds(block, f))
        })
    }

    /// Every node of `root`, resolved into its frame - ready for [`meet`]
    /// when `contact` asks.
    fn parts_of(root: &Generator, contact: bool) -> Vec<Part> {
        let mut parts = Vec::new();
        walk(
            root,
            Vec3::ZERO,
            Quat::IDENTITY,
            Vec3::ONE,
            "0".to_string(),
            &mut parts,
            contact,
        );
        parts
    }

    /// What the touch graph of `root` looks like: the number of connected
    /// components, and the name of every node that meets nothing at all.
    pub(crate) fn report(root: &Generator) -> (usize, Vec<String>) {
        let parts = parts_of(root, true);
        let n = parts.len();
        let mut parent: Vec<usize> = (0..n).collect();
        fn find(parent: &mut [usize], mut a: usize) -> usize {
            while parent[a] != a {
                parent[a] = parent[parent[a]];
                a = parent[a];
            }
            a
        }
        let mut met = vec![false; n];
        for i in 0..n {
            for j in (i + 1)..n {
                // A pair that can change neither answer is not tested (#1393):
                // both of its parts already meet something, and they are
                // already one piece, so their touching would join nothing and
                // mark nothing. The test it skips is the costly one - every
                // sample of one part against the other's solid, which was
                // nearly all of the boats' one-machine sweep (profiled at
                // #1393: the tube containment loop, not this pairwise scan,
                // so a spatial index over the boxes would have bought little)
                // - and the answer is the same: a pair already in one set
                // unions nothing, and a part is never skipped before it meets
                // something.
                if met[i] && met[j] && find(&mut parent, i) == find(&mut parent, j) {
                    continue;
                }
                // Cheap reject first: nothing can touch across disjoint boxes.
                let (a, b) = (&parts[i], &parts[j]);
                if apart(a.lo, a.hi, b.lo, b.hi) {
                    continue;
                }
                if meet(a, b) {
                    met[i] = true;
                    met[j] = true;
                    let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                    parent[ri] = rj;
                }
            }
        }
        let mut roots: Vec<usize> = (0..n).map(|i| find(&mut parent, i)).collect();
        roots.sort_unstable();
        roots.dedup();
        let loose = (0..n)
            .filter(|&i| !met[i])
            .map(|i| parts[i].name.clone())
            .collect();
        (roots.len(), loose)
    }

    /// The highest point `root` draws, in its own frame (m): the top of every
    /// vertex its nodes are drawn through.
    ///
    /// What an air-draft guard checks a DRAWN rig against, rather than the
    /// arithmetic that placed it - a spar standing over the height its rig
    /// was resolved to is exactly what a derivation cannot see (#1366). What
    /// is still read whole - a hollow, a lathe's, a cylinder's or a sphere's
    /// profile cut, a torus's or a sphere's path cut - only over-reads a
    /// height; a deform is read through the mesher's own map (#1393 T1).
    pub(crate) fn highest(root: &Generator) -> f32 {
        parts_of(root, false)
            .iter()
            .map(|p| p.hi.y)
            .fold(f32::MIN, f32::max)
    }

    /// The lowest point `root` draws, in its own frame (m): [`highest`]'s
    /// mirror. A low-resolution hull sweep is read as the polygon it is
    /// drawn as (#1393 T2a), so a res-3 sweep's bottom is where its lowest
    /// vertex lies, not a round tube's radius under its centreline.
    pub(crate) fn lowest(root: &Generator) -> f32 {
        parts_of(root, false)
            .iter()
            .map(|p| p.lo.y)
            .fold(f32::MAX, f32::min)
    }

    /// How wide `root` draws, in its own frame (m): the full `x` extent of
    /// every vertex its nodes are drawn through.
    ///
    /// [`highest`]'s sideways twin. What it still reads generously - the cuts
    /// [`highest`] names, a Bevel's chamfer, a Superellipsoid's or a Wedge's
    /// box - reads WIDER than the craft is drawn, so a gateway-mouth guard
    /// written against this can only be pessimistic.
    pub(crate) fn widest(root: &Generator) -> f32 {
        let parts = parts_of(root, false);
        let hi = parts.iter().map(|p| p.hi.x).fold(f32::MIN, f32::max);
        let lo = parts.iter().map(|p| p.lo.x).fold(f32::MAX, f32::min);
        hi - lo
    }

    /// Assert that `root` is one machine: every node meets another and the
    /// whole tree is a single connected component.
    pub(crate) fn assert_one_machine(root: &Generator, what: &str) {
        let (components, loose) = report(root);
        assert!(
            loose.is_empty(),
            "{what}: {} part(s) touch nothing at all: {loose:?}",
            loose.len()
        );
        assert_eq!(
            components, 1,
            "{what}: the craft is in {components} pieces - every part meets \
             something, but not all of them meet each other"
        );
    }

    /// What `report` answers on trees small enough to read, and on real
    /// craft: the pieces and the loose parts a guard is told about come out
    /// of the skip, the boxes and the prefilters (#1393) exactly as every
    /// pair asked the costly way gives them.
    mod tests {
        use super::super::{cuboid, id_quat, prim, spine};
        use super::report;
        use crate::pds::SovereignMaterialSettings;
        use crate::pds::generator::Generator;

        fn block(size: f32, at: [f32; 3]) -> Generator {
            prim(
                cuboid([size; 3], SovereignMaterialSettings::default()),
                at,
                id_quat(),
            )
        }

        fn with(mut root: Generator, children: Vec<Generator>) -> Generator {
            root.children = children;
            root
        }

        /// The far block touches nothing; the root keeps its neighbour, so
        /// only the far block is loose (with no neighbour the root would be
        /// loose too - "meets nothing at all" is said of every such node).
        #[test]
        fn a_part_touching_nothing_is_loose_and_a_piece_of_its_own() {
            let tree = with(
                block(1.0, [0.0; 3]),
                vec![block(0.5, [0.75, 0.0, 0.0]), block(0.5, [5.0, 0.0, 0.0])],
            );
            assert_eq!(report(&tree), (2, vec!["0/1:Cuboid".to_string()]));
        }

        #[test]
        fn two_groups_that_touch_within_are_two_pieces_and_nothing_loose() {
            let tree = with(
                block(1.0, [0.0; 3]),
                vec![
                    block(0.5, [0.75, 0.0, 0.0]),
                    block(1.0, [10.0, 0.0, 0.0]),
                    block(0.5, [10.75, 0.0, 0.0]),
                ],
            );
            assert_eq!(report(&tree), (2, Vec::<String>::new()));
        }

        /// The far block meets only the middle one: the root and the far
        /// block never touch, so one piece is the skip keeping every pair
        /// that could still join something.
        #[test]
        fn a_chain_joined_only_through_its_middle_is_one_piece() {
            let tree = with(
                block(1.0, [0.0; 3]),
                vec![block(1.0, [1.0, 0.0, 0.0]), block(1.0, [2.0, 0.0, 0.0])],
            );
            assert_eq!(report(&tree), (1, Vec::<String>::new()));
        }

        /// A tube ends at a FLAT cap square to its end station, as the mesher
        /// draws it (#1393 T2b): a point 0.1 m past either cap's plane is
        /// outside, though the old reading - each end clamped into a round
        /// cap - took anything within the end radius of the end point. Asked
        /// of `contains` itself: in `report` the box test already turns away
        /// a part lying wholly past a cap (the tube's box ends at its end
        /// ring), so the round cap only ever reached a part straddling the
        /// cap's plane, and no guard in the fleet moved when it went.
        #[test]
        fn a_tube_ends_at_its_flat_caps() {
            use bevy::math::{Quat, Vec3};
            let tube = prim(
                spine(
                    &[([0.0, 0.0, 0.0], 0.3), ([5.0, 0.0, 0.0], 0.3)],
                    8,
                    SovereignMaterialSettings::default(),
                ),
                [0.0; 3],
                id_quat(),
            );
            let mut parts = Vec::new();
            super::walk(
                &tube,
                Vec3::ZERO,
                Quat::IDENTITY,
                Vec3::ONE,
                "0".into(),
                &mut parts,
                true,
            );
            let inside = |x: f32| parts[0].contains(Vec3::new(x, 0.1, 0.0));
            assert!(inside(4.9) && inside(0.1), "inside, by either end");
            assert!(inside(5.0) && inside(0.0), "on either cap's plane");
            assert!(!inside(5.1), "past the far cap");
            assert!(!inside(-0.1), "past the near cap");
        }

        /// A tube forty points long resamples into dozens of chunks; a block
        /// at its far end is met through the last of them, and one a metre
        /// off it is met by none.
        #[test]
        fn a_long_tube_is_met_at_its_far_end_and_nowhere_off_it() {
            let points: Vec<([f32; 3], f32)> =
                (0..40).map(|i| ([i as f32 * 0.5, 0.0, 0.0], 0.2)).collect();
            let tube = || {
                prim(
                    spine(&points, 8, SovereignMaterialSettings::default()),
                    [0.0; 3],
                    id_quat(),
                )
            };
            let met = with(tube(), vec![block(0.3, [19.5, 0.3, 0.0])]);
            assert_eq!(report(&met), (1, Vec::<String>::new()));
            let off = with(tube(), vec![block(0.3, [19.5, 1.0, 0.0])]);
            assert_eq!(report(&off).0, 2);
        }

        /// A ring's reach on numbers that can be checked by hand (#1393
        /// T2a): a whole res-3 ring reaches its vertices and its faces'
        /// midpoints alike - half its radius out, where the round reading
        /// saw a whole radius - and a half-pipe of three faces is half a
        /// hexagon, open above its cut faces but closed on them.
        #[test]
        fn a_ring_reaches_its_polygon_and_a_cut_ring_its_wedge() {
            use super::Section;
            use std::f32::consts::{FRAC_PI_3, PI};
            let close = |a: f32, b: f32| (a - b).abs() < 1e-5;
            let tri = Section::whole(3);
            assert!(close(tri.reach(1.0, 0.0, 0.0), 1.0), "a vertex");
            let (s, c) = FRAC_PI_3.sin_cos();
            assert!(
                close(tri.reach(0.5 * c, 0.5 * s, 0.0), 1.0),
                "a face's midpoint"
            );
            assert!(tri.reach(0.6 * c, 0.6 * s, 0.0) > 1.0, "past that face");
            let gutter = Section {
                a0: PI,
                span: PI,
                sides: 3,
            };
            let apothem = (PI / 6.0).cos();
            assert!(
                close(gutter.reach(0.0, -apothem, 0.0), 1.0),
                "its bottom face"
            );
            assert!(close(gutter.reach(-1.0, 0.0, 0.0), 1.0), "a cut vertex");
            assert!(gutter.reach(0.0, 0.5, 1e-4).is_infinite(), "the open top");
            assert!(gutter.reach(0.5, 5e-5, 1e-4).is_finite(), "on a cut face");
        }

        /// A cone tapers to its apex: near the tip, a point at most of the
        /// base radius is outside it, where the cylinder it used to be read
        /// as held it.
        #[test]
        fn a_cone_is_read_to_its_apex() {
            use super::super::cone;
            use bevy::math::{Quat, Vec3};
            let tip = prim(
                cone(0.5, 1.0, 12, SovereignMaterialSettings::default()),
                [0.0; 3],
                id_quat(),
            );
            let mut parts = Vec::new();
            super::walk(
                &tip,
                Vec3::ZERO,
                Quat::IDENTITY,
                Vec3::ONE,
                "0".into(),
                &mut parts,
                true,
            );
            assert!(parts[0].contains(Vec3::new(0.4, -0.45, 0.0)), "by its base");
            assert!(parts[0].contains(Vec3::new(0.02, 0.45, 0.0)), "by its tip");
            assert!(
                !parts[0].contains(Vec3::new(0.4, 0.4, 0.0)),
                "round the tip"
            );
        }

        /// Touch reads each prim the way the real mesher draws it - the
        /// control #1393 asks for, against `build_primitive_mesh` itself.
        /// Every vertex the mesher draws is inside touch's solid; touch's
        /// extents are the vertices' own bounds; and every face's centre
        /// stood 2 mm out along the face's outward normal is OUTSIDE - which
        /// the round reading failed on every low-resolution face, the chord
        /// windows on a bend's rings, and the undeformed one on every face a
        /// deform moved. Only the Bevel is spared the last two: its chamfer
        /// is read as its box, as the module says.
        #[test]
        fn touch_reads_each_prim_as_the_mesher_draws_it() {
            use super::super::{
                bevel, cone, cylinder, lathe, torus, with_cut, with_shape, with_taper,
            };
            use crate::pds::generator::GeneratorKind;
            use crate::pds::types::Fp2;
            use bevy::math::{Quat, Vec3};
            use bevy::mesh::{Indices, Mesh, VertexAttributeValues};
            let m = SovereignMaterialSettings::default;
            let line = |points: &[([f32; 3], f32)], res: u32| spine(points, res, m());
            let bent = || {
                line(
                    &[
                        ([0.0, 0.0, -0.6], 0.1),
                        ([0.2, 0.3, 0.0], 0.12),
                        ([0.0, 0.1, 0.6], 0.08),
                    ],
                    4,
                )
            };
            let bulged = |mut kind: GeneratorKind, bulge: [f32; 2]| {
                if let Some(t) = kind.torture_mut() {
                    t.bulge = Fp2(bulge);
                }
                kind
            };
            let raw_lathe = || lathe(&[(0.3, -0.4), (0.4, 0.0), (0.2, 0.4)], 8, false, m());
            let cases: Vec<(&str, GeneratorKind)> = vec![
                (
                    "a res-3 sweep",
                    line(&[([0.0, 0.0, -0.6], 0.25), ([0.0, 0.1, 0.6], 0.2)], 3),
                ),
                (
                    "a res-5 sweep along y",
                    line(&[([0.1, -0.5, 0.0], 0.15), ([0.1, 0.5, 0.0], 0.15)], 5),
                ),
                (
                    "a half-pipe of three faces",
                    with_cut(
                        line(&[([0.0, 0.0, -0.6], 0.3), ([0.0, 0.0, 0.6], 0.3)], 3),
                        [0.5, 1.0],
                        [0.0, 1.0],
                        0.0,
                    ),
                ),
                ("a bent res-4 sweep", bent()),
                (
                    "a band of a bent sweep",
                    with_cut(bent(), [0.0, 1.0], [0.2, 0.7], 0.0),
                ),
                ("a res-6 cylinder", cylinder(0.3, 0.8, 6, m())),
                (
                    "a cut cylinder",
                    with_cut(cylinder(0.3, 0.8, 6, m()), [0.1, 0.6], [0.0, 1.0], 0.0),
                ),
                (
                    "a bulged cylinder",
                    bulged(cylinder(0.3, 0.8, 8, m()), [0.3, 0.2]),
                ),
                ("a res-8 cone", cone(0.4, 0.9, 8, m())),
                (
                    "a cut cone",
                    with_cut(cone(0.4, 0.9, 8, m()), [0.0, 0.75], [0.0, 1.0], 0.0),
                ),
                ("a res-8 lathe", raw_lathe()),
                (
                    "a smooth lathe",
                    lathe(
                        &[(0.2, -0.4), (0.35, -0.1), (0.3, 0.2), (0.1, 0.4)],
                        12,
                        true,
                        m(),
                    ),
                ),
                (
                    "a cut lathe",
                    with_cut(raw_lathe(), [0.25, 1.0], [0.0, 1.0], 0.0),
                ),
                ("a torus", torus(0.08, 0.4, m())),
                (
                    "a sail's taper, bend and shear",
                    with_shape(
                        cuboid([0.02, 1.2, 0.9], m()),
                        [0.0, 0.97],
                        [0.07, 0.0, 0.0],
                        [0.0, -0.55],
                    ),
                ),
                (
                    "a tapered and sheared box",
                    with_shape(
                        cuboid([0.5, 0.8, 0.6], m()),
                        [0.4, 0.2],
                        [0.0; 3],
                        [0.1, -0.2],
                    ),
                ),
                (
                    "a plate tapered at both ends",
                    with_taper(bevel([0.6, 0.4, 0.2], 0.03, 1, m()), [0.5, 0.0], [0.5, 0.0]),
                ),
            ];
            for (what, kind) in cases {
                let node = prim(kind.clone(), [0.0; 3], id_quat());
                let mut parts = Vec::new();
                super::walk(
                    &node,
                    Vec3::ZERO,
                    Quat::IDENTITY,
                    Vec3::ONE,
                    "0".into(),
                    &mut parts,
                    true,
                );
                let part = &parts[0];
                let mesh = crate::world_builder::build_primitive_mesh(&kind).mesh;
                let Some(VertexAttributeValues::Float32x3(pos)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    panic!("{what}: no positions");
                };
                let pos: Vec<Vec3> = pos.iter().map(|p| Vec3::from(*p)).collect();
                let Some(VertexAttributeValues::Float32x3(nor)) =
                    mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
                else {
                    panic!("{what}: no normals");
                };
                let nor: Vec<Vec3> = nor.iter().map(|n| Vec3::from(*n)).collect();
                let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
                for p in &pos {
                    assert!(part.contains(*p), "{what}: the drawn vertex {p} is outside");
                    lo = lo.min(*p);
                    hi = hi.max(*p);
                }
                if matches!(kind, GeneratorKind::Bevel { .. }) {
                    assert!(part.lo.cmple(lo + 1e-4).all() && part.hi.cmpge(hi - 1e-4).all());
                    continue;
                }
                assert!(
                    part.lo.distance(lo) < 1e-4 && part.hi.distance(hi) < 1e-4,
                    "{what}: touch spans {}..{}, the mesh {lo}..{hi}",
                    part.lo,
                    part.hi
                );
                let idx: Vec<usize> = match mesh.indices().expect("indexed") {
                    Indices::U32(v) => v.iter().map(|&i| i as usize).collect(),
                    Indices::U16(v) => v.iter().map(|&i| i as usize).collect(),
                };
                for t in idx.chunks(3) {
                    let (a, b, c) = (pos[t[0]], pos[t[1]], pos[t[2]]);
                    let n = (b - a).cross(c - a);
                    if n.length() < 1e-9 {
                        continue;
                    }
                    // Outward as the mesher's own normals say, whichever
                    // way its triangles happen to be wound.
                    let shade = nor[t[0]] + nor[t[1]] + nor[t[2]];
                    let n = n.normalize() * n.dot(shade).signum();
                    let centre = (a + b + c) / 3.0;
                    assert!(
                        !part.contains(centre + n * 2e-3),
                        "{what}: 2 mm off the face at {centre} is still inside"
                    );
                }
            }
        }

        /// The skip, the box reject and [`super::holds`]' prefilters change no
        /// answer (#1393), on real craft: every pair asked through
        /// [`super::meet`] with no skip and no box gives the pieces and the
        /// loose parts `report` gives, and every sample of one node that the
        /// prefilters turn away from another is one that node's solid turns
        /// away too.
        #[test]
        fn report_answers_as_every_pair_asked_the_costly_way() {
            use super::{holds, meet, parts_of};
            use crate::pds::generator::GeneratorKind;
            // The seeded craft as drawn, less its particle FX, which are not
            // solids; a tree carrying a prim touch has no solid for is passed.
            fn bare(mut g: Generator) -> Option<Generator> {
                g.children
                    .retain(|c| !matches!(c.kind, GeneratorKind::ParticleSystem(..)));
                let solid = matches!(
                    g.kind,
                    GeneratorKind::Cuboid { .. }
                        | GeneratorKind::Bevel { .. }
                        | GeneratorKind::Wedge { .. }
                        | GeneratorKind::Superellipsoid { .. }
                        | GeneratorKind::Sphere { .. }
                        | GeneratorKind::Cylinder { .. }
                        | GeneratorKind::Cone { .. }
                        | GeneratorKind::Torus { .. }
                        | GeneratorKind::Spine { .. }
                        | GeneratorKind::Lathe { .. }
                );
                let children: Option<Vec<Generator>> = g.children.drain(..).map(bare).collect();
                g.children = children?;
                solid.then_some(g)
            }
            let trees: Vec<Generator> = (0u64..60)
                .filter_map(|seed| {
                    crate::pds::avatar::default_visuals::build_for_seed(seed)
                        .0
                        .visuals()
                        .cloned()
                        .and_then(bare)
                })
                .take(3)
                .collect();
            assert_eq!(trees.len(), 3, "too few vehicle seeds under 60");
            for tree in &trees {
                let parts = parts_of(tree, true);
                let n = parts.len();
                let mut parent: Vec<usize> = (0..n).collect();
                fn find(parent: &mut [usize], mut a: usize) -> usize {
                    while parent[a] != a {
                        a = parent[a];
                    }
                    a
                }
                let mut met = vec![false; n];
                for i in 0..n {
                    for j in (i + 1)..n {
                        if meet(&parts[i], &parts[j]) {
                            met[i] = true;
                            met[j] = true;
                            let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                            parent[ri] = rj;
                        }
                    }
                }
                let mut roots: Vec<usize> = (0..n).map(|i| find(&mut parent, i)).collect();
                roots.sort_unstable();
                roots.dedup();
                let loose: Vec<String> = (0..n)
                    .filter(|&i| !met[i])
                    .map(|i| parts[i].name.clone())
                    .collect();
                assert_eq!(report(tree), (roots.len(), loose));
                for a in &parts {
                    for b in &parts {
                        for &p in &a.samples {
                            assert_eq!(
                                holds(b, p),
                                b.contains(p),
                                "{} at {p}: the prefilter and {}'s solid disagree",
                                a.name,
                                b.name
                            );
                        }
                    }
                }
            }
        }

        /// Two res-6 sweeps crossing at right angles, their axes 0.17 m
        /// apart: their facing flats overlap by 3.2 mm, in a lens no ring
        /// vertex reaches - each one's samples lie a station off the
        /// crossing - so only [`super::meet`]'s witness finds it. 0.18 m
        /// apart they miss by 6.8 mm, which the round reading (radii 0.1 m
        /// each) took for an overlap.
        #[test]
        fn two_crossing_sweeps_meet_where_their_flats_overlap() {
            use crate::pds::generator::GeneratorKind;
            let bar = |a: [f32; 3], b: [f32; 3]| {
                let mut kind = spine(
                    &[(a, 0.1), (b, 0.1)],
                    6,
                    SovereignMaterialSettings::default(),
                );
                if let GeneratorKind::Spine {
                    samples_per_segment,
                    ..
                } = &mut kind
                {
                    *samples_per_segment = 2;
                }
                prim(kind, [0.0; 3], id_quat())
            };
            let crossing = |gap: f32| {
                with(
                    bar([-0.3, 0.0, 0.0], [0.3, 0.0, 0.0]),
                    vec![bar([0.0, gap, -0.3], [0.0, gap, 0.3])],
                )
            };
            assert_eq!(report(&crossing(0.17)), (1, Vec::<String>::new()));
            assert_eq!(report(&crossing(0.18)).0, 2);
        }
    }
}
