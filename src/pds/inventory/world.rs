//! World items (#1839, epic #1837): a whole world, saved to the Inventory
//! and loaded back over the owner's world as a re-roll replaces it.
//!
//! A world is everything its [`RoomRecord`] holds - ground, sky, every
//! asset and placement, its name - so the item is a snapshot of the record,
//! whatever kind of world it is: procedural or a Berlin square, seeded or
//! authored. It does not fit one record (about half of fresh seeded worlds
//! already need more than one 120 KiB `applyWrites` request to save), so it
//! is written in two halves:
//!
//!   - a small **head** ([`WorldHead`]) rides the item record in
//!     [`crate::pds::INVENTORY_ITEM_COLLECTION`] as its `world` payload: what
//!     the row and the pane say about the world ([`WorldSummary`]), the
//!     part keys in order, the JSON's byte length and its digest. The login
//!     listing reads heads only.
//!   - the world's JSON - the record's own serde JSON, the text the World
//!     Editor's Raw JSON tab shows, compact and with every map in key order
//!     ([`canonical_json`]) - is cut on char boundaries into **parts**
//!     ([`PartRecord`]) in [`crate::pds::INVENTORY_PART_COLLECTION`], each
//!     small enough that its write fits one request ([`PART_DATA_BUDGET`]),
//!     each keyed by its own content ([`part_rkey`]). Parts are fetched by
//!     `getRecord` only when a world is loaded ([`load_world`]) - or, from
//!     the giver's repo, when a world given to this client is accepted
//!     (#1844) - a few at a time, joined, checked against the head's byte
//!     length, digest and part keys, decoded and passed through the SAME
//!     sanitiser a fetched room goes through ([`join_parts`]).
//!
//! A world is given by its head alone (#1844), and only from the saved
//! copy: every part the head names must be named by a stored head
//! ([`gift_blocked_reason`]), because the recipient reads the parts from the
//! giver's data server. What it keeps is its own snapshot of the world it
//! fetched and sanitised, never the giver's text.
//!
//! Content keys make parts shareable: two items holding one world, or an
//! item renamed (a new head over the same parts), name the same records.
//! The save asks the data server about every part a head it writes names
//! and every part it may delete ([`probe_parts`]) - the stored copy can be
//! behind it - and plans from the answers: it creates the needed parts
//! that are not there ([`plan_part_creates`]), refusing by name when one is
//! missing and its text is gone, and deletes a part only when a stored head
//! named it, no live head does, and it is there to delete
//! ([`plan_part_deletes`]). The inventory's own plan
//! (`plan_item_writes`) puts them around the heads in the order that keeps
//! every read safe: part creates, heads, head deletes, part deletes.
//!
//! In memory a [`WorldItem`] is its head plus, while this session holds
//! them, the parts' text - on a field serde skips, so the Inventory's
//! per-frame dirty check (#1292, #1322) serialises a few hundred bytes of
//! head, never the world. A new collection is a new OAuth grant: a sign-in
//! whose grant predates it is refused up front ([`world_grant_refusal`]),
//! never after a partial write.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::pds::INVENTORY_PART_COLLECTION;
use crate::pds::generator::Placement;
use crate::pds::room::RoomRecord;
use crate::pds::sanitize::limits;
use crate::pds::xrpc::{MAX_APPLY_WRITES_BYTES, RepoWrite};

/// The largest world JSON a world item may hold, derived from what the
/// room's own save lets an owner write: every record a room save writes
/// must fit one `applyWrites` request ([`MAX_APPLY_WRITES_BYTES`] - the
/// save refuses past it), and a room is one manifest plus at most
/// [`limits::MAX_GENERATORS`] child generators, beside its instance sets'
/// copies - at most [`limits::MAX_ROOM_INSTANCE_COPIES`] of them, each
/// written in well under [`COPY_JSON_BYTES`]. The world's JSON holds the
/// same content inline, so any world the room can save fits. About 33 MiB.
pub const MAX_WORLD_SNAPSHOT_BYTES: usize = (1 + limits::MAX_GENERATORS) * MAX_APPLY_WRITES_BYTES
    + limits::MAX_ROOM_INSTANCE_COPIES * COPY_JSON_BYTES;

/// A generous bound on one instance copy's JSON: about 46 bytes at the
/// widest numbers a copy can be written with (the copy parts' own sizing).
const COPY_JSON_BYTES: usize = 64;

/// What a part's `applyWrites#create` carries besides its data, rounded
/// well up: the write's `$type`, collection and key, the record's `$type`,
/// and the punctuation. Measured at about 230 bytes;
/// `every_part_fits_one_request` keeps the margin honest.
const PART_WRITE_OVERHEAD: usize = 512;

/// The most a part's data may weigh as the request carries it - escaped
/// as a JSON string - so its write stays under one request
/// ([`MAX_APPLY_WRITES_BYTES`]).
pub const PART_DATA_BUDGET: usize = MAX_APPLY_WRITES_BYTES - PART_WRITE_OVERHEAD;

/// The most parts a head may name. Escaping at most doubles a character
/// of compact JSON (`"` and `\` become two; control characters never appear
/// raw), so every part but the last carries at least half the budget of the
/// world's text.
pub const MAX_WORLD_PARTS: usize = MAX_WORLD_SNAPSHOT_BYTES.div_ceil(PART_DATA_BUDGET / 2);

/// The most one part's `getRecord` body may weigh: a part's write fits a
/// request, and the response wraps the record in its `uri` and `cid`.
const PART_FETCH_CAP: usize = MAX_APPLY_WRITES_BYTES + 4 * 1024;

/// The most a whole load may read: every part at its cap. A load also
/// stops as soon as the parts read outweigh what the head says the world
/// weighs, escaped ([`fetch_parts`]), which is far less for any real head.
pub const MAX_WORLD_FETCH_BYTES: usize = MAX_WORLD_PARTS * PART_FETCH_CAP;

/// How many parts a load fetches at once.
pub const PART_FETCH_CONCURRENCY: usize = 4;

/// The longest place name a summary keeps: the borough names are shorter.
const MAX_SUMMARY_PLACE_CHARS: usize = 48;

