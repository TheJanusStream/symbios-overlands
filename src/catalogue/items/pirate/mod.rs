//! Pirate-theme catalogue structures — a Golden-Age buccaneer harbour of
//! tarred planking, powder-stained rubble stone and salt-bleached canvas.
//!
//! Two prosperity registers share one harbour identity, and here the split
//! carries more than upkeep. The established ([`PORT_BAND`]) kit is a
//! *working* port — a harbour battery over the roads, a false-front tavern,
//! a prize warehouse, a careening slip, a powder magazine, and the deck
//! furniture of a crew ashore (signal mast, capstan, rum tuns, longboat).
//! The destitute kit — a rotting hulk, a gibbet cage at the tide line, and
//! the bones the tide leaves — turns eerie rather than merely poor. That is
//! the theme's second *read*, not a second identity: the same harbour after
//! its luck ran out. Which is why it shares this file's timber and iron rather
//! than taking a palette of its own, adding only [`bone`] and one cold
//! corpse-light green ([`WITCHFIRE`]) that reads as wrong precisely because
//! every other lit colour here is firelight.
//!
//! # Telling this apart from the other maritime theme
//!
//! `coastal_resort` is the same water under a holiday sky, and two seaside
//! kits that read alike would be a worse failure than either being dull. The
//! separation is deliberate and is made in the *materials*, not the
//! silhouettes: nothing here is whitewashed (that kit's `stucco` has no
//! counterpart), nothing is bright enamel, and the shore is grey shingle
//! [`STRAND_SHINGLE`] rather than the resort's golden `SAND_TAN`. Coastal
//! builds in stucco, canvas and brushed steel; this builds in tarred oak,
//! rubble limestone, wrought iron and verdigris bronze. The air separates
//! them too — see the salt-haze accent in
//! [`crate::seeded_defaults::room::accent`], which is grey and thick where
//! the resort's is blue and clear.
//!
//! # Surfaces
//!
//! Real procedural generators rather than flat colour: wide tarred [`strake`]
//! for hulls, wharf decking and ship-built walls; ordinary sawn [`board`] for
//! doors, shutters and crates; dressed [`ashlar`] and [`cobbles`] for the
//! battery and the quay; [`shingle`] roofs; woven [`sailcloth`] and coarse
//! [`hemp`]; rusting [`iron`] hardware and verdigris [`bronze`] guns;
//! small leaded [`glass`] lights re-cut per opening by [`pane_grid`], and
//! [`tinted_glass`] for the lanterns (glass on a *solid*, which the card can
//! never be); matte [`tar`] for pitch and payed seams; grey [`strand`]
//! shingle underfoot. The battery's guns breathe spent powder over a harbour
//! swell from [`fx`].

pub mod gateway;
pub mod gibbet_cage;
pub mod harbour_battery;
pub mod harbour_tavern;
pub mod longboat;
pub mod monument;
pub mod powder_magazine;
pub mod prize_warehouse;
pub mod quay_capstan;
pub mod rotting_hulk;
pub mod rum_tuns;
pub mod signal_mast;
pub mod tideline_bones;

pub mod careening_slip;

pub mod fx;

use super::util::{ageing, tile, tiles_per_metre};
use crate::pds::Generator;
use bevy_symbios_texture::fabric::WeaveKind;
use bevy_symbios_texture::metal::MetalStyle;

