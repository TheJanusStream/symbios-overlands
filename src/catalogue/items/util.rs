//! Shared construction vocabulary for primitive-built catalogue
//! entries (lighthouse, stone circle, ziggurat, observatory).
//!
//! The shape-grammar entries (villa, castle, watchtower, temple)
//! don't need these — their geometry comes from the grammar
//! interpreter. The primitive entries assemble `Generator` trees by
//! hand, and these helpers keep that assembly at the "place a tapered
//! cylinder here" altitude instead of struct-literal plumbing.

use crate::pds::generator::{FaceKey, FaceOverride, UvMapping};
use crate::pds::sanitize::limits::MAX_FACE_OVERRIDES;
use crate::pds::{
    Fp, Fp2, Fp3, Fp4, Fp64, Generator, GeneratorKind, SovereignMaterialSettings,
    SovereignTextureConfig, TortureParams, TransformData,
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
pub(super) fn nest(mut parent: Generator, children: Vec<Generator>) -> Generator {
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
        faces: Vec::new(),
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
        faces: Vec::new(),
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
        uv_mapping: UvMapping::fit(),
        material,
        faces: Vec::new(),
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
        uv_mapping: UvMapping::fit(),
        material,
        faces: Vec::new(),
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
        faces: Vec::new(),
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
        uv_mapping: UvMapping::fit(),
        material,
        faces: Vec::new(),
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
        uv_mapping: UvMapping::fit(),
        material,
        faces: Vec::new(),
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
        uv_mapping: UvMapping::fit(),
        material,
        faces: Vec::new(),
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
        uv_mapping: UvMapping::fit(),
        material,
        faces: Vec::new(),
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
        faces: Vec::new(),
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
pub(super) fn with_face(
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
        uv_scale: tiles_per_metre(tile::ROCK),
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
        faces: Vec::new(),
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
            frame_width: fp64_on_grid(frame_width),
            glass_opacity: fp64_on_grid(opacity),
            grime_level: crate::pds::Fp64(0.18),
            color_frame: Fp3(frame_color),
            ..Default::default()
        }),
        ..Default::default()
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
///    border across the rest. The [`window_card`] rule, for the same reason.
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
pub(super) fn pfp_panel(did: &str, side_m: f32, center: [f32; 3]) -> Generator {
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

/// Wrap an `f32` in an [`Fp64`] snapped to the fixed-point
/// wire grid.
///
/// `Fp64` holds a full-precision `f64` but serialises as `round(x · 10000)`,
/// and its `PartialEq` compares the raw `f64`. So a value promoted from
/// `f32` — e.g. `0.3_f32 as f64` = 0.30000001… — is *unequal* to the same
/// number written as an `f64` literal, yet both serialise to `3000`. The
/// default-eliding wire format then makes opposite keep/omit decisions
/// before and after a round-trip whenever the value equals a config default
/// (glass_opacity's default is 0.30), so the record fails its own equality
/// check (#943). Snapping to the grid here makes the value canonical, so it
/// round-trips and compares against defaults the way the wire does.
fn fp64_on_grid(x: f32) -> crate::pds::Fp64 {
    let scale = crate::pds::FP_SCALE as f64;
    crate::pds::Fp64((x as f64 * scale).round() / scale)
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
/// # Tiles versus features
///
/// Most constants here are a whole tile, because the generator's feature
/// count barely moves between uses. Where that count *does* swing widely,
/// the constant is one **feature** instead and the call site multiplies by
/// the config's own count — [`tile::BRICK_COURSE`], [`tile::ASHLAR_BLOCK`],
/// [`tile::PLANK_BOARD`], [`tile::CORRUGATED_PITCH`]. That keeps the brick (or board,
/// or rib) the same physical size between two neighbouring buildings, which
/// is the property that actually reads; pinning the tile instead would pin
/// the feature to whatever `1 / count` happened to be.
///
/// # Not for L-systems
///
/// The metre convention covers `build_primitive_mesh` **and** `Shape`: the
/// shape mesher emits world-space UVs too (`bevy_symbios_shape`'s
/// `build_profiled_mesh` scales UVs by the scope size), so grammar
/// materials convert exactly like primitive ones. Cards are handled for
/// you there — the shape pipeline derives `stretch_uvs` from the material's
/// own texture, which is that pipeline's `UvMapping::Fit` (#939).
///
/// `LSystem` is the real exception. Its mesher parameterises U as `0..1`
/// around the tube and V as arc-length over circumference, so a trunk's
/// texel density is a function of its radius, not of metres. A `Bark` or
/// foliage material on an L-system must keep its hand-tuned `uv_scale` —
/// converting it would rescale against a parameterisation that never
/// changed.
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
/// | Marble | veining | | 2.0 m |
/// | Rock | rock face | | 1.5 m |
/// | Ground / Sand / Snow / Ice | granular | | 2.0 m |
/// | Pavers | paving slabs | | 1.2 m |
pub(super) mod tile {
    /// One brick column, for configs whose `SovereignBrickConfig::scale`
    /// departs from the usual 5 (mudbrick coursing runs 14). Multiply by
    /// that count; at the usual count, prefer [`BRICK`].
    pub(in crate::catalogue::items) const BRICK_COURSE: f32 = 0.172;
    /// The common 5-column brick config.
    pub(in crate::catalogue::items) const BRICK: f32 = BRICK_COURSE * 5.0;
    /// Board-formed concrete — the board marks are the feature.
    pub(in crate::catalogue::items) const CONCRETE: f32 = 2.4;
    /// Sheet metal — plate seams and brushing.
    pub(in crate::catalogue::items) const METAL: f32 = 1.2;
    /// One dressed-ashlar block, for configs whose
    /// `SovereignAshlarConfig::cols` departs from the usual 4. Multiply by
    /// that count; at the usual count, prefer [`ASHLAR`].
    pub(in crate::catalogue::items) const ASHLAR_BLOCK: f32 = 0.45;
    /// The common 4-column ashlar config.
    pub(in crate::catalogue::items) const ASHLAR: f32 = ASHLAR_BLOCK * 4.0;
    /// Sawn-board **width**, not a whole tile — multiply by the config's
    /// `plank_count`.
    ///
    /// Like [`CORRUGATED_PITCH`], `SovereignPlankConfig::plank_count` ranges
    /// too widely (4 on a fence, 9 on clapboard) for one tile size to serve:
    /// pinning the tile pins the *board* to whatever 1/count happens to be,
    /// so barn siding came out at 125 mm and mipped to flat red. Fixing the
    /// board width instead means a fence board and a barn board are the same
    /// size, which is the property that actually matters between neighbouring
    /// buildings.
    pub(in crate::catalogue::items) const PLANK_BOARD: f32 = 0.167;
    /// `SovereignCobblestoneConfig::scale` stones per tile — 6 stones at a
    /// 150 mm fieldstone cobble.
    pub(in crate::catalogue::items) const COBBLE: f32 = 0.9;
    /// `SovereignShingleConfig::scale` courses per tile — 5 courses at a
    /// 300 mm slate or shingle.
    pub(in crate::catalogue::items) const SHINGLE: f32 = 1.5;
    /// Thatch reads as a straw *mass* rather than a countable feature; sized
    /// so a bundle layer lands near a 150 mm exposure.
    pub(in crate::catalogue::items) const THATCH: f32 = 1.2;
    /// Stucco / lime-wash daub is near-scaleless — sized large so the render
    /// stays a surface tone rather than becoming visible noise.
    pub(in crate::catalogue::items) const STUCCO: f32 = 2.0;
    /// One woven thread, for configs whose
    /// `SovereignFabricConfig::thread_count` departs from the usual 20 (fine
    /// silk runs 40, sailcloth 16). Multiply by that count; at the usual
    /// count, prefer [`FABRIC`].
    pub(in crate::catalogue::items) const FABRIC_THREAD: f32 = 0.025;
    /// The common 20-thread cloth. The weave *is* the feature, so this is
    /// the tightest tile in the table.
    pub(in crate::catalogue::items) const FABRIC: f32 = FABRIC_THREAD * 20.0;

    /// Rough rock face — undressed rubble masonry and natural stone.
    pub(in crate::catalogue::items) const ROCK: f32 = 1.5;
    // No LOG_END constant, deliberately: `LogEnd` is registered `Card`
    // upstream, so it belongs in the alpha-card group below — one slice per
    // quad at `uv_scale` 1.0 — not in this table. It was briefly given a
    // tile here during #936 and that was wrong (#940).
    /// Marble veining — a figure rather than a countable feature, sized to
    /// the block it faces.
    pub(in crate::catalogue::items) const MARBLE: f32 = 2.0;
    /// Ground / sand / snow / ice — granular, near-scaleless, and always on
    /// the largest surfaces in a scene, so it is sized to stay a tone.
    pub(in crate::catalogue::items) const GROUND: f32 = 2.0;
    /// Sand. Aliases [`GROUND`] — same granular reasoning — but named
    /// so a beach material does not read as though it borrowed soil's tile.
    pub(in crate::catalogue::items) const SAND: f32 = GROUND;
    /// Snow / ice. Aliases [`GROUND`], as [`SAND`] does.
    pub(in crate::catalogue::items) const ICE: f32 = GROUND;
    /// Asphalt — coarse aggregate and crack noise, sized large because the
    /// surfaces it lands on (forecourts, lots) are the biggest in the kit
    /// and a tight tile turns them into visible repetition.
    pub(in crate::catalogue::items) const ASPHALT: f32 = 3.0;
    /// Corrugated steel **ridge pitch**, not a whole tile — multiply by the
    /// config's `ridges` to get the tile.
    ///
    /// This one generator has to be authored the long way round because
    /// `SovereignCorrugatedConfig::ridges` is not roughly constant across
    /// uses the way the other counts are: roofing sheet runs ~14 ridges and
    /// a silo's structural ribbing ~24, against the 4–9 spread that lets
    /// `PLANK` and `SHINGLE` get away with a single tile size. Pinning one
    /// tile across that 3× spread would drive the pitch down to 25–43 mm —
    /// far under the ~76 mm of real sheet, and fine enough that the mip
    /// chain flattens it to bare colour.
    pub(in crate::catalogue::items) const CORRUGATED_PITCH: f32 = 0.076;
    /// Legibility multiplier for corrugated sheet on **broad** surfaces —
    /// a silo body, factory cladding, a dock wall (#936).
    ///
    /// The true pitch reads correctly on a roof or a small prop, but on a
    /// surface tens of metres across it falls below what the renderer ever
    /// resolves and mips to flat colour — the same washout the module doc
    /// warns about, arrived at from the honest direction. Ribbing is the
    /// whole silhouette signature of these forms, so it is drawn oversize
    /// rather than not at all. Multiply alongside `CORRUGATED_PITCH`; leave
    /// it off for roofs and props, which read fine at the real size.
    pub(in crate::catalogue::items) const CORRUGATED_BROAD: f32 = 3.0;
}

// --- Putting a tiling material into the world's frame ----------------------

/// The UV offset that puts **one face** of a prim into a shared world frame,
/// so neighbouring slabs sample one continuous pattern instead of each
/// restarting it at its own centre (#966 / #969).
///
/// The Box projection is prim-local *and* per-face: each of the six regions
/// reads its own pair of local axes, in its own sign convention — `(−x, −y)`
/// on a `−Z` wall, `(x, z)` on a top face, `(−z, −y)` on a `+X` side. It is
/// also **linear** in position, which is what makes this a one-liner: the
/// offset that turns a prim-local UV into a world-frame one is that very
/// same projection applied to the prim's own centre.
///
/// So a slab's pattern lines up with its neighbours' on the face this names,
/// and only that face. A sill wants `Top`, the outer return of a pier wants
/// the side it turns onto, and a prim's *base* material serves whichever
/// face people mostly look at — the rest are per-face overrides
/// ([`with_face`]).
///
/// Which is a shorter list than it first appears, because the four **side**
/// faces all put `V` on `−y`: turning a vertical corner, horizontal courses
/// line up on the base offset alone and only the column phase differs, which
/// matters solely where two slabs are *coplanar* (a pier and the side wall
/// behind it). Horizontal corners always need a wrap: a `Top` or `Bottom`
/// face reads depth where its neighbour reads height, so nothing about it
/// follows from the base. And a pattern with **no U features at all** —
/// unstaggered lap siding, see [`bonded_siding`] — needs no side wrap ever,
/// because `V = −y` is all there is to agree on.
pub(super) fn face_uv_offset(face: FaceKey, center: [f32; 3]) -> Fp2 {
    let [x, y, z] = center;
    Fp2(match face {
        FaceKey::SidePx => [-z, -y],
        FaceKey::SideNx => [z, -y],
        FaceKey::Top => [x, z],
        FaceKey::Bottom => [x, -z],
        FaceKey::SidePz => [x, -y],
        // `SideNz` — the usual hero-face convention — and anything else.
        _ => [-x, -y],
    })
}

/// Brick rows per texture tile — the `Brick` generator's `scale`. Ten rather
/// than the default five: the tile has to hold enough bricks that the seam
/// artifact below is rare, and rows and columns scale together.
const BOND_ROWS: f64 = 10.0;
/// Brick columns per tile, and the number of bricks a tile spans across a
/// wall.
///
/// Four is the smallest count that keeps the tile's U seam quiet. The
/// generator colours each brick by hashing its **raw** cell index, so a brick
/// straddling the seam is indexed `0` on one side and `cols` on the other and
/// renders as two half-bricks of different colour. One brick per tile per
/// staggered course always straddles; at two columns that was every fourth
/// brick on the wall, and the eye reads it immediately. Raising the count
/// dilutes it (and kills the two-brick colour repeat that banded walls into
/// vertical stripes) without changing the brick's size, since
/// [`bonded_brick`] derives `uv_scale` from the column count.
const BOND_COLS: f32 = 4.0;
/// Cell aspect. **Inverted from what the generator's own doc suggests**: it
/// derives columns as `scale × aspect_ratio` while `scale` *is* the row
/// count, so under this app's uniform metre mapping (a UV tile is square in
/// metres, #933) a value above 1 makes each cell taller than it is wide.
/// `0.4` gives 4 columns to 10 rows — a brick 2.5× longer than it is tall,
/// laid flat.
const BOND_ASPECT: f64 = BOND_COLS as f64 / BOND_ROWS;
/// Bond stagger per course, as a fraction of brick length — the classic
/// half-bond. The generator needs `scale × row_offset` to be a whole number
/// to tile cleanly in V; `10 × 0.5` is, where the kits' `5 × 0.5` was not.
const BOND_STAGGER: f64 = 0.5;
/// Per-brick colour jitter. It is what makes a wall read as fired clay rather
/// than paint, but it is also the *only* thing that makes a seam-straddling
/// brick visible — the two halves differ by up to twice this. Low enough that
/// the survivors read as shading, high enough that the wall still varies.
const BOND_VARIANCE: f64 = 0.15;

/// Re-lay a `Brick` material's courses **flat**, at a real brick's size, and
/// in the shared world course frame (#966 / #968 / #969).
///
/// `brick_len` is the brick's length in metres — `0.215` for a standard
/// brick, larger for block or adobe. The kit helpers' own sizing lays
/// whatever `1 / (tile × count)` happens to be, which came out at 172 mm and
/// small enough at street distance to mip toward flat colour.
///
/// # Laying the courses flat
///
/// The generator counts `scale` rows up V and `scale × aspect_ratio` columns
/// across U. Since #933 a UV tile is *square in metres*, so ten columns to
/// five rows makes every brick twice as tall as it is wide — upright, which
/// no bricklayer has ever produced. Flipping the aspect ([`BOND_ASPECT`])
/// turns the cell without turning the *bond*.
///
/// A 90° `uv_rotation` looks like the obvious fix and is not: it spins the
/// running bond with the bricks, so the stagger ends up between vertical
/// strips instead of between courses and the wall reads as continuous
/// vertical mortar lines running its full height. Rotation and a correct bond
/// are mutually exclusive here — the stagger is applied along U by the
/// generator itself.
///
/// # The seam the generator cannot hide
///
/// Per-brick colour comes from hashing the **raw** cell index, so the brick
/// that straddles a tile's U seam is hashed twice and renders as two
/// half-bricks of different colour. It is unavoidable at this level: a
/// running bond shifts each course by half a brick, so some course always
/// crosses the seam mid-brick, and only the generator itself could fix it (by
/// hashing the index *modulo* the column count, which would make both halves
/// agree). [`BOND_COLS`] and [`BOND_VARIANCE`] are chosen to make what
/// remains read as shading.
///
/// Non-`Brick` textures keep their config and gain only the offset, so this
/// is safe to funnel a whole wall through.
pub(super) fn bonded_brick(
    mut mat: SovereignMaterialSettings,
    brick_len: f32,
    face: FaceKey,
    center: [f32; 3],
) -> SovereignMaterialSettings {
    mat.uv_scale = tiles_per_metre(brick_len * BOND_COLS);
    mat.uv_offset = face_uv_offset(face, center);
    if let SovereignTextureConfig::Brick(cfg) = &mut mat.texture {
        cfg.scale = Fp64(BOND_ROWS);
        cfg.aspect_ratio = Fp64(BOND_ASPECT);
        cfg.row_offset = Fp64(BOND_STAGGER);
        cfg.cell_variance = Fp64(BOND_VARIANCE);
    }
    mat
}

/// Lay a `Plank` material as **unbroken courses** in the shared world frame —
/// lap siding, clapboard, a painted trim board.
///
/// # The end-joint grid
///
/// `PlankConfig::stagger` reads like a cosmetic de-correlation knob and is
/// not. Any value above `0.01` switches on a second joint grid the config
/// cannot size: the generator cuts **three** short boards per tile across U,
/// hard-coded, and staggers their ends per course. So a tile that holds ten
/// 167 mm courses also holds three 557 mm butt joints, and a wall of lap
/// siding comes out as a coarse 3.3:1 *masonry* grid — which is exactly what
/// the suburban house rendered as before #972's siding pass, and reads at a
/// glance as brick, not board.
///
/// Real siding is milled in 3–5 m lengths, so on a 10 m elevation there are a
/// couple of butt joints, not thirty. `stagger = 0` is therefore the honest
/// setting, not a compromise: it takes the end-joint path out entirely
/// (`c.stagger > 0.01` gates it) and leaves the courses running the full
/// width. Per-course grain de-correlation survives — that comes from the
/// row's own hash, not from the stagger.
///
/// # And what that buys at the corners
///
/// With no U features left, the *only* thing an offset has to line up is `V`,
/// and all four side faces read `V = −y`. So unstaggered siding wraps a
/// vertical corner on its base offset alone and needs no per-face override
/// anywhere — where the same wall in brick wanted one per visible corner (see
/// [`bonded_brick`] and [`face_uv_offset`]).
///
/// Non-`Plank` textures keep their config and gain only the offset.
pub(super) fn bonded_siding(
    mut mat: SovereignMaterialSettings,
    face: FaceKey,
    center: [f32; 3],
) -> SovereignMaterialSettings {
    mat.uv_offset = face_uv_offset(face, center);
    if let SovereignTextureConfig::Plank(cfg) = &mut mat.texture {
        cfg.stagger = Fp64(0.0);
    }
    mat
}

/// The quarter turn [`bonded_boards`] applies, in degrees counter-clockwise.
const BOARD_TURN_DEG: f32 = 90.0;

/// Stand a `Plank` material's boards **upright** — board-and-batten, the
/// barn's cladding and the one thing the generator cannot lay by itself.
///
/// # Why this is not the `uv_rotation` trap
///
/// [`bonded_brick`] documents the rule that a 90° `uv_rotation` is *not* a
/// reorientation tool, because it spins a pattern's internal logic along with
/// its cells: on brick it turns the running bond on its side and the wall
/// becomes continuous vertical mortar lines. That rule is about patterns with
/// **U features**. Plank's only U feature is the hard-coded three-butt-joints
/// grid that [`bonded_siding`] switches off, and with `stagger = 0` there is
/// nothing structured left along U at all — the boards are pure bands up V,
/// the grain is isotropic noise and the knots are a Worley field. So the
/// quarter turn has no bond to break here, and it is the *only* way to get
/// vertical boards out of a generator that lays courses up V.
///
/// Both directions still tile: the generator's noise is toroidal in U and V
/// and `plank_count` is integral, so a square tile turned through 90° maps
/// onto itself seamlessly.
///
/// # The offset has to turn with it
///
/// The material's UV transform composes as `scale · (rotate · uv + offset)`,
/// so the world-frame offset is applied **after** the rotation and has to be
/// pre-rotated to match: where [`bonded_siding`] passes `t =
/// face_uv_offset(..)` straight through, this passes `R·t`, which for a
/// quarter turn counter-clockwise is `(u, v) → (−v, u)`. Skip that and every
/// slab still gets vertical boards, but each one starts them at its own
/// centre and the joints step at every wall break.
pub(super) fn bonded_boards(
    mat: SovereignMaterialSettings,
    face: FaceKey,
    center: [f32; 3],
) -> SovereignMaterialSettings {
    let [u, v] = face_uv_offset(face, center).0;
    let mut mat = upright_boards(mat);
    mat.uv_offset = Fp2([-v, u]);
    mat
}

/// Stand a `Plank` material's boards upright with **no** world frame — for
/// the revolved prims a stave drum is made of (a rooftop water tank, a
/// barrel, a post), whose `Fit` parameterisation wraps its own surface and
/// has no shared face frame to line up with.
///
/// [`bonded_boards`] is the flat-face counterpart, and the place the whole
/// argument for turning this pattern at all is written down.
pub(super) fn upright_boards(mat: SovereignMaterialSettings) -> SovereignMaterialSettings {
    let mut mat = quarter_turn(mat);
    if let SovereignTextureConfig::Plank(cfg) = &mut mat.texture {
        cfg.stagger = Fp64(0.0);
    }
    mat
}

/// Turn a material's pattern through a quarter turn, and nothing else.
///
/// The bare rotation, for the patterns whose *axis* is wrong rather than
/// whose cells are: corrugated sheet is the case that named it. The
/// generator varies its ribs along U and streaks its rust along V, so a roof
/// plane — whose Box `Top` face reads U down the slope — comes out ribbed
/// *across* the pitch with the rust running along the ridge, which is
/// backwards on both counts. Turned, the ribs and the streaks both run down
/// the slope, the way rolled sheet and rainwater do.
///
/// Safe here for the same reason it is safe in [`bonded_boards`], and unsafe
/// on brick for the reason [`bonded_brick`] gives: what a quarter turn
/// destroys is a pattern's *cross-axis* logic (brick's stagger runs along U
/// but keys off the V course index), and corrugation has none — it is one
/// sine wave in U.
pub(super) fn quarter_turn(mut mat: SovereignMaterialSettings) -> SovereignMaterialSettings {
    mat.uv_rotation = Fp(BOARD_TURN_DEG);
    mat
}

/// Default clear spacing between balusters, in metres — see [`railing`].
///
/// Real balustrades run a 100 mm gap, which on a twelve-metre boardwalk is
/// ninety-odd prims for a handrail. This is the coarsest pitch that still
/// reads as *balusters* rather than as a ladder at the distance these props
/// are seen from, and it is a parameter rather than a constant because a
/// two-metre porch run wants a tighter one than a promenade does.
pub(super) const BALUSTER_PITCH: f32 = 0.42;

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
pub(super) fn railing(
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

/// Dim self-lit surface for the inside of a shell — the floor, lining and
/// contents seen through a [`window_card`]'s open panes.
///
/// A card's panes are masked *away*, so what fills them is whatever geometry
/// stands behind. Nothing lights the inside of an enclosed prop, so those
/// surfaces have to carry a low emissive term of their own; without it every
/// opening reads as a black rectangle and all the work behind the glass is
/// invisible. Keep `lit` low (0.1–0.6) — this is meant to read as *interior*,
/// not as a light box.
pub(super) fn lit_interior(color: [f32; 3], lit: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        emission_color: Fp3([color[0] * 1.1, color[1], color[2] * 0.85]),
        emission_strength: Fp(lit),
        roughness: Fp(0.85),
        metallic: Fp(0.0),
        ..Default::default()
    }
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

/// Minimum clearance between an owner panel and anything solid behind it.
///
/// Deliberately small — a backing plate is *supposed* to sit millimetres
/// behind the portrait, and only a body whose face comes out **in front of**
/// the panel is a bug. Four millimetres is enough to rule out a coplanar
/// depth tie without outlawing the plate. Ten of the twenty-four monuments
/// shipped with the panel between 5 mm and 40 mm behind its own body (#977).
#[cfg(test)]
const PANEL_REVEAL: f32 = 0.004;

/// Assert that an identity monument (#975) carries the owner's profile
/// picture correctly — the shared guard all 24 themed monuments call.
///
/// One helper rather than 24 copies, because these are invariants of the
/// *idiom*, not of any one monument: get any of them wrong and the panel is
/// still a perfectly valid `Sign` that renders, just a distorted, tiled,
/// night-blind or back-to-front one. None of that is visible in the render
/// tool, which never fetches an image at all.
///
/// What it pins:
///
/// * exactly one panel, pointed at **this room's** DID (a monument showing
///   somebody else's face is the worst failure available here);
/// * square — the aspect a face cannot survive losing;
/// * `uv_scale` 1.0 — Sign images are clamp-to-edge, so anything else crops
///   and smears rather than tiles;
/// * a pure white tint — `base_color` multiplies the fetched image, so any
///   other colour stains the owner's face, and it stains it *only once a
///   picture loads*, which is the state no render here can show (#976);
/// * the standing rotation `quat_x(FRAC_PI_2)` — the panel's wound front is
///   `−Y`, so the `-FRAC_PI_2` that stands up an ordinary quad turns this one
///   away from the viewer *and* upside-down;
/// * `unlit` and single-sided — legible at any hour, never mirrored;
/// * monument scale: a panel big enough and high enough to read from the
///   gateway's landing, on a prop tall enough to be a monument;
/// * something solid **behind** it, and nothing solid **around** it. The
///   panel is single-sided, so without a backing plate the monument is
///   see-through from behind — but the mirror failure is worse and was
///   shipping on ten of the twenty-four (#977): a panel authored a centimetre
///   or two *inside* the body it is mounted on, so the portrait is buried in
///   the slab and z-fights with it. Both checks lean on the family's
///   authoring convention — hero face toward `-Z`, so "behind" is `+Z` —
///   which is the same convention the render tool and the settlement placer
///   already assume.
#[cfg(test)]
pub(super) fn assert_owner_panel(entry: &dyn crate::catalogue::CatalogueEntry, did: &str) {
    use crate::pds::{GeneratorKind, SignSource};

    let slug = entry.slug();
    let root = entry.build(did);
    /// What one walk of the tree collects. A struct rather than eight
    /// out-parameters: `boxes` holds axis-aligned cuboids only, as
    /// `(centre, half-extents)`, because the burial check needs a box and a
    /// tilted prim's AABB would over-report — and every monument's *body*, the
    /// only mass that can bury a panel, is axis-aligned by the family's own
    /// rule that a tilted sub-root spins what it carries.
    #[derive(Default)]
    struct Census {
        panels: Vec<([f32; 3], f32)>,
        solids: Vec<[f32; 3]>,
        boxes: Vec<([f32; 3], [f32; 3])>,
        top: f32,
    }
    let mut census = Census {
        top: f32::MIN,
        ..Default::default()
    };

    fn walk(g: &Generator, at: [f32; 3], slug: &str, did: &str, c: &mut Census) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        c.top = c.top.max(here[1]);
        match &g.kind {
            GeneratorKind::Sign {
                source,
                size,
                material,
                double_sided,
                unlit,
                ..
            } => {
                match source {
                    SignSource::DidPfp { did: d } => assert_eq!(
                        d, did,
                        "{slug}: the panel points at {d}, not at the room owner"
                    ),
                    other => panic!("{slug}: panel source is {other:?}, not the owner's pfp"),
                }
                assert_eq!(
                    size.0[0], size.0[1],
                    "{slug}: panel {:?} is not square — a face cannot survive the stretch",
                    size.0
                );
                assert_eq!(
                    material.uv_scale.0, 1.0,
                    "{slug}: Sign images are clamp-to-edge; uv_scale above 1.0 crops and smears"
                );
                assert_eq!(
                    material.base_color.0,
                    [1.0, 1.0, 1.0],
                    "{slug}: base_color multiplies the fetched image, so anything but \
                     white stains the owner's face"
                );
                // The standing rotation, checked on the node itself: the panel
                // must be turned to face `−Z` the right way up, and the two
                // half-angles differ only in sign, so this is a one-character
                // mistake that no render taken here can catch.
                let q = g.transform.rotation.0;
                let want = std::f32::consts::FRAC_PI_4.sin();
                assert!(
                    (q[0] - want).abs() < 1e-4
                        && q[1].abs() < 1e-4
                        && q[2].abs() < 1e-4
                        && (q[3] - want).abs() < 1e-4,
                    "{slug}: panel rotation {q:?} is not quat_x(+FRAC_PI_2) — a \
                     negative half-angle here faces it away and inverts it"
                );
                assert!(*unlit, "{slug}: a lit portrait goes black at dusk");
                assert!(
                    !*double_sided,
                    "{slug}: a double-sided panel shows the owner mirrored from behind"
                );
                c.panels.push((here, size.0[0]));
            }
            GeneratorKind::Cuboid { size, .. } => {
                c.solids.push(here);
                if g.transform.rotation.0 == [0.0, 0.0, 0.0, 1.0] {
                    c.boxes
                        .push((here, [size.0[0] * 0.5, size.0[1] * 0.5, size.0[2] * 0.5]));
                }
            }
            GeneratorKind::Cylinder { .. }
            | GeneratorKind::Sphere { .. }
            | GeneratorKind::Superellipsoid { .. } => c.solids.push(here),
            _ => {}
        }
        for child in &g.children {
            walk(child, here, slug, did, c);
        }
    }
    walk(&root, [0.0; 3], slug, did, &mut census);
    let Census {
        panels,
        solids,
        boxes,
        top,
    } = census;

    assert_eq!(
        panels.len(),
        1,
        "{slug}: expected exactly one owner panel, found {}",
        panels.len()
    );
    let (at, side) = panels[0];
    assert!(
        side >= 1.2,
        "{slug}: a {side} m panel is too small to read from the gateway's landing"
    );
    assert!(
        at[1] - side * 0.5 > 1.0,
        "{slug}: the panel's bottom edge sits at {}, low enough to be walked in front of",
        at[1] - side * 0.5
    );
    assert!(
        top >= 4.0,
        "{slug}: the monument tops out at {top} m — not monument scale"
    );
    assert!(
        solids.iter().any(|s| {
            (s[0] - at[0]).abs() < side * 0.75
                && (s[1] - at[1]).abs() < side * 0.75
                && s[2] > at[2]
                && s[2] - at[2] < 1.0
        }),
        "{slug}: nothing stands behind the panel — it is single-sided, so the \
         monument is see-through from the back"
    );

    // ...and nothing stands *in* it. A body whose front face is nearer the
    // viewer than the panel swallows the portrait: the panel is either hidden
    // outright or z-fights with the slab it was meant to sit proud of.
    for (c, e) in &boxes {
        let covers_panel = (c[0] - at[0]).abs() < e[0] && (c[1] - at[1]).abs() < e[1];
        if !covers_panel {
            continue;
        }
        let front = c[2] - e[2];
        assert!(
            front > at[2] + PANEL_REVEAL,
            "{slug}: a solid at {c:?} presents its face at z = {front}, in front of \
             a panel at z = {} — the portrait is buried in the body it is mounted \
             on. The panel must stand at least {PANEL_REVEAL} m proud of it.",
            at[2]
        );
    }
}

/// Rotate `v` by the quaternion `q` (`[x, y, z, w]`) — the guards' one
/// implementation of the thing that is easiest to get backwards (#972).
///
/// A guard that checks where a *tilted* part's ends actually land has to turn
/// a local half-extent into world space, and hand-rolling that as
/// `(sin θ, cos θ)` has a fifty-fifty chance of picking the wrong handedness.
/// When it picks wrong, it agrees with a part rotated the wrong way and the
/// two errors cancel: the lifeguard tower's boarding ramp pointed downhill
/// toward its own deck, and a guard written the same afternoon confirmed both
/// ends were exactly where they should be.
///
/// So there is one of these, it uses the standard right-handed formula, and
/// no guard writes its own. `Quat::from_rotation_x(θ)` turns `+Y` toward
/// `+Z`, which means a prim's local `+Z` end goes **down** for positive θ —
/// the opposite of what "tilt it up by θ" suggests, and the reason this note
/// is longer than the function.
#[cfg(test)]
pub(super) fn rotate_by(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let u = [q[0], q[1], q[2]];
    let uv = cross(u, v);
    let uuv = cross(u, uv);
    [
        v[0] + 2.0 * (q[3] * uv[0] + uuv[0]),
        v[1] + 2.0 * (q[3] * uv[1] + uuv[1]),
        v[2] + 2.0 * (q[3] * uv[2] + uuv[2]),
    ]
}

/// One upright glazing card as the guards see it: its world centre and the
/// `[width, height]` of quad it spans.
#[cfg(test)]
pub(super) struct CardRect {
    pub center: [f32; 3],
    pub size: [f32; 2],
}

/// Collect every upright [`window_card`] in a built tree, in the prop's own
/// world frame.
///
/// "Upright" means a [`plane`] stood up by `quat_x(±FRAC_PI_2)`, which maps
/// the quad's local Z extent onto world Y — so a card's `size` reads as
/// `[width, height]` and it occupies a rectangle on a single Z plane. That is
/// the whole family's glazing idiom, and it is what makes the geometric
/// guards below expressible at all.
///
/// Translations accumulate down the tree; rotations are not composed, because
/// every prop in this family keeps its sub-roots axis-aligned by the rule
/// that a tilted parent spins what it carries ([`nest`]). A card under a
/// tilted parent is therefore reported at the wrong place — which is a bug
/// worth having surface as a failing guard rather than a silent skip.
#[cfg(test)]
pub(super) fn window_cards(root: &Generator) -> Vec<CardRect> {
    fn walk(g: &Generator, at: [f32; 3], out: &mut Vec<CardRect>) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        if let GeneratorKind::Plane { size, material, .. } = &g.kind
            && matches!(material.texture, SovereignTextureConfig::Window(_))
        {
            let q = g.transform.rotation.0;
            let upright = q[0].abs() > 0.5 && q[1].abs() < 1e-4 && q[2].abs() < 1e-4;
            if upright {
                out.push(CardRect {
                    center: here,
                    size: size.0,
                });
            }
        }
        for c in &g.children {
            walk(c, here, out);
        }
    }
    let mut out = Vec::new();
    walk(root, [0.0; 3], &mut out);
    out
}

/// Assert that no two glazing cards sharing a Z plane overlap (#972).
///
/// Two `Window` cards on the same plane, overlapping, is a depth tie the
/// rasteriser breaks arbitrarily — and because both are alpha-masked frames,
/// the result is a band of interleaved mullion that reads as neither. It is
/// the same failure the coplanar rule describes, arrived at between two cards
/// rather than between a card and its reveal, and it is invisible in a
/// straight-on render: the office block shipped with its shopfront band
/// running 0.4 m up into the bottom of its own curtain wall, and the sheet
/// simply showed a slightly muddled row.
///
/// Cards on *different* planes are none of this guard's business — a door
/// leaf proud of the glazing behind it is the idiom working.
#[cfg(test)]
pub(super) fn assert_cards_do_not_overlap(root: &Generator, slug: &str) {
    let cards = window_cards(root);
    for (i, a) in cards.iter().enumerate() {
        for b in &cards[i + 1..] {
            if (a.center[2] - b.center[2]).abs() > 1e-3 {
                continue;
            }
            let overlaps = |axis: usize| {
                (a.center[axis] - b.center[axis]).abs() < (a.size[axis] + b.size[axis]) * 0.5 - 1e-4
            };
            assert!(
                !(overlaps(0) && overlaps(1)),
                "{slug}: glazing cards at {:?} ({:?}) and {:?} ({:?}) share a Z plane \
                 and overlap — two alpha-masked frames tie for depth over the overlap",
                a.center,
                a.size,
                b.center,
                b.size
            );
        }
    }
}

/// Assert that no **solid** primitive in a tree wears a `Window` texture
/// (#972 lesson 1, stated as a prohibition rather than as a census).
///
/// The card guards elsewhere count cards and check the ones they find. This
/// checks the other side of the same rule, and it is the stronger half: a
/// `Window` texture on a cuboid or a cylinder is *always* wrong, whatever the
/// count says, because the generator masks its panes away and upstream renders
/// every card at `AlphaMode::Mask(0.5)`. So the slab becomes a frame with real
/// holes in it, showing whatever solid it was stuck to — and on a cuboid it
/// grows windows on its sides, top and bottom into the bargain.
///
/// Worth naming separately because the failure has a *sociable* form: the
/// grand hotel acquired it by reaching for another kit's
/// [`curtain_wall`](crate::catalogue::items::modern_city::curtain_wall)
/// helper, which is a lit glass box behind proud fins — correct on the tower
/// it was written for, and unable to be a window anywhere else. A count-based
/// guard passes that happily; this one does not.
#[cfg(test)]
pub(super) fn assert_no_glazing_on_solids(root: &Generator, slug: &str) {
    fn walk(g: &Generator, at: [f32; 3], slug: &str) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        let material = match &g.kind {
            GeneratorKind::Cuboid { material, .. }
            | GeneratorKind::Cylinder { material, .. }
            | GeneratorKind::Sphere { material, .. }
            | GeneratorKind::Cone { material, .. }
            | GeneratorKind::Capsule { material, .. }
            | GeneratorKind::Torus { material, .. }
            | GeneratorKind::Superellipsoid { material, .. } => Some(material),
            _ => None,
        };
        if let Some(m) = material {
            assert!(
                !matches!(m.texture, SovereignTextureConfig::Window(_)),
                "{slug}: a {} at {here:?} wears a Window texture — its panes are \
                 masked away, so it is a frame with holes onto whatever stands \
                 behind it. Cards belong on a flat Plane over a real opening.",
                g.kind.kind_tag()
            );
        }
        for c in &g.children {
            walk(c, here, slug);
        }
    }
    walk(root, [0.0; 3], slug);
}

/// Assert that a rotated node carries children **only at its own origin**
/// (#972).
///
/// The oldest gotcha in this family's list and, until now, the only one with
/// no test: rotation propagates down a tree, so a tilted parent spins
/// everything it holds. A ramp board built as a sub-root with its cleats
/// nested under it turns the cleats twice and swings their offsets out of the
/// surface — and because [`nest`] rebases only *translations*, the authored
/// world position and the rendered one part company with nothing in the
/// record looking wrong.
///
/// It is also how a guard gets fooled twice over. A footprint check that
/// accumulates translations down the tree — which is what every guard in this
/// family does, because composing rotations for a family that has almost none
/// would be noise — reports a tilted parent's children where they were
/// *authored*, not where they render. The tilt hides the fault from the render
/// and from the guard at once. Forbidding the shape is what keeps every
/// translation-only walk sound by construction.
///
/// # Why "at its own origin" and not "never"
///
/// A tilted parent is perfectly safe when its children sit at its own origin,
/// because then the rotation moves nothing and only *orientation* propagates —
/// which is the whole point of authoring a rig as one turned assembly. The
/// kit's [`valve_wheel`](crate::catalogue::items::industrial_park::valve_wheel)
/// is exactly that: a rim turned to face a pipe, with its hub and spokes at
/// `[0, 0, 0]` so they ride the turn. `nest`'s own note calls the same thing
/// the point on a leaning mast.
///
/// So the rule is about **offset children under a turn**, which is the only
/// form that displaces anything. The fix when it fires is always the same:
/// demote the tilted piece to a child and give the sub-assembly a flat root —
/// the thing it stands on (the ramp's foot kerb, the awning's poles).
#[cfg(test)]
pub(super) fn assert_no_tilted_parents(root: &Generator, slug: &str) {
    fn walk(g: &Generator, at: [f32; 3], slug: &str) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        let q = g.transform.rotation.0;
        let upright = q[0].abs() < 1e-4 && q[1].abs() < 1e-4 && q[2].abs() < 1e-4;
        if !upright {
            for c in &g.children {
                let o = c.transform.translation.0;
                assert!(
                    o[0].abs() < 1e-4 && o[1].abs() < 1e-4 && o[2].abs() < 1e-4,
                    "{slug}: a rotated {} at {here:?} carries a child offset by \
                     {o:?} — the turn spins that offset, so the child renders \
                     somewhere the record does not say, and every guard here \
                     walks translations only and will agree with the record. \
                     Demote the tilted piece to a child and give the \
                     sub-assembly a flat root. (A turned rig whose children sit \
                     at its own origin is fine — that is what the turn is for.)",
                    g.kind.kind_tag()
                );
            }
        }
        for c in &g.children {
            walk(c, here, slug);
        }
    }
    walk(root, [0.0; 3], slug);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tinted(c: [f32; 3]) -> SovereignMaterialSettings {
        SovereignMaterialSettings {
            base_color: Fp3(c),
            ..Default::default()
        }
    }

    /// The face override lands on the record where the spawner reads it,
    /// and inherits the prim's projection (a recolour must not re-mesh).
    #[test]
    fn with_face_records_an_override_that_inherits_the_projection() {
        let kind = with_face(
            cuboid_tapered([1.0, 1.0, 1.0], 0.0, tinted([0.1, 0.1, 0.1])),
            FaceKey::Top,
            tinted([0.9, 0.2, 0.2]),
        );
        let faces = kind.faces().expect("a cuboid carries face overrides");
        assert_eq!(faces.len(), 1);
        assert_eq!(faces[0].face, FaceKey::Top);
        assert_eq!(faces[0].material.base_color, Fp3([0.9, 0.2, 0.2]));
        assert_eq!(faces[0].uv_mapping, None);
    }

    /// Naming the same face twice replaces it. The sanitizer keeps the FIRST
    /// entry of a duplicate pair, so an appending helper would hand the
    /// author the value they overwrote.
    #[test]
    fn with_face_replaces_rather_than_stacking_a_duplicate() {
        let kind = with_face(
            with_face(
                cuboid_tapered([1.0, 1.0, 1.0], 0.0, tinted([0.1, 0.1, 0.1])),
                FaceKey::Top,
                tinted([0.9, 0.2, 0.2]),
            ),
            FaceKey::Top,
            tinted([0.2, 0.9, 0.2]),
        );
        let faces = kind.faces().unwrap();
        assert_eq!(faces.len(), 1, "a repeated face must not stack");
        assert_eq!(faces[0].material.base_color, Fp3([0.2, 0.9, 0.2]));
    }

    /// A railing spans its run, stands on the level it is given, and is made
    /// of things you can see between. The count is bounded so a promenade run
    /// cannot quietly cost ninety prims.
    #[test]
    fn railing_spans_its_run_and_has_gaps_in_it() {
        let run = railing(
            [-3.0, 1.5, -2.0],
            [3.0, 1.5, -2.0],
            1.0,
            BALUSTER_PITCH,
            tinted([0.5, 0.5, 0.5]),
        );
        let ys: Vec<f32> = run.iter().map(|g| g.transform.translation.0[1]).collect();
        assert!(
            ys.iter().all(|y| (1.5..=2.55).contains(y)),
            "a railing must stand on the level it is given: {ys:?}"
        );
        let balusters = run
            .iter()
            .filter(|g| match &g.kind {
                GeneratorKind::Cuboid { size, .. } => size.0[0] < 0.09 && size.0[1] > 0.5,
                _ => false,
            })
            .count();
        assert!(
            (6..=24).contains(&balusters),
            "{balusters} balusters over a 6 m run reads as a plate or a ladder"
        );
        // Two rails plus balusters plus two posts.
        assert_eq!(run.len(), balusters + 4);
        let widest = run
            .iter()
            .filter_map(|g| match &g.kind {
                GeneratorKind::Cuboid { size, .. } => Some(size.0[0]),
                _ => None,
            })
            .fold(0.0_f32, f32::max);
        assert!(
            widest > 5.0 && widest <= 6.0,
            "the handrail spans {widest} of a 6 m run"
        );
    }

    /// A kind with no faces at all (here a particle system) passes through
    /// untouched instead of panicking — helpers compose over whole trees.
    #[test]
    fn with_face_leaves_a_faceless_kind_alone() {
        let particles = GeneratorKind::default_particles();
        let out = with_face(particles.clone(), FaceKey::Top, tinted([1.0, 0.0, 0.0]));
        assert_eq!(out, particles);
    }
}