/// Salt of a content key's second hash, so a key is two independent
/// 64-bit FNV-1a passes over the text rather than one.
const KEY_SALT: &str = "network.symbios.overlands.inventory.part\u{0}";

/// What a world item's head says about its world (#1839): the row's tag
/// and the pane's text card. Taken from the record when it is saved; a
/// display, never a promise - the load reads the world itself.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct WorldSummary {
    /// The world's own name ([`crate::pds::WorldIdentity`]); empty for a
    /// world without one.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The dataset of real map data the world is built from (`berlin`), or
    /// `None` for a procedural world.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dataset: Option<String>,
    /// The Berlin borough its square's middle lies in, where it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub borough: Option<String>,
    /// How many assets (named generators) it holds.
    #[serde(default)]
    pub generators: u32,
    /// How many placements it holds.
    #[serde(default)]
    pub placements: u32,
    /// How many instance-set copies its placements hold between them.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub copies: u32,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

impl WorldSummary {
    /// The summary of `record`, as a head carries it.
    pub fn of(record: &RoomRecord) -> Self {
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        let copies: usize = record
            .placements
            .iter()
            .map(|placement| match placement {
                Placement::Instances { copies, .. } => copies.len(),
                _ => 0,
            })
            .sum();
        let geo = record.geo_source.as_ref();
        let mut summary = Self {
            name: record
                .identity
                .as_ref()
                .map(|identity| identity.name.clone())
                .unwrap_or_default(),
            dataset: geo.map(|source| source.dataset.clone()),
            borough: geo
                .and_then(|source| source.berlin_square())
                .and_then(|square| {
                    let (e, n) = square.centre();
                    geodata::berlin::Coverage::berlin()
                        .borough_at(e, n)
                        .map(|borough| borough.name().to_owned())
                }),
            generators: count(record.generators.len()),
            placements: count(record.placements.len()),
            copies: count(copies),
        };
        summary.sanitize();
        summary
    }

    /// Make the summary safe to show (#1839): the name cleaned as a world's
    /// own name is, a dataset id kept only in the form a geodata source may
    /// have, a borough kept only when it is one, the counts within what a
    /// world may hold. A hostile head shows nothing worse than wrong numbers.
    pub fn sanitize(&mut self) {
        self.name = crate::pds::identity::clean_world_name(&self.name);
        self.dataset = self.dataset.take().filter(|id| {
            (1..=crate::pds::geo_source::MAX_DATASET_CHARS).contains(&id.len())
                && id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        });
        self.borough = self.borough.take().filter(|name| {
            name.chars().count() <= MAX_SUMMARY_PLACE_CHARS
                && geodata::berlin::Borough::ALL
                    .iter()
                    .any(|borough| borough.name() == name)
        });
        let at_most = |n: u32, cap: usize| n.min(u32::try_from(cap).unwrap_or(u32::MAX));
        self.generators = at_most(self.generators, limits::MAX_GENERATORS);
        self.placements = at_most(self.placements, limits::MAX_PLACEMENTS);
        self.copies = at_most(self.copies, limits::MAX_ROOM_INSTANCE_COPIES);
    }

    /// What kind of world it is, as a person reads it: "a Berlin square in
    /// Neukoelln", "a procedural world".
    pub fn kind_label(&self) -> String {
        match (self.dataset.as_deref(), self.borough.as_deref()) {
            (None, _) => String::from("a procedural world"),
            (Some(crate::pds::geo_source::BERLIN), Some(borough)) => {
                format!("a Berlin square in {borough}")
            }
            (Some(crate::pds::geo_source::BERLIN), None) => String::from("a Berlin square"),
            (Some(_), _) => String::from("a square of real map data"),
        }
    }

    /// The short kind a row's tag carries: "Berlin" or "procedural".
    pub fn kind_tag(&self) -> &'static str {
        match self.dataset.as_deref() {
            None => "procedural",
            Some(crate::pds::geo_source::BERLIN) => "Berlin",
            Some(_) => "map data",
        }
    }
}

/// A world item's payload on the wire - the item record's `world` field.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct WorldHead {
    /// What the row and the pane say.
    #[serde(default)]
    pub summary: WorldSummary,
    /// The part keys, in order: the world's JSON is their data joined.
    #[serde(default)]
    pub parts: Vec<String>,
    /// The JSON's length in bytes.
    #[serde(default)]
    pub bytes: u64,
    /// The JSON's [`digest_of`].
    #[serde(default)]
    pub digest: String,
}

/// One part of a world item's JSON on the wire: a record in
/// [`INVENTORY_PART_COLLECTION`] at `rkey =` [`part_rkey`]`(data)`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PartRecord {
    #[serde(rename = "$type")]
    pub lex_type: String,
    /// A slice of the world's JSON, cut on a char boundary.
    pub data: String,
}

impl PartRecord {
    fn new(data: &str) -> Self {
        Self {
            lex_type: INVENTORY_PART_COLLECTION.into(),
            data: data.into(),
        }
    }
}

/// A world item in memory: its head, and - while this session holds them -
/// the parts' text, in the head's order.
///
/// Serialises as its head alone (`transparent`, the text `skip`ped), so a
/// stash holding worlds of megabytes still compares and measures in head
/// bytes. Two items are equal when their heads are: the parts are a pure
/// function of the content the digest and the part keys address.
#[derive(Serialize, Deserialize, Clone)]
#[serde(transparent)]
pub struct WorldItem {
    head: WorldHead,
    #[serde(skip)]
    slices: Option<Arc<[String]>>,
}

impl PartialEq for WorldItem {
    fn eq(&self, other: &Self) -> bool {
        self.head == other.head
    }
}

impl std::fmt::Debug for WorldItem {
    // The text is megabytes of the owner's world: a debug line says how
    // many parts are in hand, never what they hold.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorldItem")
            .field("head", &self.head)
            .field("parts_in_memory", &self.slices.as_ref().map(|s| s.len()))
            .finish()
    }
}

impl WorldItem {
    /// An item that has only its head - what the login listing lands.
    pub fn from_head(head: WorldHead) -> Self {
        Self { head, slices: None }
    }

