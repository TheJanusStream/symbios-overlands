//! A world's own name and banner (#1783): what the world says it is, beside
//! the verified handle that says whose it is.
//!
//! The name is the owner's claim about their world, and it is shown as one:
//! beside the handle, never in its place. Whose world a visitor stands in is
//! a safety question (#1231 f27, #1276 f46), answered only by a handle the
//! app has verified, so the name may not dress itself as one - it never
//! begins with the handle's `@` (that sigil belongs to the handle tier
//! alone, #1218 f299), it is never a `did:`, no full stop in it runs into a
//! letter as a handle's do, and nothing that paints as nothing can hide one
//! of those from the check ([`clean_world_name`]).
//!
//! The banner is two colours, a division of the field between them and a
//! mark drawn across it in the other colour of wherever it lies - the
//! heraldic counterchange, so a mark reads on any division. It is drawn by
//! [`crate::banner`] into one image every sign showing "this world's
//! banner" ([`crate::pds::SignSource::WorldBanner`]) shares.
//!
//! A seeded world derives both from its own stream
//! (`seeded_defaults::room::identity`); an owner changes them in the World
//! Editor. A record with no identity - every record saved before it - is
//! elided on the wire and keeps its bytes.

use serde::{Deserialize, Serialize};

use super::sanitize::limits::MAX_WORLD_NAME_CHARS;
use super::types::Fp3;

/// A world's name and banner (see the module docs).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct WorldIdentity {
    /// The world's name, at most [`MAX_WORLD_NAME_CHARS`] characters
    /// ([`clean_world_name`]). Empty names nothing: the world is shown as
    /// its owner's, as a world without an identity is.
    pub name: String,
    /// The world's banner.
    pub banner: WorldBanner,
}

impl WorldIdentity {
    /// Make the identity safe to show: its name cleaned
    /// ([`clean_world_name`]), its banner's colours clamped and a division or
    /// a mark this build does not know drawn - and saved - as the default.
    pub fn sanitize(&mut self) {
        self.name = clean_world_name(&self.name);
        self.banner.sanitize();
    }
}

/// The marks that read as a handle's sigil: `@`, its full-width and its
/// small forms.
const SIGILS: [char; 3] = ['@', '\u{FF20}', '\u{FE6B}'];

/// The colons a `did:` reads as one with: the ASCII, the full-width, the
/// small, the vertical presentation form, the modifier letter and the ratio.
const COLONS: [char; 6] = [
    ':', '\u{FF1A}', '\u{FE55}', '\u{FE13}', '\u{A789}', '\u{2236}',
];

/// The full stops a handle's dots read as: the ASCII, the one-dot leader,
/// the small and the full-width.
const DOTS: [char; 4] = ['.', '\u{2024}', '\u{FE52}', '\u{FF0E}'];

/// Whether `c` paints as nothing in the app's text (#1783): every Unicode
/// default-ignorable code point - bidi controls, joiners, variation
/// selectors, tags, fillers, the deprecated format characters, which the
/// app's font engine draws as nothing and a check of a name's first
/// character would not see past - on top of
/// [`is_invisible`](super::sanitize::names::is_invisible)'s.
fn ignorable(c: char) -> bool {
    super::sanitize::names::is_invisible(c)
        || matches!(
            u32::from(c),
            0x034F
                | 0x115F..=0x1160
                | 0x17B4..=0x17B5
                | 0x180B..=0x180F
                | 0x2060..=0x206F
                | 0x3164
                | 0xFE00..=0xFE0F
                | 0xFFA0
                | 0xFFF0..=0xFFF8
                | 0x1BCA0..=0x1BCA3
                | 0x1D173..=0x1D17A
                | 0xE0000..=0xE0FFF
        )
}

/// Whether `c` is a combining mark of the general blocks: it draws over the
/// character before it, and at a name's head over nothing.
fn combining(c: char) -> bool {
    matches!(
        u32::from(c),
        0x0300..=0x036F
            | 0x0483..=0x0489
            | 0x0591..=0x05BD
            | 0x0610..=0x061A
            | 0x064B..=0x065F
            | 0x0670
            | 0x1AB0..=0x1AFF
            | 0x1DC0..=0x1DFF
            | 0x20D0..=0x20FF
            | 0xFE20..=0xFE2F
    )
}

