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
//!   canvas and varnish on a boat; coachwork, wings and brightwork on a car.
//!   It is picked per seed from a curated list on this module's own salted
//!   stream, exactly as the craft TYPE is picked in
//!   [`crate::seeded_defaults::avatar::craft`].
//! - The **seeded accent** ([`primary_accent`](crate::seeded_defaults::AvatarPalette::primary_accent))
//!   is spent on the
//!   identity slots ONLY: the boot stripe, the burgee and the jib on a boat;
//!   the coachline, the wheel centres and the hide on a car. The secondary
//!   still tints the antifoul and the upholstery a little so two craft of one
//!   scheme are not identical, and the tertiary still lights the windows.
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
//! the lit ports that were always lit. A car is unchanged, because a lit
//! coachline and lit wheel centres on a cyberpunk machine are exactly the
//! point and were never the complaint.
//!
//! # The two colour rules this fleet learned by render
//!
//! 1. **A guard drawn at the tyre's value is not a guard** (#1364). Coachwork
//!    has to stay clear of rubber, so every skiff mass is floored at
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
use crate::seeded_defaults::MaterialKit;
use crate::seeded_defaults::scene::pick_weighted;

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

    /// A boat's glow is her burgee alone: her hull lines are paint on every
    /// theme, and the masthead flag lights exactly when the style is luminous.
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
