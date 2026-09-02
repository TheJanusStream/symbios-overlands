//! Geometry side of the [`util`](super) vocabulary: node wrappers, tree
//! assembly, rotations, the primitive constructors and the composite
//! pieces (foundations, footings, the owner panel, struts, railings) built
//! from them. Materials live in [`super::material`].

use crate::pds::PrimCommon;
use crate::pds::generator::{FaceKey, FaceOverride};
use crate::pds::sanitize::limits::MAX_FACE_OVERRIDES;
use crate::pds::{
    Fp, Fp2, Fp3, Fp4, Generator, GeneratorKind, SovereignMaterialSettings, SovereignTextureConfig,
    TortureParams, TransformData,
};

use super::material::foundation_mat;

/// Wrap a kind into a childless node at `translation` / `rotation`.
pub(in crate::catalogue::items) fn prim(
    kind: GeneratorKind,
    translation: [f32; 3],
    rotation: Fp4,
) -> Generator {
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

pub(in crate::catalogue::items) fn id_quat() -> Fp4 {
    Fp4([0.0, 0.0, 0.0, 1.0])
}

/// Like [`prim`] but with a non-identity scale — e.g. a flattened sphere for a
/// cloud-pruned foliage pad or a smooth ellipsoid pod.
pub(in crate::catalogue::items) fn prim_scaled(
    kind: GeneratorKind,
    translation: [f32; 3],
    rotation: Fp4,
    scale: [f32; 3],
) -> Generator {
    Generator {
        kind,
        transform: TransformData {
            translation: Fp3(translation),
            rotation,
            scale: Fp3(scale),
        },
        children: Vec::new(),
        audio: crate::pds::SovereignAudioConfig::None,
    }
}

/// Assemble a flat list of prims, each positioned in the prop's plain
/// ground-relative world frame, into one generator: the first prim becomes
/// the root (keeping its transform), and every other prim is reparented
/// under it with its translation rebased into the root's local frame.
///
/// Spawned generator children inherit the root's transform (Bevy
/// `add_child`), so without this rebase a child authored at world `y = 2`
/// under a root sitting at `y = 0.5` would render at `y = 2.5`. Authoring
/// against this helper lets each prop's geometry read in one consistent
/// world frame instead of threading a per-file offset through every piece.
pub(in crate::catalogue::items) fn assemble(mut prims: Vec<Generator>) -> Generator {
    let mut root = prims.remove(0);
    let [rx, ry, rz] = root.transform.translation.0;
    for mut p in prims {
        let t = &mut p.transform.translation.0;
        t[0] -= rx;
        t[1] -= ry;
        t[2] -= rz;
        root.children.push(p);
    }
    root
}

/// Nest `children` under `parent`, rebasing each child's translation from
/// the prop's world frame into the parent's local one.
///
/// The counterpart to [`assemble`] for a prop built as a **tree** rather
/// than a flat list. Both let every piece be authored in one world frame;
/// the difference is what the owner gets in the editor. Under `assemble`,
/// every piece hangs off the root, so dragging a fountain's bowl leaves its
/// jet, its rim and its spray behind — each has to be moved by hand.
/// Nested, a part carries everything it holds up, and one gizmo drag moves
/// a whole sub-assembly.
///
/// A prop reads best nested the way it was built: the root at the bottom,
/// each course parented to the one it stands on. That is also the order to
/// *write* it in — innermost first — because a subtree passed in here still
/// carries its own world translation, and only its own children have been
/// rebased so far:
///
/// ```ignore
/// let bowl = nest(prim(bowl, [0.0, 1.49, 0.0], id_quat()), vec![
///     prim(rim, [0.0, 1.56, 0.0], id_quat()),
///     fx::jet([0.0, 1.62, 0.0], seed),
/// ]);
/// let base = nest(prim(plinth, [0.0, 0.06, 0.0], id_quat()), vec![
///     nest(prim(shaft, [0.0, 1.0, 0.0], id_quat()), vec![bowl]),
/// ]);
/// ```
///
/// Rotation and scale propagate too, so a parent that is *tilted* spins
/// everything above it — which is the point on a leaning mast, and a bug on
/// a plinth. Keep a sub-assembly's own root axis-aligned unless the tilt is
/// meant to carry.
pub(in crate::catalogue::items) fn nest(
    mut parent: Generator,
    children: Vec<Generator>,
) -> Generator {
    let [px, py, pz] = parent.transform.translation.0;
    for mut c in children {
        let t = &mut c.transform.translation.0;
        t[0] -= px;
        t[1] -= py;
        t[2] -= pz;
        parent.children.push(c);
    }
    parent
}

/// Add one more child to an already-[`assemble`]d or [`nest`]ed root,
/// rebasing it out of the prop's ground frame the same way they do
/// (#1010).
///
/// The trap this closes: `assemble` and `nest` rebase the pieces handed
/// *to* them, but a child pushed onto the finished root afterwards is
/// read in the root's own local space and never rebased — so geometry
/// authored in the prop's ground frame, from the same constants as
/// everything else, silently lands one root-height out. It is an easy
/// mistake to make because the signature line reads perfectly:
///
/// ```ignore
/// let mut root = assemble(prims);
/// root.children.push(fx::hearth_smoke([x, wall_top + roof_h, 0.0], seed));
/// //                                      ^ ground-frame constants, local-frame slot
/// ```
///
/// Reach for this instead of `root.children.push` whenever the child is
/// authored in the same frame as the prims — signature FX are the usual
/// case, since they hang off a chimney or a hearth the prims placed.
///
/// Like its two siblings this rebases translation only. A root carrying a
/// non-identity scale or rotation still propagates it to everything
/// beneath, including this child.
pub(in crate::catalogue::items) fn attach(root: &mut Generator, child: Generator) {
    let [rx, ry, rz] = root.transform.translation.0;
    let mut child = child;
    let t = &mut child.transform.translation.0;
    t[0] -= rx;
    t[1] -= ry;
    t[2] -= rz;
    root.children.push(child);
}

/// Rotation around X — tilts ramps and dome slits.
pub(in crate::catalogue::items) fn quat_x(angle_rad: f32) -> Fp4 {
    let half = angle_rad * 0.5;
    Fp4([half.sin(), 0.0, 0.0, half.cos()])
}

/// Rotation around Y — yaws monoliths to face the circle centre.
pub(in crate::catalogue::items) fn quat_y(angle_rad: f32) -> Fp4 {
    let half = angle_rad * 0.5;
    Fp4([0.0, half.sin(), 0.0, half.cos()])
}

/// Rotation around Z — lays a Y-axis cylinder onto the horizontal X axis
/// (`FRAC_PI_2`), e.g. a conduit / pipe run spanning left-to-right.
pub(in crate::catalogue::items) fn quat_z(angle_rad: f32) -> Fp4 {
    let half = angle_rad * 0.5;
    Fp4([0.0, 0.0, half.sin(), half.cos()])
}

/// Hamilton product of two `[x, y, z, w]` rotations — the combined rotation
/// that applies `b` first, then `a`. Composing two unit quaternions stays
/// unit, so the result needs no renormalisation.
pub(in crate::catalogue::items) fn quat_mul(a: Fp4, b: Fp4) -> Fp4 {
    let [ax, ay, az, aw] = a.0;
    let [bx, by, bz, bw] = b.0;
    Fp4([
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ])
}

/// Cuboid with an optional taper (`0.0` = straight, `1.0` = pyramid).
pub(in crate::catalogue::items) fn cuboid_tapered(
    size: [f32; 3],
    taper: f32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Cuboid {
        size: Fp3(size),
        common: PrimCommon {
            material,
            torture: TortureParams {
                taper: Fp2([taper, taper]),
                ..Default::default()
            },
            ..Default::default()
        },
    }
}

/// Cuboid with independent X/Z taper — a ridged roof or asymmetric frustum.
/// Each component pinches the top on that axis (`0.0` keeps the full width,
/// `1.0` pinches it to a line), so `[0.1, 0.9]` yields a long ridge along X
/// with steep slopes on the Z sides; the uniform [`cuboid_tapered`] can only
/// make a square-topped frustum or a point.
pub(in crate::catalogue::items) fn cuboid_tapered_xz(
    size: [f32; 3],
    taper_xz: [f32; 2],
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Cuboid {
        size: Fp3(size),
        common: PrimCommon {
            material,
            torture: TortureParams {
                taper: Fp2(taper_xz),
                ..Default::default()
            },
            ..Default::default()
        },
    }
}

pub(in crate::catalogue::items) fn cylinder_tapered(
    radius: f32,
    height: f32,
    resolution: u32,
    taper: f32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Cylinder {
        radius: Fp(radius),
        height: Fp(height),
        resolution,
        common: PrimCommon {
            material,
            torture: TortureParams {
                taper: Fp2([taper, taper]),
                ..Default::default()
            },
            ..Default::default()
        },
    }
}

pub(in crate::catalogue::items) fn sphere(
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

/// Barr superellipsoid — the rounded-mass workhorse. `exponent_ns` shapes
/// the north–south (latitude) profile, `exponent_ew` the east–west
/// cross-section: `0.2` is a hard box, `~0.65` a filled pillow (sandbags,
/// cushions, bedrolls), `1.0` a true ellipsoid, `2.5` a pinched octahedron.
/// Reach for it where a cuboid reads too hard and a scaled sphere too soft.
pub(in crate::catalogue::items) fn superellipsoid(
    half_extents: [f32; 3],
    exponent_ns: f32,
    exponent_ew: f32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Superellipsoid {
        half_extents: Fp3(half_extents),
        common: PrimCommon::with_material(material),
        exponent_ns: Fp(exponent_ns),
        exponent_ew: Fp(exponent_ew),
        latitudes: 12,
        longitudes: 18,
    }
}

pub(in crate::catalogue::items) fn cone(
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

pub(in crate::catalogue::items) fn torus(
    minor_radius: f32,
    major_radius: f32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Torus {
        minor_radius: Fp(minor_radius),
        major_radius: Fp(major_radius),
        minor_resolution: 10,
        major_resolution: 28,
        common: PrimCommon::with_material(material),
    }
}

/// Hollow cylinder — a pipe / ring / conduit / halo. `inner_radius` is the
/// bore (`< radius`); annular caps close the ends. Axis along Y like
/// [`cylinder_tapered`].
pub(in crate::catalogue::items) fn tube(
    radius: f32,
    inner_radius: f32,
    height: f32,
    resolution: u32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Tube {
        radius: Fp(radius),
        inner_radius: Fp(inner_radius),
        height: Fp(height),
        resolution,
        common: PrimCommon::with_material(material),
    }
}

/// Helical tube — a spring / data-stream coil / spiral rail. `radius` is the
/// coil radius, `tube_radius` the wire thickness, `pitch` the vertical rise
/// per full turn, `turns` the revolution count. The coil climbs the Y axis,
/// centred on the origin (total height `turns * pitch`).
pub(in crate::catalogue::items) fn helix(
    radius: f32,
    tube_radius: f32,
    pitch: f32,
    turns: f32,
    resolution: u32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Helix {
        radius: Fp(radius),
        tube_radius: Fp(tube_radius),
        pitch: Fp(pitch),
        turns: Fp(turns),
        resolution,
        common: PrimCommon::with_material(material),
    }
}

/// A metaball group meshed by surface nets — the catalogue's route to forms
/// the primitive vocabulary cannot state.
///
/// Elements evaluate in list order, each smoothly blended into everything
/// before it (or carved out of it, with `subtract`), so a handful of boxes and
/// ellipsoids become **one continuous watertight skin** rather than a pile of
/// intersecting solids. That property is the reason to reach for it: cloth,
/// organic masses and anything whose silhouette should read as a single
/// object are exactly where a union of prims shows its seams.
///
/// The cost is that a group has **one material** (plus a `Surface` face
/// override), because there is no analytic parameterisation to tag faces
/// against. Two colours means two groups.
///
/// `resolution` is sample cells along the longest axis; the sanitiser caps it
/// at [`MAX_BLOB_RESOLUTION`](crate::pds::sanitize::limits::MAX_BLOB_RESOLUTION)
/// (48) and the element count at 16. Bake cost climbs with the cube of it, so
/// prefer the smallest that hides the faceting — a metre-scale prop reads
/// clean around 24–32.
pub(in crate::catalogue::items) fn blob_group(
    elements: Vec<crate::pds::generator::BlobElement>,
    resolution: u32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::BlobGroup {
        elements,
        resolution,
        common: PrimCommon::with_material(material),
    }
}

/// An axis-aligned box element for a [`blob_group`] — flat faces inside a
/// smooth blend, which is what keeps a blobbed slab reading as a slab.
pub(in crate::catalogue::items) fn blob_box(
    position: [f32; 3],
    half_extents: [f32; 3],
    blend: f32,
) -> crate::pds::generator::BlobElement {
    crate::pds::generator::BlobElement {
        shape: crate::pds::generator::BlobShape::Box,
        position: Fp3(position),
        rotation: id_quat(),
        radii: Fp3(half_extents),
        subtract: false,
        blend: Fp(blend),
    }
}

/// An ellipsoid element for a [`blob_group`].
pub(in crate::catalogue::items) fn blob_ellipsoid(
    position: [f32; 3],
    semi_axes: [f32; 3],
    blend: f32,
) -> crate::pds::generator::BlobElement {
    crate::pds::generator::BlobElement {
        shape: crate::pds::generator::BlobShape::Ellipsoid,
        position: Fp3(position),
        rotation: id_quat(),
        radii: Fp3(semi_axes),
        subtract: false,
        blend: Fp(blend),
    }
}

/// A capsule element for a [`blob_group`], its axis along local `+Y` before
/// `rotation` — the element to reach for when a limb, a bone or a rope needs
/// to melt into the mass rather than abut it.
pub(in crate::catalogue::items) fn blob_capsule(
    position: [f32; 3],
    radius: f32,
    half_length: f32,
    rotation: Fp4,
    blend: f32,
) -> crate::pds::generator::BlobElement {
    crate::pds::generator::BlobElement {
        shape: crate::pds::generator::BlobShape::Capsule,
        position: Fp3(position),
        rotation,
        radii: Fp3([radius, half_length, radius]),
        subtract: false,
        blend: Fp(blend),
    }
}

/// Flip a [`blob_group`] element to **carve** instead of add — eye sockets,
/// nostrils, creases, a slot in a mass. Subtraction is smooth like the union
/// is, so a carved socket has a soft rim rather than a knife edge.
pub(in crate::catalogue::items) fn carved(
    mut element: crate::pds::generator::BlobElement,
) -> crate::pds::generator::BlobElement {
    element.subtract = true;
    element
}

/// The smallest feature a [`blob_group`] at `resolution` can resolve across
/// `longest_axis`, in metres.
///
/// Surface nets samples a grid and polygonises where the field crosses zero,
/// so a feature thinner than about two cells is **missed in places** — the
/// mesh comes out with holes in it rather than merely coarse. The pirate
/// flag found this the expensive way: a 0.06 m cloth 1.9 m wide at
/// resolution 30 is 63 mm cells, so the sheet was thinner than one cell and
/// polygonised as two disconnected slabs with a gap down the middle.
///
/// Multiply by two and treat the result as a floor on any thin dimension.
/// Note that `resolution` is capped at 48 by the sanitiser, so past about
/// 2 m of span the only way to keep a sheet solid is to make it thicker.
#[cfg(test)]
pub(in crate::catalogue::items) fn blob_cell_size(longest_axis: f32, resolution: u32) -> f32 {
    longest_axis / resolution.max(1) as f32
}

/// Right-triangular prism — a ramp / awning / roof pitch / buttress. `size`
/// is the bounding box; the slope rises from the front-bottom (`+Z`, `-Y`) to
/// the back-top (`-Z`, `+Y`) across the full width (X).
pub(in crate::catalogue::items) fn wedge(
    size: [f32; 3],
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Wedge {
        size: Fp3(size),
        common: PrimCommon::with_material(material),
    }
}

/// Stamp the SL-style topology cuts onto a swept primitive (Sphere / Cylinder
/// / Cone / Torus / Tube): `path_cut` (`[begin, end]` kept angular fraction —
/// a half-torus arch, an orange-slice wedge), `profile_cut` (`[begin, end]`
/// kept latitude band — domes / bowls), and `hollow` (bore fraction).
/// Non-swept kinds pass through unchanged. Honoured by the unified sweep
/// mesher in `crate::world_builder::prim`.
pub(in crate::catalogue::items) fn with_cut(
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

/// Give one face of a primitive its own material (#955) — the SL model: an
/// override is the face's **whole** material, not a delta, so it keeps its
/// own colour, texture, uv scale/offset/rotation whatever the base material
/// later becomes.
///
/// This is what replaces the "stack a thin slab on the surface to recolour
/// it" idiom, which pays a whole extra prim (and a z-fight risk) for a
/// colour change. Cost here is one extra draw call per *distinct* material —
/// the spawn-time face plan groups faces by material, so five faces sharing
/// one override cost one group, and an override equal to the base costs
/// nothing at all.
///
/// Face names are per family — `Top`/`Bottom`/`SidePx`… on the flat family,
/// `Wall`/`Bore`/`Top`/`Bottom` plus the cut faces on the revolved one; see
/// [`FaceKey`]. Naming a face the kind doesn't emit is *dormant*, not an
/// error: it waits, harmlessly, until a cut produces that face. Repeating a
/// face replaces its override, because the sanitizer keeps the first entry
/// and a silently-dropped second one would be a trap.
///
/// Non-primitive kinds (which have no faces) pass through unchanged.
pub(in crate::catalogue::items) fn with_face(
    mut kind: GeneratorKind,
    face: FaceKey,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    if let Some(faces) = kind.faces_mut() {
        if let Some(existing) = faces.iter_mut().find(|o| o.face == face) {
            existing.material = material;
        } else if faces.len() < MAX_FACE_OVERRIDES {
            // At the cap the sanitizer would drop the entry on the way to the
            // PDS anyway; refusing here keeps the built tree and the saved one
            // identical.
            faces.push(FaceOverride {
                face,
                material,
                uv_mapping: None,
            });
        }
    }
    kind
}

/// Mark a primitive kind solid so the spawner attaches its matching
/// collider — structural pieces players can stand on or bump into.
/// Decorative trim (railings, orbs, lamps) stays non-solid.
pub(in crate::catalogue::items) fn solid(mut kind: GeneratorKind) -> GeneratorKind {
    match &mut kind {
        GeneratorKind::Cuboid { common: PrimCommon { solid, .. }, .. }
        | GeneratorKind::Sphere { common: PrimCommon { solid, .. }, .. }
        | GeneratorKind::Cylinder { common: PrimCommon { solid, .. }, .. }
        | GeneratorKind::Capsule { common: PrimCommon { solid, .. }, .. }
        | GeneratorKind::Cone { common: PrimCommon { solid, .. }, .. }
        | GeneratorKind::Torus { common: PrimCommon { solid, .. }, .. }
        // Superellipsoid carries an analytical collider too (a coarse
        // sampled convex hull), so marking it solid is as cheap here as it
        // is for the box it replaces.
        | GeneratorKind::Superellipsoid { common: PrimCommon { solid, .. }, .. } => *solid = true,
        _ => {}
    }
    kind
}

/// Reveal height of a foundation above the entry's ground plane, so a
/// terrain-snapped structure on a slope shows plinth instead of daylight
/// under its downhill edge.
const FOUNDATION_REVEAL: f32 = 0.15;

/// Total footprint shrink applied to a foundation versus the base slab it
/// sits under (callers author both at the same footprint). The slab's
/// reveal band overlaps the plinth's, so equal footprints leave their
/// vertical side faces coplanar all around the perimeter — which z-fights
/// on flat ground. Holding the plinth this much smaller makes the slab
/// oversail it (≈half this per side), breaking the shared plane and tucking
/// the plinth out of sight on flat ground while it still fills slope gaps.
const FOUNDATION_INSET: f32 = 0.12;

/// Rectangular buried foundation: a solid stone block whose top sits
/// [`FOUNDATION_REVEAL`] above the entry's y=0 ground plane and which
/// extends `depth` below it, so a terrain-snapped structure on a
/// slope shows plinth instead of daylight under its downhill edge.
/// `center` is the block's XZ centre in the entry's local frame
/// (footprint/2 for the corner-origin shape-grammar entries, the
/// origin for the centred primitive entries).
pub(in crate::catalogue::items) fn foundation_block(
    size_x: f32,
    size_z: f32,
    center: [f32; 2],
    depth: f32,
) -> Generator {
    let height = depth + FOUNDATION_REVEAL;
    prim(
        solid(cuboid_tapered(
            [size_x - FOUNDATION_INSET, height, size_z - FOUNDATION_INSET],
            0.0,
            foundation_mat(),
        )),
        [center[0], FOUNDATION_REVEAL - height * 0.5, center[1]],
        id_quat(),
    )
}

/// Round sibling of [`foundation_block`] for the drum/tower entries,
/// centred on the entry origin.
pub(in crate::catalogue::items) fn foundation_disc(radius: f32, depth: f32) -> Generator {
    let height = depth + FOUNDATION_REVEAL;
    prim(
        solid(cylinder_tapered(
            radius - FOUNDATION_INSET * 0.5,
            height,
            24,
            0.0,
            foundation_mat(),
        )),
        [0.0, FOUNDATION_REVEAL - height * 0.5, 0.0],
        id_quat(),
    )
}

/// A footing sized to the ground it will be dropped on (#1009): a
/// [`foundation_block`] whose depth comes from the entry's own footprint
/// radius via [`crate::catalogue::items::foundation::required_depth`],
/// rather than being picked by eye.
///
/// This is what a settlement building should carry. Since #1008 a seeded
/// structure is snapped to the *highest* ground under its footprint, so
/// it never sinks into a hillside — but the ground then falls away under
/// its downhill edge, and this is what closes that gap. Depth tracks the
/// footprint because the drop a building spans is proportional to how
/// wide it is; pass the same `clearance` the entry's
/// [`Footprint`](crate::catalogue::Footprint) declares.
///
/// `size_x`/`size_z` are the *building's* base footprint, not its
/// clearance — the plinth should sit under the walls, not out at the
/// keep-clear radius.
pub(in crate::catalogue::items) fn footing(
    size_x: f32,
    size_z: f32,
    center: [f32; 2],
    clearance: f32,
) -> Generator {
    foundation_block(
        size_x,
        size_z,
        center,
        crate::catalogue::items::foundation::required_depth(clearance),
    )
}

/// Round sibling of [`footing`] for drum / tower / dome entries.
pub(in crate::catalogue::items) fn footing_disc(radius: f32, clearance: f32) -> Generator {
    foundation_disc(
        radius,
        crate::catalogue::items::foundation::required_depth(clearance),
    )
}

/// Strong self-lit material — lamps, orbs, finials.
/// Flat quad in the local XZ plane, `size` = `[x_extent, z_extent]`, normal
/// `+Y`. Stand it up with [`quat_x`]`(-FRAC_PI_2)` to face `-Z` — that maps
/// the quad's local Z extent onto world Y, so `size` reads as
/// `[width, height]` for a wall opening.
pub(in crate::catalogue::items) fn plane(
    size: [f32; 2],
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Plane {
        size: Fp2(size),
        common: PrimCommon::with_material(material),
        subdivisions: 0,
    }
}

/// The room owner's profile picture, as a **square** panel (#975).
///
/// This is the one piece every themed identity monument shares: a
/// [`Sign`](GeneratorKind::Sign) whose source is
/// [`DidPfp`](crate::pds::SignSource::DidPfp), so the engine fetches
/// `app.bsky.actor.getProfile` for that DID and follows the avatar URL. The
/// reference is *live* — the owner changes their picture and it appears next
/// session without the record being rewritten — and every panel pointing at
/// one DID coalesces onto a single HTTPS round trip in the shared
/// `BlobImageCache`.
///
/// # The rules, and why the signature is what it is
///
/// 1. **Square, always.** A profile picture is square; stretched onto an
///    oblong panel it distorts a person's face, which is the one subject
///    where distortion is unmistakable. The size is therefore *one scalar*
///    rather than a pair — the aspect cannot be got wrong at a call site.
/// 2. **`uv_scale` stays `1.0`.** Sign images upload clamp-to-edge, and the
///    panel mesh already spans the image exactly once. A scale above one is a
///    *crop*, not a tile: it shrinks the image into a corner and smears the
///    border across the rest. The [`window_card`](super::material::window_card) rule, for the same reason.
/// 3. **`unlit`.** A portrait that goes black at dusk defeats the point, and
///    a face lit by the scene's sun reads as a lit *object* rather than as an
///    image. Portal's own pfp face made the same call.
/// 4. **Single-sided.** The image would be mirrored on the back. Put a
///    backing plate behind the panel — which every monument wants anyway, as
///    the plate the portrait is fixed to.
/// 5. **The tint is pure white, always.** `base_color` *multiplies* the
///    fetched image, so any other colour silently stains the owner's face —
///    a themed "blank" tint looked right on an empty panel and turned every
///    real portrait sepia, blue or half-black the moment one loaded (#976).
///    The blank state is therefore a white square, and the *frame* is what
///    carries the theme. There is deliberately no parameter for this.
///
/// # Orientation, and why this returns a positioned node
///
/// The panel is a flat quad in the local XZ plane, and the rotation that
/// stands it up is **`quat_x(FRAC_PI_2)`** — not the `-FRAC_PI_2` that
/// stands up an ordinary [`plane`], which is the trap this helper now closes
/// by applying the rotation itself.
///
/// The mesh's wound front face is `−Y` (see `world_builder::sign`), so the
/// negative rotation turns the panel's visible side to `+Z`, *away* from
/// whoever the prop faces, and maps the image's downward axis to world `+Y`
/// — backwards and upside-down at once, which is exactly how all 24
/// monuments shipped before #976. The positive rotation puts the front on
/// `−Z`, `V` on world `−Y` and `U` on world `−X`, which is the viewer's
/// right.
pub(in crate::catalogue::items) fn pfp_panel(
    did: &str,
    side_m: f32,
    center: [f32; 3],
) -> Generator {
    let kind = GeneratorKind::Sign {
        source: crate::pds::SignSource::DidPfp {
            did: did.to_string(),
        },
        size: Fp2([side_m, side_m]),
        // The legacy UV window, written at the identity so the sanitizer has
        // nothing to fold and an older client still spans the image once.
        uv_repeat: Fp2([1.0, 1.0]),
        uv_offset: Fp2([0.0, 0.0]),
        material: SovereignMaterialSettings {
            // See rule 5 — anything but white stains the portrait.
            base_color: Fp3([1.0, 1.0, 1.0]),
            roughness: Fp(0.55),
            metallic: Fp(0.0),
            // See rule 2 — the mesh already spans the image once.
            uv_scale: Fp(1.0),
            // The fetched image *is* the texture; a procedural one here would
            // be painted over the moment the blob lands.
            texture: SovereignTextureConfig::None,
            ..Default::default()
        },
        double_sided: false,
        alpha_mode: crate::pds::AlphaModeKind::Opaque,
        unlit: true,
        texture_filter: crate::pds::TextureFilter::default(),
    };
    prim(kind, center, quat_x(std::f32::consts::FRAC_PI_2))
}

/// The rotation that turns a prim's own axis (`+Y`) onto the unit direction
/// `dir` — the one place this family converts "it points that way" into a
/// quaternion (#972 lesson 23 for authoring: a hand-rolled rotate is a coin
/// flip, and it was flipped three times in one file before [`strut`]
/// existed).
///
/// Axis-angle: axis = `Ŷ × d̂`, angle = `atan2(|Ŷ × d̂|, Ŷ · d̂)`, packed via
/// the half-angle. Straight up is the identity, straight down a half-turn
/// about X. [`strut`] uses it for a bar between two points; a steering wheel
/// or a dish aimed at a driver uses it directly, because a torus's ring
/// normal is its local `+Y` too.
pub(in crate::catalogue::items) fn aim_y(dir: [f32; 3]) -> Fp4 {
    let d = dir;
    // axis = Y × d = (d.z, 0, -d.x); its length is sin(angle), d.y is cos.
    let (ax, az) = (d[2], -d[0]);
    let sin_a = (ax * ax + az * az).sqrt();
    if sin_a < 1e-5 {
        if d[1] >= 0.0 {
            id_quat() // straight up: the prim's own axis already
        } else {
            Fp4([1.0, 0.0, 0.0, 0.0]) // straight down: half-turn about X
        }
    } else {
        let angle = sin_a.atan2(d[1]);
        let (half_s, half_c) = (angle * 0.5).sin_cos();
        Fp4([ax / sin_a * half_s, 0.0, az / sin_a * half_s, half_c])
    }
}

/// A cylinder spanning two world points — the catalogue's ONE conversion
/// from "this rope/spar/shore runs from A to B" into a rotation.
///
/// This exists because the conversion was hand-rolled three times in one file
/// and every one of them was wrong in a different way (#1028): a fall whose
/// yaw ignored the fore-and-aft component of its own run, a shore leaning
/// away from the hull it propped (`quat_z(-lean)` where the handedness wanted
/// `+`), and a set of capstan bars "laid flat" by snapping to whichever
/// quarter turn was nearest. Each looked plausible at three of four angles.
/// #972 lesson 23 named this failure for guards — a hand-rolled rotate is a
/// coin flip — and the same is true of authoring.
///
/// The rotation is the axis-angle turn from the cylinder's own axis (`+Y`)
/// onto the run: axis = `Ŷ × d̂`, angle = `atan2(|Ŷ × d̂|, Ŷ · d̂)`, packed as
/// a quaternion via the half-angle. Degenerate runs (straight up, straight
/// down) fall out naturally: up is the identity, down is a half-turn about X.
///
/// Returns a leaf prim centred on the midpoint. A leaf, deliberately — a
/// strut carries nothing, so its rotation displaces nothing, which keeps
/// every translation-only guard sound (#972 lesson 22).
pub(in crate::catalogue::items) fn strut(
    from: [f32; 3],
    to: [f32; 3],
    radius: f32,
    resolution: u32,
    material: SovereignMaterialSettings,
) -> Generator {
    let v = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-4);
    let rotation = aim_y([v[0] / len, v[1] / len, v[2] / len]);
    prim(
        cylinder_tapered(radius, len, resolution, 0.0, material),
        [
            (from[0] + to[0]) * 0.5,
            (from[1] + to[1]) * 0.5,
            (from[2] + to[2]) * 0.5,
        ],
        rotation,
    )
}

/// Default clear spacing between balusters, in metres — see [`railing`].
///
/// Real balustrades run a 100 mm gap, which on a twelve-metre boardwalk is
/// ninety-odd prims for a handrail. This is the coarsest pitch that still
/// reads as *balusters* rather than as a ladder at the distance these props
/// are seen from, and it is a parameter rather than a constant because a
/// two-metre porch run wants a tighter one than a promenade does.
pub(in crate::catalogue::items) const BALUSTER_PITCH: f32 = 0.42;

/// An open railing: two horizontal rails, balusters between them, and a
/// heavier post at each end (#972).
///
/// This exists because the same wrong thing was built four times. A railing
/// authored as **one slab** — a 0.55 m plate on the hotel's balconies, a
/// 0.5 m plate along the beach house's porch, a single bar with no posts at
/// all on the boardwalk and the lifeguard tower — reads as a parapet *wall*,
/// and a parapet wall in front of a window hides the one thing the opening
/// was cut for. What makes a railing read as a railing is that you can see
/// through it, which is a property of having gaps, which is a property of
/// having balusters.
///
/// `from` and `to` are the ends of the run at the level it stands on (the
/// deck top, the balcony slab); they must share a `y` and lie on a common `X`
/// or `Z` axis, which is every railing in this family. `height` is measured
/// from that level to the top of the handrail.
///
/// Returns a flat list, so the caller decides what it hangs off — normally
/// [`nest`]ed under the deck it stands on, which is also what makes the
/// footprint guards able to check it.
pub(in crate::catalogue::items) fn railing(
    from: [f32; 3],
    to: [f32; 3],
    height: f32,
    pitch: f32,
    mat: SovereignMaterialSettings,
) -> Vec<Generator> {
    debug_assert!(
        (from[1] - to[1]).abs() < 1e-4,
        "a railing run is level; {from:?} and {to:?} are not"
    );
    let dx = (to[0] - from[0]).abs();
    let dz = (to[2] - from[2]).abs();
    let along_x = dx >= dz;
    debug_assert!(
        if along_x { dz < 1e-3 } else { dx < 1e-3 },
        "a railing run is axis-aligned; {from:?} → {to:?} is diagonal"
    );
    let len = if along_x { dx } else { dz };
    let base = from[1];
    let mid = [(from[0] + to[0]) * 0.5, base, (from[2] + to[2]) * 0.5];
    /// Handrail and baluster stock, and the heavier end post.
    const BAR: f32 = 0.07;
    const POST: f32 = 0.11;
    // A rail's own thickness has to come out of the run, or two railings
    // meeting at a corner overlap by exactly one post.
    let span = (len - POST).max(0.1);
    let rail = |cross: f32| -> [f32; 3] {
        if along_x {
            [span, BAR, cross]
        } else {
            [cross, BAR, span]
        }
    };

    let mut out = Vec::new();
    for y in [height * 0.28, height - BAR * 0.5] {
        out.push(prim(
            cuboid_tapered(rail(BAR), 0.0, mat.clone()),
            [mid[0], base + y, mid[2]],
            id_quat(),
        ));
    }
    let n = ((span / pitch).round() as i32).clamp(2, 24);
    for i in 0..n {
        let f = (i as f32 + 0.5) / n as f32 - 0.5;
        let at = if along_x {
            [mid[0] + f * span, base + height * 0.5, mid[2]]
        } else {
            [mid[0], base + height * 0.5, mid[2] + f * span]
        };
        out.push(prim(
            cuboid_tapered([BAR * 0.7, height - BAR, BAR * 0.7], 0.0, mat.clone()),
            at,
            id_quat(),
        ));
    }
    for end in [from, to] {
        out.push(prim(
            solid(cuboid_tapered([POST, height, POST], 0.0, mat.clone())),
            [end[0], base + height * 0.5, end[2]],
            id_quat(),
        ));
    }
    out
}
