//! Harbour Battery — the Pirate theme's landmark.
//!
//! A two-tier stone water-battery standing over the roads: four casemate
//! ports at quay level and six embrasures on the terreplein above, every one
//! of them with a gun run out through it, over a lit chamber. A deep arched
//! guard house opens in the middle of the face; a talus, a cordon and a
//! coped parapet band the mass; the black colours fly from a staff on the
//! deck, and a flight of steps climbs the gorge behind.
//!
//! # Why this building has no glazing at all
//!
//! Almost every entry in the #972 ledger reached the same place by a
//! different road: a wall wants openings, an opening wants something behind
//! it, and a `Window` card is how a *domestic* opening is filled. A battery
//! is the building where that is simply the wrong question. Its openings are
//! embrasures and casemate ports — genuine holes with a gun in them — so the
//! alpha-card idiom never enters into it (#972 lesson 24: ask what the real
//! thing does before reaching for the idiom, and the boardwalk's open serving
//! hatch is the same answer arrived at from a kiosk).
//!
//! That is most of why this subject was chosen as the landmark. The card
//! rules are the ones the ledger has broken most often; a hero building that
//! cannot break them is a hero building whose first render is about its
//! massing.
//!
//! # What is behind each opening
//!
//! Ten openings, ten things to look at, because a shell is not enough and the
//! fit-out has to be laid out bay by bay (#972 lesson 9). Every casemate has
//! its own gun, its own lit floor, its own warm rear lining held 2.5 m back,
//! and its own lantern hung **below** the port head — the head spans the
//! opening, so anything at ceiling level is in the shadow of its own reveal
//! (#972 lesson 10). The guard house has a lit passage, a barred inner gate,
//! a table and a powder budge-barrel.
//!
//! # Structure
//!
//! Nested (#972 lesson 3), root at the bottom: apron → talus → wall (piers,
//! head band, rear mass) → cordon → terreplein deck → parapet → guns and
//! colours, with the gorge steps as their own subtree off the apron. One
//! gizmo drag on the deck takes the parapet, the guns and the flag with it.
//! The hero face is `-Z`, which is where the render tool and the settlement
//! placer both look from.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    attach, bonded_siding, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, face_uv_offset,
    footing, glow, id_quat, lit_interior, nest, prim, quat_x, quat_z, solid, sphere,
    superellipsoid, torus, wedge, with_cut, with_face,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRONZE_GUN, CANVAS_BONE, CANVAS_SHADE, DECK_HOLY, ENSIGN_RED, GOLD_LEAF, HULL_OAK, HULL_TAR,
    IRON_BLACK, PORT_BAND, ROPE_HEMP, SHINGLE_GREY, STONE_LIME, STONE_QUAY, STRAND_SHINGLE,
    WHARF_GREY, ashlar, board, bronze, cobbles, fx, hemp, iron, jolly_roger, lantern, sailcloth,
    shingle, strake, strand,
};

// ---------------------------------------------------------------------------
// The work, stated once
// ---------------------------------------------------------------------------
//
// Every dimension below is derived from these. #972 lesson 18: a quantity
// that exists twice is a quantity that will disagree with itself — the
// factory's stack built its brick frame from one reading of "the middle" and
// placed itself from another, and the courses came out 0.8 m out of the frame
// every other surface on the building shared.

/// Cobbled apron the whole work stands on — and the sub-root every footprint
/// guard measures against (#972 lesson 19).
///
/// Sized to **hug** the work rather than to sit under it as a plate. The first
/// render made the case: at 25 m the apron oversailed the talus by better than
/// two metres a side, and a pale cobbled rectangle that big stops reading as
/// ground and starts reading as a table the fort has been put on top of. It is
/// now 0.7 m clear of the talus in X.
///
/// The depth is a different quantity and stays generous, because it is not
/// chosen — it is *derived*, by the gorge flight. Thirteen treads at a 0.30 m
/// going run 3.9 m out from the back of the wall, and the apron has to contain
/// them or the flight lands on bare ground, which is the fault
/// `every_part_stands_on_the_apron` exists to catch. Shrinking Z here would
/// only move that error somewhere a render cannot see it.
const APRON: [f32; 3] = [22.0, 0.35, 20.0];
/// Apron top: quay level.
const QUAY: f32 = APRON[1];

/// Battered talus at the foot of the scarp — footprint at its base.
const TALUS: [f32; 3] = [20.6, 0.90, 11.6];
/// Talus batter, `[x, z]`. A masonry talus runs about one in six; over its
/// 0.9 m this is the fraction that gives that slope, and it pinches the
/// **top**, which is the way a talus is built and the opposite of a corbel.
const TALUS_BATTER: [f32; 2] = [0.03, 0.06];
/// Talus top — and the sill the casemate ports fire over.
const CASEMATE_SILL: f32 = QUAY + TALUS[1];

/// Vertical scarp wall above the talus: width, height, depth.
const WALL: [f32; 3] = [19.8, 2.60, 10.8];
/// Wall top.
const WALL_TOP: f32 = CASEMATE_SILL + WALL[1];
/// The wall's seaward face — the hero plane the guns run out through.
const FACE_Z: f32 = -WALL[2] * 0.5;

/// Clear height of a casemate port, and the head band above it.
const PORT_H: f32 = 1.70;
/// Casemate port clear width.
const PORT_W: f32 = 1.90;
/// Head band over the ports, spanning the whole face in one prim.
const HEAD_H: f32 = WALL[1] - PORT_H;
/// Port head — the underside of the head band.
const PORT_HEAD: f32 = CASEMATE_SILL + PORT_H;

/// Depth of every chamber cut into the mass: the four casemates and the guard
/// house passage.
///
/// #972 lesson 6: goods against the back wall of a 7 m shop are unreadable
/// specks, so the display run is held forward. The same arithmetic upward and
/// inward — 2.5 m is far enough that the chamber has depth and near enough
/// that its lit lining is the thing you see through the port rather than a
/// dark smudge.
const CHAMBER_D: f32 = 2.5;
/// The plane every chamber's rear lining stands on.
const CHAMBER_BACK: f32 = FACE_Z + CHAMBER_D;

/// How far a chamber's fit-out is held inside the opening it sits in.
///
/// Everything loose in a chamber — the floor, the rear lining — is sized to
/// the CLEAR span less this on each side, so no piece of fit-out ever runs
/// into the masonry that frames it or shares a plane with it. The first build
/// sized the floor at `PORT_W + 0.9`, which put 0.45 m of it inside each pier.
const CHAMBER_INSET: f32 = 0.05;

/// How far a chamber floor is bedded BELOW the sill it lies on.
///
/// Laid exactly on the sill its underside is coplanar with the talus top over
/// its whole footprint, which is a z-fight across the brightest surface inside
/// the port. Bedded in, there is no shared plane and it reads as a boarded
/// floor laid over stone, which is what it is.
const FLOOR_SINK: f32 = 0.03;

/// Guard house opening — clear width, and its head (shared with the ports so
/// the elevation has one head line).
const GUARD_W: f32 = 2.60;
const GUARD_HEAD: f32 = PORT_HEAD;

/// Cordon — the projecting ring at the wall head. A ring is centred on the
/// **building** and its projection goes into its SIZE (#972 lesson 31); a
/// ring centred on a trim plane becomes a cantilevered shelf the width of the
/// site.
const CORDON_PROJECT: f32 = 0.40;
const CORDON_H: f32 = 0.32;
const CORDON_TOP: f32 = WALL_TOP + CORDON_H;

/// Terreplein deck — the gun platform, set inside the cordon.
const DECK: [f32; 3] = [19.2, 0.28, 10.2];
/// Deck top: the level the guns and their crews stand on.
const DECK_TOP: f32 = CORDON_TOP + DECK[1];

/// Parapet along the seaward edge of the deck.
const PARAPET_H: f32 = 1.50;
const PARAPET_D: f32 = 0.90;
/// Merlon stock between the embrasures.
const MERLON_W: f32 = 1.45;
/// How many embrasures the battery fires through.
///
/// Five, not the six the first draft had, and one casemate port per wing
/// rather than two. The cut is a **record-size** decision rather than a
/// compositional one: at six and four the entry came to 270 nodes and 138 KB,
/// which is two and a half times the next heaviest landmark in the catalogue
/// (the factory, at 55 KB) and enough on its own to put a seeded room over the
/// soft budget. Guns are ~7 prims each and they were 40 % of the build.
///
/// Five embrasures over two ports still reads unmistakably as a two-tier
/// battery — the thing that carries that read is the *tier*, not the count —
/// and it costs about half the record.
const EMBRASURES: usize = 5;
/// Height of an embrasure's sole above the deck.
const SOLE_H: f32 = 0.45;
/// Parapet top.
const PARAPET_TOP: f32 = DECK_TOP + PARAPET_H;

/// Rear (gorge) parapet — a breast wall, lower than the seaward face so the
/// guns and the colours read over it from every landward angle.
const GORGE_H: f32 = 0.85;

/// Clear width of the gorge flight, and therefore of the opening left for it
/// in the breast wall above. One constant, so the stair and the gap it climbs
/// through cannot disagree.
const FLIGHT_W: f32 = 4.4;

/// Gorge steps: rise and going per tread. Both authored, and the count
/// derived from them, so the flight always lands exactly on the deck
/// (#972 lesson 16's shape — pin the relationship, not the number).
const RISER: f32 = 0.292;
const GOING: f32 = 0.30;