/// `raw` as it paints: invisible characters out ([`clean_name`]'s, and every
/// other [default-ignorable](ignorable) one), runs of spaces one, trimmed.
///
/// [`clean_name`]: super::sanitize::names::clean_name
fn visible(raw: &str) -> String {
    let kept = super::sanitize::names::clean_name(raw, usize::MAX).unwrap_or_default();
    // A joiner `clean_name` kept is doing visible work between two
    // characters (an emoji family); every other ignorable goes.
    let kept: String = kept
        .chars()
        .filter(|&c| c == '\u{200D}' || !ignorable(c))
        .collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `name` without what may not begin a world's name: a sigil, a space, or a
/// combining mark over nothing.
fn without_head(name: &str) -> &str {
    name.trim_start_matches(|c: char| SIGILS.contains(&c) || c.is_whitespace() || combining(c))
}

/// Whether `name` begins as a DID does: `did` and a colon, in any case and
/// with any colon that reads as one.
fn reads_as_did(name: &str) -> bool {
    let mut chars = name.chars();
    let head: String = chars.by_ref().take(3).collect();
    head.eq_ignore_ascii_case("did") && chars.next().is_some_and(|c| COLONS.contains(&c))
}

/// Whether `name` holds a full stop run straight into a letter or a digit,
/// as a handle's are (`alice.bsky.social`).
fn reads_as_handle(name: &str) -> bool {
    let chars: Vec<char> = name.chars().collect();
    chars
        .windows(2)
        .any(|pair| DOTS.contains(&pair[0]) && pair[1].is_alphanumeric())
}

/// `raw` as a world's name may stand: what [paints](visible), with no
/// leading `@` (nor a full-width or a small one, nor anything that paints
/// as nothing before it), never a `did:`, a space after every full stop
/// run into a letter or a digit - `alice.bsky.social` stands as
/// `alice. bsky. social`, which reads as no handle - and at most
/// [`MAX_WORLD_NAME_CHARS`] characters; empty where nothing is left.
pub fn clean_world_name(raw: &str) -> String {
    let shown = visible(raw);
    let name = without_head(&shown);
    if reads_as_did(name) {
        return String::new();
    }
    let mut spaced = String::with_capacity(name.len());
    let mut chars = name.chars().peekable();
    while let Some(c) = chars.next() {
        spaced.push(c);
        if DOTS.contains(&c) && chars.peek().is_some_and(|next| next.is_alphanumeric()) {
            spaced.push(' ');
        }
    }
    super::sanitize::names::clean_name(&spaced, MAX_WORLD_NAME_CHARS).unwrap_or_default()
}

/// Why a draft name would not stand as it is typed, or `None` (#1783): what
/// the World Editor says beside the field, so that the sanitiser
/// ([`clean_world_name`]) never changes a name the owner sees under their
/// cursor - beyond dropping what paints as nothing and joining runs of
/// spaces. An empty draft names nothing, and stands.
pub fn world_name_refusal(draft: &str) -> Option<String> {
    if draft.trim().is_empty() {
        return None;
    }
    let shown = visible(draft);
    if shown.is_empty() {
        return Some("Nothing of it would show.".into());
    }
    let name = without_head(&shown);
    if shown
        .trim_start_matches(|c: char| c.is_whitespace() || combining(c))
        .starts_with(SIGILS)
    {
        return Some(
            "A world's name may not start with @ - that is how handles are written.".into(),
        );
    }
    if reads_as_did(name) {
        return Some("A world's name may not be a DID.".into());
    }
    if name.is_empty() {
        return Some("Nothing of it would show.".into());
    }
    if reads_as_handle(name) {
        return Some(
            "A world's name may not read as a handle: put a space after a full stop.".into(),
        );
    }
    if name.chars().count() > MAX_WORLD_NAME_CHARS {
        return Some(format!("At most {MAX_WORLD_NAME_CHARS} characters."));
    }
    None
}

/// A world's banner: two colours, a division of the field and a mark (see
/// the module docs).
///
/// Every field defaults ([`WorldBanner::default`]'s), so a banner missing one
/// - written by another client - reads rather than failing the record.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct WorldBanner {
    /// The field's colour, sRGB 0 to 1.
    pub field: Fp3,
    /// The other colour, sRGB 0 to 1: the division's other part, and the
    /// mark's colour wherever it lies on the field's.
    pub second: Fp3,
    /// How the field is divided between the two colours.
    pub division: BannerDivision,
    /// The mark across it.
    pub mark: BannerMark,
}

