//! Heritage liveries for the seeded fleet - the one home for what colour a
//! boat or a land-skiff is painted (#1365).
//!
//! # Why a curated list at all
//!
//! Until this landed, the two families spent the raw seeded accent triad on
//! their big masses: a hull was `primary_accent` lifted to a topsides value
//! and a car was `primary_accent` floored off black. The palette samples in
//! OkLCH around a per-DID hue, so that produced pink cars with green windows
//! and a fleet of pastel hulls - and the reason it read as a toy is COLOUR,
//! not shape. Real craft wear a small set of heritage colours on their big
//! masses and carry their owner's identity in a few square inches of trim:
//! a boot top, a coachline, a burgee, wheel centres, the upholstery.
//!
//! The airship gets away with a saturated envelope because real balloons are
//! saturated. Boats and cars are not, and that asymmetry is the whole reason
//! this module exists rather than a shared one.
//!
//! # What is scheme and what is seed
//!
//! - The **scheme** owns every large surface: topsides, antifoul, deck,
//!   canvas and varnish on a boat, and a junk's hull and sails (#1371);
//!   coachwork, wings and brightwork on a car, the tube frame on a dune
//!   buggy (#1374), a cyclecar's pod (#1376) and every plate of an armoured
//!   car (#1375). It is picked per seed from
//!   a curated list on this module's own salted stream, exactly as the craft
//!   TYPE is picked in [`crate::seeded_defaults::avatar::craft`].
//! - The **seeded accent** ([`primary_accent`](crate::seeded_defaults::AvatarPalette::primary_accent))
//!   is spent on the
//!   identity slots ONLY: the boot stripe, the burgee and the jib on a boat,
//!   the band round a steam tug's funnel (#1370), and a junk's transom
//!   roundel and the eye on each bow (#1371);
//!   the coachline, the wheel centres and the hide on a car; a horseless
//!   wagon's spoked wheels (#1377); a dune buggy's rims and the hide of her
//!   seats (#1374); a cyclecar's rims and the strip along her flanks (#1376);
//!   and an armoured car's unit flash and the band round her turret - never
//!   her wheel centres, which on a military machine read as a toy
//!   (#1375). The secondary still tints the antifoul and the upholstery a
//!   little so two craft of one scheme are not identical, and the tertiary
//!   still lights the windows.
//!
//! Both the accent hue and the livery index are DID-seeded and independent,
//! so two owners rarely match, and a craft stays recognisable by its trim
//! across a settlement.
//!
//! # Theme speaks through finish, not hue
//!
//! Nothing here is keyed to the avatar's
//! [`ThemeArchetype`](crate::seeded_defaults::ThemeArchetype). A cyberpunk
//! avatar and a medieval one can both roll a bottle-green car; what tells
//! them apart is the [`MaterialKit`] - its finish family, its wear, and its
//! luminous flag, which lights the identity trim on the neon themes through
//! [`trim`]. That is deliberate: hue-keying the schemes by theme would put
//! every avatar of a theme in the same car, which is the variety problem this
//! module is supposed to solve.
//!
//! **A boat's hull lines are the one exception, and it is a render result**
//! (#1365 phase 2). The boot top and the cove line carry the accent as PAINT
//! on every theme, luminous or not. Lit, they are a neon pinstripe running the
//! whole length of a gaff boat's sheer, and on a battered SpaceOutpost seed
//! that was the loudest thing in the frame - louder than her sails, her cabin
//! and her deck, which is what a boat is actually read by at play distance. A
//! boat's glow belongs where it is small and high: her masthead burgee, and
//! the lit ports that were always lit. The one boat hull line that does glow
//! is the neon SKIMMER's rub rail (#1372): the owner's call, on the one
//! variant whose neon outline is the point, after a lit chine drawn first did
//! not read at 12 m - and it is the rub rail, not a pinstripe down her
//! topsides. A car is unchanged, because a lit coachline and lit wheel
//! centres on a cyberpunk machine are exactly the point and were never the
//! complaint.
//!
//! # The two colour rules this fleet learned by render
//!
//! 1. **A guard drawn at the tyre's value is not a guard** (#1364). Coachwork
//!    has to stay clear of rubber, so every car mass is floored at
//!    [`GUARD_FLOOR`] - and the floor is owed on the FINISHED surface, which
//!    is why it is divided by [`MaterialKit::value_after_grime`] before it is
//!    applied. Flooring before the grime is flooring the wrong number.
//! 2. **A topside must read against what is under it.** The old code floored
//!    every hull at 0.58 for that reason; a curated navy or black scheme
//!    breaks that floor on purpose, and the contrast is carried by the boot
//!    stripe, the rub rail and the deck instead - all of which the scheme
//!    now owns, so it can promise a contrast the palette never could.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use super::colour::{floor_value, luma, mix, shade, to_value, window_light, window_material};
use super::parts::PartCtx;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::Fp;
use crate::seeded_defaults::scene::pick_weighted;
use crate::seeded_defaults::{BuggyVariant, MaterialKit, RunaboutVariant, WagonBody};

/// Sub-stream salt for the livery draw - distinct from every sibling avatar
/// deriver salt, so which scheme a craft wears is decorrelated from its type,
/// its palette and its proportions.
const AVATAR_LIVERY_SALT: u64 = 0x11BE_4712_11BE_4712;

/// The value every painted surface on a land-skiff is floored at, **after**
/// the kit's wear grime.
///
/// The one colour rule the roadster found by render (#1364): a guard drawn
/// near-black - which is what a period photograph suggests - carries the
/// TYRE's own value, so the eye merges the two and the four wheels read as
/// detached blobs with nothing over them. It applies to the body as well as
/// the wings, because a black car whose wings are floored above its body is a
/// two-tone nobody asked for.
///
/// **0.15, not the 0.11 the roadster shipped with, and that is a render
/// result** (#1365): at 109 px/m a wing floored at 0.11 still merges with the
/// tyre under it, because the arch is in the wheel's own shadow and two dark
/// greys a stop apart are one dark grey there. It shows worst on a two-tone,
/// where a pale body over a black undercarriage reads as a car floating on a
/// blob. The cost is that a "black" car is a charcoal one - which is what
/// black coachwork looks like in daylight anyway, and the same trade the
/// guards themselves already made.
pub(crate) const GUARD_FLOOR: f32 = 0.15;

/// The value a hull's topsides are floored at, after grime. Far lower than
/// the 0.58 the pre-livery code used, because a navy or a black scheme is a
/// deliberate choice here rather than an accident of a dark seed - but not
/// zero, because a battered seed on a black scheme would otherwise go to mud.
const BOAT_MASS_FLOOR: f32 = 0.060;

/// How far the boot stripe's value is held from the topsides it divides, and
/// the coachline's from the coachwork it runs along. A hand's width of paint
/// only exists if it separates from what it lies on.
const BOOT_DELTA: f32 = 0.30;
const COACHLINE_DELTA: f32 = 0.26;

/// How far a wheel centre is held from the guard over it - less than a boot
/// stripe needs, because a disc is a solid area rather than a line.
const DISC_DELTA: f32 = 0.22;

// ---------------------------------------------------------------------------
// The schemes
// ---------------------------------------------------------------------------

/// One heritage scheme for a boat: the colours of her big surfaces.
///
/// Every field is a colour those surfaces really are, so a scheme can promise
/// a contrast - a dark hull is given a pale deck and a light canvas, a
/// varnished hull a red bottom. `weight` is the scheme's share of the fleet.
#[derive(Clone, Copy, Debug)]
pub struct BoatLivery {
    /// The scheme's name, as the readouts print it and as the owner agreed
    /// the list. Public for the same reason
    /// [`BoatType::label`](crate::seeded_defaults::BoatType::label) is: which
    /// livery a seed wears is a property of the avatar that the survey
    /// readouts answer for today and the pinned re-roll will show (#1380).
    pub name: &'static str,
    weight: u32,
    topsides: [f32; 3],
    /// Bright-finished rather than painted - a varnished hull is the one
    /// scheme whose topsides are not paint, and the difference is gloss.
    varnished_hull: bool,
    antifoul: [f32; 3],
    /// Laid deck, rub rail and spars.
    deck: [f32; 3],
    /// Toe rail, coamings, hatch tops, tiller.
    varnish: [f32; 3],
    /// Mainsail. White for a yacht, cream for a cruiser, tanbark for a
    /// working boat.
    canvas: [f32; 3],
}

/// The boat fleet's schemes. Seven, in descending share.
pub const BOAT_LIVERIES: &[BoatLivery] = &[
    BoatLivery {
        name: "White",
        weight: 7,
        topsides: [0.90, 0.90, 0.88],
        varnished_hull: false,
        antifoul: [0.42, 0.13, 0.10],
        deck: [0.62, 0.45, 0.26],
        varnish: [0.58, 0.40, 0.19],
        canvas: [0.92, 0.90, 0.84],
    },
    BoatLivery {
        name: "Cream",
        weight: 6,
        topsides: [0.88, 0.82, 0.65],
        varnished_hull: false,
        antifoul: [0.40, 0.16, 0.10],
        deck: [0.55, 0.38, 0.20],
        varnish: [0.58, 0.40, 0.19],
        canvas: [0.90, 0.86, 0.75],
    },
    BoatLivery {
        name: "Navy",
        weight: 5,
        topsides: [0.075, 0.105, 0.22],
        varnished_hull: false,
        antifoul: [0.40, 0.13, 0.10],
        // A pale deck, deliberately: on a dark hull the deck is the contrast.
        deck: [0.66, 0.50, 0.30],
        varnish: [0.60, 0.42, 0.20],
        canvas: [0.92, 0.90, 0.84],
    },
    BoatLivery {
        name: "Black",
        weight: 4,
        topsides: [0.055, 0.055, 0.060],
        varnished_hull: false,
        antifoul: [0.30, 0.10, 0.08],
        deck: [0.60, 0.44, 0.26],
        varnish: [0.62, 0.45, 0.18],
        // Tanbark: a working gaff boat's sails were dressed with ochre and
        // tar, and on a black hull white canvas is the only thing in frame.
        canvas: [0.55, 0.27, 0.15],
    },
    BoatLivery {
        name: "Oxblood",
        weight: 4,
        topsides: [0.26, 0.055, 0.060],
        varnished_hull: false,
        antifoul: [0.10, 0.10, 0.11],
        deck: [0.58, 0.42, 0.24],
        varnish: [0.58, 0.40, 0.19],
        canvas: [0.90, 0.86, 0.75],
    },
    BoatLivery {
        name: "Bottle green",
        weight: 4,
        topsides: [0.055, 0.170, 0.110],
        varnished_hull: false,
        antifoul: [0.40, 0.13, 0.10],
        deck: [0.60, 0.44, 0.26],
        varnish: [0.58, 0.40, 0.19],
        canvas: [0.90, 0.86, 0.75],
    },
    BoatLivery {
        name: "Varnished timber",
        weight: 3,
        // Mahogany, and the one hull carrying a gloss varnish rather than
        // paint. NOT the timber finish: that is a Plank texture, and a plank
        // pattern wrapped round a curved skin renders as scales (#784). A
        // bright-finished hull at 109 px/m is a warm gloss, not a grain.
        topsides: [0.34, 0.16, 0.08],
        varnished_hull: true,
        antifoul: [0.32, 0.11, 0.09],
        deck: [0.64, 0.48, 0.28],
        varnish: [0.60, 0.42, 0.20],
        canvas: [0.90, 0.86, 0.75],
    },
];

/// One heritage scheme for a land-skiff.
#[derive(Clone, Copy, Debug)]
pub struct SkiffLivery {
    /// See [`BoatLivery::name`].
    pub name: &'static str,
    weight: u32,
    /// Coachwork.
    body: [f32; 3],
    /// Wings, running boards and valances. `None` is a single-colour car,
    /// whose guards are its own paint darkened; `Some` is a two-tone.
    wings: Option<[f32; 3]>,
    /// Lamp shells, radiator shell, bumper, screen frame: nickel-chrome on
    /// the later schemes, brass on the earlier ones.
    brightwork: [f32; 3],
}

/// Brightwork, the two kinds a car of this period carries.
const CHROME: [f32; 3] = [0.80, 0.80, 0.82];
const BRASS: [f32; 3] = [0.72, 0.55, 0.22];

/// Wing black on a two-tone: a charcoal rather than a true black, set here
/// rather than left to [`GUARD_FLOOR`] to catch it, so the colour on the sheet
/// is the colour in the list.
const WING_BLACK: [f32; 3] = [0.195, 0.195, 0.205];

/// The skiff fleet's schemes. Eight, in descending share: six solids and two
/// two-tones with dark wings.
pub const SKIFF_LIVERIES: &[SkiffLivery] = &[
    SkiffLivery {
        name: "Racing green",
        weight: 6,
        body: [0.045, 0.170, 0.100],
        wings: None,
        brightwork: BRASS,
    },
    SkiffLivery {
        name: "French blue",
        weight: 5,
        body: [0.100, 0.240, 0.500],
        wings: None,
        brightwork: CHROME,
    },
    SkiffLivery {
        name: "Maroon",
        weight: 5,
        body: [0.240, 0.050, 0.070],
        wings: None,
        brightwork: BRASS,
    },
    SkiffLivery {
        name: "Cream",
        weight: 5,
        body: [0.860, 0.810, 0.660],
        wings: None,
        brightwork: BRASS,
    },
    SkiffLivery {
        name: "Gunmetal",
        weight: 4,
        body: [0.240, 0.260, 0.280],
        wings: None,
        brightwork: CHROME,
    },
    SkiffLivery {
        name: "Black",
        weight: 4,
        body: [0.060, 0.060, 0.065],
        wings: None,
        brightwork: CHROME,
    },
    SkiffLivery {
        name: "Ivory over black",
        weight: 3,
        body: [0.870, 0.840, 0.720],
        wings: Some(WING_BLACK),
        brightwork: CHROME,
    },
    SkiffLivery {
        name: "Carmine over black",
        weight: 3,
        body: [0.480, 0.055, 0.050],
        wings: Some(WING_BLACK),
        brightwork: CHROME,
    },
    // The third two-tone answers the dark-wing problem a different way: a
    // wing that is dark but COLOURED separates from a neutral tyre by hue
    // where it cannot separate by value, which is the harder half of the
    // problem at 109 px/m. Kept beside the two black-winged schemes so the
    // list can be judged on the sheet rather than on this note.
    SkiffLivery {
        name: "Cream over green",
        weight: 3,
        body: [0.860, 0.810, 0.660],
        wings: Some([0.055, 0.170, 0.110]),
        brightwork: BRASS,
    },
];

// ---------------------------------------------------------------------------
// The pick
// ---------------------------------------------------------------------------

/// The livery for a seed, or the one `over` names.
///
/// `over` is the render tool's `--livery <index>`, which draws every scheme in
/// the list on one hull so a list can be judged side by side instead of hunted
/// for across seeds. It is an index into the family's table, wrapped rather
/// than clamped so a survey loop cannot silently draw the last scheme twice.
fn pick<T: Copy>(table: &[T], weight: impl Fn(T) -> u32, seed: u64, over: Option<usize>) -> &T {
    if let Some(i) = over {
        return &table[i % table.len()];
    }
    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ AVATAR_LIVERY_SALT);
    // The table is a fixed list of indices so the draw can name a POSITION -
    // `pick_weighted` needs `Copy` and a livery is only `Copy` by accident of
    // being small.
    let idx: Vec<usize> = (0..table.len()).collect();
    let i = pick_weighted(&idx, |i| weight(table[i]), &mut rng)
        .expect("every livery table carries at least one non-zero weight");
    &table[i]
}

/// The heritage scheme this boat seed wears.
pub fn boat_livery(seed: u64, over: Option<usize>) -> &'static BoatLivery {
    pick(BOAT_LIVERIES, |l| l.weight, seed, over)
}

/// The heritage scheme this land-skiff seed wears.
pub fn skiff_livery(seed: u64, over: Option<usize>) -> &'static SkiffLivery {
    pick(SKIFF_LIVERIES, |l| l.weight, seed, over)
}

/// A horseless wagon's scheme (#1377): its box and its brightwork, and - on
/// the two forced schemes only - its running gear.
///
/// A wagon is not a car, and the car schemes proved it by render: a cream
/// hearse is a bread van. So the wagon has a list of its own, of the colours
/// a working wagon was actually painted, and the seeded accent goes on its
/// WHEELS, which are the wagon's wheel centres - the strongest identity mark
/// a land craft carries at play distance (#1365).
#[derive(Clone, Copy, Debug)]
pub struct WagonLivery {
    /// See [`BoatLivery::name`].
    pub name: &'static str,
    weight: u32,
    /// The box, the hearse's body, the ox-cart's cabin: painted boards.
    body: [f32; 3],
    /// The wheels. `None` is the seed's own accent (every listed scheme);
    /// `Some` is a forced scheme's own gear colour, because a hearse with
    /// pink wheels is not a hearse.
    gear: Option<[f32; 3]>,
    /// Lantern frames, rails, urns and the gilt ridge.
    brightwork: [f32; 3],
}

/// The wagon schemes a cart, a buckboard and a chariot draw from. Four, the
/// owner agreed the set on the phase-1 renders (#1377); the weights are mine.
pub const WAGON_LIVERIES: &[WagonLivery] = &[
    WagonLivery {
        name: "Farm green",
        weight: 5,
        body: [0.16, 0.30, 0.16],
        gear: None,
        brightwork: BRASS,
    },
    WagonLivery {
        name: "Barn red",
        weight: 5,
        body: [0.46, 0.10, 0.07],
        gear: None,
        brightwork: BRASS,
    },
    WagonLivery {
        name: "Weathered oak",
        weight: 4,
        body: [0.46, 0.34, 0.22],
        gear: None,
        brightwork: BRASS,
    },
    WagonLivery {
        name: "Prussian blue",
        weight: 4,
        body: [0.10, 0.18, 0.34],
        gear: None,
        brightwork: BRASS,
    },
];

/// The hearse's one scheme: black box, black wheels, silver fittings.
pub const MOURNING_BLACK: WagonLivery = WagonLivery {
    name: "Mourning black",
    weight: 0,
    body: [0.060, 0.060, 0.065],
    gear: Some([0.060, 0.060, 0.065]),
    brightwork: [0.72, 0.72, 0.74],
};

/// The ox-cart's one scheme: black lacquer on red wheels, gilt fittings.
pub const LACQUER: WagonLivery = WagonLivery {
    name: "Lacquer",
    weight: 0,
    body: [0.07, 0.05, 0.05],
    gear: Some([0.50, 0.08, 0.06]),
    brightwork: [0.80, 0.62, 0.24],
};

/// The scheme this wagon seed wears on `body` - a forced one for the hearse
/// and the ox-cart, whatever `over` says, and a pick from
/// [`WAGON_LIVERIES`] for the rest.
pub fn wagon_livery(seed: u64, body: WagonBody, over: Option<usize>) -> &'static WagonLivery {
    match body {
        WagonBody::Hearse => &MOURNING_BLACK,
        WagonBody::OxCart => &LACQUER,
        WagonBody::Cart | WagonBody::Buckboard | WagonBody::Chariot => {
            pick(WAGON_LIVERIES, |l| l.weight, seed, over)
        }
    }
}

/// A dune buggy's scheme (#1374): the colour of her TUBE FRAME.
///
/// A buggy's coachwork is her frame, so the scheme goes on it, over a dark
/// gelcoat pod; on the pod the seven schemes looked alike at 12 m, a coloured
/// patch inside a grey cage. And a buggy is not a roadster either: on a frame
/// the heritage car list goes dull - its black is charcoal lines over black
/// tyres, and a two-tone's "over black" can only mean the pod, which every
/// buggy already has - so she has lists of her own, as the wagon does. Her
/// brightwork is chrome on every scheme (the 1960s buggy's, where brass is
/// the roadster's era), so a scheme carries nothing else.
#[derive(Clone, Copy, Debug)]
pub struct BuggyLivery {
    /// See [`BoatLivery::name`].
    pub name: &'static str,
    weight: u32,
    /// The tube frame, its suspension arms and the canopy's stripes.
    frame: [f32; 3],
}

