//! Shared construction vocabulary for primitive-built catalogue
//! entries (lighthouse, stone circle, ziggurat, observatory).
//!
//! The shape-grammar entries (villa, castle, watchtower, temple)
//! don't need these — their geometry comes from the grammar
//! interpreter. The primitive entries assemble `Generator` trees by
//! hand, and these helpers keep that assembly at the "place a tapered
//! cylinder here" altitude instead of struct-literal plumbing.

use crate::pds::generator::UvMapping;
use crate::pds::{
    Fp, Fp2, Fp3, Fp4, Generator, GeneratorKind, SovereignMaterialSettings, TortureParams,
    TransformData,
};

/// Wrap a kind into a childless node at `translation` / `rotation`.
pub(super) fn prim(kind: GeneratorKind, translation: [f32; 3], rotation: Fp4) -> Generator {
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

pub(super) fn id_quat() -> Fp4 {
    Fp4([0.0, 0.0, 0.0, 1.0])
}

/// Like [`prim`] but with a non-identity scale — e.g. a flattened sphere for a
/// cloud-pruned foliage pad or a smooth ellipsoid pod.
pub(super) fn prim_scaled(
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
pub(super) fn assemble(mut prims: Vec<Generator>) -> Generator {
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

/// Rotation around X — tilts ramps and dome slits.
pub(super) fn quat_x(angle_rad: f32) -> Fp4 {
    let half = angle_rad * 0.5;
    Fp4([half.sin(), 0.0, 0.0, half.cos()])
}

/// Rotation around Y — yaws monoliths to face the circle centre.
pub(super) fn quat_y(angle_rad: f32) -> Fp4 {
    let half = angle_rad * 0.5;
    Fp4([0.0, half.sin(), 0.0, half.cos()])
}

/// Rotation around Z — lays a Y-axis cylinder onto the horizontal X axis
/// (`FRAC_PI_2`), e.g. a conduit / pipe run spanning left-to-right.
pub(super) fn quat_z(angle_rad: f32) -> Fp4 {
    let half = angle_rad * 0.5;
    Fp4([0.0, 0.0, half.sin(), half.cos()])
}

/// Hamilton product of two `[x, y, z, w]` rotations — the combined rotation
/// that applies `b` first, then `a`. Composing two unit quaternions stays
/// unit, so the result needs no renormalisation.
pub(super) fn quat_mul(a: Fp4, b: Fp4) -> Fp4 {
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
pub(super) fn cuboid_tapered(
    size: [f32; 3],
    taper: f32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Cuboid {
        size: Fp3(size),
        uv_mapping: UvMapping::default(),
        solid: false,
        material,
        torture: TortureParams {
            taper: Fp2([taper, taper]),
            ..Default::default()
        },
    }
}

/// Cuboid with independent X/Z taper — a ridged roof or asymmetric frustum.
/// Each component pinches the top on that axis (`0.0` keeps the full width,
/// `1.0` pinches it to a line), so `[0.1, 0.9]` yields a long ridge along X
/// with steep slopes on the Z sides; the uniform [`cuboid_tapered`] can only
/// make a square-topped frustum or a point.
pub(super) fn cuboid_tapered_xz(
    size: [f32; 3],
    taper_xz: [f32; 2],
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Cuboid {
        size: Fp3(size),
        uv_mapping: UvMapping::default(),
        solid: false,
        material,
        torture: TortureParams {
            taper: Fp2(taper_xz),
            ..Default::default()
        },
    }
}

pub(super) fn cylinder_tapered(
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
        solid: false,
        material,
        torture: TortureParams {
            taper: Fp2([taper, taper]),
            ..Default::default()
        },
    }
}

pub(super) fn sphere(
    radius: f32,
    resolution: u32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Sphere {
        radius: Fp(radius),
        resolution,
        solid: false,
        material,
        torture: TortureParams::default(),
    }
}

/// Barr superellipsoid — the rounded-mass workhorse. `exponent_ns` shapes
/// the north–south (latitude) profile, `exponent_ew` the east–west
/// cross-section: `0.2` is a hard box, `~0.65` a filled pillow (sandbags,
/// cushions, bedrolls), `1.0` a true ellipsoid, `2.5` a pinched octahedron.
/// Reach for it where a cuboid reads too hard and a scaled sphere too soft.
pub(super) fn superellipsoid(
    half_extents: [f32; 3],
    exponent_ns: f32,
    exponent_ew: f32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Superellipsoid {
        half_extents: Fp3(half_extents),
        uv_mapping: UvMapping::default(),
        exponent_ns: Fp(exponent_ns),
        exponent_ew: Fp(exponent_ew),
        latitudes: 12,
        longitudes: 18,
        solid: false,
        material,
        torture: TortureParams::default(),
    }
}

pub(super) fn cone(
    radius: f32,
    height: f32,
    resolution: u32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Cone {
        radius: Fp(radius),
        height: Fp(height),
        resolution,
        solid: false,
        material,
        torture: TortureParams::default(),
    }
}

pub(super) fn torus(
    minor_radius: f32,
    major_radius: f32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Torus {
        minor_radius: Fp(minor_radius),
        major_radius: Fp(major_radius),
        minor_resolution: 10,
        major_resolution: 28,
        solid: false,
        material,
        torture: TortureParams::default(),
    }
}

/// Hollow cylinder — a pipe / ring / conduit / halo. `inner_radius` is the
/// bore (`< radius`); annular caps close the ends. Axis along Y like
/// [`cylinder_tapered`].
pub(super) fn tube(
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
        solid: false,
        material,
        torture: TortureParams::default(),
    }
}

/// Helical tube — a spring / data-stream coil / spiral rail. `radius` is the
/// coil radius, `tube_radius` the wire thickness, `pitch` the vertical rise
/// per full turn, `turns` the revolution count. The coil climbs the Y axis,
/// centred on the origin (total height `turns * pitch`).
pub(super) fn helix(
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
        solid: false,
        material,
        torture: TortureParams::default(),
    }
}

/// Right-triangular prism — a ramp / awning / roof pitch / buttress. `size`
/// is the bounding box; the slope rises from the front-bottom (`+Z`, `-Y`) to
/// the back-top (`-Z`, `+Y`) across the full width (X).
pub(super) fn wedge(size: [f32; 3], material: SovereignMaterialSettings) -> GeneratorKind {
    GeneratorKind::Wedge {
        size: Fp3(size),
        uv_mapping: UvMapping::default(),
        solid: false,
        material,
        torture: TortureParams::default(),
    }
}

/// Stamp the SL-style topology cuts onto a swept primitive (Sphere / Cylinder
/// / Cone / Torus / Tube): `path_cut` (`[begin, end]` kept angular fraction —
/// a half-torus arch, an orange-slice wedge), `profile_cut` (`[begin, end]`
/// kept latitude band — domes / bowls), and `hollow` (bore fraction).
/// Non-swept kinds pass through unchanged. Honoured by the unified sweep
/// mesher in `crate::world_builder::prim`.
pub(super) fn with_cut(
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

/// Mark a primitive kind solid so the spawner attaches its matching
/// collider — structural pieces players can stand on or bump into.
/// Decorative trim (railings, orbs, lamps) stays non-solid.
pub(super) fn solid(mut kind: GeneratorKind) -> GeneratorKind {
    match &mut kind {
        GeneratorKind::Cuboid { solid, .. }
        | GeneratorKind::Sphere { solid, .. }
        | GeneratorKind::Cylinder { solid, .. }
        | GeneratorKind::Capsule { solid, .. }
        | GeneratorKind::Cone { solid, .. }
        | GeneratorKind::Torus { solid, .. }
        // Superellipsoid carries an analytical collider too (a coarse
        // sampled convex hull), so marking it solid is as cheap here as it
        // is for the box it replaces.
        | GeneratorKind::Superellipsoid { solid, .. } => *solid = true,
        _ => {}
    }
    kind
}

/// Shared foundation material — neutral rough-cut stone that sits
/// under any of the structure palettes.
pub(super) fn foundation_mat() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.45, 0.43, 0.40]),
        roughness: Fp(0.95),
        uv_scale: Fp(2.0),
        texture: crate::pds::SovereignTextureConfig::Rock(
            crate::pds::SovereignRockConfig::default(),
        ),
        ..Default::default()
    }
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
pub(super) fn foundation_block(
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
pub(super) fn foundation_disc(radius: f32, depth: f32) -> Generator {
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

/// Strong self-lit material — lamps, orbs, finials.
/// Flat quad in the local XZ plane, `size` = `[x_extent, z_extent]`, normal
/// `+Y`. Stand it up with [`quat_x`]`(-FRAC_PI_2)` to face `-Z` — that maps
/// the quad's local Z extent onto world Y, so `size` reads as
/// `[width, height]` for a wall opening.
pub(super) fn plane(size: [f32; 2], material: SovereignMaterialSettings) -> GeneratorKind {
    GeneratorKind::Plane {
        size: Fp2(size),
        uv_mapping: UvMapping::fit(),
        subdivisions: 0,
        solid: false,
        material,
        torture: TortureParams::default(),
    }
}

/// Glazing for a wall opening: the `Window` generator's alpha card, on the
/// material settings it actually wants.
///
/// **The `Window` texture is not a window you stick on a wall — it is the
/// pane that fills a hole you already cut.** Four properties drive that,
/// and every one of them is silently wrong if the card is used as a face
/// plate on a solid box:
///
/// 1. **It is an alpha card, and the panes are cut away.** The generator
///    writes opaque alpha for the frame and mullions and `glass_opacity`
///    for the glass; upstream renders every card at `AlphaMode::Mask(0.5)`.
///    So any `opacity` below `0.5` discards the pane pixels outright — the
///    card becomes a frame with real holes in it. Stuck on a solid wall
///    those holes show the wall; spanning an opening they show what is
///    behind it, which is the entire point. Author an interior worth
///    seeing, or the holes show sky.
/// 2. **`uv_scale` must stay `1.0`.** Cards upload clamp-to-edge, not
///    repeating. A `uv_scale` above one runs the UVs off the end of the
///    card and smears its last texel across the remainder — one card is
///    one opening, always.
/// 3. **One card, one flat quad.** On a cuboid every face takes the same
///    texture, so a "window slab" grows windows on its sides, top and
///    bottom. Use [`plane`].
/// 4. **Pane counts carry the scale.** The card stretches to whatever quad
///    it lands on, so `panes_x`/`panes_y` are what tell the viewer how big
///    the opening is. Pick them against the opening's real aspect ratio so
///    the panes come out roughly square.
///
/// `frame_width` and `mullion_thickness` are fractions of the card, so a
/// wide opening wants a smaller `frame_width` than a square one if the
/// frame is to look the same thickness all round.
pub(super) fn window_card(
    frame_color: [f32; 3],
    panes_x: u32,
    panes_y: u32,
    opacity: f32,
    frame_width: f32,
) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(frame_color),
        roughness: Fp(0.35),
        metallic: Fp(0.2),
        // See rule 2 — cards are clamp-to-edge; anything but 1.0 smears.
        uv_scale: Fp(1.0),
        texture: crate::pds::SovereignTextureConfig::Window(crate::pds::SovereignWindowConfig {
            panes_x,
            panes_y,
            frame_width: crate::pds::Fp64(frame_width as f64),
            glass_opacity: crate::pds::Fp64(opacity as f64),
            grime_level: crate::pds::Fp64(0.18),
            color_frame: Fp3(frame_color),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// `uv_scale` for a texture whose repeating patch should measure `tile_m`
/// metres on a side (#936).
///
/// Since #933 `uv_scale` is *tiles per metre*, which is an awkward number to
/// author in — you think "a brick course is about 7 cm", not "14.5 repeats
/// per metre". This converts, so material helpers read as physical sizes.
///
/// **`tile_m` is the size of the generator's whole repeating patch, not of
/// one feature in it.** The generators bake several features per tile —
/// `SovereignBrickConfig::scale` is brick columns per tile,
/// `SovereignPlankConfig::plank_count` planks per tile,
/// `SovereignCorrugatedConfig::ridges` ridges per tile — so the tile size is
/// the feature size times that count. Getting this backwards is the easy
/// mistake: it makes brickwork a hundred times too fine, and at that density
/// the mip chain washes it to flat colour rather than showing an obvious
/// error.
pub(super) fn tiles_per_metre(tile_m: f32) -> Fp {
    Fp(1.0 / tile_m.max(1e-4))
}

/// Physical tile sizes, in metres, for the surface generators the catalogue
/// uses on **primitive** geometry. Each is the generator's default feature
/// count times a real-world feature size, so one constant reads the same on
/// a 0.8 m pier and an 8 m wall.
///
/// # Only primitives
///
/// The metre convention (#933–#938) is a property of `build_primitive_mesh`.
/// `LSystem` and `Shape` geometry is meshed by its own pipeline and still
/// carries that pipeline's own UVs, so a material used on a tree trunk or a
/// grammar-built wall must keep its hand-tuned `uv_scale` — converting it
/// would rescale against a parameterisation that never changed. Check what
/// a material is actually applied to before touching it.
///
/// # Alpha cards
///
/// Deliberately absent. `Window`, `StainedGlass`, `IronGrille`, `ChainLink`
/// and the foliage/sprite generators upload clamp-to-edge and must span
/// their quad exactly once, so they hold `uv_scale` at `1.0` and pick
/// `UvMapping::Fit` instead.
///
/// # The rest of the table
///
/// Constants land here as each theme is converted, so nothing sits unused.
/// The sizing already worked out, for when they do:
///
/// | generator | features / tile | feature | tile |
/// |---|---|---|---|
/// | Ashlar | 4 × 4 blocks | 450 mm block | 1.8 m |
/// | Plank | 5 planks | 200 mm board | 1.0 m |
/// | Cobblestone | 6 stones | 150 mm cobble | 0.9 m |
/// | Shingle | 5 courses | 300 mm shingle | 1.5 m |
/// | Corrugated | 8 ridges | 76 mm sheet pitch | 0.6 m |
/// | Thatch | — reads as a mass | | 1.2 m |
/// | Stucco | — near-scaleless | | 2.0 m |
/// | Marble | veining | | 2.0 m |
/// | Rock | rock face | | 1.5 m |
/// | Ground / Sand / Snow / Ice | granular | | 2.0 m |
/// | Asphalt | coarse aggregate | | 3.0 m |
/// | Fabric | the weave *is* the feature | | 0.5 m |
/// | Pavers | paving slabs | | 1.2 m |
pub(super) mod tile {
    /// 4 brick columns per tile at a 215 mm brick.
    pub(in crate::catalogue::items) const BRICK: f32 = 0.86;
    /// Board-formed concrete — the board marks are the feature.
    pub(in crate::catalogue::items) const CONCRETE: f32 = 2.4;
    /// Sheet metal — plate seams and brushing.
    pub(in crate::catalogue::items) const METAL: f32 = 1.2;
}

pub(super) fn glow(color: [f32; 3], strength: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        emission_color: Fp3(color),
        emission_strength: Fp(strength),
        roughness: Fp(0.4),
        metallic: Fp(0.1),
        ..Default::default()
    }
}

/// Assert that `sanitize_generator` leaves a primitive-built entry
/// geometrically untouched. Rotations are compared with an epsilon
/// because the sanitiser renormalises every quaternion, which can
/// shift the last ulp of an already-normalised rotation; everything
/// else must be bit-identical.
#[cfg(test)]
pub(super) fn assert_sanitize_stable(built: &Generator, name: &str) {
    fn tree_eq(a: &Generator, b: &Generator, name: &str) {
        assert_eq!(a.kind, b.kind, "{name}: kind rewritten by sanitiser");
        assert_eq!(
            a.transform.translation, b.transform.translation,
            "{name}: translation rewritten"
        );
        assert_eq!(
            a.transform.scale, b.transform.scale,
            "{name}: scale rewritten"
        );
        for i in 0..4 {
            assert!(
                (a.transform.rotation.0[i] - b.transform.rotation.0[i]).abs() < 1e-5,
                "{name}: rotation rewritten beyond renormalisation: {:?} vs {:?}",
                a.transform.rotation,
                b.transform.rotation
            );
        }
        assert_eq!(a.children.len(), b.children.len(), "{name}: child dropped");
        for (ca, cb) in a.children.iter().zip(b.children.iter()) {
            tree_eq(ca, cb, name);
        }
    }
    let mut sanitized = built.clone();
    crate::pds::sanitize_generator(&mut sanitized);
    tree_eq(built, &sanitized, name);
}

/// Walk a built tree and report whether any primitive is strongly emissive
/// (emission strength > 1.0) — the shared "did the kit's firelit hero keep
/// its glow?" check the per-theme kits assert on (forge fire, saloon lamps,
/// brazier coals, …), so escalation's broken-emissive ruin pass has something
/// to snuff.
#[cfg(test)]
pub(super) fn has_emissive(g: &crate::pds::Generator) -> bool {
    use crate::pds::GeneratorKind::*;
    let own = match &g.kind {
        Cuboid { material, .. }
        | Cylinder { material, .. }
        | Sphere { material, .. }
        | Cone { material, .. }
        | Torus { material, .. }
        | Capsule { material, .. } => material.emission_strength.0 > 1.0,
        _ => false,
    };
    own || g.children.iter().any(has_emissive)
}