impl Default for WorldBanner {
    /// A red field with a gold star: what a banner starts as before it is
    /// derived or chosen.
    fn default() -> Self {
        Self {
            field: Fp3([0.62, 0.09, 0.1]),
            second: Fp3([0.84, 0.66, 0.2]),
            division: BannerDivision::Plain,
            mark: BannerMark::Star,
        }
    }
}

impl WorldBanner {
    /// Clamp its colours into 0 to 1 (a colour that is no number is black),
    /// and draw a division or a mark this build does not know as the
    /// default, so that the banner can always be written back.
    pub fn sanitize(&mut self) {
        for colour in [&mut self.field, &mut self.second] {
            colour.0 = colour.0.map(|c| {
                if c.is_finite() {
                    c.clamp(0.0, 1.0)
                } else {
                    0.0
                }
            });
        }
        if self.division == BannerDivision::Unknown {
            self.division = BannerDivision::default();
        }
        if self.mark == BannerMark::Unknown {
            self.mark = BannerMark::default();
        }
    }
}

/// How a banner's field is divided between its two colours.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(tag = "$type")]
pub enum BannerDivision {
    /// One colour: the second is the mark's.
    #[default]
    #[serde(rename = "network.symbios.banner.division.plain")]
    Plain,
    /// Halved down the middle, the second colour on the viewer's right.
    #[serde(rename = "network.symbios.banner.division.per_pale")]
    PerPale,
    /// Halved across, the second colour below.
    #[serde(rename = "network.symbios.banner.division.per_fess")]
    PerFess,
    /// Halved on a diagonal from the top on the viewer's left, the second
    /// colour below it.
    #[serde(rename = "network.symbios.banner.division.per_bend")]
    PerBend,
    /// Quartered, the second colour in the top right and the bottom left.
    #[serde(rename = "network.symbios.banner.division.quarterly")]
    Quarterly,
    /// Under a chevron's point, the second colour below.
    #[serde(rename = "network.symbios.banner.division.per_chevron")]
    PerChevron,
    /// A division a newer client wrote: drawn plain, and saved plain.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl BannerDivision {
    /// Every division a banner may have, as the World Editor offers them.
    pub const ALL: [Self; 6] = [
        Self::Plain,
        Self::PerPale,
        Self::PerFess,
        Self::PerBend,
        Self::Quarterly,
        Self::PerChevron,
    ];

    /// The name the World Editor shows.
    pub fn label(self) -> &'static str {
        match self {
            Self::Plain | Self::Unknown => "Plain",
            Self::PerPale => "Halved down",
            Self::PerFess => "Halved across",
            Self::PerBend => "Halved on a diagonal",
            Self::Quarterly => "Quartered",
            Self::PerChevron => "Under a chevron",
        }
    }
}

/// The mark across a banner.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(tag = "$type")]
pub enum BannerMark {
    /// No mark: the field alone.
    #[serde(rename = "network.symbios.banner.mark.none")]
    None,
    /// A five-pointed star.
    #[default]
    #[serde(rename = "network.symbios.banner.mark.star")]
    Star,
    /// A crescent, horns up.
    #[serde(rename = "network.symbios.banner.mark.crescent")]
    Crescent,
    /// A rayed sun.
    #[serde(rename = "network.symbios.banner.mark.sun")]
    Sun,
    /// A disc.
    #[serde(rename = "network.symbios.banner.mark.roundel")]
    Roundel,
    /// A diamond.
    #[serde(rename = "network.symbios.banner.mark.lozenge")]
    Lozenge,
    /// A cross of equal arms.
    #[serde(rename = "network.symbios.banner.mark.cross")]
    Cross,
    /// A diagonal cross.
    #[serde(rename = "network.symbios.banner.mark.saltire")]
    Saltire,
    /// A battlemented tower with its door.
    #[serde(rename = "network.symbios.banner.mark.tower")]
    Tower,
    /// A fir tree.
    #[serde(rename = "network.symbios.banner.mark.tree")]
    Tree,
    /// Mountain peaks.
    #[serde(rename = "network.symbios.banner.mark.peaks")]
    Peaks,
    /// Three waves.
    #[serde(rename = "network.symbios.banner.mark.waves")]
    Waves,
    /// A mark a newer client wrote: drawn as the default, and saved so.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl BannerMark {
    /// Every mark a banner may carry, as the World Editor offers them.
    pub const ALL: [Self; 12] = [
        Self::None,
        Self::Star,
        Self::Crescent,
        Self::Sun,
        Self::Roundel,
        Self::Lozenge,
        Self::Cross,
        Self::Saltire,
        Self::Tower,
        Self::Tree,
        Self::Peaks,
        Self::Waves,
    ];

