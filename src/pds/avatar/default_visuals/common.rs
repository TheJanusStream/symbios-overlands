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

#[cfg(test)]
pub(crate) mod touch {
    use bevy::math::cubic_splines::{CubicCardinalSpline, CubicGenerator};
    use bevy::math::{Quat, Vec2, Vec3, Vec3Swizzles, Vec4};

    use crate::pds::generator::{Generator, GeneratorKind};

    /// Ring segments taken round a swept or revolved surface when sampling it.
    const RING: usize = 6;
    /// Stride through a resampled centreline - every station would be far more
    /// samples than the question needs.
    const STRIDE: usize = 2;

    /// The solid of one node, in its own local frame.
    enum Shape {
        /// Half-extents. Also stands in for a Bevel (whose chamfer this
        /// ignores) and a Superellipsoid (which is never fatter than its box).
        Block(Vec3),
        Ball(f32),
        /// Axis along local Y.
        Cylinder {
            radius: f32,
            half_h: f32,
        },
        Ring {
            minor: f32,
            major: f32,
        },
        /// Resampled centreline with its radius.
        Tube(Vec<(Vec3, f32)>),
        /// Resampled `(radius, height)` silhouette, revolved about local Y.
        Revolve(Vec<Vec2>),
    }

    /// One node, resolved into the root's frame.
    struct Part {
        name: String,
        translation: Vec3,
        rotation: Quat,
        scale: Vec3,
        shape: Shape,
        samples: Vec<Vec3>,
        lo: Vec3,
        hi: Vec3,
    }

    impl Part {
        /// A world point in this node's own frame.
        fn local(&self, p: Vec3) -> Vec3 {
            (self.rotation.inverse() * (p - self.translation)) / self.scale
        }