/// Bore of a gun barrel, as a fraction of its outer radius.
///
/// `TortureParams::hollow` carves the barrel along its axis, so the muzzle is
/// a ring with darkness inside it rather than a flat bronze disc. That black
/// circle is the entire read at any distance a gun is seen from here, and it
/// costs one field rather than one prim. A real piece runs a thin wall at the
/// muzzle and a much thicker one at the breech; one fraction cannot say both,
/// so it is sized for the end anybody sees.
const BORE_FRACTION: f32 = 0.42;

pub struct HarbourBattery;

impl CatalogueEntry for HarbourBattery {
    fn slug(&self) -> &'static str {
        "harbour_battery"
    }
    fn name(&self) -> &'static str {
        "Harbour Battery"
    }
    fn description(&self) -> &'static str {
        "A two-tier stone water-battery: four casemate ports under six embrasures, every gun run \
         out over a lantern-lit chamber, with the black colours over the terreplein."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Pirate]
    }
    fn prosperity_band(&self) -> ProsperityBand {
        PORT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 15.0,
            min_spawn_dist: 34.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Ashlar in the shared world course frame, on the face that will be looked
/// at (#972 lesson 2e/18).
///
/// The centre is taken as an argument and used for *both* the material's
/// frame and the caller's placement, because passing a different expression
/// to each is the one way to defeat the world-frame guard silently.
fn coursed(center: [f32; 3], face: FaceKey, seed: u32) -> SovereignMaterialSettings {
    bonded_siding(ashlar(STONE_LIME, seed), face, center)
}

/// Half the clear width of one wing's wall, and the x-centre of that wing.
///
/// Both wings are the mirror of each other about the guard house, so this is
/// stated once and mirrored rather than written out twice with a sign flipped
/// in nine places.
fn wing_span() -> (f32, f32) {
    let inner = GUARD_W * 0.5;
    let outer = WALL[0] * 0.5;
    (inner, outer)
}

/// The x-centre of one wing's casemate port, given the wing's sign.
///
/// One port per wing rather than two — see [`EMBRASURES`] for why the counts
/// came down. A single wide port centred in each wing also gives the lower
/// tier a stronger rhythm against the five embrasures above it than two
/// crowded ones did.
fn port_centres(side: f32) -> [f32; 1] {
    let (inner, outer) = wing_span();
    [side * (inner + (outer - inner) * 0.5)]
}

/// One gun on its truck carriage, pointing out along `-Z`.
///
/// `bore_y` is the axis height, `muzzle_z` where the muzzle ends up, and
/// `len` the barrel length — so the same gun serves the long pieces on the
/// terreplein and the shorter ones in the casemates.
///
/// # What makes it read as a gun rather than a rod on blocks
///
/// Three things, all learned from the first build looking wrong in-world.
///
/// 1. **The bore is real.** `hollow` carves the barrel out along its axis, so
///    the muzzle is a ring with darkness inside it. A solid drum pointed at
///    you is a post; the black circle is the entire read at any distance, and
///    it costs one field rather than one prim.
/// 2. **Trunnions carry it.** The barrel is held *up* by two stub axles that
///    pass into the carriage cheeks. Without them the cheeks stand beside the
///    barrel touching nothing, which is exactly why it looked like it was
///    hanging in the air — there was no member anywhere between the carriage
///    and the gun.
/// 3. **The cheeks are stepped.** A real carriage cheek falls away toward the
///    breech in steps so the gun can be elevated; a plain slab reads as
///    packaging. Two boxes per side instead of one.
///
/// The barrel is a **leaf** prim carrying its own quarter turn: a rotated
/// parent with offset children spins those offsets out of the geometry and
/// then hides the fault from every guard here, all of which walk translations
/// only (#972 lesson 22). So the carriage is not nested under the barrel —
/// both hang off the same flat sub-root.
fn gun(x: f32, bore_y: f32, muzzle_z: f32, len: f32, seed: u32) -> Generator {
    let r = len * 0.075;
    let breech_z = muzzle_z + len;
    let barrel_c = [x, bore_y, muzzle_z + len * 0.5];
    // Trunnions sit forward of the balance point, as they do on a real piece,
    // and the cheeks are built up to meet them.
    let trunnion_z = muzzle_z + len * 0.42;
    let cheek_x = r + 0.055;
    // The carriage's own bed, and the height its cheeks rise to.
    let bed_y = bore_y - r - 0.30;
    let bed_c = [x, bed_y, breech_z - len * 0.30];

    let mut carried = vec![
        // Barrel: tapered breech-to-muzzle, laid along Z, and BORED. `quat_x`
        // turns a Y-axis cylinder's +Y toward +Z, so the taper's narrow end
        // (the cylinder's top) points seaward — which is the muzzle.
        prim(
            solid(with_cut(
                cylinder_tapered(r, len, 12, 0.28, bronze(BRONZE_GUN, seed)),
                [0.0, 1.0],
                [0.0, 1.0],
                BORE_FRACTION,
            )),
            barrel_c,
            quat_x(-FRAC_PI_2),
        ),
        // Cascabel at the breech — and it caps the bore, so the hollow reads
        // as a muzzle opening rather than as a tube you can see through.
        prim(
            sphere(r * 0.66, 3, bronze(BRONZE_GUN, seed ^ 0x07)),
            [x, bore_y, breech_z + r * 0.3],
            id_quat(),
        ),
        // One reinforcing ring at the breech — what makes a tapered drum read
        // as an ordnance piece rather than as a pipe. The muzzle astragal the
        // first draft also carried is a 30 mm bead that no view of this
        // building resolves, and at seven guns it was seven prims of record.
        prim(
            torus(r * 0.16, r * 1.06, bronze(BRONZE_GUN, seed ^ 0x08)),
            [x, bore_y, breech_z - len * 0.16],
            quat_x(FRAC_PI_2),
        ),
    ];

    // Trunnions: the stub axles the whole gun hangs on. Laid along X, long
    // enough to reach from the barrel's flank into the outside of each cheek.
    for sx in [-1.0_f32, 1.0] {
        carried.push(prim(
            solid(cylinder_tapered(
                r * 0.34,
                (cheek_x + 0.05 - r * 0.6) * 2.0,
                8,
                0.0,
                bronze(BRONZE_GUN, seed ^ 0x0B),
            )),
            [
                x + sx * (r * 0.6 + (cheek_x + 0.05 - r * 0.6)),
                bore_y,
                trunnion_z,
            ],
            quat_z(FRAC_PI_2),
        ));
    }

    // Two stepped cheeks, and a truck under each. Painted the deep red a
    // period sea-service carriage actually wore — the one place in the kit
    // where `ENSIGN_RED` lands on something structural rather than on cloth,
    // and the thing that stops seven guns reading as seven brown sticks.
    let cheek_top = bore_y + r * 0.15;
    for sx in [-1.0_f32, 1.0] {
        // Forward step: rises to the trunnion, carrying the gun.
        let fwd_h = cheek_top - bed_y;
        carried.push(prim(
            solid(cuboid_tapered(
                [0.10, fwd_h, len * 0.30],
                0.0,
                strake(ENSIGN_RED),
            )),
            [
                x + sx * cheek_x,
                bed_y + fwd_h * 0.5,
                trunnion_z + len * 0.06,
            ],
            id_quat(),
        ));
        // Rear step: lower, running back under the breech.
        let aft_h = fwd_h * 0.58;
        carried.push(prim(
            solid(cuboid_tapered(
                [0.10, aft_h, len * 0.34],
                0.0,
                strake(ENSIGN_RED),
            )),
            [x + sx * cheek_x, bed_y + aft_h * 0.5, breech_z - len * 0.16],
            id_quat(),
        ));
        // One truck per cheek rather than two. The rear pair sits under the
        // carriage bed and behind the parapet, where nothing sees it.
        carried.push(prim(
            solid(cylinder_tapered(
                0.14,
                0.09,
                10,
                0.0,
                iron(IRON_BLACK, seed ^ 0x0A),
            )),
            [x + sx * cheek_x, bed_y - 0.14, trunnion_z + len * 0.08],
            quat_z(FRAC_PI_2),
        ));
    }

    // Breeching rope through the cascabel — the detail that says the gun is
    // rigged rather than parked. Worth its prim on the terreplein pieces,
    // which are seen whole; the casemate guns skip it, since a rope behind a
    // barrel in a 2.5 m chamber is invisible through the port.
    if len > 2.4 {
        carried.push(prim(
            cylinder_tapered(0.035, 1.5, 6, 0.0, hemp(ROPE_HEMP)),
            [x, bore_y - 0.05, breech_z + r * 0.5],
            quat_z(FRAC_PI_2),
        ));
    }

    nest(
        prim(
            solid(cuboid_tapered(
                [cheek_x * 2.0 + 0.2, 0.14, len * 0.7],
                0.0,
                strake(HULL_TAR),
            )),
            [x, bed_y - 0.07, bed_c[2]],
            id_quat(),
        ),
        carried,
    )
}

/// A garland of round shot, as **one** prim standing on `base`.
///
/// Three spheres in a pyramid is the obvious modelling and is what the first
/// draft had, at seven garlands and twenty-one prims. A pinched
/// superellipsoid gives the same silhouette — a low heap with a rounded
/// crown — for a third of the record, and at the distance a garland is ever
/// seen from, the individual balls were never resolvable anyway.
///
/// The exponents are chosen to read as *stacked spheres* rather than as a
/// cushion: `1.35` north-south pinches the crown, `0.9` east-west keeps the
/// plan nearly round.
fn shot_pile(base: [f32; 3], seed: u32) -> Generator {
    let r = 0.34;
    prim(
        solid(superellipsoid(
            [r, r * 0.62, r],
            1.35,
            0.9,
            iron(IRON_BLACK, seed),
        )),
        [base[0], base[1] + r * 0.62, base[2]],
        id_quat(),
    )
}

/// One casemate: its lit floor, warm rear lining, lantern and gun.
///
/// Everything is authored against [`CHAMBER_BACK`] and [`CASEMATE_SILL`], so
/// the chamber cannot drift out of the hole it is seen through.
fn casemate(x: f32, seed: u32) -> Generator {
    let floor_y = CASEMATE_SILL;
    let ceil_y = PORT_HEAD;
    // The floor is the chamber's sub-root: it is what everything inside
    // stands on, so nesting under it makes the fit-out one sub-assembly a
    // single gizmo drag moves, and gives the footprint guards something to
    // measure against (#972 lessons 3 and 19).
    // Sized to the CLEAR OPENING and bedded into the sill, not laid on it.
    //
    // Both faults were visible in-world as one grey rectangle fighting the
    // wall beside it. At `PORT_W + 0.9` the platform was 0.45 m wider than
    // its own hole on each side, so it ran into the piers; and its underside
    // sat exactly on the talus top, so those two faces were coplanar across
    // the whole footprint. Narrower than the opening and sunk below the sill
    // fixes both by construction — there is no shared plane left to fight
    // over, and nothing of it inside the masonry.
    let floor = prim(
        solid(cuboid_tapered(
            [
                PORT_W - CHAMBER_INSET * 2.0,
                0.09,
                CHAMBER_D - CHAMBER_INSET * 2.0,
            ],
            0.0,
            // Dim, because an interior has to read darker than the sunlit
            // masonry round its opening — a floor tuned to look good on its
            // own comes out brighter than the wall and flattens the very
            // depth the port exists to show.
            lit_interior([0.30, 0.27, 0.24], 0.16),
        )),
        [
            x,
            floor_y + 0.09 * 0.5 - FLOOR_SINK,
            FACE_Z + CHAMBER_D * 0.5,
        ],
        id_quat(),
    );
    let carried = vec![
        // Rear lining, held the full chamber depth back and given a WARMER
        // tone than the floor: three interior surfaces at one tone make a
        // flat grey box however well they are lit.
        prim(
            solid(cuboid_tapered(
                [PORT_W - CHAMBER_INSET * 2.0, PORT_H, 0.1],
                0.0,
                lit_interior([0.42, 0.31, 0.20], 0.3),
            )),
            // Held clear of the rear mass's own front face. Flush against it
            // the two are coplanar over the whole lining, which is the same
            // fault as the floor's arriving from the other axis.
            [
                x,
                (floor_y + ceil_y) * 0.5,
                CHAMBER_BACK - 0.05 - CHAMBER_INSET,
            ],
            id_quat(),
        ),
        // The lantern hangs inside the cone the port admits — see
        // `every_chamber_is_lit_within_the_cone_its_own_opening_admits` for
        // what that means and why "hang it low" is the wrong reading of #972
        // lesson 10 for an opening whose sill is near eye height.
        lantern(
            [x + PORT_W * 0.42, floor_y + 1.05, CHAMBER_BACK - 0.45],
            0.5,
            seed,
        ),
        // Shot garland against the lining.
        shot_pile(
            [x - PORT_W * 0.35, floor_y, CHAMBER_BACK - 0.35],
            seed ^ 0x31,
        ),
        // The gun, run out through the port.
        gun(x, floor_y + 0.62, FACE_Z - 0.45, 2.1, seed ^ 0x40),
    ];
    nest(floor, carried)
}

/// The guard house recess: a lit passage under an arched head, with an iron
/// gate standing open on it.
fn guard_house() -> Generator {
    let floor_y = QUAY;
    let mid_y = (floor_y + GUARD_HEAD) * 0.5;
    // Same sub-root rule as the casemates: the passage floor carries the
    // passage.
    let floor = prim(
        solid(cuboid_tapered(
            [
                GUARD_W - CHAMBER_INSET * 2.0,
                0.09,
                CHAMBER_D - CHAMBER_INSET * 2.0,
            ],
            0.0,
            lit_interior([0.32, 0.29, 0.25], 0.18),
        )),
        [
            0.0,
            floor_y + 0.09 * 0.5 - FLOOR_SINK,
            FACE_Z + CHAMBER_D * 0.5,
        ],
        id_quat(),
    );
    let mut out = vec![
        prim(
            solid(cuboid_tapered(
                [GUARD_W - CHAMBER_INSET * 2.0, GUARD_HEAD - floor_y, 0.1],
                0.0,
                lit_interior([0.44, 0.33, 0.21], 0.32),
            )),
            [0.0, mid_y, CHAMBER_BACK - 0.05 - CHAMBER_INSET],
            id_quat(),
        ),
        // Lantern under the head, on the same reasoning as the casemates'.
        lantern([0.62, floor_y + 1.55, CHAMBER_BACK - 0.5], 0.6, 0x61),
        // Powder budge-barrel and a table — a bay with nothing in it is a
        // black rectangle however well the shell is built (#972 lesson 9).
        prim(
            solid(cylinder_tapered(0.28, 0.62, 12, -0.12, strake(HULL_OAK))),
            [-0.78, floor_y + 0.31, CHAMBER_BACK - 0.55],
            id_quat(),
        ),
        prim(
            torus(0.035, 0.29, iron(IRON_BLACK, 0x62)),
            [-0.78, floor_y + 0.5, CHAMBER_BACK - 0.55],
            id_quat(),
        ),
        // Table, its top scrubbed pale the way a deck is — the tone that
        // separates worked timber from tarred timber at a glance.
        prim(
            solid(cuboid_tapered([0.9, 0.07, 0.5], 0.0, board(DECK_HOLY))),
            [0.55, floor_y + 0.74, CHAMBER_BACK - 0.7],
            id_quat(),
        ),
    ];
    // Segmental arch over the opening, in voussoirs. Each voussoir is a leaf
    // prim carrying its own rotation, so nothing offset rides a turn.
    let arch_r = GUARD_W * 0.5 + 0.18;
    const VOUSSOIRS: usize = 5;
    for i in 0..VOUSSOIRS {
        let t = (i as f32 + 0.5) / VOUSSOIRS as f32;
        let a = std::f32::consts::PI * (0.12 + 0.76 * t);
        out.push(prim(
            solid(cuboid_tapered(
                [0.3, 0.46, 0.5],
                0.0,
                ashlar(STONE_LIME, 0x63 + i as u32),
            )),
            [
                -arch_r * a.cos(),
                GUARD_HEAD - 0.1 + arch_r * a.sin() * 0.34,
                FACE_Z + 0.24,
            ],
            quat_z(a - FRAC_PI_2),
        ));
    }
    // Iron gate, hung open against the left jamb. A closed gate makes the
    // whole recess a darker rectangle on the wall and throws away the one
    // lit interior at eye level.
    out.push(prim(
        solid(cuboid_tapered(
            [0.08, GUARD_HEAD - floor_y - 0.2, 1.15],
            0.0,
            iron(IRON_BLACK, 0x64),
        )),
        [-GUARD_W * 0.5 + 0.06, mid_y, FACE_Z + 0.62],
        id_quat(),
    ));
    for i in 0..3 {
        let z = FACE_Z + 0.16 + i as f32 * 0.4;
        out.push(prim(
            cuboid_tapered(
                [0.05, GUARD_HEAD - floor_y - 0.3, 0.05],
                0.0,
                iron(IRON_BLACK, 0x65),
            ),
            [-GUARD_W * 0.5 + 0.06, mid_y, z],
            id_quat(),
        ));
    }
    nest(floor, out)
}

/// The seaward parapet: merlons, their copings, the embrasure soles and the
/// six guns that fire over them.
fn parapet() -> Vec<Generator> {
    let z = -DECK[2] * 0.5 + PARAPET_D * 0.5;
    // Merlon pitch derived from the run, so the embrasures divide the deck
    // evenly however the deck is re-proportioned.
    let pitch = (DECK[0] - MERLON_W) / EMBRASURES as f32;
    let clear = pitch - MERLON_W;
    let mut out = Vec::new();

    for i in 0..=EMBRASURES {
        let x = -DECK[0] * 0.5 + MERLON_W * 0.5 + i as f32 * pitch;
        let c = [x, DECK_TOP + PARAPET_H * 0.5, z];
        out.push(prim(
            solid(cuboid_tapered_xz(
                [MERLON_W, PARAPET_H, PARAPET_D],
                [0.0, 0.05],
                coursed(c, FaceKey::SideNz, 0x70 + i as u32),
            )),
            c,
            id_quat(),
        ));
        // Coping, standing proud of the merlon on every side. Flush is a
        // coplanar seam running the whole head of the most looked-at part of
        // the building, and it is invisible in a still.
        let cop_c = [x, PARAPET_TOP + 0.09, z];
        out.push(prim(
            solid(with_face(
                cuboid_tapered(
                    [MERLON_W + 0.14, 0.18, PARAPET_D + 0.14],
                    0.0,
                    ashlar(STONE_LIME, 0x80 + i as u32),
                ),
                // A Top face reads depth where its neighbours read height, so
                // it never follows from the base offset and always needs its
                // own wrap (#972 lesson 2f).
                FaceKey::Top,
                bonded_siding(ashlar(STONE_LIME, 0x80 + i as u32), FaceKey::Top, cop_c),
            )),
            cop_c,
            id_quat(),
        ));
    }

    for i in 0..EMBRASURES {
        let x = -DECK[0] * 0.5 + MERLON_W + clear * 0.5 + i as f32 * pitch;
        // Sole under the embrasure — the sill a gun fires over.
        let sole_c = [x, DECK_TOP + SOLE_H * 0.5, z];
        out.push(prim(
            solid(cuboid_tapered(
                [clear, SOLE_H, PARAPET_D],
                0.0,
                coursed(sole_c, FaceKey::SideNz, 0x90 + i as u32),
            )),
            sole_c,
            id_quat(),
        ));
        out.push(gun(
            x,
            DECK_TOP + SOLE_H + 0.5,
            -DECK[2] * 0.5 - 0.55,
            2.7,
            0xA0 + i as u32,
        ));
        // Shot garland behind each gun — every bay gets its own thing to look
        // at from the deck as well as from the sea.
        out.push(shot_pile([x, DECK_TOP, z + 1.5], 0xB0 + i as u32));
    }
    out
}

/// Ready-use powder locker: a boarded chest with a pitched shingle lid.
///
/// Replaces a sentry box, which was the first thing tried here and did not
/// work. A sentry box is a hut, and a hut at this scale on a stone platform
/// reads as an outhouse — the user's word for it in-world was "hut(?)", the
/// question mark being the whole problem. A **chest** is furniture-scale and
/// has no other reading: a low boarded body under a pitched lid with iron
/// straps and a hasp is a chest from any angle and at any distance.
///
/// It also earns its place functionally. A gun deck needs powder to hand and
/// cannot keep it in the magazine two storeys down, so ready-use lockers are
/// what actually stood between the pieces.
fn powder_locker(x: f32, z: f32) -> Generator {
    let body = [1.55, 0.72, 0.82];
    let body_c = [x, DECK_TOP + body[1] * 0.5, z];
    nest(
        prim(
            solid(cuboid_tapered(
                body,
                0.0,
                // Boarded UP, the way a chest's carcase is built — the quarter
                // turn is free on unstaggered plank (#972 lesson 15).
                crate::catalogue::items::util::bonded_boards(
                    board(WHARF_GREY),
                    FaceKey::SideNz,
                    body_c,
                ),
            )),
            body_c,
            id_quat(),
        ),
        vec![
            // Pitched lid, pinched on ONE axis: `cuboid_tapered` pinches both
            // and would round the chest's whole profile away (the barn's
            // shipped fault).
            prim(
                solid(cuboid_tapered_xz(
                    [body[0] + 0.12, 0.26, body[2] + 0.12],
                    [0.0, 0.75],
                    shingle(SHINGLE_GREY),
                )),
                [x, DECK_TOP + body[1] + 0.13, z],
                id_quat(),
            ),
            // Two iron straps over the carcase, and a hasp on the front.
            prim(
                solid(cuboid_tapered(
                    [0.07, body[1] + 0.02, body[2] + 0.03],
                    0.0,
                    iron(IRON_BLACK, 0xF1),
                )),
                [x - body[0] * 0.28, body_c[1], z],
                id_quat(),
            ),
            prim(
                solid(cuboid_tapered(
                    [0.07, body[1] + 0.02, body[2] + 0.03],
                    0.0,
                    iron(IRON_BLACK, 0xF2),
                )),
                [x + body[0] * 0.28, body_c[1], z],
                id_quat(),
            ),
            prim(
                solid(cuboid_tapered(
                    [0.18, 0.22, 0.06],
                    0.0,
                    iron(IRON_BLACK, 0xF3),
                )),
                [x, DECK_TOP + body[1] * 0.72, z - body[2] * 0.5 - 0.02],
                id_quat(),
            ),
        ],
    )
}

/// A rack of rammers and sponges standing by the guns.
///
/// Replaces a furled tarpaulin, which was the other prop that did not read —
/// "roll of cloth(?)" — and which also intersected the hut beside it. A rolled
/// anything is an ambiguous cylinder; a rack of long poles with pale heads on
/// them is a rack of tools, and on a gun deck it is the one piece of kit that
/// says what the platform is *for* without needing a caption.
///
/// This is also where the kit's [`sailcloth`] lives on this entry: a sponge
/// head is canvas wound on a stave, which is a use the material is actually
/// for, rather than the unreadable "gun apron" it was carrying before.
fn rammer_rack(x: f32, z: f32) -> Generator {
    let post_h = 1.15;
    let span = 1.3;
    let base_c = [x, DECK_TOP + 0.06, z];
    let mut carried = Vec::new();
    for sx in [-1.0_f32, 1.0] {
        carried.push(prim(
            solid(cuboid_tapered([0.1, post_h, 0.1], 0.0, board(WHARF_GREY))),
            [x + sx * span * 0.5, DECK_TOP + post_h * 0.5, z],
            id_quat(),
        ));
    }
    // Cross rail the staves rest in.
    carried.push(prim(
        solid(cuboid_tapered(
            [span + 0.1, 0.09, 0.12],
            0.0,
            board(WHARF_GREY),
        )),
        [x, DECK_TOP + post_h - 0.12, z],
        id_quat(),
    ));
    // Three staves, each with a canvas-wound head at the top.
    for (i, dx) in [-0.42_f32, 0.0, 0.42].into_iter().enumerate() {
        let stave_h = 2.0;
        carried.push(prim(
            solid(cylinder_tapered(0.035, stave_h, 8, 0.0, board(HULL_OAK))),
            [x + dx, DECK_TOP + stave_h * 0.5, z],
            id_quat(),
        ));
        carried.push(prim(
            solid(cylinder_tapered(
                0.085,
                0.3,
                10,
                0.06,
                sailcloth(CANVAS_BONE, CANVAS_SHADE),
            )),
            [x + dx, DECK_TOP + stave_h - 0.1, z],
            id_quat(),
        ));
        // A worm on the middle stave, so the three read as different tools.
        if i == 1 {
            carried.push(prim(
                torus(0.02, 0.07, iron(IRON_BLACK, 0xF4)),
                [x + dx, DECK_TOP + stave_h + 0.14, z],
                quat_x(FRAC_PI_2),
            ));
        }
    }
    nest(
        prim(
            solid(cuboid_tapered(
                [span + 0.3, 0.12, 0.42],
                0.0,
                board(WHARF_GREY),
            )),
            base_c,
            id_quat(),
        ),
        carried,
    )
}

/// The colours on their staff, at the gorge end of the deck./// The colours on their staff, at the gorge end of the deck.
///
/// The flag itself is the kit's shared [`jolly_roger`] — two BlobGroups, a
/// rippled cloth and a bone relief seated in its face — so the skull cannot
/// poke through the back of the flag the way a sphere laid on a slab did.
fn colours() -> Generator {
    let staff_h = 5.6;
    let cz = DECK[2] * 0.5 - 1.5;
    let base = prim(
        solid(cylinder_tapered(
            0.34,
            0.4,
            12,
            0.2,
            ashlar(STONE_LIME, 0xC0),
        )),
        [0.0, DECK_TOP + 0.2, cz],
        id_quat(),
    );
    let staff_top = DECK_TOP + 0.4 + staff_h;
    let mut staff = nest(
        base,
        vec![
            prim(
                solid(cylinder_tapered(0.09, staff_h, 10, 0.35, strake(HULL_OAK))),
                [0.0, DECK_TOP + 0.4 + staff_h * 0.5, cz],
                id_quat(),
            ),
            prim(
                sphere(0.13, 3, glow(GOLD_LEAF, 0.4)),
                [0.0, staff_top + 0.1, cz],
                id_quat(),
            ),
            // Bent to the staff itself: `jolly_roger` takes the attachment
            // point and laps the luff onto the timber, so the colours cannot
            // end up hanging beside their own staff.
            jolly_roger([0.0, staff_top - 0.32, cz], 1.25),
        ],
    );
    // The halyard slatting against the staff. Its own spatial patch rather
    // than a second voice on the root: the swell is heard from the water, the
    // rigging only when you are up on the deck beside it, and that difference
    // is the whole reason node audio is positional.
    staff.audio = fx::rigging_creak();
    staff
}

/// The gorge steps, and the cheek walls either side of them.
///
/// The **rise** is authored and the tread count derived from it, so the
/// flight always lands exactly on the deck. Picking the count instead leaves
/// the riser to fall out at whatever the numbers give, which is how a flight
/// ends up with a half-height step at the top that nothing in a render shows.
fn gorge_steps() -> Generator {
    let rise = DECK_TOP - QUAY;
    let treads = (rise / RISER).round().max(2.0) as usize;
    let riser = rise / treads as f32;
    // The flight starts beyond the CORDON's projection, not at the wall face.
    // Started at the wall it ran its top treads straight through the cordon's
    // 0.4 m oversail and stopped 0.3 m short of the deck's own edge, which is
    // why it read as a stair leaning on the fort rather than joining it.
    let start_z = WALL[2] * 0.5 + CORDON_PROJECT;
    let width = FLIGHT_W;
    let mut out = Vec::new();
    for i in 1..treads {
        let h = riser * (i + 1) as f32;
        let z = start_z + (treads - 1 - i) as f32 * GOING;
        out.push(prim(
            solid(cuboid_tapered(
                [width, h, GOING],
                0.0,
                cobbles(STONE_QUAY, 0xD0 + i as u32),
            )),
            [0.0, QUAY + h * 0.5, z + GOING * 0.5],
            id_quat(),
        ));
    }
    // Landing: the piece that actually makes the flight meet the building.
    // It bears on the cordon and reaches in to the deck's own edge, so the
    // top tread arrives at a floor rather than at a 0.7 m gap over a
    // projecting moulding. Sized from both — the cordon's outer face and the
    // deck's edge — so it cannot come apart if either is re-proportioned.
    let landing_back = DECK[2] * 0.5;
    let landing_d = start_z - landing_back;
    out.push(prim(
        solid(cuboid_tapered(
            [width, DECK[1], landing_d],
            0.0,
            cobbles(STONE_QUAY, 0xDF),
        )),
        [
            0.0,
            DECK_TOP - DECK[1] * 0.5,
            landing_back + landing_d * 0.5,
        ],
        id_quat(),
    ));
    // Cheek walls, derived from the flight's own run so they cannot be left
    // behind when the rise changes.
    let run = treads as f32 * GOING;
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(wedge([0.3, rise, run], ashlar(STONE_LIME, 0xE0))),
            [
                sx * (width * 0.5 + 0.15),
                QUAY + rise * 0.5,
                start_z + run * 0.5,
            ],
            id_quat(),
        ));
    }
    // The bottom tread is the flight's sub-root: it is the thing the rest of
    // the flight stands beside, and nesting the treads under *each other*
    // instead would build a thirteen-deep chain against a MAX_GENERATOR_DEPTH
    // of 16 for no editing benefit — a flight is moved as one object or not
    // at all.
    nest(
        prim(
            solid(cuboid_tapered(
                [width, riser, GOING],
                0.0,
                cobbles(STONE_QUAY, 0xD0),
            )),
            [
                0.0,
                QUAY + riser * 0.5,
                start_z + (treads - 1) as f32 * GOING + GOING * 0.5,
            ],
            id_quat(),
        ),
        out,
    )
}