/// The gelcoat brights a sand rail and a beach buggy draw from - the colours
/// dune buggies were actually sold in. Seven; the owner agreed the list and
/// the weights on the phase-1 renders (#1374), and the pick walks it in this
/// order.
pub const BUGGY_LIVERIES: &[BuggyLivery] = &[
    BuggyLivery {
        name: "Tangerine",
        weight: 5,
        frame: [0.90, 0.38, 0.06],
    },
    BuggyLivery {
        name: "Lime",
        weight: 4,
        frame: [0.50, 0.74, 0.12],
    },
    BuggyLivery {
        name: "Sunshine yellow",
        weight: 4,
        frame: [0.95, 0.76, 0.10],
    },
    BuggyLivery {
        name: "Surf turquoise",
        weight: 4,
        frame: [0.07, 0.56, 0.62],
    },
    BuggyLivery {
        name: "Candy red",
        weight: 4,
        frame: [0.72, 0.07, 0.06],
    },
    BuggyLivery {
        name: "Metalflake purple",
        weight: 3,
        frame: [0.38, 0.13, 0.54],
    },
    BuggyLivery {
        name: "Baja white",
        weight: 3,
        frame: [0.88, 0.88, 0.84],
    },
];

/// The desert raider's wasteland list: in the brights she reads as a toy.
/// Four, at equal weights - the wagon's forced hearse and ox-cart schemes
/// are the precedent for a variant with colours of its own. "Gunmetal" is
/// also a heritage skiff scheme's name, in another colour; nothing looks a
/// scheme up across the lists.
pub const RAIDER_LIVERIES: &[BuggyLivery] = &[
    BuggyLivery {
        name: "Desert tan",
        weight: 1,
        frame: [0.64, 0.52, 0.34],
    },
    BuggyLivery {
        name: "Olive drab",
        weight: 1,
        frame: [0.30, 0.34, 0.17],
    },
    BuggyLivery {
        name: "Gunmetal",
        weight: 1,
        frame: [0.30, 0.31, 0.33],
    },
    BuggyLivery {
        name: "Rust red",
        weight: 1,
        frame: [0.46, 0.18, 0.10],
    },
];

/// The scheme this buggy seed wears as `variant`: a pick from
/// [`RAIDER_LIVERIES`] for a raider and from [`BUGGY_LIVERIES`] for the
/// rest. `over` wraps inside the variant's own list.
pub fn buggy_livery(seed: u64, variant: BuggyVariant, over: Option<usize>) -> &'static BuggyLivery {
    match variant {
        BuggyVariant::Raider => pick(RAIDER_LIVERIES, |l| l.weight, seed, over),
        BuggyVariant::Rail | BuggyVariant::Beach => pick(BUGGY_LIVERIES, |l| l.weight, seed, over),
    }
}

/// A cyclecar's scheme (#1376): her pod, and on a two-tone the colour under
/// its widest line.
///
/// She has a list of her own, as the wagon and the buggy do, and for the
/// buggy's reason turned round: the heritage car schemes are a 1920s
/// coachbuilder's colours - racing green, cream - on a neon machine, and the
/// hardtop roadster beside her wears exactly those. Her schemes are neutral
/// and deep coachwork a lit accent reads against. And a heritage two-tone
/// collapses on a wingless pod (there is nowhere for "over black" to go), so
/// her one two-tone draws a SPLIT pod instead: the upper half in `body`, the
/// lower half and the spat in `lower`.
#[derive(Clone, Copy, Debug)]
pub struct CyclecarLivery {
    /// See [`BoatLivery::name`].
    pub name: &'static str,
    weight: u32,
    /// The pod, and her fin, lamp shells and cycle wings.
    body: [f32; 3],
    /// A two-tone's lower half-pod and spat, or `None` for one colour.
    lower: Option<[f32; 3]>,
}

/// The cyclecar's six schemes (#1376), as the owner agreed them on the
/// phase-1 renders, weights and order: the pick walks the table in this
/// order.
pub const CYCLECAR_LIVERIES: &[CyclecarLivery] = &[
    CyclecarLivery {
        name: "Obsidian",
        weight: 5,
        body: [0.030, 0.030, 0.036],
        lower: None,
    },
    CyclecarLivery {
        name: "Pearl",
        weight: 4,
        body: [0.86, 0.86, 0.84],
        lower: None,
    },
    CyclecarLivery {
        name: "Midnight",
        weight: 4,
        body: [0.05, 0.08, 0.22],
        lower: None,
    },
    CyclecarLivery {
        name: "Pearl over obsidian",
        weight: 3,
        body: [0.86, 0.86, 0.84],
        lower: Some([0.030, 0.030, 0.036]),
    },
    CyclecarLivery {
        name: "Quicksilver",
        weight: 3,
        body: [0.60, 0.62, 0.66],
        lower: None,
    },
    CyclecarLivery {
        name: "Ultraviolet",
        weight: 3,
        body: [0.20, 0.07, 0.32],
        lower: None,
    },
];

/// The scheme this cyclecar seed wears, a pick from [`CYCLECAR_LIVERIES`];
/// `over` wraps inside it (`render --livery 0` is Obsidian, `1` Pearl).
pub fn cyclecar_livery(seed: u64, over: Option<usize>) -> &'static CyclecarLivery {
    pick(CYCLECAR_LIVERIES, |l| l.weight, seed, over)
}

/// An armoured car's scheme (#1375): her armour plate.
///
/// A list of her own, as the wagon, the buggy and the cyclecar have: a
/// racing green or a cream armoured car is a coachbuilder's colour on a
/// machine with a turret, and the roadster parked beside her wears exactly
/// those. These are the colours a works, municipal or raider machine is
/// really painted, and every one of them is a value a seeded accent reads
/// against - which it has to be, because her accent goes on a unit flash and
/// a turret band and nowhere else.
#[derive(Clone, Copy, Debug)]
pub struct ArmouredLivery {
    /// See [`BoatLivery::name`].
    pub name: &'static str,
    weight: u32,
    /// Every plate of her: the tub, the decks, the fighting compartment, the
    /// glacis, the arches, the turret, the cupola and the lamp shells.
    hull: [f32; 3],
}

/// The armoured car's six schemes (#1375), as the owner agreed them on the
/// phase-1 renders, weights and order: the pick walks the table in this
/// order.
pub const ARMOUR_LIVERIES: &[ArmouredLivery] = &[
    ArmouredLivery {
        name: "Olive drab",
        weight: 5,
        hull: [0.190, 0.200, 0.120],
    },
    ArmouredLivery {
        name: "Works grey",
        weight: 4,
        hull: [0.300, 0.310, 0.320],
    },
    ArmouredLivery {
        name: "Desert sand",
        weight: 4,
        hull: [0.660, 0.580, 0.400],
    },
    ArmouredLivery {
        name: "Midnight blue",
        weight: 4,
        hull: [0.070, 0.105, 0.190],
    },
    ArmouredLivery {
        name: "Oxide red",
        weight: 3,
        hull: [0.260, 0.125, 0.095],
    },
    ArmouredLivery {
        name: "Slate green",
        weight: 3,
        hull: [0.130, 0.195, 0.180],
    },
];

/// The scheme this armoured-car seed wears, a pick from [`ARMOUR_LIVERIES`];
/// `over` wraps inside it (`render --livery 0` is Olive drab, `1` Works
/// grey). "Olive drab" is also a dune buggy raider scheme's name, in another
/// colour; nothing looks a scheme up across the lists.
pub fn armoured_livery(seed: u64, over: Option<usize>) -> &'static ArmouredLivery {
    pick(ARMOUR_LIVERIES, |l| l.weight, seed, over)
}

/// A rover's scheme (#1378): the one colour every mass of her carries.
///
/// A list of her own, as the wagon, the buggy, the cyclecar and the armoured
/// car have: a racing green or a cream rover is a coachbuilder's colour on a
/// machine with a solar panel, and the roadster parked beside her wears
/// exactly those. These are the finishes an instrument platform is really
/// built in - thermal white, foil gold, bare titanium, basalt, regolith and a
/// deep violet - and every one of them is a value a seeded accent reads
/// against, which it has to be, because her accent lights a band round her
/// deck and six rims.
#[derive(Clone, Copy, Debug)]
pub struct RoverLivery {
    /// See [`BoatLivery::name`].
    pub name: &'static str,
    weight: u32,
    /// Every mass of her: the deck, the mast, the camera head's housing, the
    /// dish, the carapace's shell, the monolith's slab, its obelisk and its
    /// stele.
    body: [f32; 3],
}

/// The rover's six schemes (#1378), as the owner agreed them on the phase-1
/// renders, weights and order: the pick walks the table in this order.
pub const ROVER_LIVERIES: &[RoverLivery] = &[
    RoverLivery {
        name: "Thermal white",
        weight: 5,
        body: [0.860, 0.860, 0.830],
    },
    RoverLivery {
        name: "Foil gold",
        weight: 4,
        body: [0.640, 0.480, 0.170],
    },
    RoverLivery {
        name: "Titanium",
        weight: 4,
        body: [0.400, 0.410, 0.430],
    },
    RoverLivery {
        name: "Basalt",
        weight: 4,
        body: [0.085, 0.088, 0.100],
    },
    RoverLivery {
        name: "Regolith",
        weight: 3,
        body: [0.420, 0.300, 0.220],
    },
    RoverLivery {
        name: "Deep violet",
        weight: 3,
        body: [0.170, 0.080, 0.300],
    },
];

/// The scheme this rover seed wears, a pick from [`ROVER_LIVERIES`]; `over`
/// wraps inside it (`render --livery 0` is Thermal white, `1` Foil gold).
pub fn rover_livery(seed: u64, over: Option<usize>) -> &'static RoverLivery {
    pick(ROVER_LIVERIES, |l| l.weight, seed, over)
}

/// A junk's scheme (#1371): her HULL and her SAILS.
///
/// She has a list of her own, as the wagon, the buggy and the cyclecar do,
/// and the scheme owns both her big masses. The sails first: they are the
/// largest mass any sailing craft carries at 12 m, and a junk's cloth is
/// tanbark, ochre, vermilion or black, never a yacht's white - under the
/// heritage boat schemes six of seven junks wore white or cream sails and
/// drifted toward the pale-sailed sloops. And the hull, whose transom is the
/// face the chase camera looks at: the heritage colours on her hull alone,
/// under one tanbark, drew seven junks that were one junk.
#[derive(Clone, Copy, Debug)]
pub struct JunkLivery {
    /// See [`BoatLivery::name`].
    pub name: &'static str,
    weight: u32,
    /// The shell, the transom and the break's bulkhead.
    hull: [f32; 3],
    /// Oiled teak under a gloss (brightwork) rather than paint - and so not
    /// floored, as a varnished hull is not.
    bright: bool,
    /// Every sail she sets.
    sail: [f32; 3],
}

/// A junk's black lacquer, and the colours her sails are dyed.
const LACQUER_BLACK: [f32; 3] = [0.055, 0.050, 0.050];
const TANBARK: [f32; 3] = [0.50, 0.24, 0.12];

/// The junk's seven schemes (#1371), as the owner agreed them on the
/// phase-1 renders, weights and order: the pick walks the table in this
/// order.
pub const JUNK_LIVERIES: &[JunkLivery] = &[
    JunkLivery {
        name: "Black and tanbark",
        weight: 5,
        hull: LACQUER_BLACK,
        bright: false,
        sail: TANBARK,
    },
    JunkLivery {
        name: "Oiled teak",
        weight: 4,
        hull: [0.42, 0.25, 0.12],
        bright: true,
        sail: TANBARK,
    },
    JunkLivery {
        name: "Oxblood and ochre",
        weight: 4,
        hull: [0.30, 0.065, 0.055],
        bright: false,
        sail: [0.70, 0.50, 0.22],
    },
    JunkLivery {
        name: "Vermilion sails",
        weight: 4,
        hull: LACQUER_BLACK,
        bright: false,
        sail: [0.72, 0.17, 0.07],
    },
    JunkLivery {
        name: "Vermilion lacquer",
        weight: 3,
        hull: [0.70, 0.15, 0.06],
        bright: false,
        sail: [0.12, 0.11, 0.10],
    },
    JunkLivery {
        name: "Jade",
        weight: 3,
        hull: [0.14, 0.32, 0.26],
        bright: false,
        sail: TANBARK,
    },
    JunkLivery {
        name: "Ghost",
        weight: 2,
        hull: LACQUER_BLACK,
        bright: false,
        sail: [0.84, 0.80, 0.68],
    },
];

/// The scheme this junk seed wears, a pick from [`JUNK_LIVERIES`]; `over`
/// wraps inside it (`render --livery 0` is Black and tanbark, `1` Oiled
/// teak).
pub fn junk_livery(seed: u64, over: Option<usize>) -> &'static JunkLivery {
    pick(JUNK_LIVERIES, |l| l.weight, seed, over)
}

/// A longship's scheme (#1369): her HULL and her SAIL.
///
/// She has a list of her own, as the junk, the wagon, the buggy and the
/// cyclecar do, and for the junk's reason: her ONE square sail is the
/// largest mass she carries at 12 m, and four of the first four heritage
/// boat schemes give a white or a cream sail - four longships that were one
/// longship on the render. What a Norse, a medieval, a fae and a classical
/// hull and sail actually were is a short list, and this is it.
#[derive(Clone, Copy, Debug)]
pub struct LongshipLivery {
    /// See [`BoatLivery::name`].
    pub name: &'static str,
    weight: u32,
    /// The shell, her end plugs and - shaded - her strakes and her interior.
    hull: [f32; 3],
    /// Oiled oak under a gloss (brightwork) rather than paint - and so not
    /// floored, as a varnished hull is not.
    bright: bool,
    /// Her square sail, and her stripes are the accent held clear of it.
    sail: [f32; 3],
}

/// The hulls a longship was, and the colours her wool sail was dyed.
const PITCH_BLACK: [f32; 3] = [0.060, 0.058, 0.055];
const OILED_OAK: [f32; 3] = [0.38, 0.24, 0.115];
const TARRED_PINE: [f32; 3] = [0.20, 0.155, 0.105];
const LIMEWASH: [f32; 3] = [0.78, 0.74, 0.66];
const OXBLOOD_HULL: [f32; 3] = [0.30, 0.070, 0.055];
const SEA_GREY: [f32; 3] = [0.30, 0.33, 0.34];
const WOOL_NATURAL: [f32; 3] = [0.66, 0.60, 0.48];

/// The longship's seven schemes (#1369), weights and order: the pick walks
/// the table in this order.
pub const LONGSHIP_LIVERIES: &[LongshipLivery] = &[
    LongshipLivery {
        name: "Oak and red wool",
        weight: 5,
        hull: OILED_OAK,
        bright: true,
        sail: [0.60, 0.16, 0.11],
    },
    LongshipLivery {
        name: "Tarred and ochre",
        weight: 4,
        hull: TARRED_PINE,
        bright: false,
        sail: [0.70, 0.50, 0.20],
    },
    LongshipLivery {
        name: "Black and white wool",
        weight: 4,
        hull: PITCH_BLACK,
        bright: false,
        sail: [0.82, 0.79, 0.70],
    },
    LongshipLivery {
        name: "Oxblood and natural",
        weight: 4,
        hull: OXBLOOD_HULL,
        bright: false,
        sail: WOOL_NATURAL,
    },
    LongshipLivery {
        name: "Limewashed",
        weight: 3,
        hull: LIMEWASH,
        bright: false,
        sail: [0.16, 0.26, 0.44],
    },
    LongshipLivery {
        name: "Sea grey",
        weight: 3,
        hull: SEA_GREY,
        bright: false,
        sail: WOOL_NATURAL,
    },
    LongshipLivery {
        name: "Oak and black wool",
        weight: 2,
        hull: OILED_OAK,
        bright: true,
        sail: [0.14, 0.13, 0.12],
    },
];

/// The scheme this longship seed wears, a pick from [`LONGSHIP_LIVERIES`];
/// `over` wraps inside it (`render --livery 0` is Oak and red wool, `1`
/// Tarred and ochre).
pub fn longship_livery(seed: u64, over: Option<usize>) -> &'static LongshipLivery {
    pick(LONGSHIP_LIVERIES, |l| l.weight, seed, over)
}

// ---------------------------------------------------------------------------
// Turning a scheme into surfaces
// ---------------------------------------------------------------------------

/// A seeded identity surface: self-lit where the avatar's style lights its
/// accents, ordinary paint everywhere else.
///
/// This is where "theme speaks through finish" actually happens. A neon seed's
/// coachline, its wheel centres and its masthead burgee GLOW - through
/// [`MaterialKit::accent`], whose strength is the register's own (4.5, or 8.0
/// for the bold register) and well inside the sanitiser's clamp - while a
/// medieval seed's are the same hue in flat enamel. Nothing else on the craft
/// changes.
///
/// **A boat's boot top and cove line do not come through here**, and that is
/// the one place this helper is deliberately not applied to an identity slot:
/// see the module docs. They are `paint` on every theme.
fn trim(m: &MaterialKit, color: [f32; 3]) -> SovereignMaterialSettings {
    if m.emissive_accents() {
        m.accent(color)
    } else {
        m.paint(color)
    }
}

/// Floor `c`'s value at `min_l` **on the finished surface** - so the floor
/// still holds after [`MaterialKit`] has put its wear grime through it.
///
/// The trap this closes: `MaterialKit::finish` darkens every colour it is
/// given by up to 35 %, so a guard floored at 0.11 and then handed to `paint`
/// arrives at 0.072 on a battered seed, which is back inside the tyre's own
/// value - the exact defect [`GUARD_FLOOR`] exists to prevent (#1365).
fn floor_finished(m: &MaterialKit, c: [f32; 3], min_l: f32) -> [f32; 3] {
    floor_value(c, min_l / m.value_after_grime())
}

/// Hold `c` at least `delta` clear of `ref_l` in value, moving it to the side
/// that has room: UP off a dark mass, DOWN off a pale one.
///
/// [`super::colour::ensure_delta`] is the general helper and it keeps whichever
/// side the colour already sits on, which is right for two surfaces of one
/// garment. It is wrong for a trim line on a livery, and the failure is
/// specific: on a MID-dark mass - a mahogany hull at 0.20, a maroon car at
/// 0.11 - a dark accent is pushed further down, hits `ensure_delta`'s own 0.04
/// floor, and the boot stripe paints itself black on a dark hull. Choosing the
/// side by the mass rather than by the accent is what makes the trim legible
/// on every scheme in the list rather than on most of them (#1365).
fn clear_of(c: [f32; 3], ref_l: f32, delta: f32) -> [f32; 3] {
    if (luma(c) - ref_l).abs() >= delta {
        return c;
    }
    if ref_l < 0.45 {
        to_value(c, (ref_l + delta).min(0.92))
    } else {
        to_value(c, (ref_l - delta).max(0.08))
    }
}

