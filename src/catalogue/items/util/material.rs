//! Material side of the [`util`](super) vocabulary: the shared stone and
//! glazing recipes, physical tile sizes, ageing, the bonded-brick and
//! board UV rules, and the self-lit surfaces. Geometry lives in
//! [`super::build`].

use crate::pds::generator::FaceKey;
use crate::pds::{Fp, Fp2, Fp3, Fp64, SovereignMaterialSettings, SovereignTextureConfig};

/// Shared foundation material — neutral rough-cut stone that sits
/// under any of the structure palettes.
pub(in crate::catalogue::items) fn foundation_mat() -> SovereignMaterialSettings {
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
///    bottom. Use [`plane`](super::build::plane).
/// 4. **Pane counts carry the scale.** The card stretches to whatever quad
///    it lands on, so `panes_x`/`panes_y` are what tell the viewer how big
///    the opening is. Pick them against the opening's real aspect ratio so
///    the panes come out roughly square.
///
/// `frame_width` and `mullion_thickness` are fractions of the card, so a
/// wide opening wants a smaller `frame_width` than a square one if the
/// frame is to look the same thickness all round.
pub(in crate::catalogue::items) fn window_card(
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
pub(in crate::catalogue::items) fn tiles_per_metre(tile_m: f32) -> Fp {
    Fp(1.0 / tile_m.max(1e-4))
}

/// Correct a material's tiling for a sub-assembly authored **oversized** and
/// instanced through its root's `Transform.scale`.
///
/// # The oversized-sub-assembly technique
///
/// A detail-heavy piece — a device on a flag, a carving on a transom, a coat of
/// arms — is far easier to get right drawn at ten times the size it is used at,
/// where its features are read in whole metres instead of centimetres. Author it
/// once at a canonical size with the *carrier* as the root and the details as
/// children, then instance it into any context by setting one uniform scale on
/// that root: the hierarchy carries the children along, so every proportion is
/// preserved by construction and cannot drift between call sites.
///
/// Two properties of this codebase make it work cleanly, and both are worth
/// knowing before reaching for it:
///
/// * **`BlobGroup` fidelity is scale-invariant.** Surface nets samples
///   `resolution` cells across the prim's *local* extent and the scale is
///   applied to the finished mesh (see the frame note in
///   `world_builder::prim::uv`), so relative detail — and the
///   two-cells-minimum rule that `blob_cell_size` exists for — is identical at
///   every instanced size. Solve it once at the canonical size.
/// * **The sanitiser's floors are local too** (blob radii ≥ 0.01, torus minor
///   ≥ 0.011, cuboid min dimension 0.01). Authoring at 10× lets a genuine 1 mm
///   world feature exist as a 10 mm authored one instead of being clamped away,
///   which is how the oversized draft buys detail a direct one cannot have.
///
/// And three things bite:
///
/// 1. **UVs do not scale — hence this function.** Projections emit UVs in
///    prim-*local* metres, so `uv_scale` is tiles per local metre and a cloth
///    drawn at 10× keeps ten times the tile repeats when it is scaled back down;
///    its weave comes out ten times too fine. Pass every *textured* material
///    through here with the same factor the root carries. Untextured materials
///    are unaffected (their `uv_scale` is inert), so applying it uniformly is
///    safe and is the habit to keep.
/// 2. **Uniform scale only.** The [`TransformData`](crate::pds::TransformData) sanitiser clamps each component
///    independently, so a non-uniform scale survives — and a non-uniform parent
///    scale shears any *rotated* child, because a transform composes as `T·R·S`
///    per node. If a context wants a different aspect ratio, change the authored
///    geometry, not the scale. In practice this means the instancing function
///    should take ONE dimension and derive the rest.
/// 3. **[`nest`](super::build::nest) and [`attach`](super::build::attach) do not divide by scale.** They rebase
///    translation only, as their own docs say, so a sub-assembly must be authored
///    with its children already in the root's local frame — not built in the
///    prop's ground frame and handed to `nest`. In practice the children sit at
///    the root's origin with every offset inside their own geometry, which is the
///    simplest thing that can work and the easiest to check.
///
/// Guards on the sub-assembly's internals should be written as **ratios**, since
/// an absolute "stands proud by 8 mm" claim is a statement about the authored
/// size and says nothing about the instanced one.
///
/// Worth being clear about what this does *not* buy: generator trees are
/// expanded rather than referenced, so N instances still cost N subtrees in the
/// record. This is an authoring and consistency win, not a payload one.
pub(in crate::catalogue::items) fn uv_for_scale(
    mut m: SovereignMaterialSettings,
    scale: f32,
) -> SovereignMaterialSettings {
    m.uv_scale = Fp(m.uv_scale.0 * scale.max(1e-4));
    m
}

/// Shared ageing recipes.
///
/// Weathering is one of the few material decisions that is genuinely
/// theme-agnostic — rust is rust whether it is on a shanty or a factory — so
/// these live here rather than being copied into each theme kit the way the
/// colour-carrying helpers are. A theme picks a recipe and a strength; the
/// seed keeps neighbouring surfaces from ageing in lockstep.
pub(in crate::catalogue::items) mod ageing {
    use crate::pds::{
        Fp, Fp3, Fp64, SovereignCorrosion, SovereignCreviceDirt, SovereignEdgeWear,
        SovereignStreaks, SovereignWeatheringConfig,
    };

    /// Heavy corrosion for exposed steel: pitting out of the crevices, wear
    /// back to bare metal on the arrises, and run-off staining below.
    pub(in crate::catalogue::items) fn corroded(
        seed: u32,
        amount: f32,
    ) -> SovereignWeatheringConfig {
        SovereignWeatheringConfig {
            seed,
            corrosion: SovereignCorrosion {
                amount: Fp(amount),
                coverage: Fp(0.34),
                spread: Fp64(0.07),
                ..Default::default()
            },
            edge_wear: SovereignEdgeWear {
                amount: Fp(amount * 0.7),
                ..Default::default()
            },
            streaks: SovereignStreaks {
                amount: Fp(amount * 0.8),
                density: Fp(0.5),
                ..Default::default()
            },
            crevice_dirt: SovereignCreviceDirt {
                amount: Fp(amount * 0.6),
                ..Default::default()
            },
        }
    }

    /// Weathering for masonry, which does not corrode: grime settling into
    /// the recesses and rain drawing it down the face.
    pub(in crate::catalogue::items) fn stained(
        seed: u32,
        amount: f32,
    ) -> SovereignWeatheringConfig {
        SovereignWeatheringConfig {
            seed,
            crevice_dirt: SovereignCreviceDirt {
                amount: Fp(amount),
                ..Default::default()
            },
            streaks: SovereignStreaks {
                amount: Fp(amount * 0.9),
                density: Fp(0.45),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// The green patina copper and bronze grow outdoors. Structurally this is
    /// corrosion like rust — it is the *colour* that separates a weathered
    /// statue from a weathered girder.
    pub(in crate::catalogue::items) fn verdigris(
        seed: u32,
        amount: f32,
    ) -> SovereignWeatheringConfig {
        SovereignWeatheringConfig {
            seed,
            corrosion: SovereignCorrosion {
                amount: Fp(amount),
                color: Fp3([0.10, 0.32, 0.26]),
                coverage: Fp(0.45),
                spread: Fp64(0.08),
                // Patina is a thin film, not the flaking crust rust leaves.
                relief: Fp64(0.015),
                ..Default::default()
            },
            crevice_dirt: SovereignCreviceDirt {
                amount: Fp(amount * 0.7),
                ..Default::default()
            },
            streaks: SovereignStreaks {
                amount: Fp(amount * 0.6),
                // Copper run-off stains the stone below it green too.
                color: Fp3([0.14, 0.28, 0.22]),
                density: Fp(0.4),
                ..Default::default()
            },
            ..Default::default()
        }
    }
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
/// | Cracked Earth / Gravel / Forest Floor | terrain-only so far | | — |
pub(in crate::catalogue::items) mod tile {
    /// One brick column, for configs whose `SovereignBrickConfig::scale`
    /// departs from the usual 5 (mudbrick coursing runs 14). Multiply by
    /// that count; at the usual count, prefer [`BRICK`].
    pub(in crate::catalogue::items) const BRICK_COURSE: f32 = 0.172;
    /// The common 5-column brick config.
    pub(in crate::catalogue::items) const BRICK: f32 = BRICK_COURSE * 5.0;
    /// Board-formed concrete — the board marks are the feature.
    pub(in crate::catalogue::items) const CONCRETE: f32 = 2.4;
    /// Precast paving slabs, at a 0.6 m slab across the default five-cell
    /// grid.
    pub(in crate::catalogue::items) const PAVERS: f32 = 3.0;
    /// Glazed floor tile — the default five cells at a 0.2 m tile.
    pub(in crate::catalogue::items) const ENCAUSTIC: f32 = 1.0;
    /// One framed wall panel.
    pub(in crate::catalogue::items) const WAINSCOTING: f32 = 0.9;
    /// Sheet metal — plate seams and brushing.
    pub(in crate::catalogue::items) const METAL: f32 = 1.2;
    /// Fired enamel / glazed ceramic. The clear coat is near-scaleless — the
    /// only feature is a fine orange-peel — so this is sized to match the
    /// sheet metal it is usually sprayed onto, keeping panel UVs consistent
    /// where a kit mixes painted and bare metal.
    pub(in crate::catalogue::items) const ENAMEL: f32 = METAL;
    /// Carapace plating. The default six plates per tile at roughly a
    /// 0.2 m plate.
    pub(in crate::catalogue::items) const CHITIN: f32 = 1.2;
    /// Obsidian flow banding — a figure rather than a countable feature,
    /// sized like marble so the sheets read at architectural scale.
    pub(in crate::catalogue::items) const OBSIDIAN: f32 = 2.0;
    /// Knapped obsidian — blades, mirrors and inlays, worked at a few
    /// centimetres rather than quarried in sheets. [`OBSIDIAN`] sized to an
    /// architectural face puts less than a tenth of a tile across a 0.15 m
    /// inlay, which mips to flat black; the figure only survives on small
    /// pieces if the tile shrinks with them.
    pub(in crate::catalogue::items) const OBSIDIAN_KNAPPED: f32 = 0.25;
    /// One photovoltaic wafer. Real cells are 156 mm square, and the
    /// generator lays four across a tile.
    pub(in crate::catalogue::items) const SOLAR_CELL: f32 = 0.156;
    /// The default four-by-four wafer array.
    pub(in crate::catalogue::items) const SOLAR_PANEL: f32 = SOLAR_CELL * 4.0;
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
/// ([`with_face`](super::build::with_face)).
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
pub(in crate::catalogue::items) fn face_uv_offset(face: FaceKey, center: [f32; 3]) -> Fp2 {
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
/// than the default five, so `BOND_ROWS × BOND_STAGGER` is a whole number and
/// the bond carries across the V seam; rows and columns scale together.
const BOND_ROWS: f64 = 10.0;
/// Brick columns per tile, and the number of bricks a tile spans across a
/// wall.
///
/// Four kills the two-brick colour repeat that banded walls into vertical
/// stripes, without changing the brick's size — [`bonded_brick`] derives
/// `uv_scale` from the column count.
///
/// It was originally four for a second reason that no longer holds: the
/// generator hashed each brick's **raw** cell index, so the one straddling the
/// tile's U seam drew as two half-bricks of different colour, and a wider tile
/// diluted it. symbios-texture 0.4.3 wraps the index modulo the column count
/// (its #12), so the split is gone at any count and this is a look decision
/// again (#1167).
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
/// Per-brick colour jitter — what makes a wall read as fired clay rather than
/// paint. The value used to carry a ceiling as well as a floor: jitter was the
/// only thing that made a seam-straddling brick *visible*, so it had to stay
/// low enough that the survivors read as shading. symbios-texture 0.4.3
/// removed the split, so this is now a free aesthetic choice and is kept at
/// the value the 24 themes were tuned against (#1167).
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
/// # The seam, and where it went
///
/// A running bond shifts each course by half a brick, so some course always
/// crosses the tile's U seam mid-brick. The generator used to hash that
/// brick's **raw** cell index, giving its two halves two different colours,
/// and nothing at this level could repair it — [`BOND_COLS`] and
/// [`BOND_VARIANCE`] were picked to make what remained read as shading.
///
/// symbios-texture 0.4.3 wraps the index modulo the column count, which is
/// the fix this comment used to say only the generator could make (its #12,
/// overlands #1167). Both constants stay, for the reasons their own docs now
/// give, but neither is holding a defect down any more.
///
/// Non-`Brick` textures keep their config and gain only the offset, so this
/// is safe to funnel a whole wall through.
pub(in crate::catalogue::items) fn bonded_brick(
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
pub(in crate::catalogue::items) fn bonded_siding(
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
pub(in crate::catalogue::items) fn bonded_boards(
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
pub(in crate::catalogue::items) fn upright_boards(
    mat: SovereignMaterialSettings,
) -> SovereignMaterialSettings {
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
pub(in crate::catalogue::items) fn quarter_turn(
    mut mat: SovereignMaterialSettings,
) -> SovereignMaterialSettings {
    mat.uv_rotation = Fp(BOARD_TURN_DEG);
    mat
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
pub(in crate::catalogue::items) fn lit_interior(
    color: [f32; 3],
    lit: f32,
) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        emission_color: Fp3([color[0] * 1.1, color[1], color[2] * 0.85]),
        emission_strength: Fp(lit),
        roughness: Fp(0.85),
        metallic: Fp(0.0),
        ..Default::default()
    }
}

pub(in crate::catalogue::items) fn glow(
    color: [f32; 3],
    strength: f32,
) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        emission_color: Fp3(color),
        emission_strength: Fp(strength),
        roughness: Fp(0.4),
        metallic: Fp(0.1),
        ..Default::default()
    }
}