fn build_tree() -> Generator {
    let apron_c = [0.0, QUAY * 0.5, 0.0];
    let mut paving = cobbles(STONE_QUAY, 0x11);
    paving.uv_offset = face_uv_offset(FaceKey::Top, apron_c);

    let mut carried = vec![
        footing(TALUS[0], TALUS[2], [0.0, 0.0], 15.0),
        // A band of storm-beach shingle bedded into the seaward edge of the
        // apron — the tide line the work stands at. Held inside the apron's
        // own extent rather than measured off the wall (#972 lesson 8), and
        // deliberately the cold grey counterpart to the resort kit's golden
        // sand: the two maritime themes separate on their shore before they
        // separate on their buildings.
        prim(
            solid(cuboid_tapered(
                [APRON[0] - 1.2, 0.1, 3.0],
                0.0,
                strand(STRAND_SHINGLE),
            )),
            [0.0, QUAY - 0.02, -APRON[2] * 0.5 + 1.6],
            id_quat(),
        ),
    ];

    // --- Each wing: a talus carrying its piers and its two casemates -------
    //
    // The talus is the wing's sub-root because it is what the wing stands on
    // (#972 lesson 3: a tree that stands the way the prop does). It also
    // keeps the root's direct children down to the tiers themselves — the
    // first draft hung all seventy-nine pieces flat off the apron, which the
    // subtree guard caught and which is the flat-list smell the whole lesson
    // exists to name.
    let (inner, outer) = wing_span();
    for side in [-1.0_f32, 1.0] {
        let w = outer + TALUS[0] * 0.5 - WALL[0] * 0.5 - inner;
        let cx = side * (inner + w * 0.5);
        let c = [cx, QUAY + TALUS[1] * 0.5, 0.0];

        let ports = port_centres(side);
        let mut on_talus = Vec::new();

        // Piers: the spans between the wing's edges that are not ports.
        let mut edges = vec![side * inner, side * outer];
        for p in ports {
            edges.push(p - side * PORT_W * 0.5);
            edges.push(p + side * PORT_W * 0.5);
        }
        edges.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        for pair in edges.chunks(2) {
            let (a, b) = (pair[0], pair[1]);
            let pw = (b - a).abs();
            if pw < 0.05 {
                continue;
            }
            let pcx = (a + b) * 0.5;
            if ports.iter().any(|p| (p - pcx).abs() < 0.05) {
                continue;
            }
            // Only as deep as the chamber it frames. Full-depth piers ran
            // the whole 10.8 m of the wall, so they occupied the same volume
            // as the rear mass behind them AND presented a face at the same
            // x = ±9.9 over eight metres of overlap — two coplanar walls
            // fighting for depth down the middle of the base, which is what
            // showed in-world. Abutting the rear mass instead of passing
            // through it removes the shared plane rather than nudging it.
            let pc = [pcx, CASEMATE_SILL + PORT_H * 0.5, FACE_Z + CHAMBER_D * 0.5];
            on_talus.push(prim(
                solid(cuboid_tapered(
                    [pw, PORT_H, CHAMBER_D],
                    0.0,
                    coursed(pc, FaceKey::SideNz, 0x13),
                )),
                pc,
                id_quat(),
            ));
        }

        for (i, x) in ports.into_iter().enumerate() {
            on_talus.push(casemate(
                x,
                0x50 + i as u32 + if side < 0.0 { 0 } else { 8 },
            ));
        }

        carried.push(nest(
            prim(
                solid(cuboid_tapered_xz(
                    [w, TALUS[1], TALUS[2]],
                    TALUS_BATTER,
                    coursed(c, FaceKey::SideNz, 0x12),
                )),
                c,
                id_quat(),
            ),
            on_talus,
        ));
    }

    // --- Head band: one prim across the whole face ------------------------
    let head_c = [0.0, PORT_HEAD + HEAD_H * 0.5, 0.0];
    carried.push(prim(
        solid(cuboid_tapered(
            [WALL[0], HEAD_H, WALL[2]],
            0.0,
            coursed(head_c, FaceKey::SideNz, 0x14),
        )),
        head_c,
        id_quat(),
    ));

    // --- Rear mass behind every chamber -----------------------------------
    let rear_d = WALL[2] * 0.5 - CHAMBER_BACK;
    let rear_c = [0.0, (QUAY + PORT_HEAD) * 0.5, CHAMBER_BACK + rear_d * 0.5];
    carried.push(prim(
        solid(cuboid_tapered(
            [WALL[0], PORT_HEAD - QUAY, rear_d],
            0.0,
            coursed(rear_c, FaceKey::SidePz, 0x15),
        )),
        rear_c,
        id_quat(),
    ));

    // The guard house stands on the apron rather than on a talus, so it hangs
    // off the root as its own subtree.
    carried.push(guard_house());

    // --- Cordon: a RING, centred on the building --------------------------
    let cordon_c = [0.0, WALL_TOP + CORDON_H * 0.5, 0.0];
    carried.push(prim(
        solid(with_face(
            cuboid_tapered(
                [
                    WALL[0] + CORDON_PROJECT * 2.0,
                    CORDON_H,
                    WALL[2] + CORDON_PROJECT * 2.0,
                ],
                0.0,
                coursed(cordon_c, FaceKey::SideNz, 0x16),
            ),
            FaceKey::Top,
            coursed(cordon_c, FaceKey::Top, 0x16),
        )),
        cordon_c,
        id_quat(),
    ));

    // --- The deck and everything it carries -------------------------------
    let deck_c = [0.0, CORDON_TOP + DECK[1] * 0.5, 0.0];
    let mut flags = cobbles(STONE_QUAY, 0x17);
    flags.uv_offset = face_uv_offset(FaceKey::Top, deck_c);
    let mut on_deck = parapet();
    on_deck.push(colours());
    // Two props with unmistakable silhouettes, placed well apart. The pair
    // they replace — a sentry box and a furled tarpaulin — were unreadable
    // individually AND intersecting each other, which is the combination that
    // makes a deck look like a bin rather than a working battery.
    on_deck.push(powder_locker(
        DECK[0] * 0.5 - 2.8,
        DECK[2] * 0.5 - PARAPET_D - 0.9,
    ));
    on_deck.push(rammer_rack(
        -(DECK[0] * 0.5 - 2.8),
        DECK[2] * 0.5 - PARAPET_D - 0.9,
    ));
    // Gorge breast wall and the two flank parapets, each a ring segment
    // rather than a face detail, so each takes the deck's own extent.
    //
    // The breast wall is TWO runs with the flight's opening between them. One
    // continuous wall walled the stair off from the deck it climbs to, which
    // is a flight to nowhere — obvious the moment somebody walks up it, and
    // invisible in every elevation.
    let gorge_z = DECK[2] * 0.5 - PARAPET_D * 0.5;
    let gap_half = FLIGHT_W * 0.5 + 0.12;
    for sx in [-1.0_f32, 1.0] {
        let run = DECK[0] * 0.5 - gap_half;
        let c = [
            sx * (gap_half + run * 0.5),
            DECK_TOP + GORGE_H * 0.5,
            gorge_z,
        ];
        on_deck.push(prim(
            solid(cuboid_tapered(
                [run, GORGE_H, PARAPET_D],
                0.0,
                coursed(c, FaceKey::SidePz, 0x18),
            )),
            c,
            id_quat(),
        ));
        // A stub pier at each side of the opening, standing proud of the runs,
        // so the gap reads as a doorway somebody built rather than as a
        // missing length of wall.
        on_deck.push(prim(
            solid(cuboid_tapered(
                [0.34, GORGE_H + 0.26, PARAPET_D + 0.16],
                0.0,
                ashlar(STONE_LIME, 0x1A),
            )),
            [sx * gap_half, DECK_TOP + (GORGE_H + 0.26) * 0.5, gorge_z],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        let c = [
            sx * (DECK[0] * 0.5 - PARAPET_D * 0.5),
            DECK_TOP + PARAPET_H * 0.5,
            0.0,
        ];
        on_deck.push(prim(
            solid(cuboid_tapered(
                [PARAPET_D, PARAPET_H, DECK[2] - PARAPET_D * 2.0],
                0.0,
                coursed(
                    c,
                    if sx < 0.0 {
                        FaceKey::SideNx
                    } else {
                        FaceKey::SidePx
                    },
                    0x19,
                ),
            )),
            c,
            id_quat(),
        ));
    }
    // A tarred timber platform strip under the gun trucks, so the deck reads
    // as a working battery rather than as a paved roof.
    on_deck.push(prim(
        solid(cuboid_tapered(
            [DECK[0] - 1.0, 0.06, 2.6],
            0.0,
            strake(HULL_TAR),
        )),
        [0.0, DECK_TOP + 0.03, -DECK[2] * 0.5 + 2.4],
        id_quat(),
    ));

    carried.push(nest(
        prim(solid(cuboid_tapered(DECK, 0.0, flags)), deck_c, id_quat()),
        on_deck,
    ));

    carried.push(gorge_steps());

    let mut root = nest(
        prim(
            solid(cuboid_tapered(APRON, 0.0, paving)),
            apron_c,
            id_quat(),
        ),
        carried,
    );

    // Spent powder drifting out of the middle embrasures and away seaward.
    // Attached rather than pushed: a child added to a finished root is read
    // in the root's local frame and never rebased, so ground-frame constants
    // land one root-height out (#1010).
    let pitch = (DECK[0] - MERLON_W) / EMBRASURES as f32;
    for i in [2_usize, 4] {
        let x = -DECK[0] * 0.5 + MERLON_W + (pitch - MERLON_W) * 0.5 + i as f32 * pitch;
        attach(
            &mut root,
            fx::powder_smoke(
                [x, DECK_TOP + SOLE_H + 0.55, -DECK[2] * 0.5 - 1.0],
                0x5A17_u64.wrapping_add(i as u64),
            ),
        );
    }
    root.audio = fx::harbour_swell();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::measure;
    use crate::catalogue::items::util::{
        assert_cards_do_not_overlap, assert_no_glazing_on_solids, assert_no_tilted_parents,
        assert_sanitize_stable, has_emissive,
    };
    use crate::pds::GeneratorKind as K;
    use crate::pds::PrimCommon;

    fn built() -> Generator {
        HarbourBattery.build("")
    }

    /// The entry stays inside the record-size band the rest of the catalogue's
    /// landmarks occupy.
    ///
    /// A seeded room's whole budget is spent on a handful of entries, and this
    /// is the biggest single one in it, so "is it too heavy" is a question
    /// somebody has to be able to answer without measuring by hand. The first
    /// build was 270 nodes and 138 KB — two and a half times the next heaviest
    /// landmark in the catalogue, and on its own enough to put a seeded room
    /// over the soft budget. Everything that came out came out for that
    /// reason: two casemate ports, one embrasure, a muzzle astragal, two
    /// trucks a gun, the casemate breeching ropes, twenty-one shot spheres
    /// (now seven pinched superellipsoids), and every nested weathering block
    /// on a metal fitting.
    ///
    /// The ceiling is a little above the current figure so the entry can be
    /// detailed further, and well under the doubling that would make it an
    /// outlier again. It was raised once, from 85 to 95 KB, when the guns
    /// gained their bores, trunnions and stepped cheeks (#1025) — detail the
    /// user asked for after seeing them in-world, on the prop that is the
    /// whole point of the building.
    ///
    /// There is no room for a second such raise. Since #1027 the record
    /// budget is measured against the largest record a publish actually
    /// writes, and across all twenty-four themes that record is **this one**,
    /// at 91 % of the 100 KiB soft budget. The next entry to grow past the
    /// battery inherits the constraint; the battery itself has spent it.
    #[test]
    fn the_entry_stays_within_the_landmark_record_band() {
        fn count(g: &Generator) -> usize {
            1 + g.children.iter().map(count).sum::<usize>()
        }
        let g = built();
        let bytes = serde_json::to_vec(&g)
            .expect("a built entry serialises")
            .len();
        let nodes = count(&g);
        assert!(
            bytes < 95_000,
            "the battery serialises to {bytes} B over {nodes} nodes; the next \
             heaviest landmark in the catalogue is ~55 KB, and this entry IS \
             the largest published record in any seeded room — it sits at 91 % \
             of the 100 KiB soft budget, so there is no slack above it"
        );
        assert!(
            nodes < 215,
            "{nodes} nodes — a gun is ~10 prims and there are seven of them, \
             so the guns are the first place to look when this trips"
        );
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&built(), "harbour_battery");
    }

    #[test]
    fn no_rotated_node_carries_an_offset_child() {
        assert_no_tilted_parents(&built(), "harbour_battery");
    }

    /// The building has no glazing at all, by design, and this states it from
    /// both sides: nothing solid wears a card (#972 lesson 20), and there are
    /// no cards to overlap either (#972 lesson 17).
    ///
    /// Worth asserting rather than assuming. The pressure to add a window to
    /// a big stone building is constant, and the moment one arrives it should
    /// arrive as a card on a plane over a real opening — which is a decision
    /// somebody should have to make deliberately rather than by reflex.
    #[test]
    fn the_battery_carries_no_glazing() {
        let g = built();
        assert_no_glazing_on_solids(&g, "harbour_battery");
        assert_cards_do_not_overlap(&g, "harbour_battery");
        assert!(
            crate::catalogue::items::util::window_cards(&g).is_empty(),
            "the battery has grown a window; its openings are embrasures and \
             ports, which are real holes with guns in them"
        );
    }

    /// Ten openings, ten guns. The count is the point: an elevation with six
    /// embrasures and four ports and three guns is a building whose fit-out
    /// was authored for "the face" rather than bay by bay (#972 lesson 9).
    #[test]
    fn every_opening_has_a_gun_in_it() {
        let g = built();
        // A gun barrel is the only tapered cylinder in bronze on this build,
        // and it is selected by *that* rather than by size — a selector on an
        // incidental property is as much a source of false results as the
        // assertion is (#972 lesson 24).
        fn count_barrels(g: &Generator) -> usize {
            let own = match &g.kind {
                K::Cylinder {
                    common:
                        PrimCommon {
                            material, torture, ..
                        },
                    ..
                } => {
                    let bronze_ish = material.base_color.0 == BRONZE_GUN;
                    let tapered = torture.taper.0[0] > 0.2;
                    (bronze_ish && tapered) as usize
                }
                _ => 0,
            };
            own + g.children.iter().map(count_barrels).sum::<usize>()
        }
        // Derived from the counts themselves, not restated: the first version
        // hardcoded four casemates and went stale the moment the ports came
        // down to one per wing, which is a guard agreeing with a number rather
        // than with the building.
        let casemates = port_centres(-1.0).len() * 2;
        assert_eq!(
            count_barrels(&g),
            EMBRASURES + casemates,
            "one gun per embrasure and per casemate port"
        );
    }

    /// Every chamber is lit, and every lantern sits inside the cone its own
    /// opening admits to somebody standing on the quay.
    ///
    /// # Why this is not "hang the light low"
    ///
    /// #972 lesson 10 was learned on a garage whose rolled-up door drum
    /// physically crossed the sightline, and the fix there was a second lamp
    /// *below* the thing that spanned the head. Generalised to "hang it low"
    /// it is wrong here, and the first draft of this guard duly failed the
    /// guard house's lantern for hanging at 1.87 m — a lantern that is in
    /// fact perfectly visible, because the guard house sill is at the quay
    /// and a standing viewer looks straight into it.
    ///
    /// The real invariant is the **cone**. An opening admits a wedge bounded
    /// by two rays from the viewer's eye — one grazing the head, one grazing
    /// the sill — and what matters is whether the lit thing falls inside that
    /// wedge at its own depth. For an opening *above* eye level the binding
    /// constraint is the sill, not the head: looking up, you see the ceiling
    /// and lose the floor. For one at eye level neither binds. Stating it as
    /// a height instead of as a cone gets the casemates and the guard house
    /// backwards from each other, which is exactly what happened.
    #[test]
    fn every_chamber_is_lit_within_the_cone_its_own_opening_admits() {
        /// Eye height of somebody standing on the quay.
        const EYE: f32 = 1.7;
        /// How far off the face they are standing. Close enough that the
        /// cone is genuinely narrow — the further back you go, the more
        /// forgiving this becomes, so the near view is the one to check.
        const VIEW_DIST: f32 = 6.0;
        let g = built();
        assert!(has_emissive(&g), "the battery lost its lit chambers");
        fn flames(g: &Generator, at: [f32; 3], out: &mut Vec<[f32; 3]>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let K::Sphere {
                common: PrimCommon { material, .. },
                ..
            } = &g.kind
                && material.emission_strength.0 > 2.0
            {
                out.push(here);
            }
            for c in &g.children {
                flames(c, here, out);
            }
        }
        let mut found = Vec::new();
        flames(&g, [0.0; 3], &mut found);
        // Every casemate, plus the guard house.
        let want = port_centres(-1.0).len() * 2 + 1;
        let inside: Vec<_> = found
            .iter()
            .filter(|p| p[2] > FACE_Z && p[2] < CHAMBER_BACK)
            .collect();
        assert_eq!(
            inside.len(),
            want,
            "expected a lantern in each casemate and in the guard house, found \
             {} inside the chambers out of {} flames",
            inside.len(),
            found.len()
        );
        for p in inside {
            let is_guard_house = p[0].abs() < GUARD_W;
            let head = if is_guard_house {
                GUARD_HEAD
            } else {
                PORT_HEAD
            };
            let sill = if is_guard_house { QUAY } else { CASEMATE_SILL };
            // How far past the face this flame sits, and hence how much the
            // cone has opened by the time it reaches it.
            let depth = p[2] - FACE_Z;
            let spread = (VIEW_DIST + depth) / VIEW_DIST;
            let top = EYE + (head - EYE) * spread;
            let bottom = EYE + (sill - EYE) * spread;
            assert!(
                p[1] < top && p[1] > bottom,
                "a lantern at {p:?} falls outside the {bottom}..{top} band its \
                 own opening admits to an eye at {EYE} m standing {VIEW_DIST} m \
                 off the face — it is behind the sill or behind the head, and \
                 either way the chamber reads unlit from the quay"
            );
        }
    }

    /// No two solids of the base occupy the same space (#1025).
    ///
    /// The in-world fault this replaces: the wall piers ran the full 10.8 m
    /// depth of the wall while the rear mass filled everything behind the
    /// chambers, so eight metres of the two interpenetrated AND presented
    /// faces on the same x = ±9.9 plane — a coplanar seam fighting for depth
    /// down the middle of the base, on the most-looked-at part of the
    /// building.
    ///
    /// The rule is stated for the *masonry* only: the fit-out inside the
    /// chambers is supposed to sit against its lining, guns pass through their
    /// own ports, and trim is supposed to be proud of what it trims. Masonry
    /// blocks are structure, and two of those in the same place is always a
    /// mistake.
    #[test]
    fn no_two_masonry_blocks_interpenetrate() {
        use crate::pds::SovereignTextureConfig as Tex;
        // Coursed ashlar is what the structural blocks wear, and it is what
        // defines them — selecting on size would sweep in the copings and the
        // sentry box (#972 lesson 24).
        fn blocks(g: &Generator, at: [f32; 3], out: &mut Vec<([f32; 3], [f32; 3])>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let crate::pds::GeneratorKind::Cuboid {
                size,
                common: PrimCommon { material, torture, .. },
                ..
            } = &g.kind
                && matches!(material.texture, Tex::Ashlar(_))
                // Battered blocks narrow with height; their AABB overstates
                // them, so they are left to the eye rather than reported as
                // false overlaps.
                && torture.taper.0 == [0.0, 0.0]
                // Structure, not trim: a coping or a stub pier is meant to
                // lap what it caps.
                && size.0[1] > 1.0
            {
                out.push((here, [size.0[0] * 0.5, size.0[1] * 0.5, size.0[2] * 0.5]));
            }
            for c in &g.children {
                blocks(c, here, out);
            }
        }
        let mut found = Vec::new();
        blocks(&built(), [0.0; 3], &mut found);
        assert!(
            found.len() >= 5,
            "only {} masonry blocks found — the selector has stopped seeing \
             the piers and the rear mass",
            found.len()
        );
        for (i, (ca, ea)) in found.iter().enumerate() {
            for (cb, eb) in &found[i + 1..] {
                let clash = |ax: usize| (ca[ax] - cb[ax]).abs() < ea[ax] + eb[ax] - 1e-3;
                assert!(
                    !(clash(0) && clash(1) && clash(2)),
                    "two masonry blocks share space: {ca:?} (half {ea:?}) and \
                     {cb:?} (half {eb:?}) — coplanar faces inside an overlap \
                     are what z-fights"
                );
            }
        }
    }

    /// The gorge flight actually joins the fort (#1025).
    ///
    /// Three relationships, because the fault was three faults: the flight
    /// stopped short of the deck edge, its top treads ran through the cordon's
    /// oversail, and the breast wall it climbed to had no opening in it — a
    /// stair to a parapet. Each is checked against the built tree, and each
    /// would be invisible in an elevation.
    #[test]
    fn the_flight_arrives_on_the_deck_through_a_real_opening() {
        let g = built();
        let solids = measure::solids(&g);
        // The landing bridges the cordon's outer face to the deck's own edge.
        let landing = solids
            .iter()
            .find(|p| {
                let s = p.bounds.size();
                // The top tread shares the landing's width AND its top face,
                // so depth is the only property that tells them apart —
                // select on what defines the thing (#972 lesson 24).
                (s.x - FLIGHT_W).abs() < 0.05
                    && (p.bounds.max.y - DECK_TOP).abs() < 1e-3
                    && s.z > GOING * 1.5
            })
            .expect("the flight lands on a landing at deck level");
        assert!(
            landing.bounds.min.z <= DECK[2] * 0.5 + 1e-3,
            "the landing stops at z = {}, short of the deck edge at {}",
            landing.bounds.min.z,
            DECK[2] * 0.5
        );
        assert!(
            landing.bounds.max.z >= WALL[2] * 0.5 + CORDON_PROJECT - 1e-3,
            "the landing does not reach the cordon's outer face, so the top \
             tread has nothing to bear on"
        );
        // Nothing in the flight cuts through the cordon's projection.
        let cordon_band = (WALL_TOP, CORDON_TOP);
        for p in &solids {
            let b = &p.bounds;
            let in_band = b.max.y > cordon_band.0 + 1e-3 && b.min.y < cordon_band.1 - 1e-3;
            let over_cordon =
                b.min.z < WALL[2] * 0.5 + CORDON_PROJECT - 1e-3 && b.max.z > WALL[2] * 0.5 + 1e-3;
            let is_the_cordon = b.size().x > WALL[0];
            assert!(
                !(in_band && over_cordon && !is_the_cordon),
                "{} at {:?} passes through the cordon's oversail",
                p.kind_tag,
                b.center()
            );
        }
        // And the breast wall has a gap the flight's width at the centre.
        let gorge_z = DECK[2] * 0.5 - PARAPET_D * 0.5;
        let runs: Vec<_> = solids
            .iter()
            .filter(|p| {
                let b = &p.bounds;
                (b.center().z - gorge_z).abs() < 0.2
                    && (b.size().y - GORGE_H).abs() < 0.05
                    && b.size().x > 1.0
            })
            .collect();
        assert_eq!(
            runs.len(),
            2,
            "the gorge breast wall must be two runs with the flight's opening \
             between them, found {}",
            runs.len()
        );
        let gap = runs
            .iter()
            .map(|p| p.bounds.min.x.abs().min(p.bounds.max.x.abs()))
            .fold(f32::MAX, f32::min)
            * 2.0;
        assert!(
            gap >= FLIGHT_W,
            "the opening in the breast wall is {gap} m for a {FLIGHT_W} m \
             flight — somebody coming up the stair walks into a wall"
        );
    }

    /// Nothing inside a chamber touches the masonry that frames it (#1026).
    ///
    /// The casemate platform ran 0.45 m into the pier on each side AND sat
    /// exactly on the talus top, so its underside was coplanar with the sill
    /// across its whole footprint — one grey rectangle fighting the wall
    /// beside it, which is what showed in-world. Stated as clearance rather
    /// than as the numbers that produced it, so re-proportioning a chamber
    /// cannot reopen either half.
    #[test]
    fn a_chambers_fit_out_keeps_clear_of_its_own_masonry() {
        use crate::pds::SovereignTextureConfig as Tex;
        // The fit-out is what is lit; the masonry is what is coursed. Two
        // populations, each defined by its material rather than its size.
        fn lit_parts(g: &Generator, at: [f32; 3], out: &mut Vec<([f32; 3], [f32; 3])>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let crate::pds::GeneratorKind::Cuboid {
                size,
                common: PrimCommon { material, .. },
                ..
            } = &g.kind
                && matches!(material.texture, Tex::None)
                && material.emission_strength.0 > 0.0
            {
                out.push((here, [size.0[0] * 0.5, size.0[1] * 0.5, size.0[2] * 0.5]));
            }
            for c in &g.children {
                lit_parts(c, here, out);
            }
        }
        let g = built();
        let mut fit_out = Vec::new();
        lit_parts(&g, [0.0; 3], &mut fit_out);
        // Two casemates and the guard house, each a floor and a lining.
        let want = port_centres(-1.0).len() * 2 * 2 + 2;
        assert_eq!(
            fit_out.len(),
            want,
            "expected {want} lit fit-out slabs, found {} — the selector has \
             stopped seeing the floors or the linings",
            fit_out.len()
        );
        for (c, e) in &fit_out {
            // Inside the clear width of the opening it belongs to.
            let is_guard = c[0].abs() < GUARD_W;
            let clear = if is_guard { GUARD_W } else { PORT_W };
            let centre = if is_guard {
                0.0
            } else {
                port_centres(c[0].signum())[0]
            };
            assert!(
                (c[0] - centre).abs() + e[0] <= clear * 0.5 - CHAMBER_INSET + 1e-3,
                "a fit-out slab at {c:?} (half {e:?}) runs into the masonry \
                 either side of its own {clear} m opening"
            );
            // Forward of the rear mass, not flush against it.
            assert!(
                c[2] + e[2] <= CHAMBER_BACK - CHAMBER_INSET + 1e-3,
                "a fit-out slab reaches z = {} and the rear mass begins at \
                 {CHAMBER_BACK} — the two share a plane",
                c[2] + e[2]
            );
        }
        // And every chamber floor is bedded BELOW its own sill rather than
        // laid on it, which is the coplanar half of the same fault.
        for (c, e) in &fit_out {
            let is_floor = e[1] < 0.1;
            if !is_floor {
                continue;
            }
            let sill = if c[0].abs() < GUARD_W {
                QUAY
            } else {
                CASEMATE_SILL
            };
            assert!(
                c[1] - e[1] < sill - 1e-3,
                "a chamber floor's underside is at {} and its sill at {sill} \
                 — laid exactly on it, those two faces z-fight across the \
                 whole floor",
                c[1] - e[1]
            );
        }
    }

    /// The deck props are readable objects standing apart from each other
    /// (#1026).
    ///
    /// The pair this replaces were unreadable individually — "hut(?)" and
    /// "roll of cloth(?)" — and intersecting each other as well, which is the
    /// combination that makes a gun deck look like a bin. Legibility is not
    /// testable; **separation** is, and it is the half that shipped broken.
    #[test]
    fn the_deck_props_do_not_stand_in_each_other() {
        let g = built();
        let props: Vec<_> = measure::solids(&g)
            .into_iter()
            .filter(|p| {
                let c = p.bounds.center();
                // On the deck, behind the guns, and not part of the parapet
                // ring or the flagstaff.
                c.y > DECK_TOP && c.y < DECK_TOP + 2.6 && c.z > 2.0 && c.x.abs() > 2.0
            })
            .collect();
        assert!(
            props.len() >= 8,
            "only {} deck-prop pieces found — the locker and the rack are not \
             both there",
            props.len()
        );
        // The two assemblies live on opposite sides; nothing from one may
        // reach across the axis into the other.
        let (left, right): (Vec<_>, Vec<_>) = props.iter().partition(|p| p.bounds.center().x < 0.0);
        assert!(
            !left.is_empty() && !right.is_empty(),
            "both deck props must be present, one either side"
        );
        let left_edge = left.iter().map(|p| p.bounds.max.x).fold(f32::MIN, f32::max);
        let right_edge = right
            .iter()
            .map(|p| p.bounds.min.x)
            .fold(f32::MAX, f32::min);
        assert!(
            right_edge - left_edge > 1.0,
            "the two deck props are {} m apart — they were intersecting when \
             this entry shipped",
            right_edge - left_edge
        );
    }

    /// The work is a stack and the stack is contiguous (#972 lesson 33):
    /// apron → talus → wall → cordon → deck → parapet, each seated on the one
    /// below. A gap anywhere here is a floating tier, and a four-angle sheet
    /// cannot see a hundred millimetres of air behind a cordon.
    #[test]
    fn the_tiers_are_a_contiguous_stack() {
        let levels = [
            ("apron top / talus base", QUAY, QUAY),
            ("talus top / casemate sill", CASEMATE_SILL, QUAY + TALUS[1]),
            ("wall top", WALL_TOP, CASEMATE_SILL + WALL[1]),
            ("cordon top", CORDON_TOP, WALL_TOP + CORDON_H),
            ("deck top", DECK_TOP, CORDON_TOP + DECK[1]),
            ("parapet top", PARAPET_TOP, DECK_TOP + PARAPET_H),
        ];
        for (name, got, want) in levels {
            assert!(
                (got - want).abs() < 1e-5,
                "{name}: {got} is not {want} — the stack has opened up"
            );
        }
    }

    /// The head band exactly fills the wall above the ports, and is deep
    /// enough to read as a band.
    ///
    /// `const` blocks rather than runtime assertions: every operand is a
    /// constant, so the build can carry the claim and fail earlier than the
    /// test suite would. If it did not hold there would be either a slot of
    /// daylight over every port or a band burying the openings it heads.
    const _: () = assert!(
        PORT_H + HEAD_H == WALL[1],
        "the port height plus the head band does not make the wall"
    );
    const _: () = assert!(HEAD_H > 0.4, "that head band is a lintel, not a band");

    /// The cordon is a RING — centred on the building, with its projection in
    /// its size (#972 lesson 31). Centring a ring on a trim plane makes a
    /// cantilevered shelf the width of the site, and it is the kind of slab
    /// nobody would author on purpose.
    #[test]
    fn the_cordon_rings_the_building_rather_than_shelving_off_its_face() {
        let g = built();
        let cordon = measure::solids(&g)
            .into_iter()
            .find(|p| {
                let s = p.bounds.size();
                (s.y - CORDON_H).abs() < 1e-3 && s.x > WALL[0]
            })
            .expect("the cordon is in the tree");
        let c = cordon.bounds.center();
        assert!(
            c.z.abs() < 1e-3,
            "the cordon is centred at z = {} rather than on the building",
            c.z
        );
        let s = cordon.bounds.size();
        assert!(
            (s.x - (WALL[0] + CORDON_PROJECT * 2.0)).abs() < 1e-3
                && (s.z - (WALL[2] + CORDON_PROJECT * 2.0)).abs() < 1e-3,
            "the cordon measures {s:?}, which does not project {CORDON_PROJECT} \
             m on all four sides of a {WALL:?} wall"
        );
    }

    /// Nothing stands off the apron it is nested under (#972 lessons 8 and
    /// 19). The gorge steps are what this is really guarding: a flight whose
    /// run is derived from its rise grows and shrinks with the building, and
    /// the moment it outgrows the paving it lands on bare ground — which no
    /// camera angle here would show.
    #[test]
    fn every_part_stands_on_the_apron() {
        let g = built();
        let half = [APRON[0] * 0.5, APRON[2] * 0.5];
        let mut checked = 0;
        for p in measure::solids(&g) {
            let b = &p.bounds;
            // Guns and copings oversail the face on purpose; the rule is
            // about what stands on the ground.
            if b.center().y > CASEMATE_SILL {
                continue;
            }
            checked += 1;
            assert!(
                b.min.x >= -half[0] - 1e-3 && b.max.x <= half[0] + 1e-3,
                "{} at {:?} overhangs the apron in X",
                p.kind_tag,
                b.center()
            );
            assert!(
                b.min.z >= -half[1] - 1e-3 && b.max.z <= half[1] + 1e-3,
                "{} at {:?} overhangs the apron in Z ({} .. {})",
                p.kind_tag,
                b.center(),
                b.min.z,
                b.max.z
            );
        }
        // A guard that silently checks nothing is worse than no guard
        // (#972 lesson 29): the talus, the steps and the cheek walls are all
        // below the sill, so the population is never empty.
        assert!(
            checked > 8,
            "only {checked} ground parts were examined — the selector has \
             stopped finding the flight and the talus"
        );
    }

    /// The gorge flight lands exactly on the deck, with equal risers.
    ///
    /// The relationship, not the count: the beach house and the lifeguard
    /// tower both shipped flights picked by eye, one floating 0.3 m clear of
    /// the deck edge and one with a half-metre drop off its top tread.
    #[test]
    fn the_flight_lands_flush_on_the_deck() {
        let rise = DECK_TOP - QUAY;
        let treads = (rise / RISER).round().max(2.0) as usize;
        let riser = rise / treads as f32;
        assert!(
            (riser * treads as f32 - rise).abs() < 1e-5,
            "{treads} risers of {riser} do not make the {rise} m climb"
        );
        assert!(
            (0.15..=0.32).contains(&riser),
            "a {riser} m riser is a step nobody can take"
        );
        // And the top tread really reaches the deck.
        let g = built();
        let top = measure::solids(&g)
            .into_iter()
            .filter(|p| {
                let b = &p.bounds;
                b.min.z > WALL[2] * 0.5 - 1e-3 && (b.size().x - 4.4).abs() < 1e-3
            })
            .map(|p| p.bounds.max.y)
            .fold(f32::MIN, f32::max);
        assert!(
            (top - DECK_TOP).abs() < 1e-3,
            "the flight tops out at {top}, not at the {DECK_TOP} deck"
        );
    }

    /// No hot emissive surface is a broad panel. Above a strength of 2.0 at
    /// most one of a prim's three dimensions may exceed 0.3 m — a hot run has
    /// to be a bar or a point, never a lid (#972 lesson 30, stated as the
    /// prohibition rather than as a census of the lights).
    #[test]
    fn no_hot_emissive_surface_is_a_panel() {
        fn walk(g: &Generator, at: [f32; 3], bad: &mut Vec<String>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            let (dims, strength) = match &g.kind {
                K::Cuboid {
                    size,
                    common: PrimCommon { material, .. },
                    ..
                } => (Some(size.0), material.emission_strength.0),
                K::Sphere {
                    radius,
                    common: PrimCommon { material, .. },
                    ..
                } => (Some([radius.0 * 2.0; 3]), material.emission_strength.0),
                K::Cylinder {
                    radius,
                    height,
                    common: PrimCommon { material, .. },
                    ..
                } => (
                    Some([radius.0 * 2.0, height.0, radius.0 * 2.0]),
                    material.emission_strength.0,
                ),
                _ => (None, 0.0),
            };
            if let Some(d) = dims
                && strength > 2.0
            {
                let big = d.iter().filter(|v| **v > 0.3).count();
                if big > 1 {
                    bad.push(format!("{d:?} at strength {strength}, at {here:?}"));
                }
            }
            for c in &g.children {
                walk(c, here, bad);
            }
        }
        let mut bad = Vec::new();
        walk(&built(), [0.0; 3], &mut bad);
        assert!(
            bad.is_empty(),
            "a hot emissive must be a bar or a point, not a panel: {bad:?}"
        );
    }

    /// The tree is a tree (#972 lesson 3): the deck carries the parapet, the
    /// guns and the colours, so one gizmo drag moves the whole upper work.
    ///
    /// Pinned as a subtree *size*, which is the editability contract and the
    /// thing that breaks silently later — a refactor that flattens the deck
    /// back onto the root leaves every world position identical and every
    /// render identical.
    #[test]
    fn the_deck_carries_the_upper_work_as_one_subtree() {
        fn count(g: &Generator) -> usize {
            1 + g.children.iter().map(count).sum::<usize>()
        }
        let g = built();
        let deck = g
            .children
            .iter()
            .find(|c| match &c.kind {
                // Selected by the property that DEFINES it — a deck's own
                // thickness and plan — rather than by child count, which is a
                // selector that has broken in three files now.
                K::Cuboid { size, .. } => {
                    (size.0[1] - DECK[1]).abs() < 1e-4 && (size.0[0] - DECK[0]).abs() < 1e-4
                }
                _ => false,
            })
            .expect("the terreplein deck is a direct child of the apron");
        let n = count(deck);
        assert!(
            n > 40,
            "the deck subtree holds {n} nodes — the parapet, the guns and the \
             colours have been flattened back onto the root"
        );
        assert!(
            g.children.len() < 60,
            "the root has {} direct children; the upper work belongs under \
             the deck",
            g.children.len()
        );
    }
}