/// Hold an identity colour `delta` clear of the value `ref_l` it is seen
/// against AND at `min_l` or over on the finished surface, on whichever side
/// has room for both (#1374).
///
/// [`clear_of`] and then [`floor_finished`] can undo each other: on a worn
/// kit the floor lifts a colour cleared DOWN off a mid-bright mass straight
/// back into that mass's value, and floors a dark colour on a mid-dark mass
/// up into it. Flooring first and choosing the side by the room left above
/// the floor keeps both promises.
fn clear_over_floor(m: &MaterialKit, c: [f32; 3], ref_l: f32, delta: f32, min_l: f32) -> [f32; 3] {
    let floor = min_l / m.value_after_grime();
    let c = floor_value(c, floor);
    if (luma(c) - ref_l).abs() >= delta {
        return c;
    }
    if ref_l >= 0.45 && ref_l - delta >= floor {
        to_value(c, (ref_l - delta).max(0.08))
    } else {
        to_value(c, (ref_l + delta).min(0.92))
    }
}

/// The surfaces a boat is painted in: her scheme on the big masses, the
/// seeded accent on three identity slots, and the colours the rest of it
/// simply is.
pub(crate) struct BoatColours {
    pub(crate) topsides: SovereignMaterialSettings,
    /// **Identity.** The boot top at the waterline and the cove line under the
    /// sheer, in the seeded accent held clear of the topsides between them.
    ///
    /// Always `paint`, never [`trim`]: these two run the whole length of the
    /// hull, and lit they are the loudest thing on the boat (#1365 phase 2).
    /// The glow a luminous style is owed goes to [`Self::pennant`].
    pub(crate) boot: SovereignMaterialSettings,
    pub(crate) antifoul: SovereignMaterialSettings,
    /// Laid deck, rub rail and spars.
    pub(crate) timber: SovereignMaterialSettings,
    /// Toe rail, coaming, hatch, tiller.
    pub(crate) brightwork: SovereignMaterialSettings,
    /// The mainsail.
    pub(crate) canvas: SovereignMaterialSettings,
    /// **Identity.** The jib, dyed in the seeded accent - the one sail small
    /// enough to be trim rather than mass, and the way a boat is told apart
    /// across a bay.
    pub(crate) jib: SovereignMaterialSettings,
    /// **Identity.** The masthead burgee - and the boat's one lit slot, so a
    /// luminous style still speaks on her without painting a neon pinstripe
    /// down her sheer. It is the highest and smallest thing she carries.
    pub(crate) pennant: SovereignMaterialSettings,
    /// Standing rigging: tarred wire, not an accent. It used to be the boot
    /// stripe's colour, which spent the seed's identity on three shrouds
    /// nobody can see at play distance.
    pub(crate) rigging: SovereignMaterialSettings,
    /// The cockpit well and the companionway - the inside of the boat, which
    /// is a shadow whatever she is painted.
    pub(crate) interior: SovereignMaterialSettings,
    pub(crate) lead: SovereignMaterialSettings,
    pub(crate) window: SovereignMaterialSettings,
    /// The tender stowed bottom-up on an Ornate boat's foredeck (#1366).
    ///
    /// Paint, and off the scheme's ANTIFOUL rather than its topsides: an
    /// upturned tender shows her bottom, and drawn in the hull's own white she
    /// is a bar of soap on the foredeck. Held clear of the laid deck she lies
    /// on, which is the one surface she has to read against.
    pub(crate) dinghy: SovereignMaterialSettings,
    /// The boom tent over an Adorned boat's cockpit - cut from sailcloth, and
    /// taken a step toward the deck's warmth so it does not merge with the
    /// mainsail standing over it.
    pub(crate) awning: SovereignMaterialSettings,
    /// The tarp lashed over a worn boat's coachroof: the scheme's canvas
    /// taken well down in value, and held clear of the topsides it covers. In
    /// the canvas itself a tarp on a white boat is invisible (#1366).
    pub(crate) tarp: SovereignMaterialSettings,
}

/// How far the tender is held from the deck under her, and the tarp from the
/// coachroof under it. A mass rather than a line, so less than a boot stripe
/// needs, like a wheel centre.
const DINGHY_DELTA: f32 = 0.18;
const TARP_DELTA: f32 = 0.20;

/// The tarp's value as a fraction of the scheme's canvas - a weathered cloth
/// under a salt crust, not a clean sail.
const TARP_VALUE: f32 = 0.42;

/// How far a boom tent's cloth is mixed toward the scheme's deck.
const AWNING_WARMTH: f32 = 0.15;

/// Ballast lead, and the tarred wire a gaff boat's shrouds are set up with.
const LEAD: [f32; 3] = [0.30, 0.31, 0.33];
const TARRED: [f32; 3] = [0.13, 0.13, 0.14];

pub(crate) fn boat_colours(ctx: &PartCtx) -> BoatColours {
    let p = &ctx.palette;
    let m = &ctx.materials;
    let l = boat_livery(ctx.seed, ctx.livery);

    let topsides = floor_finished(m, l.topsides, BOAT_MASS_FLOOR);
    // The boot stripe is a hand's width of paint at the waterline and the
    // first place a seed's own colour lands; it only exists if it separates
    // from the hull above it, on a white scheme and a black one alike.
    let boot = clear_of(p.primary_accent, luma(topsides), BOOT_DELTA);
    BoatColours {
        topsides: if l.varnished_hull {
            m.brightwork(topsides)
        } else {
            m.paint(topsides)
        },
        // Paint on every theme, luminous or not - see [`BoatColours::boot`].
        boot: m.paint(boot),
        // Antifouling is antifouling - the scheme's own - with a little of the
        // seed's secondary mixed through so two bottoms are not identical.
        antifoul: m.antifoul(mix(l.antifoul, shade(p.secondary_accent, 0.5), 0.18)),
        timber: m.timber(l.deck),
        brightwork: m.brightwork(l.varnish),
        canvas: m.canvas(l.canvas),
        // A dyed sail, not a painted one: the accent is pulled most of the way
        // back toward the scheme's own canvas, so the jib reads as cloth that
        // took a dye rather than as a triangle of the seed's raw accent.
        jib: m.canvas(clear_of(
            mix(l.canvas, p.primary_accent, 0.62),
            luma(l.canvas),
            0.10,
        )),
        pennant: trim(m, boot),
        rigging: m.paint(TARRED),
        interior: m.paint(shade(topsides, 0.45)),
        lead: m.metal(LEAD),
        window: window_material(window_light(p.tertiary_accent)),
        dinghy: m.paint(clear_of(l.antifoul, luma(l.deck), DINGHY_DELTA)),
        awning: m.canvas(mix(l.canvas, l.deck, AWNING_WARMTH)),
        tarp: m.canvas(clear_of(
            to_value(l.canvas, luma(l.canvas) * TARP_VALUE),
            luma(topsides),
            TARP_DELTA,
        )),
    }
}

/// The surfaces a runabout is built in (#1372): her scheme on the big masses
/// where the variant honours one, the seeded accent on the spray rail, the
/// cove line and the ensign, and the colours the rest of her simply is.
pub(crate) struct RunaboutColours {
    /// The shell and the transom face: mahogany under varnish on a coastal
    /// launch whatever the scheme, the scheme's paint on the others.
    pub(crate) topsides: SovereignMaterialSettings,
    pub(crate) antifoul: SovereignMaterialSettings,
    /// **Identity.** The spray rail on the chine and the cove line under the
    /// sheer: the accent held clear of the hull, as PAINT on every theme -
    /// the sloop's rule for a line that runs the whole length of a hull.
    pub(crate) boot: SovereignMaterialSettings,
    /// The rub rail on the deck edge: chrome - except on the SKIMMER, where
    /// it is the accent through [`trim`] and so a lit neon outline on every
    /// neon seed. The owner's call on #1372: the one hull line on the fleet
    /// that glows, because the skimmer's lit chine did not read at 12 m and
    /// the lit rail does.
    pub(crate) rail: SovereignMaterialSettings,
    /// The laid decks: planked mahogany on the launch, the scheme's paint
    /// taken toward grey on the skimmer, pale non-skid on the catamaran.
    pub(crate) deck: SovereignMaterialSettings,
    /// The engine hatch.
    pub(crate) hatch: SovereignMaterialSettings,
    /// Bench cushions, the sunpad and the helm seat. On the launch this is
    /// where the SCHEME's colour goes (her hull is always varnished), held
    /// clear of the mahogany it sits in; on the skimmer a dark hide; on the
    /// catamaran the seed's own accent.
    pub(crate) upholstery: SovereignMaterialSettings,
    /// Windscreen frames, the stem band, hoops, posts and the flagstaff.
    pub(crate) chrome: SovereignMaterialSettings,
    /// The cockpit sole.
    pub(crate) sole: SovereignMaterialSettings,
    /// The inside of the boat: her engine bed, in shadow.
    pub(crate) interior: SovereignMaterialSettings,
    /// The surrey top and the T-top: the scheme's sailcloth.
    pub(crate) canvas: SovereignMaterialSettings,
    /// A battered boat's tarp: the sloop's rule (#1366).
    pub(crate) tarp: SovereignMaterialSettings,
    /// A worn boat's replaced foredeck panel: grey primer held clear of the
    /// deck it is let into.
    pub(crate) primer: SovereignMaterialSettings,
    /// A battered boat's fuel can.
    pub(crate) can: SovereignMaterialSettings,
    /// Propeller and rudder.
    pub(crate) bronze: SovereignMaterialSettings,
    /// The steering wheel's rim: varnished mahogany.
    pub(crate) wheel: SovereignMaterialSettings,
    /// **Identity.** The launch's ensign.
    pub(crate) flag: SovereignMaterialSettings,
    /// **Identity.** The skimmer's nozzles and her arch's lit bar: the accent
    /// through [`trim`], lit on every neon seed.
    pub(crate) glow: SovereignMaterialSettings,
    /// The skimmer's pods, fins and arch: brushed metal, held clear of her
    /// hull.
    pub(crate) pod: SovereignMaterialSettings,
    /// The catamaran's console: the scheme, held clear of her deck.
    pub(crate) console: SovereignMaterialSettings,
    /// The outboards' cowls - black - and the odd one out on a worn
    /// catamaran, a white replacement cowl on the near engine.
    pub(crate) cowl: SovereignMaterialSettings,
    pub(crate) cowl_odd: SovereignMaterialSettings,
    /// Outboard legs and the catamaran's crossbeam.
    pub(crate) leg: SovereignMaterialSettings,
}

/// A runabout's fixed colours (#1372): what these parts simply are.
const MAHOGANY: [f32; 3] = [0.34, 0.16, 0.08];
const MAHOGANY_DECK: [f32; 3] = [0.40, 0.20, 0.10];
const BRONZE: [f32; 3] = [0.55, 0.40, 0.20];
const GUNMETAL: [f32; 3] = [0.20, 0.21, 0.23];
const PALE_METAL: [f32; 3] = [0.62, 0.63, 0.66];
const NONSKID: [f32; 3] = [0.80, 0.80, 0.76];
const DARK_HIDE: [f32; 3] = [0.12, 0.12, 0.13];
const FUEL_RED: [f32; 3] = [0.62, 0.12, 0.08];
const OUTBOARD: [f32; 3] = [0.07, 0.07, 0.08];
const COWL_ODD: [f32; 3] = [0.86, 0.86, 0.84];

/// How far the launch's upholstery is held from her mahogany, the skimmer's
/// hide from her deck and the catamaran's seat from hers: a cushion is a
/// mass, and the cockpit is read by it.
const UPHOLSTERY_DELTA: f32 = 0.25;

pub(crate) fn runabout_colours(ctx: &PartCtx, variant: RunaboutVariant) -> RunaboutColours {
    let p = &ctx.palette;
    let m = &ctx.materials;
    let l = boat_livery(ctx.seed, ctx.livery);
    let scheme = floor_finished(m, l.topsides, BOAT_MASS_FLOOR);
    // The hull's own colour, which every trim and mass is held clear of.
    let hull = match variant {
        RunaboutVariant::Coastal => MAHOGANY,
        // A varnished scheme on a painted skimmer is gunmetal instead: a
        // mahogany hull with lit pods is not a thing.
        RunaboutVariant::Skimmer if l.varnished_hull => GUNMETAL,
        RunaboutVariant::Skimmer | RunaboutVariant::Catamaran => scheme,
    };
    let boot = clear_of(p.primary_accent, luma(hull), BOOT_DELTA);
    let chrome = m.brightwork(CHROME);
    let deck = match variant {
        RunaboutVariant::Coastal => MAHOGANY_DECK,
        RunaboutVariant::Skimmer => clear_of(mix(hull, [0.5, 0.5, 0.52], 0.5), luma(hull), 0.18),
        RunaboutVariant::Catamaran => clear_of(NONSKID, luma(hull), 0.28),
    };
    let (topsides, deck_m, hatch, upholstery, sole, rail) = match variant {
        RunaboutVariant::Coastal => (
            m.brightwork(MAHOGANY),
            m.timber(MAHOGANY_DECK),
            m.brightwork(MAHOGANY_DECK),
            m.leather(clear_of(scheme, luma(MAHOGANY), UPHOLSTERY_DELTA)),
            m.timber(l.deck),
            chrome.clone(),
        ),
        RunaboutVariant::Skimmer => (
            m.paint(hull),
            m.paint(deck),
            m.paint(if luma(hull) > 0.3 {
                shade(hull, 0.7)
            } else {
                mix(hull, [1.0; 3], 0.25)
            }),
            m.leather(clear_of(DARK_HIDE, luma(deck), UPHOLSTERY_DELTA)),
            m.paint(shade(hull, 0.45)),
            trim(m, boot),
        ),
        RunaboutVariant::Catamaran => (
            m.paint(hull),
            m.paint(deck),
            m.paint(deck),
            m.leather(clear_of(p.primary_accent, luma(NONSKID), UPHOLSTERY_DELTA)),
            m.paint(deck),
            chrome.clone(),
        ),
    };
    RunaboutColours {
        topsides,
        antifoul: m.antifoul(mix(l.antifoul, shade(p.secondary_accent, 0.5), 0.18)),
        boot: m.paint(boot),
        rail,
        deck: deck_m,
        hatch,
        upholstery,
        chrome,
        sole,
        interior: m.paint(shade(hull, 0.45)),
        canvas: m.canvas(l.canvas),
        tarp: m.canvas(clear_of(
            to_value(l.canvas, luma(l.canvas) * TARP_VALUE),
            luma(hull),
            TARP_DELTA,
        )),
        primer: m.paint(clear_of(PRIMER, luma(deck), PRIMER_DELTA)),
        can: m.paint(FUEL_RED),
        bronze: m.brightwork(BRONZE),
        wheel: m.brightwork(MAHOGANY),
        flag: m.paint(boot),
        glow: trim(m, boot),
        pod: m.brightwork(if luma(hull) > 0.3 {
            GUNMETAL
        } else {
            PALE_METAL
        }),
        console: m.paint(clear_of(hull, luma(NONSKID), 0.20)),
        cowl: m.paint(OUTBOARD),
        cowl_odd: m.paint(COWL_ODD),
        leg: m.paint(MACHINERY),
    }
}

/// The surfaces a working scow is built in (#1373). Her HULL is working
/// timber - tarred, or bare weathered boards on the varnished scheme - and
/// her scheme goes on her DECKHOUSE, the one mass the chase camera sees whole
/// from 22.9 degrees: its topsides colour on the walls (varnished mahogany on
/// the varnished scheme), its canvas on the barrel roof, its deck on the
/// decks. All seven schemes are honoured, on the house rather than the hull,
/// because on a flared hull the topsides roll away under the deck edge
/// (#1365) and the owner agreed the house on the phase-1 renders.
///
/// The seeded accent is spent on the identity slots: the roof trim (lit
/// through [`trim`] on a luminous style - the cyberpunk canal barge's neon
/// outline), the door, and the sweep's blade or the stern wheel's rims.
pub(crate) struct ScowColours {
    /// The shell and both end faces: paint, not the Plank - its grain runs
    /// across its boards and combs into a fringe on the flared facets (#1388).
    pub(crate) hull: SovereignMaterialSettings,
    pub(crate) transom: SovereignMaterialSettings,
    /// The fore and after decks, the scheme's deck in boards run along her.
    pub(crate) deck: SovereignMaterialSettings,
    /// The gunwale rail.
    pub(crate) rail: SovereignMaterialSettings,
    /// The keelson post in the void under the hold floor.
    pub(crate) interior: SovereignMaterialSettings,
    /// The hold floor.
    pub(crate) floor: SovereignMaterialSettings,
    /// The deckhouse walls: the scheme's topsides as boards.
    pub(crate) house: SovereignMaterialSettings,
    /// The barrel roof: the scheme's canvas.
    pub(crate) roof: SovereignMaterialSettings,
    /// **Identity.** The strip along each shoulder of the roof, through
    /// [`trim`].
    pub(crate) trim: SovereignMaterialSettings,
    /// **Identity.** The deckhouse door.
    pub(crate) door: SovereignMaterialSettings,
    pub(crate) window: SovereignMaterialSettings,
    /// The stovepipe, the sweep's crutch, the wheel's axle and hub.
    pub(crate) iron: SovereignMaterialSettings,
    /// The samson post.
    pub(crate) post: SovereignMaterialSettings,
    /// The sweep's loom and the quant pole.
    pub(crate) oar: SovereignMaterialSettings,
    /// **Identity.** The sweep's blade.
    pub(crate) blade: SovereignMaterialSettings,
    /// The stern wheel's beams.
    pub(crate) beam: SovereignMaterialSettings,
    /// **Identity.** The stern wheel's rims.
    pub(crate) wheel: SovereignMaterialSettings,
    pub(crate) paddle: SovereignMaterialSettings,
    /// Pine packing crates, two shades, and oak casks.
    pub(crate) crate_a: SovereignMaterialSettings,
    pub(crate) crate_b: SovereignMaterialSettings,
    pub(crate) cask: SovereignMaterialSettings,
    /// Hay bales, two shades.
    pub(crate) hay_a: SovereignMaterialSettings,
    pub(crate) hay_b: SovereignMaterialSettings,
    /// The scrap load's three painted drums.
    pub(crate) drums: [SovereignMaterialSettings; 3],
    /// The fire drum on the foredeck, and the embers glowing in its mouth.
    pub(crate) drum_fire: SovereignMaterialSettings,
    pub(crate) ember: SovereignMaterialSettings,
    pub(crate) tyre: SovereignMaterialSettings,
    /// Sheet iron, a rusted panel and a car's bonnet on the scrap heap.
    pub(crate) sheet: SovereignMaterialSettings,
    pub(crate) rust: SovereignMaterialSettings,
    pub(crate) car: SovereignMaterialSettings,
    /// A battered scow's tarp over her load: the sloop's rule (#1366).
    pub(crate) tarp: SovereignMaterialSettings,
    /// A worn scow's re-laid foredeck boards, pale and new.
    pub(crate) patch: SovereignMaterialSettings,
    /// A battered scow's tarred felt patch on the roof.
    pub(crate) felt: SovereignMaterialSettings,
}

/// A scow's fixed colours (#1373): what these parts simply are.
const BARE_TIMBER: [f32; 3] = [0.47, 0.43, 0.37];
const TAR: [f32; 3] = [0.07, 0.065, 0.06];
const PINE: [f32; 3] = [0.72, 0.58, 0.38];
const CASK: [f32; 3] = [0.52, 0.34, 0.18];
const STRAW: [f32; 3] = [0.80, 0.68, 0.38];
const STRAW_B: [f32; 3] = [0.70, 0.58, 0.30];
const RUST: [f32; 3] = [0.42, 0.20, 0.10];
const DRUM_BLUE: [f32; 3] = [0.18, 0.26, 0.36];
const DRUM_RED: [f32; 3] = [0.46, 0.14, 0.10];
const DRUM_YELLOW: [f32; 3] = [0.60, 0.48, 0.16];
const SHEET_IRON: [f32; 3] = [0.40, 0.40, 0.42];
const CAR_PAINT: [f32; 3] = [0.30, 0.44, 0.46];
const FELT: [f32; 3] = [0.10, 0.10, 0.11];
const EMBER: [f32; 3] = [1.0, 0.42, 0.10];