    /// Copy `record` - the live world, unsaved edits included - into an
    /// item: its JSON cut into parts, held in memory until a save writes
    /// them. Refuses, with the sentence to show, a world this build cannot
    /// copy whole ([`snapshot_refused`]) and one past
    /// [`MAX_WORLD_SNAPSHOT_BYTES`].
    pub fn snapshot(record: &RoomRecord) -> Result<Self, String> {
        if let Some(reason) = snapshot_refused(record) {
            return Err(reason);
        }
        let text = canonical_json(record)?;
        if let Some(reason) = too_large(text.len()) {
            return Err(reason);
        }
        let slices = slice_text(&text, PART_DATA_BUDGET);
        let head = WorldHead {
            summary: WorldSummary::of(record),
            parts: slices.iter().map(|slice| part_rkey(slice)).collect(),
            bytes: text.len() as u64,
            digest: digest_of(&text),
        };
        Ok(Self {
            head,
            slices: Some(slices.into()),
        })
    }

    /// An item over `text` cut into slices of at most `budget`, with an
    /// empty summary - a world's parts without the cost of building one,
    /// for the save plan's tests.
    #[cfg(test)]
    pub(crate) fn of_text(text: &str, budget: usize) -> Self {
        let slices = slice_text(text, budget);
        Self {
            head: WorldHead {
                summary: WorldSummary::default(),
                parts: slices.iter().map(|slice| part_rkey(slice)).collect(),
                bytes: text.len() as u64,
                digest: digest_of(text),
            },
            slices: Some(slices.into()),
        }
    }

    /// The head - what the wire carries and what compares.
    pub fn head(&self) -> &WorldHead {
        &self.head
    }

    /// The parts' text, in the head's order, while this session holds it.
    pub fn slices(&self) -> Option<&Arc<[String]>> {
        self.slices.as_ref()
    }

    /// Let the parts' text go - after a save has written them, so a session
    /// that saved a large world does not keep it twice (wasm's heap never
    /// shrinks). A load fetches them again.
    pub fn forget_slices(&mut self) {
        self.slices = None;
    }

    /// Bound what the head shows ([`WorldSummary::sanitize`]). The rest is
    /// left as it came - a head this build would refuse to load is still
    /// written back unchanged by a rename, never lossy - and judged when a
    /// load reads it ([`check_head`]).
    pub fn sanitize(&mut self) {
        self.head.summary.sanitize();
    }
}

/// Why `record` cannot be copied whole into a world item, or `None` when it
/// can (#1839): what the room's own save refuses
/// ([`crate::pds::room::write_refused`] - a placement or a kind of a newer
/// build), and generators this build holds by reference only, because it
/// could not read them ([`RoomRecord::opaque_refs`]) - a copy would hold a
/// key into the owner's room that a loaded world cannot follow.
pub fn snapshot_refused(record: &RoomRecord) -> Option<String> {
    if let Some(reason) = crate::pds::room::write_refused(record) {
        return Some(reason);
    }
    let held = record.opaque_refs.len();
    (held > 0).then(|| {
        format!(
            "this world holds {held} {} made by a newer version of Overlands, which this build \
             reads by reference only and cannot copy. Update, and try again.",
            crate::text::plural(held, "item", "items")
        )
    })
}

/// Why a world `bytes` long is past what an item may hold, if it is.
fn too_large(bytes: usize) -> Option<String> {
    (bytes > MAX_WORLD_SNAPSHOT_BYTES).then(|| {
        format!(
            "this world is {} - past the {} a world item can hold.",
            crate::pds::record_size::human_bytes(bytes),
            crate::pds::record_size::human_bytes(MAX_WORLD_SNAPSHOT_BYTES),
        )
    })
}

/// Why the live world cannot be copied into an item right now, or `None`
/// when it can: [`snapshot_refused`], then its size. The question a
/// disabled Save to Inventory asks; it serialises the world, so the World
/// Editor asks it on its size readout's throttle, not every frame.
pub fn snapshot_blocked(record: &RoomRecord) -> Option<String> {
    if let Some(reason) = snapshot_refused(record) {
        return Some(reason);
    }
    match canonical_json(record) {
        Ok(text) => too_large(text.len()),
        Err(reason) => Some(reason),
    }
}

/// The world's JSON as an item holds it: the record's own serde JSON,
/// compact, with every map's keys in order - deterministic, so the same
/// world is always the same bytes and the same parts. A `HashMap`
/// serialises in its hasher's order, so the record goes through
/// `serde_json::Value`, whose maps are `BTreeMap`s as long as nothing in
/// the dependency graph turns on serde_json's `preserve_order` (nothing
/// does; `json_maps_iterate_in_key_order` fails if that changes).
pub fn canonical_json(record: &RoomRecord) -> Result<String, String> {
    let value = serde_json::to_value(record)
        .map_err(|e| crate::pds::record_size::unserializable_reason("world", &e.to_string()))?;
    serde_json::to_string(&value).map_err(|e| format!("serialize (world): {e}"))
}

/// How many bytes `c` takes inside a JSON string as serde_json writes it.
fn escaped_len(c: char) -> usize {
    match c {
        '"' | '\\' | '\u{08}' | '\u{0c}' | '\n' | '\r' | '\t' => 2,
        c if (c as u32) < 0x20 => 6,
        c => c.len_utf8(),
    }
}

/// Cut `text` into slices on char boundaries, each weighing at most
/// `budget` bytes escaped as a JSON string - in order, so joining them is
/// `text` again, byte for byte. Deterministic: the same text, the same
/// slices.
pub fn slice_text(text: &str, budget: usize) -> Vec<String> {
    let mut slices = Vec::new();
    let mut start = 0;
    let mut weight = 0;
    for (at, c) in text.char_indices() {
        let cost = escaped_len(c);
        if weight + cost > budget && at > start {
            slices.push(text[start..at].to_owned());
            start = at;
            weight = 0;
        }
        weight += cost;
    }
    if start < text.len() {
        slices.push(text[start..].to_owned());
    }
    slices
}

/// Two 64-bit FNV-1a passes over `text`, the second salted, as 32
/// lowercase hex characters.
fn content_key(text: &str) -> String {
    let first = crate::seeded_defaults::fnv1a_64(text);
    let second =
        crate::seeded_defaults::fnv1a_64_continue(crate::seeded_defaults::fnv1a_64(KEY_SALT), text);
    format!("{first:016x}{second:016x}")
}