        fn contains(&self, p: Vec3) -> bool {
            const EPS: f32 = 1e-4;
            let q = self.local(p);
            match &self.shape {
                Shape::Block(h) => {
                    q.x.abs() <= h.x + EPS && q.y.abs() <= h.y + EPS && q.z.abs() <= h.z + EPS
                }
                Shape::Ball(r) => q.length() <= r + EPS,
                Shape::Cylinder { radius, half_h } => {
                    q.xz().length() <= radius + EPS && q.y.abs() <= half_h + EPS
                }
                Shape::Ring { minor, major } => {
                    let d = q.xz().length() - major;
                    (d * d + q.y * q.y).sqrt() <= minor + EPS
                }
                Shape::Tube(path) => path.windows(2).any(|w| {
                    let (a, ra) = w[0];
                    let (b, rb) = w[1];
                    let ab = b - a;
                    let t = if ab.length_squared() > 1e-12 {
                        ((q - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    (q - (a + ab * t)).length() <= ra + (rb - ra) * t + EPS
                }),
                // Point in the silhouette polygon, closed down the axis: the
                // lathe's caps are exactly that closure.
                Shape::Revolve(prof) => {
                    let pt = Vec2::new(q.xz().length(), q.y);
                    let mut poly: Vec<Vec2> = prof.clone();
                    poly.push(Vec2::new(0.0, prof[prof.len() - 1].y));
                    poly.push(Vec2::new(0.0, prof[0].y));
                    let mut inside = false;
                    let n = poly.len();
                    for i in 0..n {
                        let (a, b) = (poly[i], poly[(i + 1) % n]);
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

    /// Surface samples for a shape, in its local frame.
    fn surface(shape: &Shape) -> Vec<Vec3> {
        let ring = |c: Vec3, u: Vec3, v: Vec3, r: f32, out: &mut Vec<Vec3>| {
            for k in 0..RING {
                let a = std::f32::consts::TAU * k as f32 / RING as f32;
                out.push(c + (u * a.cos() + v * a.sin()) * r);
            }
        };
        let mut out = Vec::new();
        match shape {
            Shape::Block(h) => {
                for sx in [-1.0f32, 1.0] {
                    for sy in [-1.0f32, 1.0] {
                        for sz in [-1.0f32, 1.0] {
                            out.push(Vec3::new(sx * h.x, sy * h.y, sz * h.z));
                        }
                    }
                }
                for a in [Vec3::X, Vec3::Y, Vec3::Z] {
                    out.push(*h * a);
                    out.push(-*h * a);
                }
            }
            Shape::Ball(r) => {
                for a in [Vec3::X, Vec3::Y, Vec3::Z] {
                    out.push(a * *r);
                    out.push(-a * *r);
                }
            }
            Shape::Cylinder { radius, half_h } => {
                for s in [-1.0f32, 1.0] {
                    ring(Vec3::Y * (s * half_h), Vec3::X, Vec3::Z, *radius, &mut out);
                }
            }
            Shape::Ring { minor, major } => {
                for k in 0..(RING * 2) {
                    let a = std::f32::consts::TAU * k as f32 / (RING * 2) as f32;
                    let dir = Vec3::new(a.cos(), 0.0, a.sin());
                    ring(dir * *major, dir, Vec3::Y, *minor, &mut out);
                }
            }
            Shape::Tube(path) => {
                for (i, &(p, r)) in path.iter().enumerate() {
                    if i % STRIDE != 0 && i + 1 != path.len() {
                        continue;
                    }
                    let t = if i + 1 < path.len() {
                        (path[i + 1].0 - p).normalize_or_zero()
                    } else {
                        (p - path[i - 1].0).normalize_or_zero()
                    };
                    let u = if t.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
                    let n = t.cross(u).normalize_or_zero();
                    ring(p, n, t.cross(n).normalize_or_zero(), r, &mut out);
                }
            }
            Shape::Revolve(prof) => {
                for (i, s) in prof.iter().enumerate() {
                    if i % STRIDE != 0 && i + 1 != prof.len() {
                        continue;
                    }
                    ring(Vec3::Y * s.y, Vec3::X, Vec3::Z, s.x, &mut out);
                }
            }
        }
        out
    }

    /// The mesher's own Catmull-Rom resample of a spine (`sweeps.rs`), so a
    /// curved tube is tested where it is actually drawn rather than on the
    /// chord between its control points.
    fn resample_spine(points: &[(Vec3, f32)], per_segment: u32) -> Vec<(Vec3, f32)> {
        if points.len() < 2 {
            return points.to_vec();
        }
        let ctrl: Vec<Vec4> = points.iter().map(|(p, r)| p.extend(r.max(0.005))).collect();
        let per = per_segment.clamp(2, 64);
        let n = (ctrl.len() as u32 - 1) * per;
        let Ok(curve) = CubicCardinalSpline::new_catmull_rom(ctrl).to_curve() else {
            return points.to_vec();
        };
        (0..=n)
            .map(|i| {
                let v = curve.position(i as f32 / per as f32);
                (v.truncate(), v.w.max(0.005))
            })
            .collect()
    }

    /// The mesher's resample of a lathe silhouette.
    fn resample_lathe(points: &[Vec2], smooth: bool) -> Vec<Vec2> {
        if !smooth || points.len() < 2 {
            return points.to_vec();
        }
        const PER: u32 = 6;
        let n = (points.len() as u32 - 1) * PER;
        let Ok(curve) = CubicCardinalSpline::new_catmull_rom(points.to_vec()).to_curve() else {
            return points.to_vec();
        };
        (0..=n)
            .map(|i| {
                let mut v = curve.position(i as f32 / PER as f32);
                v.x = v.x.max(0.0);
                v
            })
            .collect()
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
            GeneratorKind::Cylinder { radius, height, .. }
            | GeneratorKind::Cone { radius, height, .. } => Shape::Cylinder {
                radius: radius.0,
                half_h: height.0 * 0.5,
            },
            GeneratorKind::Torus {
                minor_radius,
                major_radius,
                ..
            } => Shape::Ring {
                minor: minor_radius.0,
                major: major_radius.0,
            },
            GeneratorKind::Spine {
                points,
                samples_per_segment,
                ..
            } => Shape::Tube(resample_spine(
                &points
                    .iter()
                    .map(|p| (Vec3::from(p.position.0), p.radius.0))
                    .collect::<Vec<_>>(),
                *samples_per_segment,
            )),
            GeneratorKind::Lathe { points, smooth, .. } => Shape::Revolve(resample_lathe(
                &points
                    .iter()
                    .map(|p| Vec2::new(p.radius.0, p.height.0))
                    .collect::<Vec<_>>(),
                *smooth,
            )),
            other => panic!(
                "touch: no solid for {} - teach this helper the prim before \
                 a craft uses it",
                other.kind_tag()
            ),
        }
    }

    fn walk(node: &Generator, t: Vec3, r: Quat, s: Vec3, path: String, out: &mut Vec<Part>) {
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
        let samples: Vec<Vec3> = surface(&shape)
            .into_iter()
            .map(|p| wt + wr * (ws * p))
            .collect();
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for p in &samples {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        out.push(Part {
            name: format!("{path}:{}", node.kind.kind_tag()),
            translation: wt,
            rotation: wr,
            scale: ws,
            shape,
            samples,
            lo,
            hi,
        });
        for (i, child) in node.children.iter().enumerate() {
            walk(child, wt, wr, ws, format!("{path}/{i}"), out);
        }
    }

    /// What the touch graph of `root` looks like: the number of connected
    /// components, and the name of every node that meets nothing at all.
    pub(crate) fn report(root: &Generator) -> (usize, Vec<String>) {
        let mut parts = Vec::new();
        walk(
            root,
            Vec3::ZERO,
            Quat::IDENTITY,
            Vec3::ONE,
            "0".to_string(),
            &mut parts,
        );
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
                // Cheap reject first: nothing can touch across disjoint boxes.
                let (a, b) = (&parts[i], &parts[j]);
                if a.lo.cmpgt(b.hi).any() || b.lo.cmpgt(a.hi).any() {
                    continue;
                }
                if a.samples.iter().any(|&p| b.contains(p))
                    || b.samples.iter().any(|&p| a.contains(p))
                {
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
    /// node's sampled surface.
    ///
    /// What an air-draft guard checks a DRAWN rig against, rather than the
    /// arithmetic that placed it - a spar standing over the height its rig
    /// was resolved to is exactly what a derivation cannot see (#1366). Like
    /// the rest of this helper it ignores torture and cuts. A cut-away sweep
    /// read as the whole tube only over-reads a height; a taper and a shear
    /// act across a prim, never along its height; a bend with a `y` term
    /// WOULD lift a top edge, so a craft that authors one needs this taught
    /// the deform first (#1393). MEASURED over every seeded tree at #1382:
    /// no craft in the fleet authors a bend, an s-bend, a bulge or a twist -
    /// the 3.4 % of nodes that carry a deform carry taper and shear, which
    /// act ACROSS a prim - so no air-draft reading here is wrong today.
    pub(crate) fn highest(root: &Generator) -> f32 {
        let mut parts = Vec::new();
        walk(
            root,
            Vec3::ZERO,
            Quat::IDENTITY,
            Vec3::ONE,
            "0".to_string(),
            &mut parts,
        );
        parts.iter().map(|p| p.hi.y).fold(f32::MIN, f32::max)
    }

    /// The lowest point `root` draws, in its own frame (m): the bottom of
    /// every node's sampled surface - [`highest`]'s mirror, blind to the
    /// same things. And one more: a tube is read ROUND, so the flat bottom
    /// of a res-3 hull sweep, whose polygon's bottom lies at `cos 30` of its
    /// radius, is read too deep (#1393 - 60 % of the fleet's sweeps are under
    /// resolution 8, measured at #1382). A guard asks this of the parts that
    /// are not such a hull, and asks the profile about the hull.
    pub(crate) fn lowest(root: &Generator) -> f32 {
        let mut parts = Vec::new();
        walk(
            root,
            Vec3::ZERO,
            Quat::IDENTITY,
            Vec3::ONE,
            "0".to_string(),
            &mut parts,
        );
        parts.iter().map(|p| p.lo.y).fold(f32::MAX, f32::min)
    }

    /// How wide `root` draws, in its own frame (m): the full `x` extent of
    /// every node's sampled surface.
    ///
    /// [`highest`]'s sideways twin, sharing every one of its blind spots -
    /// and sharing them in the SAFE direction for a clearance guard. A
    /// tortured cuboid read as its undeformed box, and a low-resolution
    /// sweep read as a round tube, both read WIDER than the craft is drawn,
    /// so a gateway-mouth guard written against this can only be
    /// pessimistic. (`highest` has the same property against a cut, and the
    /// opposite one against a bend with a `y` term - see its note.)
    pub(crate) fn widest(root: &Generator) -> f32 {
        let mut parts = Vec::new();
        walk(
            root,
            Vec3::ZERO,
            Quat::IDENTITY,
            Vec3::ONE,
            "0".to_string(),
            &mut parts,
        );
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
}