/// How far the scow's roof is held from the walls under it, and her
/// identity slots from what they lie on.
const ROOF_DELTA: f32 = 0.25;
const SCOW_TRIM_DELTA: f32 = 0.30;

pub(crate) fn scow_colours(ctx: &PartCtx) -> ScowColours {
    let p = &ctx.palette;
    let m = &ctx.materials;
    let l = boat_livery(ctx.seed, ctx.livery);
    let hull = if l.varnished_hull { BARE_TIMBER } else { TAR };
    let house = if l.varnished_hull {
        MAHOGANY
    } else {
        floor_finished(m, l.topsides, BOAT_MASS_FLOOR)
    };
    let roof = clear_of(l.canvas, luma(house), ROOF_DELTA);
    let accent = p.primary_accent;
    ScowColours {
        hull: m.paint(hull),
        transom: m.paint(hull),
        deck: boards(m, l.deck),
        rail: boards(m, shade(l.varnish, 0.55)),
        interior: m.paint(shade(hull, 0.45)),
        floor: boards(m, shade(l.deck, 0.55)),
        house: if l.varnished_hull {
            m.brightwork(house)
        } else {
            m.timber(house)
        },
        roof: m.canvas(roof),
        trim: trim(m, clear_of(accent, luma(roof), SCOW_TRIM_DELTA)),
        door: m.paint(clear_of(accent, luma(house), SCOW_TRIM_DELTA)),
        window: window_material(window_light(p.tertiary_accent)),
        iron: m.paint(MACHINERY),
        post: m.timber(shade(l.varnish, 0.7)),
        oar: m.timber(l.varnish),
        blade: m.paint(clear_of(accent, luma(TAR), SCOW_TRIM_DELTA)),
        beam: m.timber(shade(l.varnish, 0.6)),
        wheel: m.paint(clear_of(accent, luma(MACHINERY), SCOW_TRIM_DELTA)),
        paddle: m.timber(l.deck),
        crate_a: m.timber(PINE),
        crate_b: m.timber(shade(PINE, 0.82)),
        cask: m.timber(CASK),
        hay_a: m.canvas(STRAW),
        hay_b: m.canvas(STRAW_B),
        drums: [m.paint(DRUM_BLUE), m.paint(DRUM_RED), m.paint(DRUM_YELLOW)],
        drum_fire: m.paint(RUST),
        ember: window_material(window_light(EMBER)),
        tyre: m.rubber(TYRE),
        sheet: m.metal(SHEET_IRON),
        rust: m.paint(RUST),
        car: m.paint(CAR_PAINT),
        tarp: m.canvas(clear_of(
            to_value(l.canvas, luma(l.canvas) * TARP_VALUE),
            luma(hull),
            TARP_DELTA,
        )),
        patch: boards(m, PATCH_BOARD),
        felt: m.paint(FELT),
    }
}

/// The surfaces a steam tug is built in (#1370). Her scheme goes on her
/// FUNNEL, as a company's colours do: the scheme's topsides colour on the
/// stack under a black top, with the seeded accent as the band round it -
/// over a black steel hull on the scheme's antifoul, a superstructure in the
/// scheme's canvas (white, cream, or tanbark on the black scheme) and the
/// scheme's deck on her deck. The varnished scheme is a varnished mahogany
/// superstructure under a buff funnel. The owner agreed this on the phase-1
/// renders: from the chase camera the pale superstructure always reads
/// against the black hull and the funnel carries the scheme, and on the
/// three dark schemes, whose funnels are close at 12 m, the band carries
/// the seed.
///
/// The seeded accent is spent on ONE identity slot, the funnel band, through
/// [`trim`] - a luminous style would light it, though no tug theme is one.
pub(crate) struct TugColours {
    /// The shell, the stem and the counter's end face: black steel.
    pub(crate) hull: SovereignMaterialSettings,
    pub(crate) transom: SovereignMaterialSettings,
    /// Below the waterline, the forefoot and the keel with it: the scheme's
    /// antifoul.
    pub(crate) antifoul: SovereignMaterialSettings,
    /// The sunk deck, the scheme's deck in boards run along her.
    pub(crate) deck: SovereignMaterialSettings,
    /// The capping rail along the bulwark's top.
    pub(crate) rail: SovereignMaterialSettings,
    /// The keelson post in the void under the deck.
    pub(crate) interior: SovereignMaterialSettings,
    /// The engine casing and the wheelhouse: the scheme's canvas, or
    /// varnished mahogany on the varnished scheme.
    pub(crate) house: SovereignMaterialSettings,
    /// The casing top and the wheelhouse roof: the canvas taken down and
    /// held clear of the walls under it.
    pub(crate) roof: SovereignMaterialSettings,
    pub(crate) window: SovereignMaterialSettings,
    /// The funnel: the scheme's topsides colour, or buff on the varnished
    /// scheme.
    pub(crate) funnel: SovereignMaterialSettings,
    /// The funnel's sooted top, whose closed cap is its dark mouth.
    pub(crate) funnel_top: SovereignMaterialSettings,
    /// **Identity.** The band round the funnel, through [`trim`], held clear
    /// of the funnel it is painted on.
    pub(crate) band: SovereignMaterialSettings,
    /// A battered funnel, rusted through at its base.
    pub(crate) rust: SovereignMaterialSettings,
    /// The towing hook and arches, the bitts, the boat's chocks, and the
    /// derrick's lift, sling and winch.
    pub(crate) iron: SovereignMaterialSettings,
    /// The tyre fenders.
    pub(crate) tyre: SovereignMaterialSettings,
    /// The stem collar, the counter fender and the Ornate hawser: manila.
    pub(crate) rope: SovereignMaterialSettings,
    /// Propeller and rudder: the runabout's.
    pub(crate) bronze: SovereignMaterialSettings,
    /// The ship's boat an Adorned tug carries, white inside her gunwale.
    pub(crate) boat: SovereignMaterialSettings,
    /// Her canvas cover: the scheme's canvas taken down, held clear of the
    /// casing top she rides over.
    pub(crate) cover: SovereignMaterialSettings,
    /// A battered tug's boat under a weathered tarp: the sloop's rule
    /// (#1366), held clear of the superstructure.
    pub(crate) tarp: SovereignMaterialSettings,
    /// The cowl ventilators, in the funnel's colour.
    pub(crate) vent: SovereignMaterialSettings,
    /// The Ornate signal mast and its crosstree, and its lit masthead lamp.
    pub(crate) mast: SovereignMaterialSettings,
    pub(crate) lamp: SovereignMaterialSettings,
    /// A worn tug's re-laid deck boards, pale and new.
    pub(crate) patch: SovereignMaterialSettings,
    /// The crate the derrick tender has slung, pine.
    pub(crate) cargo: SovereignMaterialSettings,
    /// The derrick's post and boom: buff, held clear of the deck under them.
    pub(crate) derrick: SovereignMaterialSettings,
}

/// A tug's fixed colours (#1370): what these parts simply are.
const STEEL_BLACK: [f32; 3] = [0.050, 0.050, 0.055];
const SOOT: [f32; 3] = [0.035, 0.034, 0.034];
const MANILA: [f32; 3] = [0.66, 0.56, 0.38];
const BOAT_WHITE: [f32; 3] = [0.86, 0.86, 0.83];
const LAMP: [f32; 3] = [1.0, 0.93, 0.74];
/// A varnished tug's funnel, and every derrick.
const BUFF: [f32; 3] = [0.78, 0.62, 0.36];

/// How far the funnel band is held from the funnel, the roof and the boat's
/// cover from what they lie on, and the derrick from the deck it stands on.
const FUNNEL_BAND_DELTA: f32 = 0.30;
const TUG_ROOF_DELTA: f32 = 0.22;
const COVER_DELTA: f32 = 0.22;
const DERRICK_DELTA: f32 = 0.12;

pub(crate) fn tug_colours(ctx: &PartCtx) -> TugColours {
    let p = &ctx.palette;
    let m = &ctx.materials;
    let l = boat_livery(ctx.seed, ctx.livery);
    let funnel = if l.varnished_hull {
        BUFF
    } else {
        floor_finished(m, l.topsides, BOAT_MASS_FLOOR)
    };
    let house = if l.varnished_hull { MAHOGANY } else { l.canvas };
    let roof = clear_of(shade(l.canvas, 0.62), luma(house), TUG_ROOF_DELTA);
    TugColours {
        hull: m.paint(STEEL_BLACK),
        transom: m.paint(STEEL_BLACK),
        antifoul: m.antifoul(mix(l.antifoul, shade(p.secondary_accent, 0.5), 0.18)),
        deck: boards(m, l.deck),
        rail: boards(m, shade(l.varnish, 0.70)),
        interior: m.paint(shade(STEEL_BLACK, 0.8)),
        house: if l.varnished_hull {
            m.brightwork(house)
        } else {
            m.paint(house)
        },
        roof: m.paint(roof),
        window: window_material(window_light(p.tertiary_accent)),
        funnel: m.paint(funnel),
        funnel_top: m.paint(SOOT),
        band: trim(
            m,
            clear_of(p.primary_accent, luma(funnel), FUNNEL_BAND_DELTA),
        ),
        rust: m.paint(RUST),
        iron: m.paint(MACHINERY),
        tyre: m.rubber(TYRE),
        rope: m.canvas(MANILA),
        bronze: m.brightwork(BRONZE),
        boat: m.paint(BOAT_WHITE),
        cover: m.canvas(clear_of(shade(l.canvas, 0.78), luma(roof), COVER_DELTA)),
        tarp: m.canvas(clear_of(
            to_value(l.canvas, luma(l.canvas) * TARP_VALUE),
            luma(house),
            TARP_DELTA,
        )),
        vent: m.paint(funnel),
        mast: m.timber(l.varnish),
        lamp: window_material(window_light(LAMP)),
        patch: boards(m, PATCH_BOARD),
        cargo: m.timber(PINE),
        derrick: m.paint(clear_of(BUFF, luma(l.deck), DERRICK_DELTA)),
    }
}

/// The surfaces a land-skiff is painted in.
pub(crate) struct SkiffColours {
    pub(crate) paint: SovereignMaterialSettings,
    /// Wings and running boards - coachwork, not rubber. See [`GUARD_FLOOR`].
    pub(crate) guard: SovereignMaterialSettings,
    pub(crate) rubber: SovereignMaterialSettings,
    pub(crate) brightwork: SovereignMaterialSettings,
    /// **Identity.** Upholstery, the seed's own accent through the hide.
    pub(crate) leather: SovereignMaterialSettings,
    /// **Identity.** Wheel discs and hub caps - the painted wheel centre a
    /// racing car is told apart by.
    pub(crate) disc: SovereignMaterialSettings,
    /// **Identity.** The coachline along each flank.
    pub(crate) coachline: SovereignMaterialSettings,
    /// Axles, louvres, the radiator matrix - the dark machinery.
    pub(crate) machinery: SovereignMaterialSettings,
    pub(crate) lamp: SovereignMaterialSettings,
    pub(crate) tail_lamp: SovereignMaterialSettings,
    /// The folded hood's cloth, and the rope a jerrycan is lashed with
    /// (#1367): tan duck, held clear of the coachwork the hood lies on - on a
    /// black car a black hood is simply not there.
    pub(crate) hood: SovereignMaterialSettings,
    /// The trunk an Ornate car carries on its tail mount: a dark hide, held
    /// clear of the coachwork it rides over (#1367).
    pub(crate) trunk: SovereignMaterialSettings,
    /// The mismatched wing a worn car wears: a replacement guard in grey
    /// primer, held clear of the guards it failed to match - that is the whole
    /// point of it - and floored off the tyre like any guard, on the finished
    /// surface ([`GUARD_FLOOR`]) (#1367).
    pub(crate) primer: SovereignMaterialSettings,
    /// A battered car's jerrycan: olive drab, held clear of the dark running
    /// board it stands on (#1367).
    pub(crate) can: SovereignMaterialSettings,
}

/// The colours those things are, fixed rather than seeded and fixed rather
/// than schemed: a machine whose guards are the same hue as its tyres has no
/// guards at play distance, and neither a palette nor a livery list can
/// promise a contrast it does not know about.
const TYRE: [f32; 3] = [0.045, 0.045, 0.050];
const HIDE: [f32; 3] = [0.42, 0.24, 0.13];
const MACHINERY: [f32; 3] = [0.10, 0.10, 0.11];
const TAIL_LAMP: [f32; 3] = [0.90, 0.10, 0.08];

/// The roadster's dressing (#1367): each a colour a real car's part simply is,
/// and each held clear of the one surface it has to read against. None is
/// identity trim, so a luminous style lights none of them.
const HOOD_CLOTH: [f32; 3] = [0.50, 0.40, 0.25];
const HOOD_DELTA: f32 = 0.20;
const TRUNK_HIDE: [f32; 3] = [0.30, 0.19, 0.10];
const TRUNK_DELTA: f32 = 0.18;
/// Grey primer, held clear of the GUARDS rather than the coachwork. The two
/// agree on every dark scheme, but on a single-colour cream car the guards are
/// the cream darkened to half its value, and a primer held off the body alone
/// would sit a few hundredths from them - a wing that matches after all.
const PRIMER: [f32; 3] = [0.46, 0.46, 0.44];
const PRIMER_DELTA: f32 = 0.22;
const CAN: [f32; 3] = [0.26, 0.30, 0.13];
const CAN_DELTA: f32 = 0.16;

pub(crate) fn skiff_colours(ctx: &PartCtx) -> SkiffColours {
    let p = &ctx.palette;
    let m = &ctx.materials;
    let l = skiff_livery(ctx.seed, ctx.livery);

    // Coachwork can be genuinely dark - a racing green or a maroon is the
    // point of this machine - but never as dark as the rubber under it.
    let body = floor_finished(m, l.body, GUARD_FLOOR);
    let guard = floor_finished(m, l.wings.unwrap_or(shade(body, 0.62)), GUARD_FLOOR);
    SkiffColours {
        paint: m.paint(body),
        guard: m.paint(guard),
        rubber: m.rubber(TYRE),
        brightwork: m.brightwork(l.brightwork),
        // A little of the seed's own accent through the hide, but not enough
        // to lose what it is.
        leather: m.leather(mix(HIDE, shade(p.primary_accent, 0.8), 0.28)),
        // The wheel centre is a solid disc a hand across at play distance,
        // held clear of the guard arching over it and floored well above the
        // tyre it is set into.
        disc: trim(
            m,
            floor_finished(
                m,
                clear_of(p.primary_accent, luma(guard), DISC_DELTA),
                GUARD_FLOOR * 1.6,
            ),
        ),
        coachline: trim(m, clear_of(p.primary_accent, luma(body), COACHLINE_DELTA)),
        machinery: m.paint(MACHINERY),
        lamp: window_material(window_light(p.tertiary_accent)),
        tail_lamp: m.glow(TAIL_LAMP),
        hood: m.canvas(clear_of(HOOD_CLOTH, luma(body), HOOD_DELTA)),
        trunk: m.leather(clear_of(TRUNK_HIDE, luma(body), TRUNK_DELTA)),
        primer: m.paint(floor_finished(
            m,
            clear_of(PRIMER, luma(guard), PRIMER_DELTA),
            GUARD_FLOOR,
        )),
        can: m.paint(clear_of(CAN, luma(MACHINERY), CAN_DELTA)),
    }
}

/// The surfaces a wagon is built in (#1377): its scheme's paint on painted
/// boards, the seeded accent on its wheels, and the colours the rest simply
/// is - iron, oak, canvas, cane, a candle.
pub(crate) struct WagonColours {
    /// The hearse's body and the ox-cart's cabin and roof: smooth paint.
    pub(crate) paint: SovereignMaterialSettings,
    /// The box's side, front and tail boards, a buckboard's riser and dash:
    /// timber IN THE SCHEME'S COLOUR, so a painted box still shows its
    /// boards, with the grain turned a quarter to run along them (box UVs run
    /// it across a side board, and it read as a comb).
    pub(crate) boards: SovereignMaterialSettings,
    /// The floor, bolsters, poles, shafts and bows: dark oak.
    pub(crate) timber: SovereignMaterialSettings,
    /// Tyres, axles, stakes, springs, cap rails and lantern irons.
    pub(crate) iron: SovereignMaterialSettings,
    /// **Identity.** Felloes and spokes, in the seeded accent held clear of
    /// the box - or a forced scheme's own gear colour.
    pub(crate) wheel: SovereignMaterialSettings,
    pub(crate) brightwork: SovereignMaterialSettings,
    /// The bench's cushion and back.
    pub(crate) seat: SovereignMaterialSettings,
    /// The tilt.
    pub(crate) canvas: SovereignMaterialSettings,
    /// Every lantern, the hearse's glass and the ox-cart's side window: a
    /// candle, not the accent - a lantern is a flame, and the accent drawn
    /// there read as pink lamps and beige glass in daylight.
    pub(crate) lamp: SovereignMaterialSettings,
    /// The ox-cart's cane blinds.
    pub(crate) blind: SovereignMaterialSettings,
    /// The Ornate cask and the Worn crate.
    pub(crate) cask: SovereignMaterialSettings,
    /// The Worn sacks.
    pub(crate) sack: SovereignMaterialSettings,
    /// The pale replacement board a worn tilted cart wears on its near side.
    pub(crate) patch: SovereignMaterialSettings,
}

const DARK_OAK: [f32; 3] = [0.26, 0.18, 0.11];
const OAK: [f32; 3] = [0.46, 0.33, 0.20];
const TILT_CANVAS: [f32; 3] = [0.84, 0.80, 0.68];
const CANE: [f32; 3] = [0.72, 0.60, 0.36];
const SACKING: [f32; 3] = [0.66, 0.58, 0.42];
const PATCH_BOARD: [f32; 3] = [0.66, 0.60, 0.50];
const CANDLE: [f32; 3] = [1.0, 0.62, 0.22];

/// Timber whose grain runs along a board that box UVs would run it across.
fn boards(m: &MaterialKit, color: [f32; 3]) -> SovereignMaterialSettings {
    let mut t = m.timber(color);
    t.uv_rotation = Fp(90.0);
    t
}

pub(crate) fn wagon_colours(ctx: &PartCtx, body: WagonBody) -> WagonColours {
    let p = &ctx.palette;
    let m = &ctx.materials;
    let l = wagon_livery(ctx.seed, body, ctx.livery);
    let wheel = match l.gear {
        Some(gear) => m.paint(gear),
        // The accent held clear of the box it turns beside, and floored off
        // the iron tyre round it - the roadster's wheel-centre rule.
        None => trim(
            m,
            floor_finished(
                m,
                clear_of(p.primary_accent, luma(l.body), DISC_DELTA),
                GUARD_FLOOR * 1.6,
            ),
        ),
    };
    WagonColours {
        paint: m.paint(l.body),
        boards: boards(m, l.body),
        timber: m.timber(DARK_OAK),
        iron: m.paint(MACHINERY),
        wheel,
        brightwork: m.brightwork(l.brightwork),
        seat: m.leather(mix(HIDE, shade(p.primary_accent, 0.8), 0.28)),
        canvas: m.canvas(TILT_CANVAS),
        lamp: window_material(window_light(CANDLE)),
        blind: m.canvas(CANE),
        cask: m.timber(OAK),
        sack: m.canvas(SACKING),
        patch: boards(m, PATCH_BOARD),
    }
}