/// Record key for a part: lowercase hex of the crate's `fnv1a_64` over its
/// data, then a second, salted pass - 32 characters of `[0-9a-f]`, always a
/// valid record key. Content-addressed, so equal parts are one record; the
/// second pass makes two different parts meeting on one key a 128-bit
/// accident rather than a 64-bit one.
pub fn part_rkey(data: &str) -> String {
    content_key(data)
}

/// The digest a head carries of the whole JSON: [`part_rkey`]'s two passes
/// over all of it. It guards against a part that is damaged or swapped,
/// not against a hostile data server - which writes the head too, and
/// which the sanitiser stands against.
pub fn digest_of(text: &str) -> String {
    content_key(text)
}

/// Whether `key` has the shape [`content_key`] gives: 32 lowercase hex.
fn is_content_key(key: &str) -> bool {
    key.len() == 32
        && key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Whether `head` could name a world this build would load, before
/// anything is fetched: some parts and not too many, a byte length an item
/// may hold, and keys of a part's shape - a key goes into a request's URL,
/// so nothing else is ever sent. Each refusal is the sentence to show.
pub fn check_head(head: &WorldHead) -> Result<usize, String> {
    const DAMAGED: &str = "This world item is damaged, so nothing was loaded";
    if head.parts.is_empty() {
        return Err(format!("{DAMAGED}: it names no parts."));
    }
    if head.parts.len() > MAX_WORLD_PARTS {
        return Err(format!(
            "{DAMAGED}: it names {} parts, more than any world is saved in ({MAX_WORLD_PARTS}).",
            head.parts.len()
        ));
    }
    let bytes = usize::try_from(head.bytes)
        .ok()
        .filter(|bytes| (head.parts.len()..=MAX_WORLD_SNAPSHOT_BYTES).contains(bytes))
        .ok_or_else(|| {
            format!(
                "{DAMAGED}: it says the world is {} bytes, which no saved world is.",
                head.bytes
            )
        })?;
    if !head.parts.iter().all(|key| is_content_key(key)) || !is_content_key(&head.digest) {
        return Err(format!("{DAMAGED}: it names a part that is not one."));
    }
    Ok(bytes)
}

/// Join a world item's parts back into its world (#1839) and make it safe:
/// every part the record its key names ([`part_rkey`]), the whole the
/// length and digest the head says, the JSON a world this build reads
/// whole - then the same sanitiser a fetched room goes through. Refusals
/// are the sentence to show; nothing is changed by one.
pub fn join_parts(head: &WorldHead, parts: &[String]) -> Result<RoomRecord, String> {
    let bytes = check_head(head)?;
    const DAMAGED: &str = "This world item is damaged, so nothing was loaded";
    if parts.len() != head.parts.len() {
        return Err(format!(
            "{DAMAGED}: {} of its {} parts arrived.",
            parts.len(),
            head.parts.len()
        ));
    }
    let mut text = String::with_capacity(bytes);
    for (index, (key, data)) in head.parts.iter().zip(parts).enumerate() {
        if part_rkey(data) != *key {
            return Err(format!(
                "{DAMAGED}: part {} is not the part its record names.",
                index + 1
            ));
        }
        if text.len() + data.len() > bytes {
            return Err(format!("{DAMAGED}: its parts are longer than it says."));
        }
        text.push_str(data);
    }
    if text.len() != bytes {
        return Err(format!("{DAMAGED}: its parts are shorter than it says."));
    }
    if digest_of(&text) != head.digest {
        return Err(format!(
            "{DAMAGED}: its parts do not add up to what it saved."
        ));
    }
    let mut record: RoomRecord = serde_json::from_str(&text)
        .map_err(|e| format!("This world item could not be read as a world - {e}"))?;
    record.lex_type = crate::pds::COLLECTION.into();
    // A snapshot never carries a reference into somebody's room
    // (`snapshot_refused`); one that does was not written by this app, and
    // the keys would point into the owner's room at records that are not
    // the world's.
    record.opaque_refs.clear();
    record.sanitize();
    if let Some(reason) = crate::pds::room::write_refused(&record) {
        return Err(format!(
            "This world was saved by a newer version of Overlands, so this build cannot load it \
             whole - {reason}"
        ));
    }
    Ok(record)
}

/// Every part key a set of heads names.
fn parts_named<'a>(worlds: impl IntoIterator<Item = &'a WorldItem>) -> BTreeSet<&'a str> {
    worlds
        .into_iter()
        .flat_map(|world| world.head.parts.iter().map(String::as_str))
        .collect()
}

/// Whether a save of `live` over `stored` writes or deletes any world item
/// - the saves a sign-in without the parts grant is refused.
pub fn touches_worlds(
    live: &HashMap<String, WorldItem>,
    stored: &HashMap<String, WorldItem>,
) -> bool {
    live.iter()
        .any(|(name, item)| stored.get(name) != Some(item))
        || stored.keys().any(|name| !live.contains_key(name))
}

/// What a save says to a sign-in whose grant predates the parts collection.
pub const SIGN_IN_AGAIN: &str = "This sign-in predates a permission saving worlds to your \
     inventory needs - a world's contents are kept in records of their own: sign out and in \
     again, then try again.";

/// Whether a sign-in whose grant is `scope` may write parts, as far as the
/// grant says (the room's own [`crate::pds::room::grant_covers`]).
pub fn grant_covers_parts(scope: &str) -> bool {
    crate::pds::room::grant_covers(scope, INVENTORY_PART_COLLECTION)
}

/// The refusal a save of `live` over `stored` meets before any write, when
/// it would write or delete a world item and the grant `scope` cannot
/// write parts.
pub fn world_grant_refusal(
    live: &HashMap<String, WorldItem>,
    stored: &HashMap<String, WorldItem>,
    scope: &str,
) -> Option<&'static str> {
    (touches_worlds(live, stored) && !grant_covers_parts(scope)).then_some(SIGN_IN_AGAIN)
}

/// Why a world item cannot be given yet (#1844): a world is given from the
/// giver's SAVED copy - the recipient fetches its parts from the giver's data
/// server - so every part its head names must be named by a head in the
/// stored stash. A renamed saved world qualifies (a new head over the same
/// parts); a new or edited one does not, until the Inventory is saved.
pub const GIFT_UNSAVED: &str = "Save your Inventory first - a world is given from your saved \
     copy, and this one isn't saved yet.";