    /// The name the World Editor shows.
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Star | Self::Unknown => "Star",
            Self::Crescent => "Crescent",
            Self::Sun => "Sun",
            Self::Roundel => "Disc",
            Self::Lozenge => "Diamond",
            Self::Cross => "Cross",
            Self::Saltire => "Diagonal cross",
            Self::Tower => "Tower",
            Self::Tree => "Tree",
            Self::Peaks => "Peaks",
            Self::Waves => "Waves",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> WorldIdentity {
        WorldIdentity {
            name: "Frostmere".into(),
            banner: WorldBanner {
                field: Fp3([0.1, 0.26, 0.6]),
                second: Fp3([0.92, 0.91, 0.87]),
                division: BannerDivision::PerBend,
                mark: BannerMark::Tower,
            },
        }
    }

    #[test]
    fn an_identity_round_trips_on_the_wire() {
        let identity = identity();
        let json = serde_json::to_string(&identity).unwrap();
        assert!(
            json.contains("network.symbios.banner.division.per_bend")
                && json.contains("network.symbios.banner.mark.tower")
                && json.contains("[1000,2600,6000]"),
            "{json}"
        );
        let back: WorldIdentity = serde_json::from_str(&json).unwrap();
        assert_eq!(back, identity);
    }

    /// A division or a mark a newer client wrote reads, and is drawn and
    /// written back as the default, so a world carrying one still saves.
    #[test]
    fn an_unknown_division_or_mark_is_drawn_and_saved_as_the_default() {
        let json = r#"{"name":"Ashcombe","banner":{"field":[0,0,0],"second":[10000,10000,10000],
            "division":{"$type":"network.symbios.banner.division.per_saltire"},
            "mark":{"$type":"network.symbios.banner.mark.griffin"}}}"#;
        let mut identity: WorldIdentity = serde_json::from_str(json).unwrap();
        assert_eq!(identity.banner.division, BannerDivision::Unknown);
        assert_eq!(identity.banner.mark, BannerMark::Unknown);
        identity.sanitize();
        assert_eq!(identity.banner.division, BannerDivision::Plain);
        assert_eq!(identity.banner.mark, BannerMark::Star);
        assert!(serde_json::to_string(&identity).is_ok());
    }

    #[test]
    fn a_banner_keeps_its_colours_in_range() {
        let mut banner = WorldBanner {
            field: Fp3([-0.5, 1.5, f32::NAN]),
            ..WorldBanner::default()
        };
        banner.sanitize();
        assert_eq!(banner.field, Fp3([0.0, 1.0, 0.0]));
    }

    /// The name is the owner's claim, shown beside the verified handle: it
    /// may not dress itself as one, nor hide in invisible characters.
    #[test]
    fn a_name_is_never_a_handle_nor_a_did() {
        for (raw, want) in [
            ("Frostmere", "Frostmere"),
            ("  Salt   holm  ", "Salt holm"),
            // A handle stands as no handle: its sigil out, its dots spaced.
            ("@alice.bsky.social", "alice. bsky. social"),
            ("alice.bsky.social", "alice. bsky. social"),
            (
                "alice\u{2024}bsky\u{FF0E}social",
                "alice\u{2024} bsky\u{FF0E} social",
            ),
            ("St. Ives", "St. Ives"),
            ("Mt.Doom", "Mt. Doom"),
            ("The End.", "The End."),
            ("\u{FE6B}Saltholm", "Saltholm"),
            ("did\u{FF1A}plc:abc", ""),
            ("DID\u{2236}web", ""),
            ("@@ @Saltholm", "Saltholm"),
            ("\u{FF20}Saltholm", "Saltholm"),
            ("did:plc:abcdefg", ""),
            ("@DID:web:example.com", ""),
            ("Did you know", "Did you know"),
            ("Frost\u{202E}mere", "Frostmere"),
            ("Frost\u{200B}mere\n", "Frostmere"),
            ("\u{200B}", ""),
            // No invisible character hides a sigil or a `did:`.
            ("\u{200B}@Saltholm", "Saltholm"),
            ("@\u{200B}did:plc:abcdefg", ""),
            ("d\u{200B}id:plc:abcdefg", ""),
            // Nor any character the app's font draws as nothing (#1783's
            // critic): the deprecated format characters, a grapheme joiner,
            // a variation selector, a tag - nor a combining mark over nothing.
            ("\u{206A}@alice.bsky.social", "alice. bsky. social"),
            ("\u{206F}did:plc:abcdefg", ""),
            ("\u{034F}@x", "x"),
            ("\u{FE0F}@x", "x"),
            ("\u{E0041}@x", "x"),
            ("\u{0301}@x", "x"),
            ("\u{0301}\u{0301}did:plc:x", ""),
            // A joined emoji keeps its joiner; a variation selector goes.
            (
                "\u{1F468}\u{200D}\u{1F469} Hollow",
                "\u{1F468}\u{200D}\u{1F469} Hollow",
            ),
            ("\u{2764}\u{FE0F} Vale", "\u{2764} Vale"),
        ] {
            assert_eq!(clean_world_name(raw), want, "{raw:?}");
        }
        let long = "a".repeat(MAX_WORLD_NAME_CHARS + 10);
        assert_eq!(
            clean_world_name(&long).chars().count(),
            MAX_WORLD_NAME_CHARS
        );
        // Cut on a character, never inside one.
        let wide = "\u{f6}".repeat(MAX_WORLD_NAME_CHARS + 3);
        assert_eq!(
            clean_world_name(&wide).chars().count(),
            MAX_WORLD_NAME_CHARS
        );
    }

    /// A cleaned name is its own clean form: what the sanitiser keeps reads
    /// back as itself.
    #[test]
    fn a_clean_name_stays_clean() {
        let joined_at_the_cut = format!(
            "{}\u{200D}\u{e9}",
            "\u{e9}".repeat(MAX_WORLD_NAME_CHARS - 1)
        );
        for raw in [
            joined_at_the_cut.as_str(),
            "alice.bsky.social",
            "\u{206A}@alice.bsky.social",
            "Frostmere",
            "@x",
            "  a  b  ",
            "Neuk\u{f6}lln",
            "Vega Station",
        ] {
            let once = clean_world_name(raw);
            assert_eq!(clean_world_name(&once), once, "{raw:?}");
        }
    }

    /// The World Editor's reasons agree with the sanitiser: what they let
    /// stand, it keeps as the owner typed it (bar what paints as nothing
    /// and runs of spaces); what they refuse, it would change.
    #[test]
    fn a_refused_draft_is_one_the_sanitiser_would_change() {
        for draft in [
            "Frostmere",
            "Vega Station",
            "St. Ives",
            "Neuk\u{f6}lln",
            "",
            "   ",
        ] {
            assert_eq!(world_name_refusal(draft), None, "{draft:?}");
            assert_eq!(clean_world_name(draft), draft.trim(), "{draft:?}");
        }
        for (draft, says) in [
            ("@alice", "@"),
            ("  \u{FF20}x", "@"),
            ("\u{0301}@x", "@"),
            ("\u{206A}@x", "@"),
            ("did:plc:abc", "DID"),
            ("\u{200B}\u{206A}", "Nothing"),
            ("alice.bsky.social", "handle"),
        ] {
            let reason = world_name_refusal(draft);
            assert!(
                reason.as_deref().is_some_and(|r| r.contains(says)),
                "{draft:?}: {reason:?}"
            );
            assert_ne!(clean_world_name(draft), draft.trim(), "{draft:?}");
        }
        let long = "x".repeat(MAX_WORLD_NAME_CHARS + 1);
        assert!(world_name_refusal(&long).is_some());
        // Counted as it paints: invisible characters and a run of spaces
        // spend nothing of the budget.
        let padded = format!(
            "{}\u{200B}\u{200B}   y",
            "x".repeat(MAX_WORLD_NAME_CHARS - 2)
        );
        assert_eq!(world_name_refusal(&padded), None);
    }

    /// A banner missing a field - written by another client - reads, with
    /// the default's colour, rather than failing the world's record.
    #[test]
    fn a_banner_missing_a_field_still_reads() {
        let identity: WorldIdentity =
            serde_json::from_str(r#"{"name":"Ashcombe","banner":{}}"#).unwrap();
        assert_eq!(identity.banner, WorldBanner::default());
        let half: WorldBanner = serde_json::from_str(r#"{"field":[0,0,0]}"#).unwrap();
        assert_eq!(half.field, Fp3([0.0, 0.0, 0.0]));
        assert_eq!(half.second, WorldBanner::default().second);
    }
}