/// The surfaces a dune buggy is built in (#1374): her scheme on the tube
/// frame, the seeded accent on her rims and through her seats, and the
/// colours the rest of her simply is - a black gelcoat pod, black engine tin,
/// cast alloy, chrome, red coilovers. None of her themes is luminous, so the
/// one identity slot [`trim`] could light never glows.
pub(crate) struct BuggyColours {
    /// The tube frame, the suspension arms and the spare's bracket: the
    /// scheme, floored off the tyres like any coachwork ([`GUARD_FLOOR`]).
    pub(crate) frame: SovereignMaterialSettings,
    /// The seat pod: dark gelcoat, floored off the tyres too.
    pub(crate) pod: SovereignMaterialSettings,
    /// **Identity.** The two buckets - the roadster's hide with the seed's
    /// accent through it.
    pub(crate) seat: SovereignMaterialSettings,
    /// The engine tin: the cylinder banks, the fan shroud, and the whip.
    pub(crate) tin: SovereignMaterialSettings,
    /// The cast case, the transaxle, the backbone and the axle shafts.
    pub(crate) alloy: SovereignMaterialSettings,
    /// The air cleaner, the headers and the stinger, the lamp shells and the
    /// light bar's cans: chrome on every scheme.
    pub(crate) bright: SovereignMaterialSettings,
    pub(crate) tyre: SovereignMaterialSettings,
    /// **Identity.** The rims: the accent held clear of the frame and floored
    /// well above the tyre it is set into - floored FIRST, so on a worn kit
    /// the floor cannot lift it back into the frame's value
    /// ([`clear_over_floor`]; the wagon's wheel still clears first, #1389).
    pub(crate) rim: SovereignMaterialSettings,
    /// A worn buggy's mismatched near-rear rim: bare steel off another
    /// buggy, held well clear of the seed's own rims - up off a dark rim and
    /// down off a pale one.
    pub(crate) odd_rim: SovereignMaterialSettings,
    /// The coilovers.
    pub(crate) spring: SovereignMaterialSettings,
    pub(crate) lamp: SovereignMaterialSettings,
    pub(crate) tail_lamp: SovereignMaterialSettings,
    /// The beach buggy's canopy: white bands, and the scheme's colour
    /// between them - no texture draws a stripe.
    pub(crate) canvas: SovereignMaterialSettings,
    pub(crate) stripe: SovereignMaterialSettings,
    /// The dune whip's pennant: safety orange, what a dune flag really is.
    pub(crate) pennant: SovereignMaterialSettings,
    pub(crate) whip: SovereignMaterialSettings,
    /// The raider's jerrycans, olive and red.
    pub(crate) can: SovereignMaterialSettings,
    pub(crate) can_red: SovereignMaterialSettings,
    /// A battered buggy's exhaust and the raider's nose plate, held clear of
    /// the frame they are seen against.
    pub(crate) rust: SovereignMaterialSettings,
    /// A battered buggy's replacement fan shroud.
    pub(crate) primer: SovereignMaterialSettings,
    /// The silver tape across a battered buggy's near seat.
    pub(crate) tape: SovereignMaterialSettings,
}

/// A buggy's fixed colours (#1374): what these parts simply are.
const POD_DARK: [f32; 3] = [0.13, 0.13, 0.14];
const TIN: [f32; 3] = [0.075, 0.075, 0.080];
const ALLOY: [f32; 3] = [0.52, 0.52, 0.50];
const SPRING: [f32; 3] = [0.80, 0.14, 0.08];
const CANVAS_WHITE: [f32; 3] = [0.90, 0.89, 0.84];
const STEEL: [f32; 3] = [0.58, 0.58, 0.60];
const SAFETY: [f32; 3] = [1.00, 0.36, 0.04];
const TAPE: [f32; 3] = [0.70, 0.71, 0.72];

/// How far the odd rim is held from the seed's own rims, and a rusted pipe
/// or plate from the frame: on a Rust red raider (and a Candy red rail)
/// rust at its own value merged with the frame.
const ODD_RIM_DELTA: f32 = 0.34;
const BUGGY_RUST_DELTA: f32 = 0.12;

pub(crate) fn buggy_colours(ctx: &PartCtx, variant: BuggyVariant) -> BuggyColours {
    let p = &ctx.palette;
    let m = &ctx.materials;
    let l = buggy_livery(ctx.seed, variant, ctx.livery);
    // A frame can be genuinely dark - an olive drab raider - but never as
    // dark as the tyres it stands on, and nor can the pod.
    let frame = floor_finished(m, l.frame, GUARD_FLOOR);
    let rim = clear_over_floor(
        m,
        p.primary_accent,
        luma(frame),
        DISC_DELTA,
        GUARD_FLOOR * 1.6,
    );
    BuggyColours {
        frame: m.paint(frame),
        pod: m.paint(floor_finished(m, POD_DARK, GUARD_FLOOR)),
        seat: m.leather(mix(HIDE, shade(p.primary_accent, 0.8), 0.28)),
        tin: m.paint(TIN),
        alloy: m.paint(ALLOY),
        bright: m.brightwork(CHROME),
        tyre: m.rubber(TYRE),
        rim: trim(m, rim),
        odd_rim: m.paint(floor_finished(
            m,
            clear_of(STEEL, luma(rim), ODD_RIM_DELTA),
            GUARD_FLOOR,
        )),
        spring: m.paint(SPRING),
        lamp: window_material(window_light(p.tertiary_accent)),
        tail_lamp: m.glow(TAIL_LAMP),
        canvas: m.canvas(CANVAS_WHITE),
        stripe: m.canvas(frame),
        pennant: m.paint(SAFETY),
        whip: m.paint(TIN),
        can: m.paint(CAN),
        can_red: m.paint(FUEL_RED),
        rust: m.paint(clear_of(RUST, luma(frame), BUGGY_RUST_DELTA)),
        primer: m.paint(floor_finished(m, PRIMER, GUARD_FLOOR)),
        tape: m.paint(TAPE),
    }
}

/// The surfaces a cyclecar is built in (#1376): her scheme on the pod, the
/// seeded accent on her rims and along her flanks, and the colours the rest
/// of her simply is.
///
/// What glows is decided here and nowhere else: the strip and the rims are
/// the accent through [`trim`], lit on a luminous kit and paint on a flat
/// one; the window band and the headlamps are lit on every theme, as every
/// lamp and window band is; the tail light glows; nothing else does.
pub(crate) struct CyclecarColours {
    /// The pod (its upper half on a two-tone), the fin, the cycle wings and
    /// the lamp shells: the scheme, floored off the tyres like any coachwork
    /// ([`GUARD_FLOOR`]).
    pub(crate) body: SovereignMaterialSettings,
    /// A two-tone's lower half-pod and the spat, floored likewise; `None` on
    /// a one-colour scheme, where the spat is `body`.
    pub(crate) lower: Option<SovereignMaterialSettings>,
    /// **Identity.** The accent strip along each flank: the coachline's own
    /// rule, held clear of the pod it lies on.
    pub(crate) trim: SovereignMaterialSettings,
    /// **Identity.** The rims, held clear of the tyre they are set into.
    pub(crate) rim: SovereignMaterialSettings,
    /// A worn cyclecar's near-front rim: bare steel, paint on every kit - so
    /// on a lit kit it reads as a dead rim.
    pub(crate) odd_rim: SovereignMaterialSettings,
    pub(crate) tyre: SovereignMaterialSettings,
    /// The stub axles, the fork, the cycle wings' stays and the hidden hub.
    pub(crate) arm: SovereignMaterialSettings,
    /// The headlamps' lenses and the window band: the tertiary's light.
    pub(crate) lamp: SovereignMaterialSettings,
    pub(crate) glass: SovereignMaterialSettings,
    pub(crate) tail_lamp: SovereignMaterialSettings,
    /// A battered cyclecar's patch on her tail: grey primer held clear of
    /// the pod, and floored off the tyres like any coachwork.
    pub(crate) primer: SovereignMaterialSettings,
}

/// How far the rims are held from the tyre's value, and the bare steel of a
/// worn cyclecar's odd rim.
const CYCLECAR_RIM_DELTA: f32 = 0.30;
const CYCLECAR_ODD_RIM: [f32; 3] = [0.58, 0.58, 0.60];

pub(crate) fn cyclecar_colours(ctx: &PartCtx) -> CyclecarColours {
    let p = &ctx.palette;
    let m = &ctx.materials;
    let l = cyclecar_livery(ctx.seed, ctx.livery);
    // An obsidian pod is the point of her, but never as dark as the tyres.
    let body = floor_finished(m, l.body, GUARD_FLOOR);
    // Both identity slots are cleared UP off a dark reference or off the
    // pod, and neither is floored after clearing: the clear-then-floor order
    // that undid the buggy's rims (#1374, #1389) never arises.
    let glass = window_material(window_light(p.tertiary_accent));
    CyclecarColours {
        body: m.paint(body),
        lower: l.lower.map(|c| m.paint(floor_finished(m, c, GUARD_FLOOR))),
        trim: trim(m, clear_of(p.primary_accent, luma(body), COACHLINE_DELTA)),
        rim: trim(
            m,
            clear_of(p.primary_accent, luma(TYRE), CYCLECAR_RIM_DELTA),
        ),
        odd_rim: m.paint(floor_finished(m, CYCLECAR_ODD_RIM, GUARD_FLOOR)),
        tyre: m.rubber(TYRE),
        arm: m.paint(MACHINERY),
        lamp: glass.clone(),
        glass,
        tail_lamp: m.glow(TAIL_LAMP),
        primer: m.paint(floor_finished(
            m,
            clear_of(PRIMER, luma(body), PRIMER_DELTA),
            GUARD_FLOOR,
        )),
    }
}

/// The surfaces an armoured car is built in (#1375): her scheme on every
/// plate, the seeded accent on her unit flash and her turret band, and the
/// colours the rest of her simply is - steel rims, olive stowage, canvas, a
/// raider's bolted slabs.
///
/// **Her wheel centres are machinery, not identity.** Painted hubs on a
/// military machine read as a toy at 12 m (the buggy's lesson about brights,
/// turned round), so the rims are steel held clear of the tyre they are set
/// into and the accent goes on the flash and the band instead. Both of those
/// go through [`trim`], so the day a luminous theme picks her they light;
/// none of her three does today, so they are paint.
///
/// What is lit: her vision slits and her lamp lenses, which are the one slot
/// (`window_material(window_light(tertiary))`), and her tail lamps. Nothing
/// else.
pub(crate) struct ArmouredColours {
    /// Every plate: the belly tub, the rear deck, the fighting compartment,
    /// the glacis, the arches, the turret, the cupola and the lamp shells.
    /// The scheme, floored off the tyres like any coachwork
    /// ([`GUARD_FLOOR`]).
    pub(crate) hull: SovereignMaterialSettings,
    /// **Identity.** The unit flash on each flank and the band round the
    /// turret's base: the coachline's own rule, held clear of the plate they
    /// lie on.
    pub(crate) flash: SovereignMaterialSettings,
    /// The wheel centres: machinery steel, held clear of the tyre.
    pub(crate) rim: SovereignMaterialSettings,
    /// A worn machine's near-front rim: bare steel off another wheel.
    pub(crate) odd_rim: SovereignMaterialSettings,
    pub(crate) tyre: SovereignMaterialSettings,
    /// The axle beams, the exhaust pipe, the stern rails, the tow cable, the
    /// lamp guards and the hidden hub.
    pub(crate) arm: SovereignMaterialSettings,
    /// The turret's hatch and the cupola's lid: the hull shaded, held clear
    /// of it so the lid reads against the roof it lies on.
    pub(crate) hatch: SovereignMaterialSettings,
    /// The headlamps' lenses and every vision slit: the tertiary's light.
    pub(crate) lamp: SovereignMaterialSettings,
    pub(crate) glass: SovereignMaterialSettings,
    pub(crate) tail_lamp: SovereignMaterialSettings,
    /// The stowage bins, and the raider's pile of scavenged kit: olive.
    pub(crate) bin: SovereignMaterialSettings,
    /// The bedroll lashed along the near bin: canvas.
    pub(crate) roll: SovereignMaterialSettings,
    /// The jerrycans in her rack.
    pub(crate) can: SovereignMaterialSettings,
    /// The raider's applique slabs: bare bolted steel, held clear of the
    /// plate they are bolted to and floored off the tyres.
    pub(crate) plate: SovereignMaterialSettings,
    /// A battered machine's patched panel on the rear deck.
    pub(crate) primer: SovereignMaterialSettings,
}

/// The armoured car's fixed colours (#1375): what these parts simply are, and
/// how far each is held from the one surface it has to read against.
const ARMOUR_RIM: [f32; 3] = [0.24, 0.245, 0.25];
const ARMOUR_RIM_DELTA: f32 = 0.30;
const BIN_OLIVE: [f32; 3] = [0.26, 0.28, 0.16];
const BIN_DELTA: f32 = 0.16;
const ROLL_CANVAS: [f32; 3] = [0.52, 0.47, 0.34];
const ROLL_DELTA: f32 = 0.18;
const APPLIQUE_STEEL: [f32; 3] = [0.30, 0.28, 0.26];
const APPLIQUE_DELTA: f32 = 0.18;
const HATCH_DELTA: f32 = 0.10;
/// How far the unit flash and the turret band are held from the plate they
/// are painted on - a coachline's delta, a little wider because a flash is
/// an area rather than a line.
const FLASH_DELTA: f32 = 0.26;

pub(crate) fn armoured_colours(ctx: &PartCtx) -> ArmouredColours {
    let p = &ctx.palette;
    let m = &ctx.materials;
    let l = armoured_livery(ctx.seed, ctx.livery);
    // A midnight blue or an olive drab plate is the point of her, but never
    // as dark as the tyres she stands on.
    let hull = floor_finished(m, l.hull, GUARD_FLOOR);
    let body = luma(hull);
    let lamp = window_material(window_light(p.tertiary_accent));
    ArmouredColours {
        hull: m.paint(hull),
        flash: trim(m, clear_of(p.primary_accent, body, FLASH_DELTA)),
        rim: m.paint(clear_of(ARMOUR_RIM, luma(TYRE), ARMOUR_RIM_DELTA)),
        odd_rim: m.paint(floor_finished(m, STEEL, GUARD_FLOOR)),
        tyre: m.rubber(TYRE),
        arm: m.paint(MACHINERY),
        hatch: m.paint(clear_of(shade(hull, 0.72), body, HATCH_DELTA)),
        lamp: lamp.clone(),
        glass: lamp,
        tail_lamp: m.glow(TAIL_LAMP),
        bin: m.paint(clear_of(BIN_OLIVE, body, BIN_DELTA)),
        roll: m.canvas(clear_of(ROLL_CANVAS, body, ROLL_DELTA)),
        can: m.paint(clear_of(CAN, luma(MACHINERY), CAN_DELTA)),
        plate: m.paint(floor_finished(
            m,
            clear_of(APPLIQUE_STEEL, body, APPLIQUE_DELTA),
            GUARD_FLOOR,
        )),
        primer: m.paint(floor_finished(
            m,
            clear_of(PRIMER, body, PRIMER_DELTA),
            GUARD_FLOOR,
        )),
    }
}

/// The surfaces a rover is built in (#1378): her scheme on every mass, the
/// seeded accent on the band round her deck and on all six rims, and the
/// colours the rest of her simply is - a dark running deck, machinery grey
/// arms, deep blue-black solar cells, bare alloy.
///
/// What glows is decided here and nowhere else: the deck band (the
/// carapace's dorsal ridge, the monolith's three seams) and the six rims are
/// the accent through [`trim`]; the camera head's pane is the tertiary's
/// window light, lit on every theme as every lamp and window band in the
/// fleet is; the antenna whip's beacon glows. Nothing else does - the panel
/// frame lit as a fourth slot was drawn and rejected, because it turns the
/// solar panel into a light box and the cells stop reading as cells.
///
/// **She is the first type in the fleet whose lit arm is live on every
/// seed.** All three of her themes are luminous, so [`trim`] takes its
/// emissive arm on all 54 seeds under 3000, in both registers (28 bold at
/// emission 8.0, 26 soft at 4.5) - the armoured car's exact opposite, whose
/// lit arm no seed exercises.
///
/// And she is the first skiff to carry a TEXTURED finish: the carapace's
/// shell is [`MaterialKit::skin`]'s chitin, 422 B on one material and one
/// slot. Rule 5 (#784) does not bite at this tile - 4 plates a metre with
/// 2.5 cm sutures puts six or eight hand-sized plates on a 0.9 m shell,
/// where #784's failure was a FINE weave mipping into fish scales.
pub(crate) struct RoverColours {
    /// The deck (the carapace's is [`Self::under`] instead), the mast, the
    /// camera head's housing, the dish, the monolith's slab, its obelisk and
    /// its stele: the scheme, floored off the tyres like any coachwork
    /// ([`GUARD_FLOOR`]).
    pub(crate) body: SovereignMaterialSettings,
    /// The carapace's shell and its head plate: the scheme in
    /// [`MaterialKit::skin`]'s chitin, un-grimed and un-floored because the
    /// colour handed to it has already been through both.
    pub(crate) shell: SovereignMaterialSettings,
    /// The carapace's running deck under her shell: a dark chassis held
    /// clear of the scheme and floored off the tyres.
    pub(crate) under: SovereignMaterialSettings,
    /// **Identity.** The band round the deck's edge, the carapace's dorsal
    /// ridge, the monolith's roof and flank seams and its stele's band: the
    /// coachline's own rule, held clear of the mass they lie on.
    pub(crate) strip: SovereignMaterialSettings,
    /// **Identity.** All six rims, held clear of the tyre they are set into.
    pub(crate) rim: SovereignMaterialSettings,
    /// A worn rover's near-REAR rim: bare steel, paint on every kit - so on
    /// a lit kit it reads as one dead rim among five lit ones.
    pub(crate) odd_rim: SovereignMaterialSettings,
    pub(crate) tyre: SovereignMaterialSettings,
    /// The rockers, the bogies, the differential bar, the six stub axles,
    /// the panel's pedestal, the dish's stem, the antenna whip and the
    /// hidden hub.
    pub(crate) arm: SovereignMaterialSettings,
    /// The camera head's pane: the tertiary's window light, the fleet's one
    /// lit pane and the only thing on her lit on every theme by
    /// construction. She carries no headlamp, so unlike every other land
    /// craft's this is her ONE window slot rather than one of two.
    pub(crate) glass: SovereignMaterialSettings,
    /// The solar cells: deep blue-black glass under the one glossy vehicle
    /// finish, whatever the scheme - a cell is a cell.
    pub(crate) cell: SovereignMaterialSettings,
    /// The solar panel's frame and the wing panel's: bare alloy, held clear
    /// of the cells laid on it.
    pub(crate) frame: SovereignMaterialSettings,
    /// The stern pallet's two sample boxes: bare alloy, held clear of the
    /// mass behind them. It is the SAME alloy as [`Self::frame`] and comes
    /// out one colour with it on a surveyor - they are the same material in
    /// life, and the fault matrix works around the collapse rather than
    /// pretending otherwise.
    pub(crate) boxes: SovereignMaterialSettings,
    /// The antenna whip's beacon cap: a red glow.
    pub(crate) tip: SovereignMaterialSettings,
    /// A battered rover's dead solar cell, or the primer patch on a
    /// monolith's roof or a carapace's shell.
    pub(crate) primer: SovereignMaterialSettings,
}

/// The rover's fixed colours (#1378): what these parts simply are, and how
/// far each is held from the one surface it has to read against.
const ROVER_CELL: [f32; 3] = [0.035, 0.050, 0.130];
const ROVER_ALLOY: [f32; 3] = [0.62, 0.63, 0.65];
const ROVER_UNDER: [f32; 3] = [0.11, 0.11, 0.12];
const ROVER_RIM_DELTA: f32 = 0.30;
const ROVER_FRAME_DELTA: f32 = 0.22;
const ROVER_BOX_DELTA: f32 = 0.18;
const ROVER_UNDER_DELTA: f32 = 0.16;