/// Why a world whose head is damaged cannot be given (#1844).
pub const GIFT_DAMAGED: &str = "This world item is damaged, so it cannot be given.";

/// Why the world `head` names cannot be given from a stash whose stored
/// copy holds `stored` worlds, or `None` when it can: its head is one a load
/// would follow ([`check_head`]), and every part it names is named by a
/// stored head ([`GIFT_UNSAVED`]). No stored stash at all is nothing saved.
pub fn gift_blocked_reason(
    head: &WorldHead,
    stored: Option<&HashMap<String, WorldItem>>,
) -> Option<&'static str> {
    if check_head(head).is_err() {
        return Some(GIFT_DAMAGED);
    }
    let saved = stored.map(|stored| parts_named(stored.values()));
    let all_saved =
        saved.is_some_and(|saved| head.parts.iter().all(|key| saved.contains(key.as_str())));
    (!all_saved).then_some(GIFT_UNSAVED)
}

/// The heads a save of `live` over `stored` writes - creates and updates:
/// every live world whose head the stored one under its name is not.
fn written_heads<'a>(
    live: &'a HashMap<String, WorldItem>,
    stored: &'a HashMap<String, WorldItem>,
) -> impl Iterator<Item = (&'a String, &'a WorldItem)> {
    live.iter()
        .filter(move |(name, item)| stored.get(*name) != Some(*item))
}

/// The parts a save of `live` over `stored` needs on the data server, in
/// key order: every part a head it writes names - whether or not a stored
/// head names it too, because `stored` can be behind the data server (a
/// save whose commit landed and whose answer was lost leaves it naming
/// parts that save deleted). A head this build would refuse to load
/// ([`check_head`]) names nothing a load could follow either way: it is
/// written back as it came, its keys never sent.
fn parts_needed<'a>(
    live: &'a HashMap<String, WorldItem>,
    stored: &'a HashMap<String, WorldItem>,
) -> BTreeSet<&'a str> {
    written_heads(live, stored)
        .filter(|(_, item)| check_head(&item.head).is_ok())
        .flat_map(|(_, item)| item.head.parts.iter().map(String::as_str))
        .collect()
}

/// The parts a save of `live` over `stored` may delete, in key order: every
/// part a stored head named and no live head names, of a part key's shape
/// - a stored head is the data server's word, and a key goes into a URL.
fn parts_doomed<'a>(
    live: &'a HashMap<String, WorldItem>,
    stored: &'a HashMap<String, WorldItem>,
) -> BTreeSet<&'a str> {
    let live_parts = parts_named(live.values());
    parts_named(stored.values())
        .into_iter()
        .filter(|key| !live_parts.contains(key) && is_content_key(key))
        .collect()
}

/// The parts a save of `live` over `stored` asks the data server about
/// before planning ([`probe_parts`]), in key order: every part it needs
/// ([`parts_needed`]) - a create over one that exists fails the whole
/// batch, and a head naming one that does not names nothing - and every
/// part it may delete ([`parts_doomed`]) - a delete of one that is not
/// there fails the whole batch too.
pub fn parts_to_probe(
    live: &HashMap<String, WorldItem>,
    stored: &HashMap<String, WorldItem>,
) -> Vec<String> {
    let mut keys = parts_needed(live, stored);
    keys.extend(parts_doomed(live, stored));
    keys.into_iter().map(str::to_owned).collect()
}

/// The part creates a save of `live` over `stored` begins with, in key
/// order: every part a head it writes names ([`parts_needed`]) that the
/// data server does not hold (`existing`, from [`probe_parts`]) - an
/// `#create` over a record that exists fails the whole batch. A needed
/// part whose text this session does not hold refuses the save, naming the
/// item: it cannot be written, and a head naming it would name nothing.
pub(crate) fn plan_part_creates(
    live: &HashMap<String, WorldItem>,
    stored: &HashMap<String, WorldItem>,
    existing: &HashSet<String>,
) -> Result<Vec<RepoWrite>, String> {
    let mut data: BTreeMap<&str, &str> = BTreeMap::new();
    for item in live.values() {
        if let Some(slices) = item.slices.as_deref() {
            for (key, slice) in item.head.parts.iter().zip(slices) {
                data.insert(key, slice);
            }
        }
    }
    let mut writes = Vec::new();
    for key in parts_needed(live, stored) {
        if existing.contains(key) {
            continue;
        }
        let Some(slice) = data.get(key) else {
            let mut names: Vec<&String> = written_heads(live, stored)
                .filter(|(_, item)| item.head.parts.iter().any(|part| part == key))
                .map(|(name, _)| name)
                .collect();
            names.sort();
            return Err(format!(
                "\"{}\" no longer holds its world in this session, so it cannot be saved. Delete \
                 it and save the world to your inventory again.",
                names.first().map_or("a world item", |name| name.as_str())
            ));
        };
        let value = serde_json::to_value(PartRecord::new(slice))
            .map_err(|e| format!("serialize (world part): {e}"))?;
        writes.push(RepoWrite::Create {
            collection: INVENTORY_PART_COLLECTION.into(),
            rkey: key.to_owned(),
            value,
        });
    }
    Ok(writes)
}

/// The part deletes a save of `live` over `stored` ends with, in key order:
/// every part it may delete ([`parts_doomed`]) that the data server holds
/// (`existing`, from [`probe_parts`]). An `#delete` of a record that is not
/// there fails the whole batch, so a part already gone - a torn save, or a
/// world saved with one missing - is left out rather than wedge every
/// later save. A part two items share survives the delete of one of them.
pub(crate) fn plan_part_deletes(
    live: &HashMap<String, WorldItem>,
    stored: &HashMap<String, WorldItem>,
    existing: &HashSet<String>,
) -> Vec<RepoWrite> {
    parts_doomed(live, stored)
        .into_iter()
        .filter(|key| existing.contains(*key))
        .map(|key| RepoWrite::Delete {
            collection: INVENTORY_PART_COLLECTION.into(),
            rkey: key.to_owned(),
        })
        .collect()
}