use crate::pds::{
    Fp, Fp3, Fp4, Fp64, SovereignAshlarConfig, SovereignCobblestoneConfig, SovereignFabricConfig,
    SovereignMaterialSettings, SovereignMetalConfig, SovereignPlankConfig, SovereignSandConfig,
    SovereignShingleConfig, SovereignTextureConfig, SovereignWindowConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the working harbour — a port with a garrison,
/// a bonded warehouse and prizes to careen reads as Modest-to-Rich. The poor
/// end of the theme is the separate cursed kit ([`PORT_POOR`]), so a
/// destitute pirate room grows the wreck and the gibbet instead.
pub(super) const PORT_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// The cursed register — a harbour after its luck ran out.
///
/// `Poor` only, and the split is the point. This is not the working port with
/// less money in it; it is the same timber and iron gone soft, with [`bone`]
/// and [`WITCHFIRE`] added and nothing else changed. A destitute pirate room
/// grows a rotting hulk, a gibbet at the tide line and the ribs of a wreck in
/// the shingle where a Modest one grows a tavern and a warehouse.
pub(super) const PORT_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

// ---------------------------------------------------------------------------
// Timber
// ---------------------------------------------------------------------------

/// Width of one ship's **strake**, in metres — the plank a hull, a wharf deck
/// or a ship-built wall is laid in.
///
/// Deliberately wider than the catalogue's ordinary
/// [`tile::PLANK_BOARD`] sawn board (167 mm),
/// and the deviation is the point rather than an oversight. A hull strake is
/// cut from a much bigger baulk than a clapboard is, and getting that
/// difference on the surface is most of what separates ship-built timber from
/// house-built timber at a glance. The two sizes coexist in this kit on
/// purpose: [`strake`] clads anything that came off a vessel, [`board`] clads
/// anything a shore carpenter made.
///
/// #972 lesson 2(a): the feature's real-world size is chosen, not inherited
/// from whatever `1 / plank_count` happened to be.
const STRAKE_W: f32 = 0.25;

/// Boards per tile, shared by [`strake`] and [`board`]. Six keeps the tile
/// square-ish in metres at both board widths and gives the grain hash enough
/// courses to de-correlate.
const PLANK_COUNT: f64 = 6.0;

/// Wide tarred ship's planking — hulls, wharf decking, ship-built walls, the
/// tavern's false front, gun-deck platforms.
///
/// `stagger` is held at zero (#972 lesson 4). Any value above 0.01 switches on
/// the generator's hard-coded three-butt-joints-per-tile grid, which on this
/// kit's 1.5 m tile would be a butt joint every half metre — a hull rendering
/// as coarse masonry. Real strakes run the length of the vessel and are
/// scarphed, not butted every stride. The per-course grain de-correlation is
/// untouched; that comes from the row's own hash.
///
/// Boards lay **up V**, so this gives horizontal courses — which is what a
/// hull and a deck both want. Vertical boarding stands them up with
/// [`util::bonded_boards`](super::util::bonded_boards); the quarter turn is
/// safe here precisely *because* the stagger is off (#972 lesson 15).
pub(super) fn strake(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        metallic: Fp(0.0),
        uv_scale: tiles_per_metre(STRAKE_W * PLANK_COUNT as f32),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3([color[0] * 1.18, color[1] * 1.16, color[2] * 1.12]),
            color_wood_dark: Fp3([color[0] * 0.55, color[1] * 0.52, color[2] * 0.48]),
            plank_count: Fp64(PLANK_COUNT),
            stagger: Fp64(0.0),
            // A caulked seam is a real, wide, black line of oakum and pitch —
            // much heavier than the 6 % hairline a dry-jointed board shows.
            joint_width: Fp64(0.1),
            knot_density: Fp64(0.18),
            grain_warp: Fp64(0.34),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Ordinary sawn boarding at the catalogue's shared board width — doors,
/// shutters, crates, gangways, barrow beds, the shore carpenter's work.
/// Same stagger rule as [`strake`]; see its note.
pub(super) fn board(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.92),
        metallic: Fp(0.0),
        uv_scale: tiles_per_metre(tile::PLANK_BOARD * PLANK_COUNT as f32),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3([color[0] * 1.2, color[1] * 1.18, color[2] * 1.14]),
            color_wood_dark: Fp3([color[0] * 0.6, color[1] * 0.57, color[2] * 0.52]),
            plank_count: Fp64(PLANK_COUNT),
            stagger: Fp64(0.0),
            knot_density: Fp64(0.28),
            grain_warp: Fp64(0.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Masonry
// ---------------------------------------------------------------------------

/// Ashlar courses per tile. Four is the generator's own default and gives the
/// battery's face a block about 450 mm on the side — the size a coursed
/// rubble-and-dressing wall of the period actually runs.
const ASHLAR_COLS: u32 = 4;

/// Dressed limestone ashlar, powder-stained — the battery's battered face,
/// embrasure jambs, quoins, copings, the magazine's blast wall.
///
/// Weathered with [`ageing::stained`] rather than left clean: masonry does not
/// corrode, it collects grime in its recesses and lets the rain draw it down
/// the face, and a sea battery gets that treatment from spray and gunsmoke at
/// once. The `seed` is per-call so two neighbouring walls do not weather in
/// lockstep.
pub(super) fn ashlar(color: [f32; 3], seed: u32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.94),
        metallic: Fp(0.0),
        uv_scale: tiles_per_metre(tile::ASHLAR_BLOCK * ASHLAR_COLS as f32),
        texture: SovereignTextureConfig::Ashlar(SovereignAshlarConfig {
            rows: ASHLAR_COLS,
            cols: ASHLAR_COLS,
            color_stone: Fp3(color),
            color_mortar: Fp3([color[0] * 0.82, color[1] * 0.82, color[2] * 0.80]),
            chisel_depth: Fp64(0.5),
            cell_variance: Fp64(0.16),
            weathering: ageing::stained(seed, 0.7),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Sea-worn cobbles — the quay, the slipway apron, the tavern's yard, rubble
/// footings. Rounded by the water rather than knapped.
pub(super) fn cobbles(color: [f32; 3], seed: u32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.96),
        metallic: Fp(0.0),
        uv_scale: tiles_per_metre(tile::COBBLE),
        texture: SovereignTextureConfig::Cobblestone(SovereignCobblestoneConfig {
            color_stone: Fp3(color),
            color_mud: Fp3([color[0] * 0.42, color[1] * 0.40, color[2] * 0.36]),
            roundness: Fp64(1.5),
            cell_variance: Fp64(0.22),
            weathering: ageing::stained(seed, 0.55),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Split-oak shingle roofing — the tavern, the warehouse, the magazine's
/// pentice. The generator's own `6 x 0.5` stagger is integral and it hashes a
/// *wrapped* column id, so unlike plank and brick it needs nothing switched
/// off (#972 lesson 4's closing note: read each generator, do not generalise).
pub(super) fn shingle(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.93),
        metallic: Fp(0.0),
        uv_scale: tiles_per_metre(tile::SHINGLE),
        texture: SovereignTextureConfig::Shingle(SovereignShingleConfig {
            color_tile: Fp3(color),
            color_grout: Fp3([color[0] * 0.4, color[1] * 0.38, color[2] * 0.34]),
            // A roof this close to the water grows more moss than a dry one.
            moss_level: Fp64(0.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Cloth and cordage
// ---------------------------------------------------------------------------

/// Threads per tile of sailcloth. Real canvas is a coarse, heavy weave —
/// sixteen to the tile puts a thread near 25 mm, which reads as *duck* rather
/// than as shirting.
const SAIL_THREADS: f64 = 16.0;

/// Salt-bleached sailcloth — furled canvas, awnings, tarpaulins, hammocks,
/// the ensign. Two tones so the weave reads at close range.
pub(super) fn sailcloth(warp: [f32; 3], weft: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(warp),
        roughness: Fp(0.95),
        metallic: Fp(0.0),
        uv_scale: tiles_per_metre(tile::FABRIC_THREAD * SAIL_THREADS as f32),
        texture: SovereignTextureConfig::Fabric(SovereignFabricConfig {
            weave: WeaveKind::Plain,
            color_warp: Fp3(warp),
            color_weft: Fp3(weft),
            thread_count: Fp64(SAIL_THREADS),
            thread_width: Fp64(0.92),
            fuzz: Fp64(0.5),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Threads per tile of cordage. Eight is deliberately coarser than any cloth
/// in the catalogue: a hawser's lay is a visible spiral of three strands, not
/// a weave, and the only way this generator approximates that is by making the
/// thread very large.
const ROPE_LAY: f64 = 8.0;

/// Coarse hemp cordage — hawsers, shrouds, careening tackle, cargo nets, the
/// gibbet's chain lashing. A twill wale is the closest the fabric generator
/// comes to the spiral lay of a laid rope.
pub(super) fn hemp(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(1.0),
        metallic: Fp(0.0),
        uv_scale: tiles_per_metre(tile::FABRIC_THREAD * ROPE_LAY as f32),
        texture: SovereignTextureConfig::Fabric(SovereignFabricConfig {
            weave: WeaveKind::Twill,
            color_warp: Fp3(color),
            color_weft: Fp3([color[0] * 0.78, color[1] * 0.74, color[2] * 0.66]),
            thread_count: Fp64(ROPE_LAY),
            thread_width: Fp64(0.95),
            weave_contrast: Fp64(0.75),
            fuzz: Fp64(0.7),
            ..Default::default()
        }),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Metal
// ---------------------------------------------------------------------------

/// Wrought iron, rusting — bands, hoops, chain, hinges, the gibbet cage, gun
/// trucks and their strapping. Salt air is merciless, so the rust runs high.
///
/// # Why there is no weathering block here
///
/// The obvious move is [`ageing::corroded`], and this helper had it. It was
/// removed on measurement, and the reasoning generalises past this kit.
///
/// A `SovereignWeatheringConfig` is *per-prim record payload* — four nested
/// blocks (corrosion, edge wear, streaks, crevice dirt) serialised onto every
/// primitive that carries the material. On the harbour battery that came to
/// 23 % of the whole entry, and the overwhelming majority of it was on iron:
/// carriage brackets 90 mm thick, gun trucks, hoops, cage bars. Pitting,
/// arris wear and run-off staining are surface *stories* at architectural
/// scale and are sub-pixel on a fitting — so the kit was paying its largest
/// single record cost for detail nobody can resolve.
///
/// What survives is the part that reads: `rust_level` and `color_rust` are
/// plain scalars inside the Metal generator's own config, and at 0.42 they
/// give iron in a sea harbour exactly the corroded tone it should have.
/// [`ashlar`] and [`cobbles`] keep their weathering, because a wall is big
/// enough to show it.
///
/// `seed` still varies the generator's own pattern, so neighbouring fittings
/// do not rust in lockstep.
pub(super) fn iron(color: [f32; 3], seed: u32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.62),
        metallic: Fp(0.75),
        uv_scale: tiles_per_metre(tile::METAL),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            seed,
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.40, 0.21, 0.10]),
            roughness: Fp64(0.62),
            metallic: Fp(0.75),
            rust_level: Fp64(0.42),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Gunmetal bronze under a green patina — the guns themselves, the ship's
/// bell, lantern frames, the battery's traversing gear.
///
/// The kit's one warm metal, and its job is to be the *only* one: against
/// tarred timber, grey stone and black iron, a verdigris barrel is the thing
/// the eye goes to, which is correct for the object a battery exists to point.
///
/// The patina is carried by the Metal generator's own rust channel with the
/// colour turned green, rather than by [`ageing::verdigris`], for the
/// record-cost reason set out on [`iron`] — the guns are the most numerous
/// prims in the kit and a nested weathering block on each is the single most
/// expensive thing this file could do. What that costs is the patina's
/// *relief*; what it keeps is the green, which is the entire read at any
/// distance a gun is seen from.
pub(super) fn bronze(color: [f32; 3], seed: u32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.45),
        metallic: Fp(0.8),
        uv_scale: tiles_per_metre(tile::METAL),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            seed,
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.16, 0.38, 0.30]),
            // A cast barrel has no rolled-plate seams to show.
            seam_count: Fp64(1.0),
            roughness: Fp64(0.45),
            metallic: Fp(0.8),
            // Higher than iron's, because here the channel is doing the
            // patina's work rather than corrosion's.
            rust_level: Fp64(0.5),
            ..Default::default()
        }),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Glazing
// ---------------------------------------------------------------------------

/// A small leaded light — the tavern's windows and the warehouse office's.
///
/// **This is a CARD and it belongs on a flat `Plane` over a real opening**
/// (#972 lesson 1). The generator masks its panes away and the pipeline draws
/// every card at `AlphaMode::Mask(0.5)`, so at `glass_opacity` 0.34 the panes
/// are genuine holes: spanning an opening they show the room behind, and stuck
/// on a solid they show the wall. `uv_scale` stays 1.0 — cards upload
/// clamp-to-edge and must span their quad exactly once.
///
/// It arrives now rather than with the rest of the kit because the landmark
/// deliberately has none: a battery's openings are embrasures and gun ports,
/// which are real holes with guns in them. A material nothing calls is a
/// decision nothing checks.
///
/// The pane grid is small and the joinery is dark oak, because the period's
/// glass came in pieces a hand could carry; a big clean light would read as
/// three centuries too late. For glass on a **solid** — a lantern, a bottle,
/// a stern light — use [`tinted_glass`], never this.
///
/// `glow` is normally zero. What lights a window is the room behind it, not
/// the pane — see [`util::lit_interior`](super::util::lit_interior).
pub(super) fn glass(tint: [f32; 3], glow: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(tint),
        emission_color: Fp3(tint),
        emission_strength: Fp(glow),
        roughness: Fp(0.3),
        metallic: Fp(0.1),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Window(SovereignWindowConfig {
            panes_x: 4,
            panes_y: 4,
            glass_opacity: Fp64(0.34),
            grime_level: Fp64(0.3),
            mullion_thickness: Fp64(0.035),
            color_frame: Fp3(OAK_JOINERY),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// The kit's [`glass`], re-cut to one opening's pane grid (#972).
///
/// The kit material already carries the grime, the joinery colour and the
/// opacity, and those are worth inheriting. What a shared material cannot know
/// is the *aspect of the hole it is filling* — and pane counts are exactly
/// what tell a viewer how big an opening is, so they are picked per opening
/// and everything else comes along.
pub(super) fn pane_grid(tint: [f32; 3], glow: f32, panes: (u32, u32)) -> SovereignMaterialSettings {
    let mut m = glass(tint, glow);
    if let SovereignTextureConfig::Window(cfg) = &mut m.texture {
        cfg.panes_x = panes.0;
        cfg.panes_y = panes.1;
    }
    m
}

// ---------------------------------------------------------------------------
// Plain surfaces
// ---------------------------------------------------------------------------

/// A pane of glass on a **solid** — a lantern's light, a stern window, a
/// bottle. Tinted, smooth and faintly lit, with no procedural texture at all.
///
/// This exists so that the kit's forthcoming `Window` card never has to serve
/// here. A card's panes are masked away, which is correct when it spans a
/// real opening with a room behind it and catastrophic when it is wrapped
/// round a solid, where the holes show whatever the solid was hiding (#972
/// lesson 1, and lesson 20 stated as the prohibition
/// `assert_no_glazing_on_solids` enforces). A lantern is the case that tempts the mistake hardest, because
/// its whole subject *is* light seen through glass — the steampunk gas lamp
/// shipped with four `Window` cuboids for exactly this reason, and each one
/// showed the sky through the far pane.
///
/// `lit` is a faint self-glow so the pane reads as glass with something behind
/// it rather than as a dark plastic drum; the flame inside supplies the actual
/// light.
pub(super) fn tinted_glass(color: [f32; 3], lit: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        emission_color: Fp3(color),
        emission_strength: Fp(lit),
        roughness: Fp(0.18),
        metallic: Fp(0.15),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

/// Matte pitch / tar — payed seams, tarred rigging and sheathing, the dark
/// void inside a gun port, a hull's boot-topping. A plain surface with no
/// procedural texture, which is honest: tar has no figure.
pub(super) fn tar(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.96),
        metallic: Fp(0.0),
        // No texture, so `uv_scale` is inert — pinned at 1.0 so it does not
        // read as a stale pre-#936 repeat count.
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

/// Grey storm-beach shingle — the strand under the slipway, the gibbet's tide
/// line, the foreshore apron.
///
/// Sized and coloured *against* `coastal_resort::sand`: the resort's is a
/// golden rippled beach, this is a cold coarse one. Same generator, opposite
/// read, which is the separation this kit is built on.
pub(super) fn strand(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(1.0),
        metallic: Fp(0.0),
        uv_scale: tiles_per_metre(tile::SAND),
        texture: SovereignTextureConfig::Sand(SovereignSandConfig {
            color_crest: Fp3(color),
            color_trough: Fp3([color[0] * 0.7, color[1] * 0.70, color[2] * 0.68]),
            ripple_count: Fp64(5.0),
            grain_density: Fp64(0.3),
            grain_scale: Fp64(14.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Bleached bone and old ivory — the cursed register's ribs, skulls and the
/// scrimshaw on the monument. Matte and untextured; bone at these scales is a
/// silhouette, and a procedural grain on it would be noise.
pub(super) fn bone(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.72),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Shared geometry
// ---------------------------------------------------------------------------

/// A ship's lantern: a bronze-capped glazed drum with a flame inside it.
///
/// Lives here rather than in whichever file needed it first (#972 lesson 5).
/// Almost everything in this kit is lit by one of these — the gate's head and
/// piers, the tavern's sign bracket, the magazine's approach, the signal
/// mast's masthead, the hulk's one surviving light — and the alternative is
/// six files each re-deriving the same drum and each free to reach for the
/// wrong glass.
///
/// `h` is the overall body height; the cap, base and flame all scale off it,
/// so the same call serves a 0.6 m bracket lamp and a 0.9 m stern lantern.
///
/// Two rules are baked in rather than left to the caller:
///
/// 1. **The glazing is [`tinted_glass`], never a card.** A `Window` card
///    wrapped round a drum masks its panes away and shows whatever is beyond
///    the lantern through its far side (#972 lesson 1). This is the prop that
///    tempts that mistake hardest — the steampunk gas lamp shipped with four
///    `Window` cuboids for its panes — so the helper takes the decision away.
/// 2. **The flame is small.** It sits at real strength because a lantern is
///    genuinely a bright point; what makes an emissive bloom to a white blank
///    is *area*, and this one is a sphere of a few centimetres inside the
///    glass. A lit face the size of the drum would go white at a quarter of
///    the strength.
pub(super) fn lantern(at: [f32; 3], h: f32, seed: u32) -> Generator {
    use super::util::{cylinder_tapered, glow, id_quat, nest, prim, quat_x, solid, sphere, torus};
    let r = h * 0.34;
    nest(
        prim(
            cylinder_tapered(r, h * 0.62, 10, 0.12, tinted_glass(GLASS_AMBER, 0.35)),
            at,
            id_quat(),
        ),
        vec![
            // Flame, well inside the glass so the drum reads as lit rather
            // than as a glowing object.
            prim(
                sphere(r * 0.42, 3, glow(LAMP_TALLOW, 3.2)),
                [at[0], at[1] - h * 0.05, at[2]],
                id_quat(),
            ),
            // Bronze cap, flared so rain runs off it.
            prim(
                solid(cylinder_tapered(
                    r * 1.25,
                    h * 0.16,
                    10,
                    0.5,
                    bronze(BRONZE_FITTING, seed),
                )),
                [at[0], at[1] + h * 0.39, at[2]],
                id_quat(),
            ),
            prim(
                solid(cylinder_tapered(
                    r * 1.2,
                    h * 0.1,
                    10,
                    0.0,
                    bronze(BRONZE_FITTING, seed ^ 0x11),
                )),
                [at[0], at[1] - h * 0.36, at[2]],
                id_quat(),
            ),
            // Suspension ring — a leaf prim, so its quarter turn carries
            // nothing with it (#972 lesson 22).
            prim(
                torus(0.015, r * 0.45, iron(IRON_BLACK, seed ^ 0x22)),
                [at[0], at[1] + h * 0.5, at[2]],
                quat_x(std::f32::consts::FRAC_PI_2),
            ),
        ],
    )
}

// ---------------------------------------------------------------------------
// The colours
// ---------------------------------------------------------------------------
//
// Drawn once at CANON_H and instanced at whatever size a context flies them at,
// through one uniform scale on the cloth's root. See
// [`util::uv_for_scale`](super::util::uv_for_scale) for the technique and its
// three traps; what follows is this flag's application of it.
//
// The reason it is worth doing here rather than parameterising by width and
// height, which is what this helper used to do: every one of the constants below
// was an ABSOLUTE metre value while the callers asked for three different sizes
// (1.9 x 1.25 over the battery, 1.55 x 1.05 on the gate, 1.5 x 1.0 at the signal
// mast). So the signal mast's flag carried a relatively 27 % thicker skin, a
// 27 % deeper bite and a finer sample grid than the battery's — three flags that
// were supposed to be the same flag. As ratios of one canonical draft they are
// the same flag by construction.

/// The size the colours are DRAWN at, and the aspect they are drawn to.
///
/// Ten times the size they are flown at, which buys two things and not a third.
/// It buys legible numbers — a 320 mm eye socket rather than a 32 mm one — and it
/// buys headroom over the sanitiser's local-space floors, so the teeth and the
/// knuckle ends below can exist at all. It does **not** buy mesh detail: blob
/// fidelity is a function of the local extent, so an oversized draft polygonises
/// exactly as finely as a direct one would.
const CANON_H: f32 = 10.0;
/// 3:2, which is what all three call sites were already asking for to within
/// 3 % (1.520, 1.476, 1.500). One aspect means one uniform scale, which is the
/// only kind that is safe — a non-uniform one shears rotated children.
const CANON_ASPECT: f32 = 1.5;
const CANON_W: f32 = CANON_H * CANON_ASPECT;

/// Half-thickness of the flag cloth, at canonical size.
///
/// **Set by the mesher, not by taste**, and the constraint is a *ratio* rather
/// than a length: a `BlobGroup` is polygonised on a sample grid, so anything
/// thinner than about two cells is missed in places and the mesh comes out with
/// holes rather than merely coarse. Cells are `CANON_W / FLAG_RES`, so what has
/// to hold is `2·skin / CANON_W > 2 / FLAG_RES` — scale-invariant, and therefore
/// true at every size the flag is flown at once it is true here. The first build
/// used 30 mm at resolution 30 on a 1.9 m flag and polygonised as two
/// disconnected slabs with a gap down the middle (#1026).
const CANON_SKIN: f32 = 0.48;
/// Sample resolution for the cloth. Near the sanitiser's 48 ceiling, because
/// every cell of it buys thinner cloth — see [`CANON_SKIN`].
const FLAG_RES: u32 = 44;
/// Half-thickness of the bone device, and its sample resolution. It spans far
/// less than the cloth, so the same cell budget goes much further.
const DEVICE_SKIN: f32 = 0.4;
const DEVICE_RES: u32 = 30;
/// How deep the device beds INTO the cloth, as a fraction of its own thickness.
///
/// Half. A device that merely touches the sheet reads as floating in front of
/// it — which is what an 8 mm bite gave — where a device *sunk into* it reads as
/// something painted or sewn on, which is what a flag's device is. Half is also
/// what makes the carved holes work: with the bone bedded to its mid-plane, an
/// eye socket cut clean through it has black cloth behind it rather than sky, so
/// the flag shows through the skull's own sockets.
const DEVICE_BED: f32 = 0.5;

/// Clear air between the skull's jaw and the crossbones' upper knuckles.
///
/// The device is two things, and a viewer only reads it as two if there is a gap.
/// Drawn with the bones crossing at the chin — which is where a naive layout puts
/// them — the skull, the jaw and the arms of the X merge into one pale mass.
const DEVICE_GAP: f32 = CANON_H * 0.02;

/// How far a carved hole reaches, as a multiple of the device's half-thickness.
///
/// Over 1.0 on purpose: at 1.0 the cut would stop exactly on the bone's back
/// face and leave a film behind, so the sockets read as recesses. Punching past
/// it is what turns a recess into a hole — and a hole is the only thing the
/// cloth can show through.
const HOLE_REACH: f32 = 1.6;
/// Amplitude of the cloth's ripple, in canonical metres of Z excursion.
///
/// Named because the device's standoff is derived from it: the sheet is not
/// flat, so anything applied to its face has to clear the deepest lobe, not just
/// the mid-plane.
const CANON_RIPPLE: f32 = 0.4;
/// How far the cloth's luff laps ONTO the staff it is bent to.
///
/// A flag that merely reaches the staff reads as floating beside it; the luff has
/// to overlap the timber. An early build placed the cloth by its centre and left
/// a 100 mm gap, which is why the colours looked detached (#1025).
const CANON_LAP: f32 = 0.55;

const _: () = assert!(
    CANON_SKIN * 2.0 * FLAG_RES as f32 > CANON_W * 2.0,
    "the cloth is under two sample cells thick and will polygonise with holes — \
     and being a ratio, it will do so at every size the flag is flown at"
);
const _: () = assert!(
    HOLE_REACH > 1.0,
    "a carved hole stops on the bone's own back face and leaves a film across it \
     — the sockets will read as recesses and nothing will show through them"
);
const _: () = assert!(
    DEVICE_SKIN > CANON_RIPPLE * (1.0 - CLOTH_FRONT_FRAC),
    "bedded this deep the device sinks behind the cloth's deepest lobe — it \
     would disappear into the sheet wherever the ripple runs toward the viewer"
);
/// Which point on the rippling sheet counts as "the cloth's front" for the
/// purpose of bedding the device into it.
///
/// The sheet is not flat, so there is no single front face: under the device it
/// runs from the bare slab at `CANON_SKIN` out to a lobe at
/// `CANON_SKIN + CANON_RIPPLE`. Bedding to a fraction of the way out means the
/// device is buried more where the cloth swells toward the viewer and less where
/// it falls away, which is what an appliqué on moving cloth actually does.
const CLOTH_FRONT_FRAC: f32 = 0.45;

/// The canonical colours, drawn at [`CANON_H`] with the cloth at the origin.
///
/// Separate from [`jolly_roger`] so the draft can be inspected and guarded at
/// the size it was authored at, which is the only size its absolute numbers mean
/// anything at. Everything is in the cloth's own local frame: the cloth's root
/// sits at the origin and both device groups sit at the origin too, with every
/// offset living inside their own blob elements. That is what makes the instanced
/// scale trivially correct — [`nest`](super::util::nest) rebases translation and
/// does *not* divide by scale, so children authored in a prop's ground frame
/// would land wrong the moment the root carried one.
///
/// # Three groups, and why
///
/// The cloth is one group, the skull is a second and the crossed bones are a
/// third. A `BlobGroup` carries one material, so bone-on-black already forces
/// two — but the skull and the bones are split from each other as well, because
/// blended into one group they melt together at the jaw and the whole device
/// reads as a single lumpy figure with arms rather than as a skull above two
/// bones. Blending is the right default *within* an object and the wrong one
/// *between* objects that only touch.
///
/// `scale` is passed in only so the cloth's weave can be corrected for it — see
/// [`uv_for_scale`](super::util::uv_for_scale), trap 1. Nothing else here depends
/// on it, which is the point.
fn colours_canonical(scale: f32) -> Generator {
    use super::util::{
        blob_box, blob_capsule, blob_ellipsoid, blob_group, carved, id_quat, prim, quat_z,
        uv_for_scale,
    };

    // --- The cloth --------------------------------------------------------
    //
    // One full-extent slab guarantees a single connected mass, and shallow lobes
    // offset in Z give it a ripple. Building the ripple out of abutting panels
    // instead — as the first version did — makes connectivity a property of the
    // blend radius, which is exactly the thing that fails quietly (#1026).
    //
    // The blends are held near the cloth's own THICKNESS rather than scaled off
    // its width, and that is a correction the oversized draft made visible. A
    // blend is a bloom: the iso-surface stands proud of the authored elements by
    // roughly its radius, so a blend set to a tenth of the flag's width — which
    // is what this used to be — meshes a flag 12 % bigger than the size it was
    // asked for. That was true before and simply went unnoticed, because a
    // 1.9 m flag asked for by width has no promise to break; a flag asked for by
    // height and instanced by scale does.
    let mut cloth = vec![blob_box(
        [0.0, 0.0, 0.0],
        [CANON_W * 0.5, CANON_H * 0.5, CANON_SKIN * 0.72],
        CANON_SKIN * 0.4,
    )];
    // Four lobes rather than three, which the oversized draft can afford: the
    // extra half-period is the difference between cloth that is bent and cloth
    // that is waving.
    for i in 0..4 {
        let t = (i as f32 + 1.0) / 5.0;
        cloth.push(blob_box(
            // The ripple grows toward the fly: the luff is laced to the staff
            // and cannot move.
            [
                (t - 0.5) * CANON_W,
                0.0,
                (t * std::f32::consts::PI * 2.4).sin() * CANON_RIPPLE * t,
            ],
            // Held INSIDE the slab's own depth, so the slab alone governs the
            // top and bottom edges. Drawn to the full half-height the two
            // iso-surfaces met along those edges and scalloped them — a row of
            // shallow bites the oversized draft showed at once and a flown one
            // never would.
            [CANON_W * 0.14, CANON_H * 0.46, CANON_SKIN],
            CANON_SKIN * 0.8,
        ));
    }

    // --- The device, bedded INTO the cloth's front face -------------------
    //
    // Sunk to [`DEVICE_BED`] of its own thickness, not stood off in front of the
    // sheet. The first version cleared the deepest lobe entirely and bit 8 mm
    // into it, which kept it proud from every angle and read as a plaque hovering
    // a hand's breadth off the canvas.
    //
    // The cloth is not flat, so "the cloth's front" is a choice —
    // [`CLOTH_FRONT_FRAC`] of the way out to the deepest lobe — and the
    // consequence is that the bone is buried more where the sheet swells toward
    // the viewer and less where it falls away. That is what an appliqué on moving
    // cloth does, and it is the whole reason to bed it rather than stand it off.
    let cloth_front = -(CANON_SKIN + CANON_RIPPLE * CLOTH_FRONT_FRAC);
    let dz = cloth_front + DEVICE_SKIN * (2.0 * DEVICE_BED - 1.0);
    // The device fills the field. A Jolly Roger whose skull is a sixth of the
    // hoist reads as a badge on a flag rather than as the flag's own device, and
    // the first oversized draft was exactly that — legible, correct, and too
    // polite.
    let skull_r = CANON_H * 0.19;
    // Set high enough that the jaw clears the crossing of the bones. At 0.15 the
    // chin sat on the crossing point and the three pieces read as one mass; the
    // device wants daylight between the skull and the bones, because that gap is
    // what says there are two things rather than one.
    let skull_y = CANON_H * 0.19;
    let skull = vec![
        blob_ellipsoid(
            [0.0, skull_y, dz],
            [skull_r, skull_r * 1.02, DEVICE_SKIN],
            skull_r * 0.12,
        ),
        // Brow ridge — the detail the direct draft could not have. At the size
        // this is flown at it is a 25 mm swell, which the sanitiser's blob floor
        // would have clamped; at canonical it is 250 mm and simply exists.
        blob_box(
            [0.0, skull_y + skull_r * 0.34, dz - DEVICE_SKIN * 0.18],
            [skull_r * 0.86, skull_r * 0.16, DEVICE_SKIN * 0.6],
            skull_r * 0.14,
        ),
        // Jaw: narrower and square, which is what separates a skull from a ball
        // at any distance this is read from.
        blob_box(
            [0.0, skull_y - skull_r * 0.92, dz],
            [skull_r * 0.58, skull_r * 0.34, DEVICE_SKIN * 0.85],
            skull_r * 0.18,
        ),
        // Two carved sockets. The single most legible feature on a skull, and
        // subtraction is the only way to get them without a second material.
        // Two carved sockets, cut CLEAN THROUGH the bone. The single most
        // legible feature on a skull, and now also the place the flag shows
        // through itself: centred on the bone's own mid-plane and reaching
        // [`HOLE_REACH`] times its half-thickness, so there is no film left
        // across the back and what fills the socket is black cloth.
        carved(blob_ellipsoid(
            [-skull_r * 0.44, skull_y + skull_r * 0.06, dz],
            [skull_r * 0.35, skull_r * 0.31, DEVICE_SKIN * HOLE_REACH],
            skull_r * 0.07,
        )),
        carved(blob_ellipsoid(
            [skull_r * 0.44, skull_y + skull_r * 0.06, dz],
            [skull_r * 0.35, skull_r * 0.31, DEVICE_SKIN * HOLE_REACH],
            skull_r * 0.07,
        )),
        // The nasal aperture, and two tooth slots cut through the jaw. Three more
        // features the oversized draft affords, and between them they are most of
        // what makes this read as a skull rather than as a face.
        // Wider than it is tall, which is what a nasal aperture is. Drawn the
        // other way up it came out as a vertical slot and merged with the tooth
        // line into one bar down the face.
        carved(blob_ellipsoid(
            [0.0, skull_y - skull_r * 0.46, dz],
            [skull_r * 0.19, skull_r * 0.13, DEVICE_SKIN * HOLE_REACH],
            skull_r * 0.05,
        )),
        carved(blob_box(
            [
                -skull_r * 0.2,
                skull_y - skull_r * 0.95,
                dz - DEVICE_SKIN * 0.5,
            ],
            [skull_r * 0.055, skull_r * 0.3, DEVICE_SKIN * 0.8],
            skull_r * 0.03,
        )),
        carved(blob_box(
            [
                skull_r * 0.2,
                skull_y - skull_r * 0.95,
                dz - DEVICE_SKIN * 0.5,
            ],
            [skull_r * 0.055, skull_r * 0.3, DEVICE_SKIN * 0.8],
            skull_r * 0.03,
        )),
    ];

    // --- The crossed bones ------------------------------------------------
    //
    // Two capsules crossed, and then the piece that actually matters: a pair of
    // knuckles at each of the four ends. A bare capsule is a stick, and two
    // crossed sticks are an X; the bifurcated ends are what make them bones. Four
    // ends times two knuckles is eight elements, which only fits under the
    // sixteen-element cap because the draft is one object rather than a whole
    // prop.
    // Longer than the skull is wide, which is what the device wants: bones that
    // span only as far as the cranium read as an afterthought under it instead of
    // as the other half of the design.
    let bone_len = CANON_W * 0.24;
    let bone_r = skull_r * 0.2;
    // Flatter than 45°, which is what buys the separation. A steeper X reaches so
    // far up that its arms flank the jaw however far the skull is raised — and
    // dropping the crossing far enough to fix that instead hangs the lower arms
    // off the bottom of the cloth. Laying the bones over reduces their vertical
    // reach without shortening them, so they stay wider than the cranium.
    let bone_lean = 1.05_f32;
    // Where the crossing sits is DERIVED from the jaw it has to clear, not chosen:
    // the jaw's underside, less a stated gap, less the bones' own reach including
    // their knuckles. Raising or resizing the skull therefore moves the bones
    // with it and cannot close the gap again (#972 lesson 18).
    let jaw_bottom = skull_y - skull_r * 1.26;
    let bone_reach = bone_len * bone_lean.cos() + bone_r * 0.72;
    let bone_y = jaw_bottom - DEVICE_GAP - bone_reach;
    let mut bones = Vec::new();
    for lean in [bone_lean, -bone_lean] {
        bones.push(blob_capsule(
            [0.0, bone_y, dz],
            bone_r,
            bone_len,
            quat_z(lean),
            bone_r * 0.4,
        ));
        // The knuckles sit ON the capsule's own axis, derived from its lean and
        // half-length rather than placed by eye — so a retuned bone cannot leave
        // its own ends behind. `quat_z(lean)` carries local `+Y` to
        // `(-sin, cos, 0)`, which is the axis the capsule runs along.
        let (s_l, c_l) = lean.sin_cos();
        let axis = [-s_l, c_l];
        for end in [1.0_f32, -1.0] {
            for side in [1.0_f32, -1.0] {
                bones.push(blob_ellipsoid(
                    [
                        axis[0] * bone_len * end + axis[1] * bone_r * 0.85 * side,
                        bone_y + axis[1] * bone_len * end - axis[0] * bone_r * 0.85 * side,
                        dz,
                    ],
                    [bone_r * 0.72, bone_r * 0.72, DEVICE_SKIN * 0.9],
                    bone_r * 0.3,
                ));
            }
        }
    }

    // The cloth's weave is the one material here that tiles, so it is the one
    // that has to be told what scale it will be instanced at. Passing the bone
    // through as well costs nothing (its `uv_scale` is inert) and keeps the habit
    // from having exceptions.
    let mut colours = prim(
        blob_group(
            cloth,
            FLAG_RES,
            uv_for_scale(sailcloth(HULL_TAR, [0.09, 0.09, 0.10]), scale),
        ),
        [0.0; 3],
        id_quat(),
    );
    colours.children = vec![
        prim(
            blob_group(skull, DEVICE_RES, uv_for_scale(bone(BONE_PALE), scale)),
            [0.0; 3],
            id_quat(),
        ),
        prim(
            blob_group(bones, DEVICE_RES, uv_for_scale(bone(BONE_PALE), scale)),
            [0.0; 3],
            id_quat(),
        ),
    ];
    colours
}

/// The black colours — a hanging flag with a skull and crossed bones, flown at
/// `height`.
///
/// Lives here rather than in whichever file needed it first (#972 lesson 5): the
/// battery flies one over its terreplein, the gate flies one from its yard and
/// the signal mast hoists one at its gaff, and three files each rolling their own
/// is three files that can drift.
///
/// `hoist` is the point **on the staff** the flag is bent to, not the cloth's
/// centre. Taking the attachment point is what makes the flag attached: the luff
/// laps [`CANON_LAP`]'s worth onto the timber by construction, and a caller
/// cannot leave it hanging in space by getting an offset wrong. The fly runs to
/// `+X` and the cloth hangs below `hoist`.
///
/// # One dimension, not two
///
/// Only the height is taken, because only a **uniform** scale is safe — a
/// non-uniform one shears rotated children, and the bones are rotated. The width
/// follows from [`CANON_ASPECT`]. That is a real restriction on callers and it is
/// the right one: the previous signature took `w` and `h` independently and so
/// let a caller ask for an aspect ratio the device had never been drawn for,
/// while every absolute constant in the draft stayed put.
pub(super) fn jolly_roger(hoist: [f32; 3], height: f32) -> Generator {
    use super::util::{id_quat, prim_scaled};

    let scale = height / CANON_H;
    let width = height * CANON_ASPECT;
    // Cloth centre, derived from the attachment point so the luff overlaps the
    // staff at whatever size the flag is flown.
    let c = [
        hoist[0] + width * 0.5 - CANON_LAP * scale,
        hoist[1] - height * 0.5,
        hoist[2],
    ];
    let drawn = colours_canonical(scale);
    let mut flown = prim_scaled(drawn.kind, c, id_quat(), [scale; 3]);
    flown.children = drawn.children;
    flown
}

/// A capstan: the barrel, its whelps, the iron pawl rim and six bars.
///
/// Promoted here the moment it had a second caller (#972 lesson 5) — the
/// careening slip heaves a hull down with one and the standalone quayside
/// prop is one. It is also the piece that taught this kit the most about
/// rotations: the bars shipped "laid flat" by snapping each to the nearest
/// quarter-turn, which left the four off-cardinal ones standing at wrong
/// angles (#1028). They are [`strut`](super::util::strut)s from socket to
/// tip now, horizontal because their endpoints are.
///
/// `at` is the barrel's foot — the deck or pad it is stepped on. `bars` is
/// how many are shipped: a working capstan has every socket filled, a resting
/// one has two or three and the rest stowed, and that difference is most of
/// what says whether anybody is heaving right now.
pub(super) fn capstan(at: [f32; 3], bars: usize, seed: u32) -> Generator {
    use super::util::{cylinder_tapered, id_quat, nest, prim, solid, strut, torus};

    let barrel_h = 1.05;
    let mut carried = Vec::new();

    // Whelps — the vertical ribs the messenger renders against. Without them
    // a capstan is a bollard: the ribs are what make the taper read as
    // something a rope grips.
    for i in 0..6 {
        let a = i as f32 * std::f32::consts::TAU / 6.0 + 0.26;
        carried.push(prim(
            solid(cylinder_tapered(
                0.07,
                barrel_h * 0.74,
                6,
                0.1,
                board(HULL_OAK),
            )),
            [
                at[0] + 0.44 * a.cos(),
                at[1] + barrel_h * 0.44,
                at[2] + 0.44 * a.sin(),
            ],
            id_quat(),
        ));
    }
    // Pawl rim at the head, and the drumhead over it.
    carried.push(prim(
        torus(0.05, 0.5, iron(IRON_BLACK, seed)),
        [at[0], at[1] + barrel_h * 0.98, at[2]],
        id_quat(),
    ));
    carried.push(prim(
        solid(cylinder_tapered(0.56, 0.14, 14, 0.06, board(DECK_HOLY))),
        [at[0], at[1] + barrel_h + 0.07, at[2]],
        id_quat(),
    ));

    // The bars, in their sockets. Struts, so a bar is horizontal because its
    // two ends are at one height — not because a formula said so.
    let bar_y = at[1] + barrel_h * 0.94;
    for i in 0..bars.min(6) {
        let a = i as f32 * std::f32::consts::TAU / 6.0;
        let dir = [a.cos(), a.sin()];
        carried.push(strut(
            [at[0] + 0.42 * dir[0], bar_y, at[2] + 0.42 * dir[1]],
            [at[0] + 2.05 * dir[0], bar_y, at[2] + 2.05 * dir[1]],
            0.065,
            6,
            board(DECK_HOLY),
        ));
    }

    nest(
        prim(
            solid(cylinder_tapered(0.52, barrel_h, 14, 0.16, board(HULL_OAK))),
            [at[0], at[1] + barrel_h * 0.5, at[2]],
            id_quat(),
        ),
        carried,
    )
}

// ---------------------------------------------------------------------------
/// One of a hull's **frames**: a `U` of curved timber that touches the keel on
/// the centreline and rises to the sheer on both sides, open to the sky.
///
/// Lives here rather than in whichever file needed it first (#972 lesson 5),
/// because both cursed-register entries are built out of these — the
/// [`rotting_hulk`]'s bare forward half and the ribs standing out of the shingle
/// in [`tideline_bones`] — and because the shape cost three renders to arrive at.
/// A frame is the single most recognisable thing about a wreck, so the kit
/// cannot afford two spellings of it.
///
/// `at` is where the frame's **ellipse centre** goes, which is one `height`
/// above the keel *in the hull's own frame* — so a caller with a tilted hull
/// passes `their_frame(0.0, height, station)` rather than the keel point itself.
/// That is the whole subtlety of the contract: the raise has to happen before
/// the tilt, or a heeled hull's frames lift straight up out of it instead of
/// standing on it.
///
/// `half_beam` and `height` are the frame's real half-width and rise, which are
/// *different numbers* — so the primitive is a torus drawn to the beam and then
/// **scaled** to the sheer, rather than a circular arc that splits the difference
/// and reaches neither. `tilt` is the hull's own rotation, applied on the left of
/// the quarter turn.
///
/// # Why it is built this way
///
/// A half-ring stood up by a quarter turn about `X` gives either an `∩` arching
/// over the centreline (a wagon hoop) or a `∪` hanging beneath the keel,
/// depending on which half of the sweep `path_cut` keeps. Neither is a frame,
/// and **no rotation converts one into the other usefully**: a semicircle is
/// congruent to its own reflection, so flipping the hanging arc simply returns
/// the hoop. The arc that hangs has exactly the right form and is merely in the
/// wrong place, so the answer is a **translation** — raise it by its own
/// semi-height and its trough lands on the keel while its ends reach the sheer.
///
/// The negative quarter turn is required because `path_cut` keeps the arc on the
/// primitive's local `+Z`; a positive one sends it to world `−Y`, which is the
/// signal mast's vault fault (#1021) reached from the other side.
pub(super) fn hull_frame(
    at: [f32; 3],
    half_beam: f32,
    height: f32,
    tilt: Fp4,
    section: f32,
    material: SovereignMaterialSettings,
) -> Generator {
    use super::util::{prim_scaled, quat_mul, quat_x, with_cut};
    let beam = half_beam.max(0.05);
    prim_scaled(
        with_cut(
            super::util::torus(section, beam, material),
            [0.5, 1.0],
            [0.0, 1.0],
            0.0,
        ),
        at,
        quat_mul(tilt, quat_x(-std::f32::consts::FRAC_PI_2)),
        // Drawn to the beam, stretched to the sheer. The tube stretches with it,
        // which is right: a frame is a sided timber, deeper than it is thick.
        [1.0, 1.0, height / beam],
    )
}

// ---------------------------------------------------------------------------
// Palette
// ---------------------------------------------------------------------------
//
// #972 lesson 32: a shared colour constant is a shared *selector*. Every guard
// in this family picks its prims by material as much as by size, so two things
// that happen to be the same colour become the same class of part to every
// test ever written against the kit. Constants below are therefore named for
// the thing they clothe, and two of them (`HULL_TAR` / `IRON_BLACK`) are kept
// separate despite being near-identical values for exactly that reason.

/// Tarred hull planking below the wale — the darkest timber in the kit.
pub(super) const HULL_TAR: [f32; 3] = [0.14, 0.13, 0.12];
/// Oiled oak — hull topsides, wharf piles, the tavern's frame, gun carriages.
pub(super) const HULL_OAK: [f32; 3] = [0.33, 0.23, 0.14];
/// Holystoned deck planking, scrubbed pale by the watch.
pub(super) const DECK_HOLY: [f32; 3] = [0.63, 0.56, 0.43];
/// Weathered grey wharf and staging timber, silvered by salt.
pub(super) const WHARF_GREY: [f32; 3] = [0.48, 0.46, 0.42];
/// Joinery oak — window frames, door stiles, sign-board mouldings. Dark, as
/// period joinery was, and the frame colour [`glass`] is glazed into.
pub(super) const OAK_JOINERY: [f32; 3] = [0.28, 0.19, 0.11];

/// Powder-stained limestone — the battery's dressed face and copings.
pub(super) const STONE_LIME: [f32; 3] = [0.60, 0.58, 0.52];
/// Wet quay cobbles and rubble footings, darker than the dressed stone.
pub(super) const STONE_QUAY: [f32; 3] = [0.40, 0.39, 0.36];
/// Split-oak shingle, weathered to grey-brown.
pub(super) const SHINGLE_GREY: [f32; 3] = [0.40, 0.37, 0.32];

/// Salt-bleached canvas — sails, awnings, tarpaulins.
pub(super) const CANVAS_BONE: [f32; 3] = [0.79, 0.75, 0.65];
/// The shaded weft of the same canvas.
pub(super) const CANVAS_SHADE: [f32; 3] = [0.64, 0.60, 0.51];
/// Hemp cordage, tan where it is new and grey where it is not.
pub(super) const ROPE_HEMP: [f32; 3] = [0.60, 0.51, 0.34];

/// Wrought iron, near-black under its rust.
pub(super) const IRON_BLACK: [f32; 3] = [0.17, 0.17, 0.18];
/// Cast gunmetal, before the patina takes it — **ordnance only**: barrels,
/// cascabels, reinforcing rings.
///
/// Kept distinct from [`BRONZE_FITTING`] although the two values are within a
/// hair of each other, because #972 lesson 32 is that a shared colour constant
/// is a shared *selector*. The battery's "one gun per opening" guard picks its
/// barrels by bronze-and-tapered, and while the lanterns' caps wore this same
/// constant it counted fifteen guns on a ten-gun fort — the caps are bronze
/// and tapered too. Two things that are the same colour by coincidence are the
/// same thing to every guard that ever reads the tree.
pub(super) const BRONZE_GUN: [f32; 3] = [0.45, 0.36, 0.19];
/// Cast bronze **fittings** — lantern caps and bases, bell metal, sheaves,
/// traversing gear. Everything bronze that is not a gun. See [`BRONZE_GUN`]
/// for why this is its own constant rather than the same one.
pub(super) const BRONZE_FITTING: [f32; 3] = [0.44, 0.37, 0.21];
/// Gold leaf — the tavern's lettering, the monument's frame, a captain's
/// vanity. Used in small areas only; a broad gold face reads as a lightbox.
pub(super) const GOLD_LEAF: [f32; 3] = [0.76, 0.60, 0.23];

/// The ensign's red — the colours flown over the battery, the tavern's
/// paintwork, a sash. The kit's one loud hue, and it is loud on purpose.
pub(super) const ENSIGN_RED: [f32; 3] = [0.50, 0.10, 0.09];

/// Grey storm-beach shingle. Deliberately the cold counterpart to
/// `coastal_resort::SAND_TAN`.
pub(super) const STRAND_SHINGLE: [f32; 3] = [0.60, 0.57, 0.50];

/// Old bone and ivory, for the cursed register.
pub(super) const BONE_PALE: [f32; 3] = [0.78, 0.75, 0.66];

// --- Lit colours -----------------------------------------------------------
//
// Standing gotcha: deep-saturate an emissive and keep the strength LOW. A pale
// hue at strength blooms to a white blank, and a broad lit face does it
// fastest. Every constant here is meant for a *small* surface — a pane behind
// glass, a lantern flame, a strip under a lintel.

/// The amber of a tallow lantern seen through a leaded light.
pub(super) const GLASS_AMBER: [f32; 3] = [0.58, 0.42, 0.20];
/// A lantern or candle flame at close range.
pub(super) const LAMP_TALLOW: [f32; 3] = [1.0, 0.82, 0.48];
/// Deep-saturated amber for the larger lit faces — a name-board, a
/// window-lit gable. Richer than [`LAMP_TALLOW`] precisely so it holds its
/// hue where the pale gold would go white.
pub(super) const SIGN_AMBER: [f32; 3] = [0.95, 0.50, 0.12];
/// The cursed register's corpse-light — a cold green that reads as *wrong*
/// beside every other lit colour in this kit.
///
/// The whole eerie register turns on this one hue, and it works because it is
/// the only thing here that is not firelight. [`LAMP_TALLOW`], [`GLASS_AMBER`]
/// and [`SIGN_AMBER`] are all warm, so the working harbour glows amber from
/// end to end; witchfire is its complement and reads as something burning that
/// has no business burning. That is a relationship between colours, not a
/// property of one — the green is only cold because everything else is warm.
///
/// Deep-saturated and used at LOW strength on SMALL surfaces, per the standing
/// gotcha above: a pale mint at strength would bloom to a white blank and lose
/// the very hue the register depends on.
pub(super) const WITCHFIRE: [f32; 3] = [0.14, 0.66, 0.36];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::SovereignTextureConfig as Tex;

    /// #972 lesson 4, pinned kit-wide rather than per item: any `stagger`
    /// above 0.01 switches on the plank generator's hard-coded
    /// three-butt-joints-per-tile grid, and a wall of it reads as masonry.
    /// Seven kits shipped that way before it was caught, so this kit states
    /// the rule as a test on its own helpers instead of trusting the note.
    #[test]
    fn plank_helpers_lay_unbroken_courses() {
        for (name, m) in [("strake", strake(HULL_OAK)), ("board", board(HULL_OAK))] {
            let Tex::Plank(cfg) = &m.texture else {
                panic!("{name} is not a Plank material");
            };
            assert_eq!(
                cfg.stagger.0, 0.0,
                "{name} carries the end-joint grid (#972 lesson 4)"
            );
        }
    }

    /// A strake is wider than a shore carpenter's board, which is the whole
    /// reason both exist. Stated as a relationship, not as two numbers: the
    /// widths may be retuned, the ordering may not.
    #[test]
    fn a_strake_is_wider_than_a_sawn_board() {
        // `uv_scale` is tiles per metre, so a *wider* board is a *smaller*
        // scale. Comparing the derived tile sizes rather than the scales
        // keeps the assertion readable in the units it was authored in.
        let strake_tile = 1.0 / strake(HULL_OAK).uv_scale.0;
        let board_tile = 1.0 / board(HULL_OAK).uv_scale.0;
        assert!(
            strake_tile > board_tile * 1.3,
            "a strake tile ({strake_tile} m) must be clearly wider than a \
             board tile ({board_tile} m), or the two timbers read alike"
        );
        assert!(
            (strake_tile / PLANK_COUNT as f32 - STRAKE_W).abs() < 1e-4,
            "the strake's board came out at {} m, not the authored {STRAKE_W}",
            strake_tile / PLANK_COUNT as f32
        );
    }

    /// No helper in this kit is a `Window` card yet, and the one that glazes
    /// a solid is deliberately not one (#972 lessons 1 and 20).
    ///
    /// Worth stating now rather than when the card arrives (#1020), because
    /// the ordering is the whole point: `tinted_glass` was written *first*,
    /// so the lanterns had a correct material to reach for before the
    /// tempting wrong one existed. The steampunk gas lamp shipped four
    /// `Window` cuboids for its panes precisely because the kit's card was
    /// the only glass in the drawer.
    #[test]
    fn the_solid_glazing_helper_is_not_a_card() {
        let m = tinted_glass(GLASS_AMBER, 0.35);
        assert!(
            !matches!(m.texture, Tex::Window(_)),
            "tinted_glass has become a card; wrapped round a lantern its panes \
             are masked away and you see the sky through the far side"
        );
        assert!(
            m.emission_strength.0 > 0.0 && m.emission_strength.0 < 1.0,
            "a lantern's glass carries a faint self-glow, not the flame's own"
        );
    }

    /// The colours are three connected groups, hung on their staff, with the
    /// device standing on the cloth's front face (#1025, #1026).
    ///
    /// Four claims, each of which was a fault in-world:
    ///
    /// * **The cloth is ONE mesh.** A `BlobGroup` is polygonised on a sample
    ///   grid, so a sheet thinner than ~2 cells is missed in places and comes
    ///   out with holes. At 30 mm thick and resolution 30 the flag meshed as
    ///   two disconnected slabs with a gap down the middle. Checked by union-
    ///   find over the triangle graph, which is the only way to see it — the
    ///   bounding box of a flag with a hole in it is the same as a whole one.
    /// * **The device is proud of the front and short of the back.** A 0.34 m
    ///   skull on a 0.06 m cloth put most of itself out the reverse.
    /// * **Skull and bones are separate groups.** Blended together they melt
    ///   at the jaw and read as one lumpy figure with arms.
    /// * **The luff laps the staff.** Positioned by its centre, the cloth left
    ///   a 100 mm gap and the colours looked detached from their own pole.
    ///
    /// Run at the size the battery flies it at, since that is what a viewer sees
    /// — but every claim here is a ratio, so it holds at all three sites.
    #[test]
    fn the_colours_are_one_cloth_carrying_a_separate_skull_and_bones() {
        use crate::catalogue::items::measure;
        use crate::catalogue::items::util::{blob_cell_size, blob_components};

        let hoist = [0.0_f32, 2.0, 0.0];
        let h = 1.25_f32;
        let flag = jolly_roger(hoist, h);

        // Three groups: cloth, skull, bones.
        let mut kinds = Vec::new();
        fn walk(g: &Generator, out: &mut Vec<crate::pds::GeneratorKind>) {
            if matches!(g.kind, crate::pds::GeneratorKind::BlobGroup { .. }) {
                out.push(g.kind.clone());
            }
            for c in &g.children {
                walk(c, out);
            }
        }
        walk(&flag, &mut kinds);
        assert_eq!(
            kinds.len(),
            3,
            "expected a cloth, a skull and a bones group, found {}",
            kinds.len()
        );
        for (i, k) in kinds.iter().enumerate() {
            assert_eq!(
                blob_components(k),
                1,
                "group {i} polygonised into more than one piece — its elements \
                 are out of blend range, or it is thinner than the sample grid \
                 can resolve"
            );
        }

        // The cloth is thick enough for its own grid to see it — checked in
        // CANONICAL units, because that is the frame the sampling happens in.
        // Scale is applied to the finished mesh, so this is the one claim here
        // that would be meaningless measured in the world.
        let cell = blob_cell_size(CANON_W, FLAG_RES);
        assert!(
            CANON_SKIN * 2.0 > cell * 2.0,
            "a {} m cloth on {cell} m cells is under two cells thick and will \
             polygonise with holes in it",
            CANON_SKIN * 2.0
        );

        let solids = measure::solids(&flag);
        assert_eq!(solids.len(), 3);
        let cloth = solids
            .iter()
            .max_by(|a, b| {
                a.bounds
                    .size()
                    .x
                    .partial_cmp(&b.bounds.size().x)
                    .expect("finite extents")
            })
            .expect("the cloth is the widest group");

        // It is flown at the size it was asked for, not the size it was drawn at.
        assert!(
            (cloth.bounds.size().y - h).abs() < h * 0.06,
            "asked for a {h} m flag and got one {} m deep — the instancing scale \
             is not doing what the signature promises",
            cloth.bounds.size().y
        );

        // The luff laps onto the staff at the hoist.
        assert!(
            cloth.bounds.min.x <= hoist[0] + 1e-3,
            "the cloth's luff is at x = {} and the staff at {} — the colours \
             are hanging beside their own pole",
            cloth.bounds.min.x,
            hoist[0]
        );

        // Hero side is -Z, so the front face is the smaller z. Every device
        // group must stand proud of it and stop short of the back.
        for part in &solids {
            if std::ptr::eq(part, cloth) {
                continue;
            }
            assert!(
                part.bounds.min.z < cloth.bounds.min.z,
                "a device group's face is at z = {} and the cloth's at {} — \
                 the relief is not standing proud of the flag",
                part.bounds.min.z,
                cloth.bounds.min.z
            );
            assert!(
                part.bounds.max.z < cloth.bounds.max.z,
                "a device group reaches z = {} and the cloth's back is at {} \
                 — it is coming out of the back of the flag",
                part.bounds.max.z,
                cloth.bounds.max.z
            );
            // ...and it is BEDDED IN, not standing off. The device's back has to
            // be behind the cloth's front face, or it is a plaque hovering in
            // front of the canvas — which is what it was, and which is also what
            // stops the carved sockets working: a hole cut through bone that has
            // sky behind it is a hole onto sky, and the whole point is that the
            // flag shows through its own skull.
            assert!(
                part.bounds.max.z > cloth.bounds.min.z,
                "a device group's back is at z = {} and the cloth's front at {} \
                 — it is floating clear of the sheet instead of sunk into it",
                part.bounds.max.z,
                cloth.bounds.min.z
            );
            assert!(
                part.bounds.size().x < cloth.bounds.size().x * 0.8
                    && part.bounds.size().y < cloth.bounds.size().y * 0.8,
                "a device group {:?} is not comfortably inside the cloth {:?}",
                part.bounds.size(),
                cloth.bounds.size()
            );
        }

        // The device reads as TWO things: the skull's jaw clears the crossbones'
        // upper knuckles with cloth showing between them. Drawn with the bones
        // crossing at the chin — where a naive layout puts them — the skull, the
        // jaw and the arms of the X merge into one pale mass, which is what the
        // first oversized draft did. The bones are the wider group; the skull is
        // the taller-placed one.
        let device: Vec<_> = solids.iter().filter(|p| !std::ptr::eq(*p, cloth)).collect();
        assert_eq!(device.len(), 2, "the device is a skull and a bones group");
        let (bones, skull) = if device[0].bounds.size().x > device[1].bounds.size().x {
            (device[0], device[1])
        } else {
            (device[1], device[0])
        };
        assert!(
            bones.bounds.max.y < skull.bounds.min.y,
            "the bones reach up to y = {} and the skull begins at {} — they \
             overlap, so the device reads as one mass rather than as a skull \
             above two bones",
            bones.bounds.max.y,
            skull.bounds.min.y
        );
        // ...and the bones are the wider of the two, which is what stops them
        // reading as an afterthought tucked under the cranium.
        assert!(
            bones.bounds.size().x > skull.bounds.size().x * 1.2,
            "the bones span {} against a skull {} wide — the crossbones are the \
             other half of the device, not a detail beneath it",
            bones.bounds.size().x,
            skull.bounds.size().x
        );
    }

    /// Every size the colours are flown at is the SAME flag.
    ///
    /// This is the invariant the oversized-sub-assembly technique exists to buy,
    /// and it is the one the previous build did not have. `jolly_roger` used to
    /// take `w` and `h` while `FLAG_SKIN`, `DEVICE_SKIN`, `DEVICE_BITE`,
    /// `FLAG_RIPPLE` and `HOIST_LAP` were all absolute metre values — so the
    /// signal mast's 1.0 m flag carried a relatively 27 % thicker cloth, a 27 %
    /// deeper bite and a finer sample grid than the battery's 1.25 m one. Three
    /// flags that were supposed to be one flag.
    ///
    /// Checked as *similarity*: build at two sizes and compare every group's
    /// bounds as a fraction of its own cloth. If those ratios agree, the two are
    /// the same object at two sizes; if any absolute constant creeps back into
    /// the draft, they stop agreeing and this says which.
    #[test]
    fn the_colours_are_geometrically_similar_at_every_size_flown() {
        use crate::catalogue::items::measure;

        let profile = |h: f32| -> Vec<[f32; 3]> {
            let flag = jolly_roger([0.0, 4.0, 0.0], h);
            let solids = measure::solids(&flag);
            let cloth_y = solids
                .iter()
                .map(|p| p.bounds.size().y)
                .fold(f32::MIN, f32::max);
            // Each group's extent as a fraction of the cloth's own depth, which
            // makes the whole profile dimensionless.
            let mut out: Vec<[f32; 3]> = solids
                .iter()
                .map(|p| {
                    let s = p.bounds.size();
                    [s.x / cloth_y, s.y / cloth_y, s.z / cloth_y]
                })
                .collect();
            out.sort_by(|a, b| a[0].partial_cmp(&b[0]).expect("finite"));
            out
        };

        let small = profile(1.0);
        let large = profile(1.25);
        assert_eq!(small.len(), 3, "expected three groups at every size");
        assert_eq!(small.len(), large.len());
        for (a, b) in small.iter().zip(large.iter()) {
            for axis in 0..3 {
                assert!(
                    (a[axis] - b[axis]).abs() < 0.03,
                    "a group measures {a:?} of its cloth at one size and {b:?} at \
                     another — the draft still carries an absolute dimension \
                     somewhere, so these are not the same flag"
                );
            }
        }
    }

    /// The cloth's weave is the same size in the WORLD at every size flown.
    ///
    /// The one trap of the oversized technique that geometry alone cannot catch:
    /// UVs are emitted in prim-*local* metres, so `uv_scale` is tiles per local
    /// metre and a draft drawn at ten times its flown size keeps ten times the
    /// tile repeats. Left uncorrected the canvas weave comes out ten times too
    /// fine — invisible in a bounding box, obvious in-world, and exactly the
    /// class of fault the kit's brick convention was built to stop.
    ///
    /// [`uv_for_scale`](super::super::util::uv_for_scale) is the correction; this
    /// checks it by asking what one tile measures in world metres, which must be
    /// the sailcloth's authored tile whatever size the flag is.
    #[test]
    fn the_weave_is_the_same_size_in_the_world_at_every_scale() {
        fn cloth_tile_metres(h: f32) -> f32 {
            let flag = jolly_roger([0.0, 4.0, 0.0], h);
            let scale = flag.transform.scale.0[0];
            let uv = flag
                .kind
                .material()
                .expect("the cloth carries a material")
                .uv_scale
                .0;
            // uv_scale is tiles per LOCAL metre; a local metre is `scale` world
            // metres, so one tile spans `1 / uv_scale * scale` in the world.
            scale / uv
        }
        let want = 1.0 / sailcloth(HULL_TAR, [0.09, 0.09, 0.10]).uv_scale.0;
        for h in [1.0_f32, 1.05, 1.25] {
            let got = cloth_tile_metres(h);
            assert!(
                (got - want).abs() < want * 0.02,
                "a {h} m flag's weave tiles every {got} m in the world where the \
                 sailcloth is authored at {want} m — the uv correction for the \
                 instancing scale is missing or has the wrong sign"
            );
        }
    }

    /// Only a uniform scale is instanced, because a non-uniform one shears any
    /// rotated child — and the crossed bones are rotated.
    #[test]
    fn the_colours_are_instanced_with_one_uniform_scale() {
        let flag = jolly_roger([0.0, 4.0, 0.0], 1.25);
        let s = flag.transform.scale.0;
        assert!(
            (s[0] - s[1]).abs() < 1e-6 && (s[1] - s[2]).abs() < 1e-6,
            "the colours are instanced at {s:?} — a non-uniform scale shears the \
             rotated bones, and the transform sanitiser clamps each component \
             independently so nothing downstream will catch it"
        );
        // ...and the children ride on it rather than carrying their own, which is
        // the whole mechanism.
        for c in &flag.children {
            assert_eq!(
                c.transform.scale.0,
                [1.0, 1.0, 1.0],
                "a device group carries its own scale — it should be inheriting \
                 the cloth's, or the two can drift apart"
            );
            assert_eq!(
                c.transform.translation.0,
                [0.0, 0.0, 0.0],
                "a device group sits at {:?} in the cloth's local frame; the draft \
                 authors every offset inside the geometry precisely so `nest`'s \
                 not dividing by scale cannot bite",
                c.transform.translation.0
            );
        }
    }

    /// Every surface generator in the kit is sized in metres through
    /// `tiles_per_metre` (#936), so no helper still carries a pre-#933
    /// per-prim repeat count. The card helpers are the deliberate exception
    /// and are checked by their own test above.
    #[test]
    fn tiling_helpers_are_sized_in_metres() {
        let tiling: [(&str, SovereignMaterialSettings); 8] = [
            ("strake", strake(HULL_OAK)),
            ("board", board(HULL_OAK)),
            ("ashlar", ashlar(STONE_LIME, 1)),
            ("cobbles", cobbles(STONE_QUAY, 2)),
            ("shingle", shingle(SHINGLE_GREY)),
            ("sailcloth", sailcloth(CANVAS_BONE, CANVAS_SHADE)),
            ("hemp", hemp(ROPE_HEMP)),
            ("strand", strand(STRAND_SHINGLE)),
        ];
        for (name, m) in tiling {
            let tile_m = 1.0 / m.uv_scale.0;
            assert!(
                (0.05..=6.0).contains(&tile_m),
                "{name}'s tile is {tile_m} m — outside the range any of these \
                 surfaces is authored at, which is what a stale repeat count \
                 looks like"
            );
            assert!(
                !matches!(m.texture, Tex::None | Tex::Window(_)),
                "{name} is not a tiling surface and does not belong in this list"
            );
        }
    }

    /// The flat helpers really are flat. `uv_scale` is inert without a
    /// texture, and a non-1.0 value left on one reads to the next author as
    /// though it meant something.
    #[test]
    fn untextured_helpers_pin_their_inert_scale() {
        for (name, m) in [
            ("tar", tar(HULL_TAR)),
            ("bone", bone(BONE_PALE)),
            ("tinted_glass", tinted_glass(GLASS_AMBER, 0.4)),
        ] {
            assert!(matches!(m.texture, Tex::None), "{name} grew a texture");
            assert_eq!(m.uv_scale.0, 1.0, "{name} carries an inert uv_scale");
        }
    }
}