pub(crate) fn rover_colours(ctx: &PartCtx) -> RoverColours {
    let p = &ctx.palette;
    let m = &ctx.materials;
    let l = rover_livery(ctx.seed, ctx.livery);
    // A basalt or a deep violet deck is the point of her, but never as dark
    // as the tyres she stands on. The shell takes this SAME colour: `skin`
    // neither grimes nor floors, so the floor has to be owed before it.
    let body = floor_finished(m, l.body, GUARD_FLOOR);
    let bl = luma(body);
    RoverColours {
        body: m.paint(body),
        shell: m.skin(body),
        under: m.paint(floor_finished(
            m,
            clear_of(ROVER_UNDER, bl, ROVER_UNDER_DELTA),
            GUARD_FLOOR,
        )),
        strip: trim(m, clear_of(p.primary_accent, bl, COACHLINE_DELTA)),
        rim: trim(m, clear_of(p.primary_accent, luma(TYRE), ROVER_RIM_DELTA)),
        odd_rim: m.paint(floor_finished(m, STEEL, GUARD_FLOOR)),
        tyre: m.rubber(TYRE),
        arm: m.paint(MACHINERY),
        glass: window_material(window_light(p.tertiary_accent)),
        cell: m.brightwork(ROVER_CELL),
        frame: m.paint(clear_of(ROVER_ALLOY, luma(ROVER_CELL), ROVER_FRAME_DELTA)),
        boxes: m.paint(clear_of(ROVER_ALLOY, bl, ROVER_BOX_DELTA)),
        tip: m.glow(TAIL_LAMP),
        primer: m.paint(floor_finished(
            m,
            clear_of(PRIMER, bl, PRIMER_DELTA),
            GUARD_FLOOR,
        )),
    }
}

/// The surfaces a junk is built in (#1371): her scheme on her hull and her
/// sails, the seeded accent on her transom roundel and her eyes, and the
/// colours the rest of her simply is - teak decks, timber spars, bamboo
/// battens, a chunam bottom.
///
/// Nothing of hers is lit but the stern lantern and the quarter windows: none
/// of her themes is luminous, so the accent is paint (were one ever to be,
/// the roundel would go through [`trim`]).
pub(crate) struct JunkColours {
    /// The shell and the headboard: the scheme's lacquer, or oiled teak
    /// under a gloss.
    pub(crate) hull: SovereignMaterialSettings,
    /// The transom - the face the chase camera looks at - and the break's
    /// bulkhead, in the hull's own finish.
    pub(crate) transom: SovereignMaterialSettings,
    pub(crate) bulkhead: SovereignMaterialSettings,
    /// Below the waterline: chunam, the lime-and-oil paste a junk's bottom
    /// was paid with, under the fleet's antifoul rule (a little of the
    /// seed's secondary).
    pub(crate) antifoul: SovereignMaterialSettings,
    /// The main deck, and the poop deck a shade darker.
    pub(crate) deck: SovereignMaterialSettings,
    pub(crate) poop: SovereignMaterialSettings,
    /// The capping rail along the bulwark's top.
    pub(crate) rail: SovereignMaterialSettings,
    /// The keelson post in the void under the deck.
    pub(crate) interior: SovereignMaterialSettings,
    /// Every sail: the scheme's cloth.
    pub(crate) sail: SovereignMaterialSettings,
    /// A worn mainsail's replaced panel: new cloth, held clear of the sail.
    pub(crate) patch: SovereignMaterialSettings,
    /// The battens and the mat shelter's hoops: pale bamboo, held clear of
    /// the sail they lie across.
    pub(crate) batten: SovereignMaterialSettings,
    /// The masts, the yards and the booms.
    pub(crate) spar: SovereignMaterialSettings,
    /// The rudder's stock, tiller and blade: dark wood.
    pub(crate) rudder: SovereignMaterialSettings,
    /// The dark inlays: the rudder's six slots and each eye's pupil.
    pub(crate) inlay: SovereignMaterialSettings,
    /// The lantern's gooseneck and hanger.
    pub(crate) iron: SovereignMaterialSettings,
    /// The stern cabin's quarter windows: the tertiary's light.
    pub(crate) window: SovereignMaterialSettings,
    /// **Identity.** The roundel on her transom and the eye on each bow: the
    /// seeded accent held clear of the hull it is painted on.
    pub(crate) mark: SovereignMaterialSettings,
    /// The roundel's gilt ring.
    pub(crate) ring: SovereignMaterialSettings,
    /// The stern lantern: a candle, as every lantern in the fleet is.
    pub(crate) lamp: SovereignMaterialSettings,
    /// The mat shelter's woven matting.
    pub(crate) mat: SovereignMaterialSettings,
}

/// A junk's fixed colours (#1371): what these parts simply are.
const CHUNAM: [f32; 3] = [0.78, 0.76, 0.68];
const TEAK_DECK: [f32; 3] = [0.58, 0.41, 0.22];
const SPAR: [f32; 3] = [0.50, 0.34, 0.16];
const BAMBOO: [f32; 3] = [0.66, 0.55, 0.32];
const RUDDER_WOOD: [f32; 3] = [0.22, 0.15, 0.09];
const INLAY: [f32; 3] = [0.03, 0.03, 0.03];
const GILT: [f32; 3] = [0.80, 0.62, 0.24];
const MATTING: [f32; 3] = [0.62, 0.52, 0.32];

/// How far the roundel and the eyes are held from the hull, the battens from
/// the sail they lie across, and a replaced panel from the sail it mends.
const MARK_DELTA: f32 = 0.26;
const BATTEN_DELTA: f32 = 0.16;
const JUNK_PATCH_DELTA: f32 = 0.12;

pub(crate) fn junk_colours(ctx: &PartCtx) -> JunkColours {
    let p = &ctx.palette;
    let m = &ctx.materials;
    let l = junk_livery(ctx.seed, ctx.livery);
    // Lacquer is floored off mud on a battered kit, as a boat's topsides
    // are; oiled teak is a colour under a gloss, as a varnished hull is.
    let hull = if l.bright {
        l.hull
    } else {
        floor_finished(m, l.hull, BOAT_MASS_FLOOR)
    };
    let finish = |c| {
        if l.bright {
            m.brightwork(c)
        } else {
            m.paint(c)
        }
    };
    // The one identity slot is cleared off the hull and never floored after:
    // the clear-then-floor order that undid the buggy's rims (#1374, #1389)
    // does not arise.
    let mark = m.paint(clear_of(p.primary_accent, luma(hull), MARK_DELTA));
    let sail_l = luma(l.sail);
    JunkColours {
        hull: finish(hull),
        transom: finish(hull),
        bulkhead: finish(hull),
        antifoul: m.antifoul(mix(CHUNAM, shade(p.secondary_accent, 0.5), 0.18)),
        deck: boards(m, TEAK_DECK),
        poop: boards(m, shade(TEAK_DECK, 0.9)),
        rail: boards(m, shade(SPAR, 0.70)),
        interior: m.paint(shade(hull, 0.45)),
        sail: m.canvas(l.sail),
        patch: m.canvas(clear_of(shade(l.sail, 0.62), sail_l, JUNK_PATCH_DELTA)),
        batten: m.paint(clear_of(BAMBOO, sail_l, BATTEN_DELTA)),
        spar: m.timber(SPAR),
        rudder: m.paint(RUDDER_WOOD),
        inlay: m.paint(INLAY),
        iron: m.paint(MACHINERY),
        window: window_material(window_light(p.tertiary_accent)),
        mark,
        ring: m.paint(GILT),
        lamp: window_material(window_light(CANDLE)),
        mat: m.canvas(MATTING),
    }
}

/// The surfaces a longship is built in (#1369): her scheme on her hull and
/// her sail, the seeded accent on her SHIELD ROW and on her sail's stripes,
/// and the colours the rest of her simply is - oak bottom boards, timber
/// spars, a tarred bottom, undyed wool.
///
/// Her ONE lit slot on a luminous kit is the masthead VANE, through
/// [`trim`]. Her two other identity surfaces may not glow: the shield row is
/// a HULL LINE, which #1365 phase 2 took the glow off for exactly the reason
/// the phase-1 renders show again, and her stripes are the largest surface
/// she carries.
///
/// No slot of hers is `floor_finished(clear_of(..))`, so the clear-then-floor
/// order that undid the buggy's rims (#1374, #1389) does not arise: the hull
/// is floored and never cleared, and every accent is cleared and never
/// floored.
pub(crate) struct LongshipColours {
    /// The bored shell and her two end plugs: the scheme's paint, or oiled
    /// oak under a gloss.
    pub(crate) hull: SovereignMaterialSettings,
    /// Below the waterline: a longship's bottom was TARRED, not coppered,
    /// under the fleet's antifoul rule (a little of the seed's secondary).
    pub(crate) antifoul: SovereignMaterialSettings,
    /// The capping rail along the shell's wall at both sheers.
    pub(crate) rail: SovereignMaterialSettings,
    /// The bottom boards you look down at inside her.
    pub(crate) sole: SovereignMaterialSettings,
    /// The keelson in the void under the boards.
    pub(crate) interior: SovereignMaterialSettings,
    /// Her twelve clinker strakes, held clear of the shell they lie on.
    pub(crate) strake: SovereignMaterialSettings,
    /// **Identity.** The SHIELD ROW along her gunwale: the seeded accent
    /// held clear of the hull it hangs on. Paint on every theme - see the
    /// struct docs.
    pub(crate) shield: SovereignMaterialSettings,
    /// A BATTERED row's one dead shield: bare timber among the painted.
    pub(crate) bare: SovereignMaterialSettings,
    /// The mast, the yard and the tent's ridge pole. PAINT, not
    /// [`MaterialKit::timber`]: a texture block is written PER NODE, and a
    /// Plank on a part thinner than a plank is wide is mips into its own
    /// pitch (#784).
    pub(crate) spar: SovereignMaterialSettings,
    /// Her square sail.
    pub(crate) sail: SovereignMaterialSettings,
    /// **Identity.** Her sail's STRIPES: the seeded accent held clear of the
    /// cloth, in canvas so they read as dyed wool rather than as paint.
    pub(crate) stripe: SovereignMaterialSettings,
    /// A worn or battered sail's replaced panel: new cloth, held clear of
    /// the sail.
    pub(crate) patch: SovereignMaterialSettings,
    /// Her two braces.
    pub(crate) rope: SovereignMaterialSettings,
    /// **Identity, and her ONE lit slot.** The masthead VANE - and the
    /// Ornate BANNER, which wears this same material rather than one of its
    /// own. That is a fact of the livery, not an oversight: the two are one
    /// cloth in life, the same accent held clear of the same sail, so at the
    /// fullest tiers a repaint of one IS a repaint of both.
    pub(crate) vane: SovereignMaterialSettings,
    /// The steering oar's stock and tiller, and the galley's bank of oars.
    /// Paint, for [`spar`](Self::spar)'s reason.
    pub(crate) oar: SovereignMaterialSettings,
    /// The steering oar's blade.
    pub(crate) blade: SovereignMaterialSettings,
    /// The galley's bronze beak.
    pub(crate) iron: SovereignMaterialSettings,
    /// The crew's tent aft: undyed wool held clear of the hull under it.
    pub(crate) tent: SovereignMaterialSettings,
    /// The serpent's neck, snout and jaw.
    pub(crate) scale: SovereignMaterialSettings,
    /// Her horns: the seed's tertiary, held clear of the hull.
    pub(crate) horn: SovereignMaterialSettings,
    /// Her eyes - the one place a LUMINOUS kit lights something low on the
    /// boat, because an eye that is not lit on a fae serpent is a dead eye.
    pub(crate) eye: SovereignMaterialSettings,
}

/// A longship's fixed colours (#1369): what these parts simply are.
const PITCH_BOTTOM: [f32; 3] = [0.10, 0.095, 0.085];
const OAK_BARE: [f32; 3] = [0.47, 0.43, 0.37];
const OAK_SOLE: [f32; 3] = [0.50, 0.38, 0.22];
const LONGSHIP_SPAR: [f32; 3] = [0.50, 0.34, 0.16];
const SEIZING: [f32; 3] = [0.64, 0.55, 0.38];
const SCALE_GREEN: [f32; 3] = [0.16, 0.26, 0.19];

/// How far the shield row is held from the hull it hangs on, the stripes and
/// the vane from the sail they are seen against, the strakes from the shell
/// they lie on, the tent from the hull under it, a replaced panel from the
/// sail it mends, and the serpent's scales, horns and eyes from the hull.
const SHIELD_DELTA: f32 = 0.26;
const STRIPE_DELTA: f32 = 0.22;
const STRAKE_DELTA: f32 = 0.10;
const TENT_DELTA: f32 = 0.16;
const LONGSHIP_PATCH_DELTA: f32 = 0.12;
const SCALE_DELTA: f32 = 0.14;
const HORN_DELTA: f32 = 0.20;
const SERPENT_EYE_DELTA: f32 = 0.30;