/// The most part creates a save of `live` over `stored` can make: every
/// needed part this session holds the text of, as if none were on the
/// data server yet. A part's write fills a request ([`PART_DATA_BUDGET`]),
/// so this counts requests too - what the save's deadline is grown by.
pub(crate) fn most_part_creates(
    live: &HashMap<String, WorldItem>,
    stored: &HashMap<String, WorldItem>,
) -> usize {
    let held: HashSet<&str> = live
        .values()
        .filter(|item| item.slices.is_some())
        .flat_map(|item| item.head.parts.iter().map(String::as_str))
        .collect();
    parts_needed(live, stored)
        .into_iter()
        .filter(|key| held.contains(key))
        .count()
}

/// The most part deletes a save of `live` over `stored` can make.
pub(crate) fn most_part_deletes(
    live: &HashMap<String, WorldItem>,
    stored: &HashMap<String, WorldItem>,
) -> usize {
    parts_doomed(live, stored).len()
}

/// Run `futures` together, each to the end, returning their outputs in
/// order. The crate's runtime is futures-lite, which has no `join_all`.
async fn join_all<F: std::future::Future>(futures: Vec<F>) -> Vec<F::Output> {
    use std::task::Poll;
    let mut futures: Vec<std::pin::Pin<Box<F>>> = futures.into_iter().map(Box::pin).collect();
    let mut outputs: Vec<Option<F::Output>> = futures.iter().map(|_| None).collect();
    futures_lite::future::poll_fn(|cx| {
        let mut pending = false;
        for (future, output) in futures.iter_mut().zip(outputs.iter_mut()) {
            if output.is_none() {
                match future.as_mut().poll(cx) {
                    Poll::Ready(value) => *output = Some(value),
                    Poll::Pending => pending = true,
                }
            }
        }
        if pending {
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await;
    outputs.into_iter().flatten().collect()
}

/// How many requests [`probe_parts`] makes for `keys` keys, one window of
/// [`PART_FETCH_CONCURRENCY`] at a time: what the save's deadline is grown
/// by, a request's bound per window.
pub fn probe_windows(keys: usize) -> usize {
    keys.div_ceil(PART_FETCH_CONCURRENCY)
}

/// Which of `keys` the owner's repo already holds as parts, asked a few at
/// a time ([`crate::pds::xrpc::record_exists`]), each request bounded on
/// its own as a load's are ([`fetch_parts`]). An existence check that
/// fails fails the save: guessing "absent" would turn a blip into an
/// `#create` over a record that exists, guessing "there" a head naming
/// nothing or an `#delete` of nothing.
pub async fn probe_parts(
    client: &reqwest::Client,
    pds: &str,
    did: &str,
    keys: &[String],
) -> Result<HashSet<String>, String> {
    let mut existing = HashSet::new();
    for window in keys.chunks(PART_FETCH_CONCURRENCY) {
        let answers = join_all(
            window
                .iter()
                .map(|key| {
                    crate::config::http::run_or(
                        crate::pds::xrpc::record_exists(
                            client,
                            pds,
                            did,
                            INVENTORY_PART_COLLECTION,
                            key,
                        ),
                        Err(crate::config::http::timed_out(
                            "Checking a part of a world in your inventory",
                        )),
                    )
                })
                .collect(),
        )
        .await;
        for (key, answer) in window.iter().zip(answers) {
            if answer? {
                existing.insert(key.clone());
            }
        }
    }
    Ok(existing)
}

/// Fetch one part's data. `index` counts from 1 for the sentence.
async fn fetch_part(
    client: &reqwest::Client,
    pds: &str,
    did: &str,
    key: &str,
    index: usize,
) -> Result<(String, usize), String> {
    #[derive(Deserialize)]
    struct GetPartResponse {
        value: PartRecord,
    }
    let url = format!(
        "{pds}/xrpc/com.atproto.repo.getRecord?repo={did}&collection={INVENTORY_PART_COLLECTION}\
         &rkey={key}"
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("part {index} could not be fetched - network - {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let body = crate::pds::xrpc::read_capped_text(resp).await;
        if status.as_u16() == 404 || body.contains("RecordNotFound") {
            return Err(format!(
                "This world item is damaged, so nothing was loaded: part {index} is missing."
            ));
        }
        return Err(format!(
            "part {index} could not be fetched - the data server answered {status}"
        ));
    }
    let (part, read) =
        crate::pds::xrpc::decode_record_json_within::<GetPartResponse>(resp, PART_FETCH_CAP)
            .await
            .map_err(|e| {
                format!(
                    "This world item is damaged, so nothing was loaded: part {index} is not text \
                     of a world ({e})."
                )
            })?;
    Ok((part.value.data, read))
}

/// Fetch every part `head` names from `did`'s repo - the owner's for a load,
/// the giver's for a gift (#1844) - a few at a time
/// ([`PART_FETCH_CONCURRENCY`]), within one byte budget: at most what the
/// head says the world weighs, escaped, plus each response's envelope, and
/// never more than [`MAX_WORLD_FETCH_BYTES`]. A head that lies about its
/// size stops the load there.
pub async fn fetch_parts(
    client: &reqwest::Client,
    did: &str,
    head: &WorldHead,
) -> Result<Vec<String>, String> {
    let bytes = check_head(head)?;
    // Whose data server: the owner's own for a load (#1839), the giver's
    // for a world given to this client (#1844) - `did` is the
    // relay-authenticated giver there, never a DID the offer names.
    let pds = crate::config::http::run_or(crate::pds::xrpc::resolve_pds(client, did), None)
        .await
        .ok_or_else(|| String::from("the data server holding it could not be found"))?;
    let budget = bytes
        .saturating_mul(2)
        .saturating_add(
            head.parts
                .len()
                .saturating_mul(PART_FETCH_CAP - PART_DATA_BUDGET),
        )
        .min(MAX_WORLD_FETCH_BYTES);
    let mut read_total = 0usize;
    let mut held = 0usize;
    let mut parts = Vec::with_capacity(head.parts.len());
    for (window_index, window) in head.parts.chunks(PART_FETCH_CONCURRENCY).enumerate() {
        let first = window_index * PART_FETCH_CONCURRENCY;
        // Each request bounded on its own (`run_or`): a world of many
        // parts takes longer than one request may, and must not be cut
        // off as if it were one.
        let answers = join_all(
            window
                .iter()
                .enumerate()
                .map(|(i, key)| {
                    let index = first + i + 1;
                    crate::config::http::run_or(
                        fetch_part(client, &pds, did, key, index),
                        Err(crate::config::http::timed_out(&format!(
                            "Fetching part {index} of the world"
                        ))),
                    )
                })
                .collect(),
        )
        .await;
        for answer in answers {
            let (data, read) = answer?;
            read_total = read_total.saturating_add(read);
            held = held.saturating_add(data.len());
            if read_total > budget || held > bytes {
                return Err(String::from(
                    "This world item is damaged, so nothing was loaded: its parts are longer \
                     than it says.",
                ));
            }
            parts.push(data);
        }
    }
    Ok(parts)
}

/// The world `item` holds, ready to replace the owner's (#1839): its parts
/// from memory when this session holds them, else fetched from `did`'s
/// repo, then joined, checked and sanitised ([`join_parts`]).
pub async fn load_world(
    client: &reqwest::Client,
    did: &str,
    item: &WorldItem,
) -> Result<RoomRecord, String> {
    match item.slices.as_deref() {
        Some(slices) => join_parts(&item.head, slices),
        None => {
            let parts = fetch_parts(client, did, &item.head).await?;
            join_parts(&item.head, &parts)
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A seeded world, as an owner's fresh one is.
    pub(crate) fn seeded_world(seed: u64) -> RoomRecord {
        let mut world = RoomRecord::default_for_seed(seed, "did:plc:worlds");
        world.sanitize();
        world
    }

    /// The world a snapshot of `world` loads back as: what a fetched room
    /// becomes, so what a loaded one must equal.
    pub(crate) fn as_loaded(world: &RoomRecord) -> RoomRecord {
        let mut held: RoomRecord =
            serde_json::from_str(&canonical_json(world).expect("serializes")).expect("decodes");
        held.sanitize();
        held
    }

    /// Nothing in the dependency graph turns on serde_json's
    /// `preserve_order`: a `Value`'s map iterates in key order, which is
    /// what makes [`canonical_json`] deterministic over `HashMap`s.
    #[test]
    fn json_maps_iterate_in_key_order() {
        let mut map = serde_json::Map::new();
        map.insert(String::from("b"), serde_json::Value::Null);
        map.insert(String::from("a"), serde_json::Value::Null);
        assert_eq!(map.keys().collect::<Vec<_>>(), vec!["a", "b"]);
    }

    /// The same world is the same bytes and the same parts, however its
    /// maps were built - a HashMap's order is its hasher's.
    #[test]
    fn the_same_world_is_the_same_parts() {
        let world = seeded_world(7);
        let mut rebuilt = world.clone();
        let names: Vec<String> = rebuilt.generators.keys().cloned().collect();
        let mut generators = HashMap::new();
        for name in names.iter().rev() {
            generators.insert(name.clone(), world.generators[name].clone());
        }
        rebuilt.generators = generators;
        let a = WorldItem::snapshot(&world).expect("copies");
        let b = WorldItem::snapshot(&rebuilt).expect("copies");
        assert_eq!(a.head, b.head);
        assert_eq!(a.slices, b.slices);
        assert_eq!(
            canonical_json(&world).expect("serializes"),
            canonical_json(&rebuilt).expect("serializes")
        );
    }

    /// Every part's write fits one request, and the parts join back into
    /// the JSON byte for byte - including multi-byte text cut at a part's
    /// edge, and text that escapes heavily.
    #[test]
    fn every_part_fits_one_request_and_joins_back_byte_exact() {
        let cap = MAX_APPLY_WRITES_BYTES;
        let quoted = "\"\\".repeat(PART_DATA_BUDGET);
        let ring = "\u{f6}\u{1f30d}x".repeat(PART_DATA_BUDGET / 3);
        let mixed = format!(
            "{}\u{e9}{}",
            "a".repeat(PART_DATA_BUDGET - 1),
            "b".repeat(9)
        );
        for text in [quoted, ring, mixed] {
            let slices = slice_text(&text, PART_DATA_BUDGET);
            assert!(slices.len() > 1, "the text needs more than one part");
            assert_eq!(slices.concat(), text, "joins back byte exact");
            for slice in &slices {
                let escaped = serde_json::to_string(slice).expect("serializes").len() - 2;
                assert!(escaped <= PART_DATA_BUDGET, "{escaped}");
                let write = RepoWrite::Create {
                    collection: INVENTORY_PART_COLLECTION.into(),
                    rkey: part_rkey(slice),
                    value: serde_json::to_value(PartRecord::new(slice)).expect("serializes"),
                };
                assert!(write.wire_bytes() <= cap, "{} > {cap}", write.wire_bytes());
            }
        }
        // The multi-byte character at the edge stayed whole: the first
        // part ends before it, at a char boundary.
        let edge = format!("{}\u{e9}", "a".repeat(PART_DATA_BUDGET - 1));
        let slices = slice_text(&edge, PART_DATA_BUDGET);
        assert_eq!(slices.len(), 2);
        assert!(slices[1].starts_with('\u{e9}'));
    }

    /// A head and its parts survive the wire: the item record's `world`
    /// field and each part's record, serialised and decoded, join back into
    /// the world it was copied from, as the sanitiser leaves it - and a
    /// Berlin world says so in its summary.
    #[test]
    fn a_head_and_its_parts_round_trip_into_the_saved_world() {
        let world = seeded_world(11);
        let item = WorldItem::snapshot(&world).expect("copies");
        let head_json = serde_json::to_string(item.head()).expect("serializes");
        let head: WorldHead = serde_json::from_str(&head_json).expect("decodes");
        assert_eq!(&head, item.head());
        let parts: Vec<String> = item
            .slices()
            .expect("in memory")
            .iter()
            .map(|slice| {
                let wire = serde_json::to_string(&PartRecord::new(slice)).expect("serializes");
                serde_json::from_str::<PartRecord>(&wire)
                    .expect("decodes")
                    .data
            })
            .collect();
        let loaded = join_parts(&head, &parts).expect("loads");
        assert_eq!(
            serde_json::to_value(&loaded).expect("serializes"),
            serde_json::to_value(as_loaded(&world)).expect("serializes"),
            "the saved world, sanitised"
        );
        assert_eq!(head.summary.generators as usize, world.generators.len());
        assert_eq!(head.summary.placements as usize, world.placements.len());
        assert_eq!(head.bytes as usize, parts.concat().len());

        let mut berlin = world.clone();
        berlin.geo_source = Some(crate::pds::GeoSource::berlin(geodata::GeoSquare {
            min_e: 391_000,
            min_n: 5_819_500,
            size_m: 1_000,
        }));
        let summary = WorldSummary::of(&berlin);
        assert_eq!(summary.kind_tag(), "Berlin");
        assert!(summary.borough.is_some(), "{summary:?}");
        assert!(summary.kind_label().starts_with("a Berlin square in "));
        assert_eq!(WorldSummary::of(&world).kind_label(), "a procedural world");
    }

    /// A world this build cannot copy whole is refused with the reason: a
    /// placement of a newer build, or an asset it holds by reference only.
    #[test]
    fn a_world_this_build_cannot_copy_whole_is_refused() {
        let mut world = seeded_world(3);
        assert_eq!(snapshot_blocked(&world), None);
        world.placements.push(Placement::Unknown);
        let reason = WorldItem::snapshot(&world).expect_err("refused");
        assert!(reason.contains("could not be read in full"), "{reason}");
        world.placements.pop();
        world
            .opaque_refs
            .insert(String::from("future"), String::from("0123"));
        let reason = snapshot_blocked(&world).expect("refused");
        assert!(reason.contains("newer version"), "{reason}");
        assert!(too_large(MAX_WORLD_SNAPSHOT_BYTES + 1).is_some());
        assert!(too_large(MAX_WORLD_SNAPSHOT_BYTES).is_none());
    }

    /// Hostile heads fail the load with a sentence: a thousand parts, a
    /// lying length, a digest that does not add up, a part that is not the
    /// one its key names, a key that is no key.
    #[test]
    fn hostile_heads_fail_the_load_with_a_sentence() {
        let world = seeded_world(5);
        let item = WorldItem::snapshot(&world).expect("copies");
        let parts: Vec<String> = item.slices().expect("in memory").to_vec();
        let good = item.head().clone();
        assert!(join_parts(&good, &parts).is_ok());

        let mut many = good.clone();
        many.parts = vec![good.parts[0].clone(); MAX_WORLD_PARTS + 1];
        let mut lying = good.clone();
        lying.bytes += 1;
        let mut huge = good.clone();
        huge.bytes = u64::MAX;
        let mut digest = good.clone();
        digest.digest = part_rkey("something else");
        let mut injected = good.clone();
        injected.parts[0] = String::from("x&collection=app.bsky.feed.post");
        let mut empty = good.clone();
        empty.parts.clear();
        for (what, head, parts) in [
            ("many", &many, parts.clone()),
            ("lying", &lying, parts.clone()),
            ("huge", &huge, parts.clone()),
            ("digest", &digest, parts.clone()),
            ("injected", &injected, parts.clone()),
            ("empty", &empty, parts.clone()),
            ("swapped", &good, {
                let mut swapped = parts.clone();
                swapped[0].push(' ');
                swapped
            }),
            (
                "short",
                &good,
                parts[..parts.len().saturating_sub(1)].to_vec(),
            ),
        ] {
            let reason = join_parts(head, &parts).expect_err(what);
            assert!(
                reason.ends_with('.') || reason.ends_with(')'),
                "{what}: {reason}"
            );
            assert!(!reason.is_empty());
        }
        // A part that is not text of a world: valid parts of a JSON that is
        // not a room.
        let text = String::from("{\"not\":\"a world\"}");
        let head = WorldHead {
            summary: WorldSummary::default(),
            parts: vec![part_rkey(&text)],
            bytes: text.len() as u64,
            digest: digest_of(&text),
        };
        let reason = join_parts(&head, &[text]).expect_err("not a world");
        assert!(reason.contains("could not be read as a world"), "{reason}");
    }

    /// The summary a hostile head carries is bounded: a name cleaned as a
    /// world's own is, a dataset and a borough kept only when they are one.
    #[test]
    fn a_hostile_summary_is_bounded() {
        let mut summary = WorldSummary {
            name: format!("{}\u{200b}", "n".repeat(500)),
            dataset: Some(String::from("Berlin; DROP")),
            borough: Some(String::from("Atlantis")),
            generators: u32::MAX,
            placements: u32::MAX,
            copies: u32::MAX,
        };
        summary.sanitize();
        assert!(summary.name.chars().count() <= limits::MAX_WORLD_NAME_CHARS);
        assert!(!summary.name.contains('\u{200b}'));
        assert_eq!(summary.dataset, None);
        assert_eq!(summary.borough, None);
        assert_eq!(summary.generators as usize, limits::MAX_GENERATORS);
        assert_eq!(summary.placements as usize, limits::MAX_PLACEMENTS);
    }

    /// The bounds hold together: a head at the part cap still fits one
    /// item record's request, and a whole load's budget covers a world at
    /// the cap escaped twice over.
    #[test]
    fn the_world_bounds_fit_the_wire() {
        let head = WorldHead {
            summary: WorldSummary {
                name: "n".repeat(limits::MAX_WORLD_NAME_CHARS),
                dataset: Some(String::from(crate::pds::geo_source::BERLIN)),
                borough: Some(String::from("Charlottenburg-Wilmersdorf")),
                generators: 256,
                placements: 1024,
                copies: 32_768,
            },
            parts: vec![part_rkey("x"); MAX_WORLD_PARTS],
            bytes: MAX_WORLD_SNAPSHOT_BYTES as u64,
            digest: digest_of("x"),
        };
        assert!(check_head(&head).is_ok());
        let bytes = serde_json::to_vec(&head).expect("serializes").len();
        assert!(
            bytes * 4 < MAX_APPLY_WRITES_BYTES,
            "a full head is {bytes} B"
        );
        // A load of a world at the cap, every character escaped twice over,
        // is inside the fetch budget.
        const { assert!(2 * MAX_WORLD_SNAPSHOT_BYTES <= MAX_WORLD_FETCH_BYTES) };
    }
}