pub(crate) fn longship_colours(ctx: &PartCtx) -> LongshipColours {
    let p = &ctx.palette;
    let m = &ctx.materials;
    let l = longship_livery(ctx.seed, ctx.livery);
    // Paint is floored off mud on a battered kit, as a boat's topsides are;
    // oiled oak is a colour under a gloss, as a varnished hull is.
    let hull = if l.bright {
        l.hull
    } else {
        floor_finished(m, l.hull, BOAT_MASS_FLOOR)
    };
    let hull_l = luma(hull);
    let sail_l = luma(l.sail);
    // Her two accent surfaces, each cleared off the mass it is seen against
    // and never floored after - see the struct docs.
    let shield = clear_of(p.primary_accent, hull_l, SHIELD_DELTA);
    let on_sail = clear_of(p.primary_accent, sail_l, STRIPE_DELTA);
    LongshipColours {
        hull: if l.bright {
            m.brightwork(hull)
        } else {
            m.paint(hull)
        },
        antifoul: m.antifoul(mix(PITCH_BOTTOM, shade(p.secondary_accent, 0.5), 0.18)),
        rail: boards(m, shade(LONGSHIP_SPAR, 0.70)),
        sole: boards(m, OAK_SOLE),
        interior: m.paint(shade(hull, 0.45)),
        strake: m.paint(clear_of(shade(hull, 0.72), hull_l, STRAKE_DELTA)),
        shield: m.paint(shield),
        bare: m.timber(OAK_BARE),
        spar: m.paint(LONGSHIP_SPAR),
        sail: m.canvas(l.sail),
        stripe: m.canvas(on_sail),
        patch: m.canvas(clear_of(shade(l.sail, 0.62), sail_l, LONGSHIP_PATCH_DELTA)),
        rope: m.canvas(SEIZING),
        vane: trim(m, on_sail),
        oar: m.paint(shade(LONGSHIP_SPAR, 0.92)),
        blade: m.paint(shade(LONGSHIP_SPAR, 0.86)),
        iron: m.paint(MACHINERY),
        tent: m.canvas(clear_of(WOOL_NATURAL, hull_l, TENT_DELTA)),
        scale: m.paint(clear_of(SCALE_GREEN, hull_l, SCALE_DELTA)),
        horn: m.paint(clear_of(p.tertiary_accent, hull_l, HORN_DELTA)),
        eye: if m.emissive_accents() {
            window_material(window_light(p.primary_accent))
        } else {
            m.paint(clear_of(p.primary_accent, hull_l, SERPENT_EYE_DELTA))
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeded_defaults::ChassisFamily;

    fn seed_of(family: ChassisFamily) -> u64 {
        (0u64..600)
            .find(|&s| ChassisFamily::for_seed(s) == family)
            .expect("the population holds one of every family")
    }

    /// Both tables are reachable in full: a scheme nobody can roll is a
    /// scheme nobody agreed to.
    #[test]
    fn every_livery_is_reachable() {
        let mut boats = vec![0usize; BOAT_LIVERIES.len()];
        let mut skiffs = vec![0usize; SKIFF_LIVERIES.len()];
        for s in 0u64..4_000 {
            let b = boat_livery(s, None);
            boats[BOAT_LIVERIES
                .iter()
                .position(|l| l.name == b.name)
                .expect("the pick came from the table")] += 1;
            let k = skiff_livery(s, None);
            skiffs[SKIFF_LIVERIES
                .iter()
                .position(|l| l.name == k.name)
                .expect("the pick came from the table")] += 1;
        }
        assert!(
            boats.iter().all(|&n| n > 0),
            "a boat livery is unreachable: {boats:?}"
        );
        assert!(
            skiffs.iter().all(|&n| n > 0),
            "a skiff livery is unreachable: {skiffs:?}"
        );
        // The wagon's own list, on the bodies that pick from it (#1377).
        let mut wagons = vec![0usize; WAGON_LIVERIES.len()];
        for s in 0u64..4_000 {
            let w = wagon_livery(s, WagonBody::Cart, None);
            wagons[WAGON_LIVERIES
                .iter()
                .position(|l| l.name == w.name)
                .expect("the pick came from the table")] += 1;
        }
        assert!(
            wagons.iter().all(|&n| n > 0),
            "a wagon livery is unreachable: {wagons:?}"
        );
        // And the dune buggy's two (#1374): the brights a rail and a beach
        // buggy pick from, and the raider's wasteland list.
        for (variant, table) in [
            (BuggyVariant::Rail, BUGGY_LIVERIES),
            (BuggyVariant::Beach, BUGGY_LIVERIES),
            (BuggyVariant::Raider, RAIDER_LIVERIES),
        ] {
            let mut hits = vec![0usize; table.len()];
            for s in 0u64..4_000 {
                let b = buggy_livery(s, variant, None);
                hits[table
                    .iter()
                    .position(|l| l.name == b.name)
                    .expect("the pick came from the variant's own table")] += 1;
            }
            assert!(
                hits.iter().all(|&n| n > 0),
                "a {variant:?} livery is unreachable: {hits:?}"
            );
        }
        // And the cyclecar's own six (#1376).
        let mut cyclecars = vec![0usize; CYCLECAR_LIVERIES.len()];
        for s in 0u64..4_000 {
            let c = cyclecar_livery(s, None);
            cyclecars[CYCLECAR_LIVERIES
                .iter()
                .position(|l| l.name == c.name)
                .expect("the pick came from the table")] += 1;
        }
        assert!(
            cyclecars.iter().all(|&n| n > 0),
            "a cyclecar livery is unreachable: {cyclecars:?}"
        );
        // And the armoured car's own six (#1375).
        let mut cars = vec![0usize; ARMOUR_LIVERIES.len()];
        for s in 0u64..4_000 {
            let a = armoured_livery(s, None);
            cars[ARMOUR_LIVERIES
                .iter()
                .position(|l| l.name == a.name)
                .expect("the pick came from the table")] += 1;
        }
        assert!(
            cars.iter().all(|&n| n > 0),
            "an armoured-car livery is unreachable: {cars:?}"
        );
        // And the rover's own six (#1378).
        let mut rovers = vec![0usize; ROVER_LIVERIES.len()];
        for s in 0u64..4_000 {
            let r = rover_livery(s, None);
            rovers[ROVER_LIVERIES
                .iter()
                .position(|l| l.name == r.name)
                .expect("the pick came from the table")] += 1;
        }
        assert!(
            rovers.iter().all(|&n| n > 0),
            "a rover livery is unreachable: {rovers:?}"
        );
        // And the junk's own seven (#1371).
        let mut junks = vec![0usize; JUNK_LIVERIES.len()];
        for s in 0u64..4_000 {
            let j = junk_livery(s, None);
            junks[JUNK_LIVERIES
                .iter()
                .position(|l| l.name == j.name)
                .expect("the pick came from the table")] += 1;
        }
        assert!(
            junks.iter().all(|&n| n > 0),
            "a junk livery is unreachable: {junks:?}"
        );
    }

    /// The hearse and the ox-cart wear their forced schemes whatever the seed
    /// or the `--livery` override says, and no other body does (#1377): a
    /// cream hearse is a bread van.
    #[test]
    fn a_hearse_and_an_ox_cart_wear_their_own_schemes() {
        for s in 0u64..200 {
            for over in [None, Some(0), Some(3)] {
                assert_eq!(
                    wagon_livery(s, WagonBody::Hearse, over).name,
                    MOURNING_BLACK.name
                );
                assert_eq!(wagon_livery(s, WagonBody::OxCart, over).name, LACQUER.name);
                for body in [WagonBody::Cart, WagonBody::Buckboard, WagonBody::Chariot] {
                    let name = wagon_livery(s, body, over).name;
                    assert!(
                        name != MOURNING_BLACK.name && name != LACQUER.name,
                        "{body:?} on seed {s} wore a forced scheme"
                    );
                }
            }
        }
    }

    /// `--livery <index>` draws the scheme it names, and wraps rather than
    /// saturating - a survey loop that runs off the end of the table must not
    /// quietly draw the last scheme twice.
    #[test]
    fn the_override_names_a_scheme_and_wraps() {
        for (i, l) in BOAT_LIVERIES.iter().enumerate() {
            assert_eq!(boat_livery(7, Some(i)).name, l.name);
            assert_eq!(boat_livery(9, Some(i + BOAT_LIVERIES.len())).name, l.name);
        }
        for (i, l) in SKIFF_LIVERIES.iter().enumerate() {
            assert_eq!(skiff_livery(7, Some(i)).name, l.name);
        }
        // A buggy's override wraps inside her variant's own list (#1374).
        for (variant, table) in [
            (BuggyVariant::Rail, BUGGY_LIVERIES),
            (BuggyVariant::Raider, RAIDER_LIVERIES),
        ] {
            for (i, l) in table.iter().enumerate() {
                assert_eq!(buggy_livery(7, variant, Some(i)).name, l.name);
                assert_eq!(buggy_livery(9, variant, Some(i + table.len())).name, l.name);
            }
        }
        // And a cyclecar's inside her own list (#1376): the agreed ladder
        // sheets draw Obsidian as `--livery 0` and Pearl as `--livery 1`.
        for (i, l) in CYCLECAR_LIVERIES.iter().enumerate() {
            assert_eq!(cyclecar_livery(7, Some(i)).name, l.name);
            assert_eq!(
                cyclecar_livery(9, Some(i + CYCLECAR_LIVERIES.len())).name,
                l.name
            );
        }
        assert_eq!(cyclecar_livery(3, Some(0)).name, "Obsidian");
        assert_eq!(cyclecar_livery(3, Some(1)).name, "Pearl");
        // And a junk's inside hers (#1371): the agreed ladder sheets draw
        // Black and tanbark as `--livery 0` and Oiled teak as `--livery 1`.
        for (i, l) in JUNK_LIVERIES.iter().enumerate() {
            assert_eq!(junk_livery(7, Some(i)).name, l.name);
            assert_eq!(junk_livery(9, Some(i + JUNK_LIVERIES.len())).name, l.name);
        }
        assert_eq!(junk_livery(3, Some(0)).name, "Black and tanbark");
        assert_eq!(junk_livery(3, Some(1)).name, "Oiled teak");
        // And an armoured car's inside hers (#1375): the agreed ladder sheets
        // draw Olive drab as `--livery 0` and Works grey as `--livery 1`.
        for (i, l) in ARMOUR_LIVERIES.iter().enumerate() {
            assert_eq!(armoured_livery(7, Some(i)).name, l.name);
            assert_eq!(
                armoured_livery(9, Some(i + ARMOUR_LIVERIES.len())).name,
                l.name
            );
        }
        assert_eq!(armoured_livery(3, Some(0)).name, "Olive drab");
        assert_eq!(armoured_livery(3, Some(1)).name, "Works grey");
        // And a rover's inside hers (#1378): the agreed ladder sheets draw
        // Thermal white as `--livery 0` and Foil gold as `--livery 1`.
        for (i, l) in ROVER_LIVERIES.iter().enumerate() {
            assert_eq!(rover_livery(7, Some(i)).name, l.name);
            assert_eq!(rover_livery(9, Some(i + ROVER_LIVERIES.len())).name, l.name);
        }
        assert_eq!(rover_livery(3, Some(0)).name, "Thermal white");
        assert_eq!(rover_livery(3, Some(1)).name, "Foil gold");
    }

    /// No painted surface on a land-skiff carries the tyres' own value, on
    /// any scheme at any wear - the roadster's one colour rule (#1364), owed
    /// on the FINISHED material rather than on the colour handed to it.
    #[test]
    fn no_coachwork_sinks_to_the_value_of_rubber() {
        let base = seed_of(ChassisFamily::Skiff);
        // Sweep the wear axis by seed: the kit's wear comes from the anchor,
        // so the population itself is the sweep, and the worst case is the
        // most battered seed of the darkest scheme.
        for s in (0u64..900).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let mut ctx = PartCtx::for_seed(s);
            for (i, scheme) in SKIFF_LIVERIES.iter().enumerate() {
                ctx.livery = Some(i);
                let c = skiff_colours(&ctx);
                let (tyre, paint, guard) = (
                    luma(c.rubber.base_color.0),
                    luma(c.paint.base_color.0),
                    luma(c.guard.base_color.0),
                );
                for (what, v) in [("paint", paint), ("guard", guard)] {
                    // The slack is the value helpers' own: `to_value` returns
                    // a colour unchanged when it is within 1e-3 of the target,
                    // so a floored surface can land that far under the number
                    // it was floored at.
                    assert!(
                        v >= GUARD_FLOOR - 2e-3,
                        "seed {s} in {}: the {what} finished at {v}, under the \
                         {GUARD_FLOOR} floor",
                        scheme.name
                    );
                    assert!(
                        v > tyre * 1.8,
                        "seed {s} in {}: the {what} ({v}) is inside the tyres' \
                         own value ({tyre})",
                        scheme.name
                    );
                }
            }
        }
        // And the floor is not vacuous: some seed of some scheme actually
        // needs it.
        let mut ctx = PartCtx::for_seed(base);
        ctx.livery = Some(
            SKIFF_LIVERIES
                .iter()
                .position(|l| l.name == "Black")
                .expect("the black scheme is in the table"),
        );
        assert!(luma(skiff_colours(&ctx).paint.base_color.0) >= GUARD_FLOOR - 2e-3);
    }

    /// The two wear-and-ornament masses that were invisible in their first
    /// render stay findable on every scheme (#1366): the upturned tender
    /// against the laid deck she lies on, and the tarp against the coachroof
    /// it covers. Both were drawn in a colour the boat already carried and
    /// both vanished at play distance - the tender in the topsides' white,
    /// the tarp in the white canvas of a white boat.
    #[test]
    fn the_dressing_reads_against_what_it_lies_on() {
        for s in (0u64..600).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat) {
            let mut ctx = PartCtx::for_seed(s);
            for (i, scheme) in BOAT_LIVERIES.iter().enumerate() {
                ctx.livery = Some(i);
                let c = boat_colours(&ctx);
                let l = |m: &SovereignMaterialSettings| luma(m.base_color.0);
                // Grime dims both sides of each pair by one factor, so the
                // delta shrinks by at most that much - as for the boot top.
                let dinghy = (l(&c.dinghy) - l(&c.timber)).abs();
                assert!(
                    dinghy > DINGHY_DELTA * 0.6,
                    "seed {s} in {}: the tender is {dinghy} from the deck",
                    scheme.name
                );
                let tarp = (l(&c.tarp) - l(&c.topsides)).abs();
                assert!(
                    tarp > TARP_DELTA * 0.6,
                    "seed {s} in {}: the tarp is {tarp} from the coachroof",
                    scheme.name
                );
            }
        }
    }

    /// A steam tug's company colours read on every scheme (#1370), on every
    /// seed drawn as a tug: the band round her funnel - the seed's one
    /// identity slot - against the funnel it is painted on, the wheelhouse
    /// roof against the walls under it, and the boat's cover against the
    /// casing top it rides over.
    #[test]
    fn a_tugs_funnel_band_and_roof_read_on_every_scheme() {
        use crate::seeded_defaults::BoatType;
        let mut tugs = 0;
        for s in (0u64..3000).filter(|&s| {
            ChassisFamily::for_seed(s) == ChassisFamily::Boat
                && BoatType::for_seed(s) == BoatType::SteamTug
        }) {
            let mut ctx = PartCtx::for_seed(s);
            for (i, scheme) in BOAT_LIVERIES.iter().enumerate() {
                ctx.livery = Some(i);
                let c = tug_colours(&ctx);
                let l = |m: &SovereignMaterialSettings| luma(m.base_color.0);
                // Grime dims both sides of each pair by one factor, so each
                // delta shrinks by at most that much - as for the boot top.
                for (what, a, b, delta) in [
                    ("funnel band", &c.band, &c.funnel, FUNNEL_BAND_DELTA),
                    ("wheelhouse roof", &c.roof, &c.house, TUG_ROOF_DELTA),
                    ("boat's cover", &c.cover, &c.roof, COVER_DELTA),
                ] {
                    let d = (l(a) - l(b)).abs();
                    assert!(
                        d > delta * 0.6,
                        "seed {s} in {}: the {what} is {d} from what it lies on",
                        scheme.name
                    );
                }
            }
            tugs += 1;
        }
        assert!(tugs > 30, "only {tugs} tug seeds under 3000");
    }

    /// A dune buggy's colours read on every scheme of both her lists, at
    /// every wear (#1374): her frame and her pod are coachwork, so each
    /// finishes at or over [`GUARD_FLOOR`] and well clear of the tyres they
    /// stand on, and her rims - the seed's one identity slot on her wheels -
    /// stay clear of the frame they turn inside. Over every skiff seed under
    /// 900, so the population's kits are the wear sweep, as for the roadster;
    /// the rims on the kits a buggy can wear, because none of her themes is
    /// luminous and a lit rim is an unground glow whose value is not the
    /// question.
    #[test]
    fn a_buggys_frame_pod_and_rims_read_on_every_scheme() {
        for s in (0u64..900).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let mut ctx = PartCtx::for_seed(s);
            let lit = ctx.materials.emissive_accents();
            for (variant, table) in [
                (BuggyVariant::Rail, BUGGY_LIVERIES),
                (BuggyVariant::Raider, RAIDER_LIVERIES),
            ] {
                for (i, scheme) in table.iter().enumerate() {
                    ctx.livery = Some(i);
                    let c = buggy_colours(&ctx, variant);
                    let l = |m: &SovereignMaterialSettings| luma(m.base_color.0);
                    let tyre = l(&c.tyre);
                    for (what, v) in [("frame", l(&c.frame)), ("pod", l(&c.pod))] {
                        // `to_value`'s own slack, as for the roadster.
                        assert!(
                            v >= GUARD_FLOOR - 2e-3,
                            "seed {s} in {}: the {what} finished at {v}, under the \
                             {GUARD_FLOOR} floor",
                            scheme.name
                        );
                        assert!(
                            v > tyre * 1.8,
                            "seed {s} in {}: the {what} ({v}) is inside the tyres' \
                             own value ({tyre})",
                            scheme.name
                        );
                    }
                    // Grime dims both sides by one factor, so the delta
                    // shrinks by at most that much - as for the coachline.
                    let d = (l(&c.rim) - l(&c.frame)).abs();
                    assert!(
                        lit || d > DISC_DELTA * 0.6,
                        "seed {s} in {}: the rims are {d} from the frame",
                        scheme.name
                    );
                }
            }
        }
    }

    /// The roadster's dressing reads against what it lies on, on every scheme
    /// at every wear (#1367): the hood and the trunk against the coachwork,
    /// the primer wing against the guards it failed to match (and off the
    /// tyre, like any guard), the can against the board under it.
    #[test]
    fn the_skiff_dressing_reads_against_what_it_lies_on() {
        for s in (0u64..600).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let mut ctx = PartCtx::for_seed(s);
            for (i, scheme) in SKIFF_LIVERIES.iter().enumerate() {
                ctx.livery = Some(i);
                let c = skiff_colours(&ctx);
                let l = |m: &SovereignMaterialSettings| luma(m.base_color.0);
                // Grime dims both sides of each pair by one factor, so each
                // delta shrinks by at most that much - as for the boot top.
                for (what, a, b, delta) in [
                    ("hood", &c.hood, &c.paint, HOOD_DELTA),
                    ("trunk", &c.trunk, &c.paint, TRUNK_DELTA),
                    ("primer wing", &c.primer, &c.guard, PRIMER_DELTA),
                    ("jerrycan", &c.can, &c.machinery, CAN_DELTA),
                ] {
                    let d = (l(a) - l(b)).abs();
                    assert!(
                        d > delta * 0.6,
                        "seed {s} in {}: the {what} is {d} from what it lies on",
                        scheme.name
                    );
                }
                assert!(
                    l(&c.primer) >= GUARD_FLOOR - 2e-3,
                    "seed {s} in {}: the primer wing is back inside the tyre's value",
                    scheme.name
                );
            }
        }
    }

    /// The seeded accent stays findable against the livery it is painted on,
    /// on a white hull and a black one alike - which is what the value
    /// helpers are for and the reason they survived this slice.
    #[test]
    fn the_accent_separates_from_every_scheme() {
        for s in (0u64..600).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat) {
            let mut ctx = PartCtx::for_seed(s);
            for (i, scheme) in BOAT_LIVERIES.iter().enumerate() {
                ctx.livery = Some(i);
                let c = boat_colours(&ctx);
                let d = (luma(c.boot.base_color.0) - luma(c.topsides.base_color.0)).abs();
                // Both are dimmed by the same grime, so the delta shrinks
                // with wear by that factor and never below it.
                assert!(
                    d > BOOT_DELTA * 0.6,
                    "seed {s} in {}: the boot stripe is {d} from the topsides",
                    scheme.name
                );
            }
        }
        for s in (0u64..600).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let mut ctx = PartCtx::for_seed(s);
            for (i, scheme) in SKIFF_LIVERIES.iter().enumerate() {
                ctx.livery = Some(i);
                let c = skiff_colours(&ctx);
                let d = (luma(c.coachline.base_color.0) - luma(c.paint.base_color.0)).abs();
                assert!(
                    d > COACHLINE_DELTA * 0.6,
                    "seed {s} in {}: the coachline is {d} from the coachwork",
                    scheme.name
                );
            }
        }
    }

    /// A luminous style lights its identity trim and nothing else: the
    /// coachwork of a cyberpunk car is paint, its coachline is a light. That
    /// is the whole of "theme speaks through finish, not hue".
    #[test]
    fn only_the_identity_trim_lights_up() {
        let mut lit = 0;
        let mut dark = 0;
        for s in (0u64..900).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let ctx = PartCtx::for_seed(s);
            let c = skiff_colours(&ctx);
            assert_eq!(
                c.paint.emission_strength.0, 0.0,
                "seed {s}: the coachwork is self-lit"
            );
            assert_eq!(
                c.guard.emission_strength.0, 0.0,
                "seed {s}: the wings are self-lit"
            );
            if ctx.materials.emissive_accents() {
                assert!(
                    c.coachline.emission_strength.0 > 0.0,
                    "seed {s}: a luminous style left its coachline dark"
                );
                lit += 1;
            } else {
                assert_eq!(c.coachline.emission_strength.0, 0.0);
                dark += 1;
            }
        }
        assert!(
            lit > 0 && dark > 0,
            "the population saw only one kind: {lit} lit, {dark} dark"
        );
    }

    /// A SLOOP's glow is her burgee alone: her hull lines are paint on every
    /// theme, and the masthead flag lights exactly when the style is luminous.
    /// These are the sloop's colours ([`boat_colours`]); the runabout, which
    /// has no masthead, keeps her own rule in
    /// [`a_runabout_glows_only_where_the_skimmer_is_lit`] (#1372).
    ///
    /// The other half of [`only_the_identity_trim_lights_up`], and a rule the
    /// car deliberately does not share (#1365 phase 2). The boot top and the
    /// cove line carry the same accent as the burgee and run the whole length
    /// of the hull, so lit they are a neon pinstripe down a gaff boat's sheer -
    /// the loudest thing in the frame on a battered SpaceOutpost seed, louder
    /// than the sails and the deck a boat is actually read by at play distance.
    /// The burgee is the slot small enough and high enough to take the glow
    /// instead.
    ///
    /// Both directions are asserted, because the cheap half-fix - dropping
    /// `trim` from the whole boat - would leave a luminous seed with nothing
    /// lit at all, and would pass a test that only said the boot is dark.
    #[test]
    fn a_boat_glows_at_her_masthead_and_nowhere_on_her_hull() {
        let mut lit = 0;
        let mut dark = 0;
        for s in (0u64..900).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat) {
            let mut ctx = PartCtx::for_seed(s);
            // Every scheme, not only the one this seed rolled: the glow is a
            // property of the style and the schemes must not differ on it.
            for (i, scheme) in BOAT_LIVERIES.iter().enumerate() {
                ctx.livery = Some(i);
                let c = boat_colours(&ctx);
                for (what, m) in [
                    ("boot stripe and cove line", &c.boot),
                    ("topsides", &c.topsides),
                    ("deck", &c.timber),
                    ("mainsail", &c.canvas),
                    ("jib", &c.jib),
                ] {
                    assert_eq!(
                        m.emission_strength.0, 0.0,
                        "seed {s} in {}: the {what} is self-lit",
                        scheme.name
                    );
                }
                if ctx.materials.emissive_accents() {
                    assert!(
                        c.pennant.emission_strength.0 > 0.0,
                        "seed {s} in {}: a luminous style left her burgee dark",
                        scheme.name
                    );
                    lit += 1;
                } else {
                    assert_eq!(
                        c.pennant.emission_strength.0, 0.0,
                        "seed {s} in {}: a flat style lit her burgee",
                        scheme.name
                    );
                    dark += 1;
                }
            }
        }
        assert!(
            lit > 0 && dark > 0,
            "the population saw only one kind: {lit} lit, {dark} dark"
        );
    }

    /// A runabout's hull lines are paint on every theme, as a sloop's are -
    /// EXCEPT the neon skimmer's rub rail, which is the accent through `trim`
    /// and so lights exactly when her style is luminous: the owner's call on
    /// #1372, because the lit chine drawn first did not read at 12 m and the
    /// lit rail does. Her nozzles glow on the same rule. Both directions, on
    /// every scheme: a skimmer rail dark on a neon seed fails as surely as a
    /// lit rail on a launch.
    #[test]
    fn a_runabout_glows_only_where_the_skimmer_is_lit() {
        let (mut lit, mut dark) = (0, 0);
        for s in (0u64..900).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat) {
            let mut ctx = PartCtx::for_seed(s);
            let luminous = ctx.materials.emissive_accents();
            for i in 0..BOAT_LIVERIES.len() {
                ctx.livery = Some(i);
                for v in RunaboutVariant::ALL {
                    let c = runabout_colours(&ctx, v);
                    for (what, m) in [
                        ("spray rail and cove line", &c.boot),
                        ("topsides", &c.topsides),
                        ("deck", &c.deck),
                        ("upholstery", &c.upholstery),
                        ("canvas", &c.canvas),
                    ] {
                        assert_eq!(
                            m.emission_strength.0, 0.0,
                            "seed {s}, {v:?}: the {what} is self-lit"
                        );
                    }
                    let rail_lit = c.rail.emission_strength.0 > 0.0;
                    let want = v == RunaboutVariant::Skimmer && luminous;
                    assert_eq!(rail_lit, want, "seed {s}, {v:?}: rub rail lit {rail_lit}");
                    assert_eq!(
                        c.glow.emission_strength.0 > 0.0,
                        luminous,
                        "seed {s}, {v:?}: nozzle glow"
                    );
                    if v == RunaboutVariant::Skimmer {
                        if luminous {
                            lit += 1;
                        } else {
                            dark += 1;
                        }
                    }
                }
            }
        }
        assert!(lit > 0 && dark > 0, "{lit} lit skimmer rails, {dark} dark");
    }

    /// A cyclecar glows at her rims and her strip, and nowhere else she is
    /// painted (#1376, owner decision 5) - both directions, on every scheme
    /// of her list over every skiff seed under 900, so the population's kits
    /// are the wear sweep. Her pod, its lower half, the spat, the fin, the
    /// cycle wings, the arms, the fork, the odd rim and the primer are paint
    /// on every kit; the window band and the headlamps are the tertiary's
    /// window light on every kit; the strip and the rims are lit exactly when
    /// the kit is luminous, un-grimed (base = emission colour). On a flat kit
    /// the finished strip is held clear of the finished pod and the finished
    /// rim clear of the tyre, by the 0.6 x delta the grime leaves.
    #[test]
    fn a_cyclecar_glows_only_at_her_rims_and_strip() {
        let (mut lit, mut dark) = (0, 0);
        for s in (0u64..900).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let mut ctx = PartCtx::for_seed(s);
            let luminous = ctx.materials.emissive_accents();
            let window = window_material(window_light(ctx.palette.tertiary_accent));
            for (i, scheme) in CYCLECAR_LIVERIES.iter().enumerate() {
                ctx.livery = Some(i);
                let c = cyclecar_colours(&ctx);
                assert_eq!(c.lower.is_some(), scheme.lower.is_some());
                let mut paint = vec![
                    ("pod, fin, wings and spat", &c.body),
                    ("odd rim", &c.odd_rim),
                    ("primer", &c.primer),
                    ("arms and fork", &c.arm),
                ];
                if let Some(lower) = &c.lower {
                    paint.push(("lower pod and spat", lower));
                }
                for (what, m) in paint {
                    assert_eq!(
                        m.emission_strength.0, 0.0,
                        "seed {s} in {}: the {what} is self-lit",
                        scheme.name
                    );
                }
                for (what, m) in [("window band", &c.glass), ("headlamps", &c.lamp)] {
                    assert_eq!(
                        *m, window,
                        "seed {s} in {}: the {what} is not the window light",
                        scheme.name
                    );
                }
                for (what, m) in [("strip", &c.trim), ("rims", &c.rim)] {
                    let on = m.emission_strength.0 > 0.0;
                    assert_eq!(
                        on, luminous,
                        "seed {s} in {}: the {what} is lit {on} on a kit luminous {luminous}",
                        scheme.name
                    );
                    if luminous {
                        assert_eq!(
                            m.base_color, m.emission_color,
                            "seed {s} in {}: the lit {what} is grimed",
                            scheme.name
                        );
                    }
                }
                if !luminous {
                    let l = |m: &SovereignMaterialSettings| luma(m.base_color.0);
                    for (what, a, b, delta) in [
                        ("strip", &c.trim, &c.body, COACHLINE_DELTA),
                        ("rims", &c.rim, &c.tyre, CYCLECAR_RIM_DELTA),
                    ] {
                        let d = (l(a) - l(b)).abs();
                        assert!(
                            d > delta * 0.6,
                            "seed {s} in {}: the {what} is {d} from what it lies on",
                            scheme.name
                        );
                    }
                }
            }
            if luminous {
                lit += 1;
            } else {
                dark += 1;
            }
        }
        assert!(
            lit > 0 && dark > 0,
            "the population saw only one kind: {lit} lit, {dark} dark"
        );
    }

    /// An armoured car's colours read on every scheme of her list, on every
    /// seed drawn as one under 3000 (#1375), BOTH directions.
    ///
    /// What is lit: her vision slits and her lamp lenses, which are one slot -
    /// the tertiary's window light - and her tail lamps. Every plate of her,
    /// her rims, her stowage, her bedroll, her cans, a raider's slabs and a
    /// battered machine's primer are paint on every kit. Her unit flash and
    /// her turret band are the seed's accent through [`trim`], so they light
    /// exactly when the kit is luminous - none of her three themes is today,
    /// and the rule is written because the flash and the band ARE identity
    /// trim.
    ///
    /// And each of her cleared colours really is clear of the one surface it
    /// reads against: the flash of the plate it is painted on, the rims of the
    /// tyre they are set into, the hatch of the roof it lies on, the stowage
    /// and the bedroll of the plate behind them.
    #[test]
    fn an_armoured_cars_flash_band_and_slits_read_on_every_scheme() {
        use crate::seeded_defaults::SkiffType;
        let (mut cars, mut lit, mut dark) = (0, 0, 0);
        for s in (0u64..3000).filter(|&s| {
            ChassisFamily::for_seed(s) == ChassisFamily::Skiff
                && SkiffType::for_seed(s) == SkiffType::ArmouredCar
        }) {
            let mut ctx = PartCtx::for_seed(s);
            let luminous = ctx.materials.emissive_accents();
            let window = window_material(window_light(ctx.palette.tertiary_accent));
            for (i, scheme) in ARMOUR_LIVERIES.iter().enumerate() {
                ctx.livery = Some(i);
                let c = armoured_colours(&ctx);
                for (what, m) in [
                    ("plate", &c.hull),
                    ("rims", &c.rim),
                    ("odd rim", &c.odd_rim),
                    ("tyres", &c.tyre),
                    ("beams, pipe and rails", &c.arm),
                    ("hatch", &c.hatch),
                    ("stowage bins", &c.bin),
                    ("bedroll", &c.roll),
                    ("jerrycans", &c.can),
                    ("applique slabs", &c.plate),
                    ("primer patch", &c.primer),
                ] {
                    assert_eq!(
                        m.emission_strength.0, 0.0,
                        "seed {s} in {}: the {what} is self-lit",
                        scheme.name
                    );
                }
                for (what, m) in [("vision slits", &c.glass), ("headlamps", &c.lamp)] {
                    assert_eq!(
                        *m, window,
                        "seed {s} in {}: the {what} is not the window light",
                        scheme.name
                    );
                }
                assert!(
                    c.tail_lamp.emission_strength.0 > 0.0,
                    "seed {s} in {}: the tail lamps are dark",
                    scheme.name
                );
                let on = c.flash.emission_strength.0 > 0.0;
                assert_eq!(
                    on, luminous,
                    "seed {s} in {}: the flash and band are lit {on} on a kit luminous {luminous}",
                    scheme.name
                );
                if luminous {
                    assert_eq!(
                        c.flash.base_color, c.flash.emission_color,
                        "seed {s} in {}: the lit flash is grimed",
                        scheme.name
                    );
                    continue;
                }
                // Grime dims both sides of each pair by one factor, so each
                // delta shrinks by at most that much - as the boot top's does.
                let l = |m: &SovereignMaterialSettings| luma(m.base_color.0);
                for (what, a, b, delta) in [
                    ("flash and band", &c.flash, &c.hull, FLASH_DELTA),
                    ("rims", &c.rim, &c.tyre, ARMOUR_RIM_DELTA),
                    ("hatch", &c.hatch, &c.hull, HATCH_DELTA),
                    ("stowage bins", &c.bin, &c.hull, BIN_DELTA),
                    ("bedroll", &c.roll, &c.hull, ROLL_DELTA),
                ] {
                    let d = (l(a) - l(b)).abs();
                    assert!(
                        d > delta * 0.6,
                        "seed {s} in {}: the {what} is {d} from what it lies on",
                        scheme.name
                    );
                }
            }
            if luminous {
                lit += 1;
            } else {
                dark += 1;
            }
            cars += 1;
        }
        assert!(cars > 50, "only {cars} armoured-car seeds under 3000");
        // Her three themes are all FLAT today, so the luminous arm above is
        // unexercised by the population and says so rather than pretending:
        // the day one of them turns luminous this test starts checking it.
        assert_eq!(
            lit, 0,
            "{lit} of her seeds now carry a luminous kit - the lit arm is live"
        );
        assert!(dark > 50, "{dark} flat armoured cars");
    }

    /// A rover's colours read on every scheme of her list, on every seed
    /// drawn as one under 3000 (#1378), BOTH directions.
    ///
    /// **Hers is the first guard in the fleet whose LIT arm is exercised on
    /// every seed.** All three of her themes are luminous, so her deck band
    /// (the carapace's dorsal ridge, the monolith's seams) and her six rims
    /// take [`trim`]'s emissive arm on all 54 seeds, in both registers - the
    /// armoured car's exact opposite, whose lit arm no seed of hers
    /// exercises. So the census at the end asserts the FLAT arm is the
    /// unexercised one, and the day a flat theme picks her it starts
    /// checking.
    ///
    /// What is lit: the deck band and the rims through [`trim`]; the camera
    /// head's pane, which is the tertiary's window light on every theme; and
    /// the antenna whip's beacon. Her deck, her shell, her running deck, her
    /// rims' odd steel, her tyres, her arms, her cells, her panel frame, her
    /// pallet boxes and her primer are paint on every kit.
    ///
    /// And each of her cleared colours really is clear of the one surface it
    /// reads against. On a LIT kit the identity colour is un-grimed while
    /// the mass under it is not, so the reference is the mass's own value
    /// with the grime divided back out - which is the number [`clear_of`]
    /// was actually given.
    ///
    /// **Rule 9, and she is the first skiff it bites on**: exactly ONE slot
    /// may carry a texture, and it is the carapace's chitin SHELL. Every
    /// other slot of hers is flat PBR, [`trim`] included - which takes
    /// `paint` rather than `accent` on a flat kit, so it is untextured in
    /// either register.
    #[test]
    fn a_rovers_band_rims_and_pane_read_on_every_scheme() {
        use crate::pds::texture::SovereignTextureConfig;
        use crate::seeded_defaults::{RoverVariant, SkiffType};
        let (mut rovers, mut lit, mut dark) = (0, 0, 0);
        for s in (0u64..3000).filter(|&s| {
            ChassisFamily::for_seed(s) == ChassisFamily::Skiff
                && SkiffType::for_seed(s) == SkiffType::Rover
        }) {
            let mut ctx = PartCtx::for_seed(s);
            let luminous = ctx.materials.emissive_accents();
            let grime = ctx.materials.value_after_grime();
            let window = window_material(window_light(ctx.palette.tertiary_accent));
            let carapace = RoverVariant::for_seed(s) == RoverVariant::Carapace;
            for (i, scheme) in ROVER_LIVERIES.iter().enumerate() {
                ctx.livery = Some(i);
                let c = rover_colours(&ctx);
                for (what, m) in [
                    ("deck and mast", &c.body),
                    ("carapace shell", &c.shell),
                    ("running deck", &c.under),
                    ("odd rim", &c.odd_rim),
                    ("tyres", &c.tyre),
                    ("arms and stubs", &c.arm),
                    ("solar cells", &c.cell),
                    ("panel frame", &c.frame),
                    ("pallet boxes", &c.boxes),
                    ("primer", &c.primer),
                ] {
                    assert_eq!(
                        m.emission_strength.0, 0.0,
                        "seed {s} in {}: the {what} is self-lit",
                        scheme.name
                    );
                }
                assert_eq!(
                    c.glass, window,
                    "seed {s} in {}: the camera head's pane is not the window light",
                    scheme.name
                );
                assert!(
                    c.tip.emission_strength.0 > 0.0,
                    "seed {s} in {}: the beacon is dark",
                    scheme.name
                );
                // Rule 9: her shell is the one textured finish in the
                // family, and it is chitin exactly on the carapace's theme.
                for (what, m) in [
                    ("deck and mast", &c.body),
                    ("running deck", &c.under),
                    ("deck band", &c.strip),
                    ("rims", &c.rim),
                    ("odd rim", &c.odd_rim),
                    ("tyres", &c.tyre),
                    ("arms and stubs", &c.arm),
                    ("camera pane", &c.glass),
                    ("solar cells", &c.cell),
                    ("panel frame", &c.frame),
                    ("pallet boxes", &c.boxes),
                    ("beacon", &c.tip),
                    ("primer", &c.primer),
                ] {
                    assert!(
                        matches!(m.texture, SovereignTextureConfig::None),
                        "seed {s} in {}: the {what} carries a texture - only the \
                         carapace's shell may",
                        scheme.name
                    );
                }
                assert_eq!(
                    matches!(c.shell.texture, SovereignTextureConfig::Chitin(..)),
                    carapace,
                    "seed {s} in {}: her shell's chitin and her carapace variant disagree",
                    scheme.name
                );
                for (what, m) in [("deck band", &c.strip), ("rims", &c.rim)] {
                    let on = m.emission_strength.0 > 0.0;
                    assert_eq!(
                        on, luminous,
                        "seed {s} in {}: the {what} is lit {on} on a kit luminous {luminous}",
                        scheme.name
                    );
                    if luminous {
                        assert_eq!(
                            m.base_color, m.emission_color,
                            "seed {s} in {}: the lit {what} is grimed",
                            scheme.name
                        );
                    }
                }
                // Grime dims both sides of a flat pair by one factor, so
                // each delta shrinks by at most that much - as the boot
                // top's does. On the lit pairs only the mass is grimed, so
                // the grime is divided back out of it instead.
                let l = |m: &SovereignMaterialSettings| luma(m.base_color.0);
                let body = l(&c.body);
                let against = if luminous { body / grime } else { body };
                for (what, a, b, delta) in [
                    ("deck band", l(&c.strip), against, COACHLINE_DELTA),
                    (
                        "rims",
                        l(&c.rim),
                        if luminous { luma(TYRE) } else { l(&c.tyre) },
                        ROVER_RIM_DELTA,
                    ),
                    ("panel frame", l(&c.frame), l(&c.cell), ROVER_FRAME_DELTA),
                    ("pallet boxes", l(&c.boxes), body, ROVER_BOX_DELTA),
                    ("primer", l(&c.primer), body, PRIMER_DELTA),
                ] {
                    let d = (a - b).abs();
                    assert!(
                        d > delta * 0.6,
                        "seed {s} in {}: the {what} is {d} from what it lies on",
                        scheme.name
                    );
                }
                // The RUNNING DECK is not in that list, and the reason is
                // worth writing down rather than hiding: it is cleared off
                // the scheme AND floored off the tyres, and on a dark scheme
                // at high wear the floor lifts it back toward the value it
                // was cleared from - 0.065 apart on a battered Regolith
                // seed, against the 0.16 the clear asked for. That is
                // #1374's clear-then-floor collision, which #1389 owns for
                // the whole fleet; the FLOOR is what wins by design, because
                // a running deck at rubber's own value stops being a
                // surface. So the floor is what is asked of it here, on the
                // three slots that carry one.
                for (what, m) in [
                    ("deck and mast", &c.body),
                    ("running deck", &c.under),
                    ("primer", &c.primer),
                ] {
                    let v = l(m);
                    // `to_value` leaves a colour alone within 1e-3 of its
                    // target, so a floored surface can land that far under.
                    assert!(
                        v >= GUARD_FLOOR - 2e-3,
                        "seed {s} in {}: the {what} finished at {v}, under the \
                         {GUARD_FLOOR} floor",
                        scheme.name
                    );
                    assert!(
                        v > l(&c.tyre) * 1.8,
                        "seed {s} in {}: the {what} ({v}) is inside the tyres' own value",
                        scheme.name
                    );
                }
            }
            if luminous {
                lit += 1;
            } else {
                dark += 1;
            }
            rovers += 1;
        }
        assert!(rovers > 40, "only {rovers} rover seeds under 3000");
        // Every one of her three themes is LUMINOUS, so unlike the armoured
        // car's the lit arm above is the exercised one and the flat arm is
        // the one the population never reaches. The day a flat theme picks
        // her, this says so.
        assert_eq!(
            dark, 0,
            "{dark} of her seeds now carry a flat kit - the unlit arm is live"
        );
        assert!(lit > 40, "{lit} luminous rovers");
    }

    /// A junk's colours read on every scheme of her list, on every seed drawn
    /// as a junk under 3000 (#1371): her roundel and her eyes - the seed's
    /// one identity slot - against the hull they are painted on, and the
    /// battens and a worn sail's replaced panel against the sail they lie on.
    /// And nothing of hers is lit but the stern lantern and the quarter
    /// windows, which are the tertiary's window light.
    #[test]
    fn a_junks_roundel_battens_and_lamps_read_on_every_scheme() {
        use crate::seeded_defaults::BoatType;
        let mut junks = 0;
        for s in (0u64..3000).filter(|&s| {
            ChassisFamily::for_seed(s) == ChassisFamily::Boat
                && BoatType::for_seed(s) == BoatType::Junk
        }) {
            let mut ctx = PartCtx::for_seed(s);
            let window = window_material(window_light(ctx.palette.tertiary_accent));
            for (i, scheme) in JUNK_LIVERIES.iter().enumerate() {
                ctx.livery = Some(i);
                let c = junk_colours(&ctx);
                let l = |m: &SovereignMaterialSettings| luma(m.base_color.0);
                // Grime dims both sides of each pair by one factor, so each
                // delta shrinks by at most that much - as for the boot top.
                for (what, a, b, delta) in [
                    ("roundel and eyes", &c.mark, &c.hull, MARK_DELTA),
                    ("battens", &c.batten, &c.sail, BATTEN_DELTA),
                    ("replaced panel", &c.patch, &c.sail, JUNK_PATCH_DELTA),
                ] {
                    let d = (l(a) - l(b)).abs();
                    assert!(
                        d > delta * 0.6,
                        "seed {s} in {}: the {what} is {d} from what it lies on",
                        scheme.name
                    );
                }
                for (what, m) in [
                    ("hull", &c.hull),
                    ("transom", &c.transom),
                    ("bulkhead", &c.bulkhead),
                    ("bottom", &c.antifoul),
                    ("deck", &c.deck),
                    ("poop deck", &c.poop),
                    ("rail", &c.rail),
                    ("keelson", &c.interior),
                    ("sails", &c.sail),
                    ("replaced panel", &c.patch),
                    ("battens", &c.batten),
                    ("spars", &c.spar),
                    ("rudder", &c.rudder),
                    ("inlays", &c.inlay),
                    ("lantern iron", &c.iron),
                    ("roundel and eyes", &c.mark),
                    ("gilt ring", &c.ring),
                    ("matting", &c.mat),
                ] {
                    assert_eq!(
                        m.emission_strength.0, 0.0,
                        "seed {s} in {}: the {what} is self-lit",
                        scheme.name
                    );
                }
                assert_eq!(
                    c.window, window,
                    "seed {s} in {}: the quarter windows are not the window light",
                    scheme.name
                );
                assert!(
                    c.lamp.emission_strength.0 > 0.0,
                    "seed {s} in {}: the stern lantern is dark",
                    scheme.name
                );
            }
            junks += 1;
        }
        assert!(junks > 50, "only {junks} junk seeds under 3000");
    }

    /// Two owners rarely wear the same livery AND the same accent: the two
    /// draws are independent streams, which is what keeps a craft
    /// recognisable across a settlement.
    #[test]
    fn a_livery_is_decorrelated_from_the_accent() {
        let mut same = 0;
        let mut n = 0;
        for s in (0u64..2_000).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let a = PartCtx::for_seed(s);
            let b = PartCtx::for_seed(s + 1);
            if skiff_livery(s, None).name == skiff_livery(s + 1, None).name
                && (luma(a.palette.primary_accent) - luma(b.palette.primary_accent)).abs() < 0.02
            {
                same += 1;
            }
            n += 1;
        }
        assert!(n > 100, "too few skiffs sampled: {n}");
        assert!(
            same * 10 < n,
            "{same} of {n} adjacent seeds matched on livery AND accent value"
        );
    }
}
