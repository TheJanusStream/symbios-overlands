//! Personal stash of items, stored as one record **per item** in
//! `collection = network.symbios.overlands.inventory.item` (#696, Stage 2
//! of the single-record-boundary plan).
//!
//! The in-world editor lets the owner tuck any generator they like into
//! their inventory, rename it, and later spawn it into whichever room they
//! happen to be editing - so a hand-authored L-system, region blueprint, or
//! deeply-nested generator hierarchy survives across rooms the same way an
//! avatar does. Since #1838 the stash also carries whole avatars
//! ([`avatar::AvatarItem`]): saved from the Avatar editor, worn back from
//! the Inventory window. Since #1839 it carries whole worlds
//! ([`world::WorldItem`]): saved from the World Editor, loaded back over
//! the owner's world as a re-roll replaces it.
//!
//! # Wire layout
//!
//! Each stash entry is its own [`InventoryItemRecord`] at
//! `rkey = hex(fnv1a_64(name))` - deterministic, clock-free (wasm-safe),
//! stable across content edits, and unique because item names are unique
//! across the whole stash. The collection **is** the stash: reads walk
//! `com.atproto.repo.listRecords` (no manifest record), and writes commit
//! the live-vs-stored diff as ONE atomic `com.atproto.repo.applyWrites`
//! batch, so a failed save leaves the published stash untouched.
//!
//! A record carries exactly ONE payload: `generator` (a blueprint, with its
//! optional `wear`), `avatar` (#1838) or `world` (#1839). A generator
//! item's JSON is byte-identical to what it was before the other fields
//! existed. An older client is safe by construction: its decode requires
//! `generator` (or, in #1838's build, exactly one of `generator` and
//! `avatar`), so it skips an avatar or world record at the fold, and its
//! save plan deletes only names it loaded - such an item is never seen and
//! never deleted. This build skips a record with no payload, two, or one it
//! cannot decode, on the same terms.
//!
//! A world does not fit one record: the item carries only its head, and the
//! world's JSON lives in content-addressed parts in
//! [`crate::pds::INVENTORY_PART_COLLECTION`] ([`world`]), which the save
//! plan creates before the heads and deletes after them.
//!
//! # Item kinds share one namespace
//!
//! [`InventoryRecord`] remains the in-memory model (`Live` / `Stored`
//! resources, editor UI, offer flow). Each kind of item has its own map -
//! `generators`, `avatars`, `worlds` - so every reader of `generators` (drops, gifts,
//! the World Editor's From-Inventory menus, the Avatar editor's wearables)
//! stays untouched. But a name lives in at most ONE map, because the rkey
//! is the name's hash: [`InventoryRecord::has_name`],
//! [`InventoryRecord::free_name`], the inserts and the rename all ask every
//! map, sanitize enforces it, and the item cap counts every kind
//! ([`InventoryRecord::item_count`]). World items (#1839) are the third
//! map, one line in each of those helpers.
//!
//! # Legacy migration
//!
//! Records published before #696 live in a single
//! `network.symbios.overlands.inventory / self` monolith. The fetch falls
//! back to it whenever the item collection is empty, and the next
//! successful save migrates: the publish plan writes every live item and
//! deletes the monolith in the same atomic commit.

pub mod avatar;
pub mod world;

use std::collections::{HashMap, HashSet};

pub use avatar::{AvatarItem, AvatarItemBody, AvatarKind, RiggedItem, Wearable};
pub use world::{WorldHead, WorldItem, WorldSummary};

use super::generator::Generator;
use super::sanitize::sanitize_generator;
use super::types::TransformData;
use super::xrpc::{
    FetchError, RepoWrite, XrpcError, decode_record_json, resolve_pds, resolve_pds_outcome,
};
use super::{INVENTORY_COLLECTION, INVENTORY_ITEM_COLLECTION};
use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde::{Deserialize, Serialize};

/// Per-owner stash of items - the **in-memory** model the editor mutates.
/// On the wire this is exploded into one [`InventoryItemRecord`] per entry
/// (#696); the legacy single-record form under `INVENTORY_COLLECTION /
/// self` is still read for migration. One name namespace across every map
/// (see the module header).
#[derive(Serialize, Deserialize, Clone, Debug, Resource)]
pub struct InventoryRecord {
    #[serde(rename = "$type")]
    pub lex_type: String,
    pub generators: HashMap<String, Generator>,
    /// How the items that can be **worn** are worn (#1096), keyed by item
    /// name exactly like [`Self::generators`]. An item with an entry here
    /// offers Wear in the Inventory window; one without is plain decor.
    /// Kept as a side table rather than folded into a per-item struct so
    /// every existing reader of `generators` - drops, gifts, the room
    /// editor's From-Inventory menu - stays untouched. Sanitize drops
    /// entries whose item is gone.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub wear: HashMap<String, WearMeta>,
    /// Whole avatars (#1838), keyed by item name. A side table exactly as
    /// [`Self::wear`] is, so nothing that reads `generators` has to learn
    /// about them - and a name here is never also a key of `generators`.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub avatars: HashMap<String, AvatarItem>,
    /// Whole worlds (#1839), keyed by item name - a side table like
    /// [`Self::avatars`], and in the same one namespace. Each serialises
    /// as its head alone ([`WorldItem`]), so the dirty check never touches
    /// a world's contents.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub worlds: HashMap<String, WorldItem>,
}

/// How an inventory item is worn (#1096): the socket it lands on, the
/// measurement-fit it declares, and the offset it was last saved with -
/// everything an [`AttachmentRecord`](super::avatar::AttachmentRecord)
/// needs beyond the item itself, so wearing from the inventory reproduces
/// the customised look exactly, and saving a worn item back writes the
/// same three things.
///
/// The offset is the socket-relative transform (see the attachment
/// record's `offset`); identity means "engine-seat me". Copied from a
/// catalogue entry's `wear_socket()` / `wear_fit()` when the item is
/// copied into the stash, then owned by the item.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WearMeta {
    /// Engine socket name ([`symbios_avatar::Socket::name`]).
    pub socket: String,
    /// Authored band inner diameter in whole millimetres; `0` = no fit.
    #[serde(default, rename = "fitBandMm", skip_serializing_if = "fit_elides")]
    pub fit_band_mm: u32,
    /// A beast's cut (#1807), in whole millimetres; `0` = not cut for one.
    #[serde(default, rename = "fitBeastMm", skip_serializing_if = "fit_elides")]
    pub fit_beast_mm: u32,
    /// Socket-relative offset, identity elided.
    #[serde(default, skip_serializing_if = "TransformData::is_identity")]
    pub offset: TransformData,
}

fn fit_elides(mm: &u32) -> bool {
    *mm == 0
}

impl WearMeta {
    /// Wear metadata for a fresh copy of a catalogue entry: its default
    /// socket, its fit declaration, and the engine-seat sentinel offset.
    pub fn for_entry(
        socket: symbios_avatar::Socket,
        fit: Option<crate::catalogue::WearFit>,
    ) -> Self {
        Self {
            socket: socket.name().into(),
            fit_band_mm: fit.map_or(0, |fit| fit.band_mm()),
            fit_beast_mm: fit.map_or(0, |fit| fit.beast_mm()),
            offset: TransformData::default(),
        }
    }

    /// The socket this metadata names, when this build knows it.
    pub fn socket(&self) -> Option<symbios_avatar::Socket> {
        symbios_avatar::Socket::from_name(&self.socket)
    }

    /// Clamp exactly as the attachment record clamps the same three fields
    /// - the two share a wire vocabulary and must share bounds.
    pub fn sanitize(&mut self) {
        super::avatar::wardrobe::sanitize_wear_fields(
            &mut self.socket,
            &mut self.fit_band_mm,
            &mut self.fit_beast_mm,
            &mut self.offset,
        );
    }
}

impl Default for InventoryRecord {
    fn default() -> Self {
        Self {
            lex_type: INVENTORY_COLLECTION.into(),
            generators: HashMap::new(),
            wear: HashMap::new(),
            avatars: HashMap::new(),
            worlds: HashMap::new(),
        }
    }
}

impl InventoryRecord {
    /// Clamp every stored generator to the same bounds the room record
    /// enforces, clean item names, and bound the overall stash size so a
    /// hostile PDS can't force the owner's client into a multi-megabyte
    /// allocation on login. Both the name pass and the count bound run
    /// in lexicographic key order so the survivor set is deterministic
    /// (HashMap iteration is SipHash-randomised).
    ///
    /// Names go through [`names::sanitize_keys`]: an over-long name is
    /// cut to [`MAX_INVENTORY_NAME_CHARS`] and invisible characters are
    /// stripped, with the wear entry following its item. Sanitize used
    /// to *drop* an over-long item (#1205) - silent deletion at the next
    /// login of something the owner had watched save.
    ///
    /// [`names::sanitize_keys`]: crate::pds::sanitize::names::sanitize_keys
    /// [`MAX_INVENTORY_NAME_CHARS`]: crate::config::state::MAX_INVENTORY_NAME_CHARS
    ///
    /// The count bound is [`MAX_INVENTORY_SANITIZE_ITEMS`] - the DoS
    /// backstop - NOT the 50-item gameplay cap (#841): sanitize used to
    /// truncate an over-cap legacy stash straight to 50, silently
    /// deleting items with the alphabet deciding which. Over-cap stashes
    /// now survive the load; the Inventory window surfaces them red and
    /// blocks publishing until the user prunes.
    ///
    /// [`MAX_INVENTORY_SANITIZE_ITEMS`]: crate::config::state::MAX_INVENTORY_SANITIZE_ITEMS
    ///
    /// **One namespace** (#1838): the name passes share one claimed set, so
    /// a name cleaned in one map never lands on a name another map holds
    /// untouched (the record under that name is the untouched item's, and
    /// the renamed one's next update or delete would hit it); a cleaned name
    /// that would collide is dropped from memory, never written, as inside
    /// one map. Two UNTOUCHED names can still meet only in a stash no PDS
    /// produced (one rkey per name): the generator keeps the name, then the
    /// avatar. The count bound counts every kind, survivors in name order.
    pub fn sanitize(&mut self) {
        use crate::pds::sanitize::names::{clean_keys, sanitize_keys_sharing};
        let cap = crate::config::state::MAX_INVENTORY_NAME_CHARS;
        let mut claimed = clean_keys(&self.generators, cap);
        claimed.extend(clean_keys(&self.avatars, cap));
        claimed.extend(clean_keys(&self.worlds, cap));
        let renames = sanitize_keys_sharing(&mut self.generators, cap, &mut claimed);
        for (old, new) in renames {
            if let Some(meta) = self.wear.remove(&old) {
                self.wear.insert(new, meta);
            }
        }
        sanitize_keys_sharing(&mut self.avatars, cap, &mut claimed);
        sanitize_keys_sharing(&mut self.worlds, cap, &mut claimed);
        let generators = &self.generators;
        self.avatars
            .retain(|name, _| !generators.contains_key(name));
        let avatars = &self.avatars;
        self.worlds
            .retain(|name, _| !generators.contains_key(name) && !avatars.contains_key(name));
        let bound = crate::config::state::MAX_INVENTORY_SANITIZE_ITEMS;
        if self.item_count() > bound {
            let mut keys: Vec<String> = self.names().cloned().collect();
            keys.sort();
            for key in keys.into_iter().skip(bound) {
                self.generators.remove(&key);
                self.avatars.remove(&key);
                self.worlds.remove(&key);
            }
        }
        for generator in self.generators.values_mut() {
            sanitize_generator(generator);
        }
        for item in self.avatars.values_mut() {
            item.sanitize();
        }
        for item in self.worlds.values_mut() {
            item.sanitize();
        }
        // Wear metadata follows its item: an orphan entry names nothing
        // wearable, and every kept one is clamped like an attachment.
        let generators = &self.generators;
        self.wear.retain(|name, _| generators.contains_key(name));
        for meta in self.wear.values_mut() {
            meta.sanitize();
        }
    }

    /// How many items the stash holds, of every kind (#1838, #1839) -
    /// what the item cap counts.
    pub fn item_count(&self) -> usize {
        self.generators.len() + self.avatars.len() + self.worlds.len()
    }

    /// Whether no more items fit under the gameplay cap.
    pub fn is_full(&self) -> bool {
        self.item_count() >= crate::config::state::MAX_INVENTORY_ITEMS
    }

    /// Whether any item, of any kind, holds `name` (#1838, #1839).
    pub fn has_name(&self, name: &str) -> bool {
        self.generators.contains_key(name)
            || self.avatars.contains_key(name)
            || self.worlds.contains_key(name)
    }

    /// Every item name, of every kind, in no particular order.
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.generators
            .keys()
            .chain(self.avatars.keys())
            .chain(self.worlds.keys())
    }

    /// The tree the item preview stages for `name` (#1301, #1838): a
    /// generator item that can be point-placed, or a generator-bodied
    /// avatar item's visuals. `None` for everything the stage must not
    /// hold as a tree: a room-scoped or unreadable generator, a rigged body
    /// (pictured through [`Self::body_picture_of`] instead, #1842), an
    /// unreadable avatar, a world (#1839: a text card, no picture), a name
    /// the stash does not hold.
    pub fn picture_of(&self, name: &str) -> Option<&Generator> {
        match self.generators.get(name) {
            Some(generator) => Some(generator),
            None => self.avatars.get(name).and_then(AvatarItem::visuals),
        }
        .filter(|generator| is_drop_placeable(generator))
    }

    /// The person or beast the item preview builds a picture of for `name`
    /// (#1842): a rigged avatar item this build can read. `None` for every
    /// other item - a vehicle avatar is a tree ([`Self::picture_of`]), and an
    /// item this build cannot read is never selected.
    pub fn body_picture_of(&self, name: &str) -> Option<&AvatarItem> {
        self.avatars
            .get(name)
            .filter(|item| matches!(item.body, AvatarItemBody::Rigged(_)) && item.is_readable())
    }

    /// Whether `name` is an item this build can read - one the Inventory
    /// lets a person select. An unreadable item is deleted and nothing
    /// else, whichever kind it is.
    ///
    /// A world item is always selectable: its head is all the stash holds,
    /// and a head this build would refuse to load says why when it is
    /// loaded (#1839).
    pub fn is_readable(&self, name: &str) -> bool {
        match self.generators.get(name) {
            Some(generator) => !matches!(generator.kind, super::GeneratorKind::Unknown),
            None => {
                self.avatars.get(name).is_some_and(AvatarItem::is_readable)
                    || self.worlds.contains_key(name)
            }
        }
    }

    /// A name no item of any kind holds: `prefix` as the sanitiser keeps
    /// it, or the first free `prefix_N` (see
    /// [`crate::pds::sanitize::names::unique_name`]).
    pub fn free_name(&self, prefix: &str) -> String {
        let max = crate::config::state::MAX_INVENTORY_NAME_CHARS;
        // A prefix that cleans to nothing (blank, invisible) would give a name
        // the sanitiser drops at the next load (critic of #1843): such a
        // prefix names the item plainly instead.
        let prefix = if crate::pds::sanitize::names::clean_name(prefix, max).is_some() {
            prefix
        } else {
            "item"
        };
        crate::pds::sanitize::names::unique_name(|name| self.has_name(name), prefix, max)
    }

    /// Insert or replace a generator item together with its wear metadata
    /// (#1096) - the one write path that keeps the side table in step.
    /// `None` makes (or leaves) the item plain decor.
    ///
    /// Refuses (returns `false`, changes nothing) when an item of ANOTHER
    /// kind holds `name` (#1838): replacing it would delete that item.
    pub fn put_item(&mut self, name: String, generator: Generator, wear: Option<WearMeta>) -> bool {
        if self.avatars.contains_key(&name) || self.worlds.contains_key(&name) {
            return false;
        }
        match wear {
            Some(meta) => {
                self.wear.insert(name.clone(), meta);
            }
            None => {
                self.wear.remove(&name);
            }
        }
        self.generators.insert(name, generator);
        true
    }

    /// Insert or replace an avatar item (#1838). Refuses (returns `false`,
    /// changes nothing) when an item of another kind holds `name`.
    pub fn put_avatar(&mut self, name: String, item: AvatarItem) -> bool {
        if self.generators.contains_key(&name) || self.worlds.contains_key(&name) {
            return false;
        }
        self.avatars.insert(name, item);
        true
    }

    /// Insert or replace a world item (#1839). Refuses (returns `false`,
    /// changes nothing) when an item of another kind holds `name`.
    pub fn put_world(&mut self, name: String, item: WorldItem) -> bool {
        if self.generators.contains_key(&name) || self.avatars.contains_key(&name) {
            return false;
        }
        self.worlds.insert(name, item);
        true
    }

    /// Remove the item called `name`, whatever its kind, with its wear
    /// metadata. Whether there was one.
    pub fn remove_item(&mut self, name: &str) -> bool {
        self.wear.remove(name);
        self.generators.remove(name).is_some()
            | self.avatars.remove(name).is_some()
            | self.worlds.remove(name).is_some()
    }

    /// Rename an item of any kind, carrying its wear metadata across. A
    /// no-op when the old name is missing or the new one is taken by an
    /// item of ANY kind.
    pub fn rename_item(&mut self, old: &str, new: String) -> bool {
        if old == new || self.has_name(&new) {
            return false;
        }
        if let Some(generator) = self.generators.remove(old) {
            let wear = self.wear.remove(old);
            return self.put_item(new, generator, wear);
        }
        if let Some(item) = self.avatars.remove(old) {
            return self.put_avatar(new, item);
        }
        // A world renamed is a new head over the same parts (#1839).
        match self.worlds.remove(old) {
            Some(item) => self.put_world(new, item),
            None => false,
        }
    }

    /// Let go of the world contents `published` wrote (#1839): every world
    /// item whose head equals the one `published` holds under its name - its
    /// parts are on the data server now, and a load fetches them again.
    /// Changes nothing anything compares or serialises; whether it let go of
    /// any.
    pub fn forget_saved_world_contents(&mut self, published: &InventoryRecord) -> bool {
        let mut forgot = false;
        for (name, item) in self.worlds.iter_mut() {
            if item.slices().is_some() && published.worlds.get(name) == Some(&*item) {
                item.forget_slices();
                forgot = true;
            }
        }
        forgot
    }

    /// Whether any world item still holds its contents in memory - the
    /// question asked before [`Self::forget_saved_world_contents`] borrows a
    /// record mutably.
    pub fn holds_world_contents(&self) -> bool {
        self.worlds.values().any(|item| item.slices().is_some())
    }

    /// Whether the named item can be worn.
    pub fn is_wearable(&self, name: &str) -> bool {
        self.wear.contains_key(name)
    }
}

/// One stash entry on the wire: a record in
/// [`INVENTORY_ITEM_COLLECTION`] at `rkey =` [`item_rkey`]`(name)`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InventoryItemRecord {
    #[serde(rename = "$type")]
    pub lex_type: String,
    /// Display name - also the in-memory `HashMap` key, so unique per
    /// stash. Kept inside the record; the rkey carries only its hash
    /// (arbitrary user strings are not valid record keys).
    pub name: String,
    /// A generator item's payload. Optional on the wire since #1838 so an
    /// avatar record can omit it - which is exactly what makes an older
    /// client, whose decode requires it, skip that record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<Generator>,
    /// Wear metadata (#1096) when the item can be worn; absent for decor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wear: Option<WearMeta>,
    /// An avatar item's payload (#1838): the whole avatar, by value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<AvatarItem>,
    /// A world item's payload (#1839): the world's head - its contents
    /// live in parts of their own ([`world`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world: Option<WorldHead>,
}

/// One stash entry in memory, as the fold lands it: what a record's
/// payload decodes to once exactly one payload is present.
enum ItemPayload {
    Generator(Box<Generator>, Option<WearMeta>),
    Avatar(Box<AvatarItem>),
    World(Box<WorldHead>),
}

impl InventoryItemRecord {
    fn new(name: &str, generator: &Generator, wear: Option<&WearMeta>) -> Self {
        Self {
            lex_type: INVENTORY_ITEM_COLLECTION.into(),
            name: name.into(),
            generator: Some(generator.clone()),
            wear: wear.cloned(),
            avatar: None,
            world: None,
        }
    }

    fn for_avatar(name: &str, item: &AvatarItem) -> Self {
        Self {
            lex_type: INVENTORY_ITEM_COLLECTION.into(),
            name: name.into(),
            generator: None,
            wear: None,
            avatar: Some(item.clone()),
            world: None,
        }
    }

    fn for_world(name: &str, head: &WorldHead) -> Self {
        Self {
            lex_type: INVENTORY_ITEM_COLLECTION.into(),
            name: name.into(),
            generator: None,
            wear: None,
            avatar: None,
            world: Some(head.clone()),
        }
    }

    /// The record's one payload, or `None` for a record carrying none or
    /// more than one - none of which this build (or any) writes. Wear
    /// metadata on an avatar or world record means nothing and is ignored.
    fn into_payload(self) -> Option<ItemPayload> {
        match (self.generator, self.avatar, self.world) {
            (Some(generator), None, None) => {
                Some(ItemPayload::Generator(Box::new(generator), self.wear))
            }
            (None, Some(avatar), None) => Some(ItemPayload::Avatar(Box::new(avatar))),
            (None, None, Some(world)) => Some(ItemPayload::World(Box::new(world))),
            _ => None,
        }
    }
}

/// Record key for an item: lowercase hex of `fnv1a_64(name)` - 16 chars of
/// `[0-9a-f]`, always a valid ATProto rkey. Deterministic and clock-free
/// (no TID clock needed on wasm), stable across content edits so editing an
/// item is an update rather than a delete+create, and derivable from the
/// stored snapshot alone so no name→rkey state has to be persisted. A
/// rename naturally becomes create-new + delete-old.
pub fn item_rkey(name: &str) -> String {
    format!("{:016x}", crate::seeded_defaults::fnv1a_64(name))
}

/// Serialized size of the largest single item record the live stash would
/// publish - the per-record figure the size-budget readout and gauge track
/// now that the stash is one record *per item* (#694/#696). `None` for an
/// empty stash.
pub fn max_item_bytes(record: &InventoryRecord) -> Option<usize> {
    measure_publish(record).bytes
}

/// Everything the size readout shows for the stash (#1207): the largest
/// item record, named as `plan_item_writes` names it when refusing one,
/// and the refusal sentence when an item cannot be serialized - a gift
/// from a newer build decodes to `GeneratorKind::Unknown`, which is
/// `skip_serializing`, and until this was measured the only symptom was a
/// Save button that never lit up.
pub fn measure_publish(record: &InventoryRecord) -> super::record_size::SizeReadout {
    let mut readout = super::record_size::SizeReadout::default();
    for (name, generator) in &record.generators {
        readout.consider(
            &InventoryItemRecord::new(name, generator, record.wear.get(name)),
            &format!("inventory item \"{name}\""),
        );
    }
    for (name, item) in &record.avatars {
        readout.consider(
            &InventoryItemRecord::for_avatar(name, item),
            &format!("inventory item \"{name}\""),
        );
    }
    // A world's head; its parts are each under one request by construction.
    for (name, item) in &record.worlds {
        readout.consider(
            &InventoryItemRecord::for_world(name, item.head()),
            &format!("inventory item \"{name}\""),
        );
    }
    readout
}

// ---------------------------------------------------------------------------
// Fetch
// ---------------------------------------------------------------------------

/// `com.atproto.repo.listRecords` response envelope. `records[].value` stays
/// a raw `Value` so one foreign / undecodable record in the collection
/// skips that record instead of failing the whole page.
#[derive(Deserialize)]
struct ListRecordsResponse {
    #[serde(default)]
    records: Vec<ListedRecord>,
    cursor: Option<String>,
}

#[derive(Deserialize)]
struct ListedRecord {
    value: serde_json::Value,
}

/// Fold a page of listed record values into the stash map, skipping
/// anything that does not decode as an [`InventoryItemRecord`] with exactly
/// one payload (#1838). Duplicate names of one kind keep the last
/// occurrence in listRecords order (rkey order), which is deterministic; a
/// name already held by the other kind keeps the first, because neither
/// insert replaces an item of another kind. A skipped record is in neither
/// `stored` nor `live`, so no save ever deletes it.
fn fold_listed_items(values: Vec<serde_json::Value>, into: &mut InventoryRecord) {
    for value in values {
        let Ok(item) = serde_json::from_value::<InventoryItemRecord>(value) else {
            continue;
        };
        let name = item.name.clone();
        match item.into_payload() {
            Some(ItemPayload::Generator(generator, wear)) => {
                into.put_item(name, *generator, wear);
            }
            Some(ItemPayload::Avatar(avatar)) => {
                into.put_avatar(name, *avatar);
            }
            Some(ItemPayload::World(head)) => {
                into.put_world(name, WorldItem::from_head(*head));
            }
            None => {}
        }
    }
}

/// Fetch the inventory for `did`. Walks the item collection first (up to
/// [`crate::config::state::MAX_INVENTORY_LIST_PAGES`] pages of 100); when
/// that yields nothing, falls back to the pre-#696 monolith record.
/// `Ok(None)` signals "no stash at all", which the caller must treat as a
/// clean empty stash - the same convention as [`super::fetch_room_record`].
///
/// The walk is bounded twice (#1292): by the page count, and by ONE
/// [`crate::config::state::MAX_INVENTORY_FETCH_BYTES`] budget spent across
/// every page. The second bound is what lets the first one grow with the
/// item cap - per-page caps multiply, and on wasm a login-time spike is
/// resident for the session.
pub async fn fetch_inventory_record(
    client: &reqwest::Client,
    did: &str,
) -> Result<Option<InventoryRecord>, FetchError> {
    let pds = resolve_pds_outcome(client, did).await?;

    let mut record = InventoryRecord::default();
    let mut cursor: Option<String> = None;
    let mut budget = crate::config::state::MAX_INVENTORY_FETCH_BYTES;
    for _ in 0..crate::config::state::MAX_INVENTORY_LIST_PAGES {
        let url = format!("{}/xrpc/com.atproto.repo.listRecords", pds);
        let mut query: Vec<(&str, String)> = vec![
            ("repo", did.to_string()),
            ("collection", INVENTORY_ITEM_COLLECTION.to_string()),
            ("limit", "100".to_string()),
        ];
        if let Some(c) = cursor.take() {
            query.push(("cursor", c));
        }
        let resp = client
            .get(&url)
            .query(&query)
            .send()
            .await
            .map_err(|e| FetchError::Network(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(FetchError::PdsError(status.as_u16()));
        }
        let (page, read): (ListRecordsResponse, usize) =
            super::xrpc::decode_record_json_within(resp, budget).await?;
        budget = budget.saturating_sub(read);
        let empty_page = page.records.is_empty();
        fold_listed_items(
            page.records.into_iter().map(|r| r.value).collect(),
            &mut record,
        );
        cursor = page.cursor;
        // Out of pages, out of cursor, or out of budget. A spent budget
        // stops the walk rather than failing it: the items already folded
        // in are real, and the alternative is refusing to load a stash the
        // owner can still see most of.
        if cursor.is_none() || empty_page || budget == 0 {
            break;
        }
    }

    if record.item_count() > 0 {
        record.sanitize();
        return Ok(Some(record));
    }

    // Empty item collection → pre-#696 monolith fallback.
    fetch_legacy_inventory_record(client, &pds, did).await
}

/// Fetch the pre-#696 single-record stash at `INVENTORY_COLLECTION / self`.
/// `Ok(None)` is the clean "no record" case (404 or `RecordNotFound`).
/// Also used by the publish plan to decide whether the migration delete
/// belongs in the write batch.
async fn fetch_legacy_inventory_record(
    client: &reqwest::Client,
    pds: &str,
    did: &str,
) -> Result<Option<InventoryRecord>, FetchError> {
    #[derive(Deserialize)]
    struct GetInventoryResponse {
        value: InventoryRecord,
    }

    let url = format!(
        "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection={}&rkey=self",
        pds, did, INVENTORY_COLLECTION
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| FetchError::Network(e.to_string()))?;
    let status = resp.status();
    if status.as_u16() == 404 {
        return Ok(None);
    }
    if !status.is_success() {
        // Capped (#1124): an error body is an XRPC envelope, and this
        // path is reachable on another identity's PDS.
        let body = super::xrpc::read_capped_text(resp).await;
        if let Ok(xrpc) = serde_json::from_str::<XrpcError>(&body)
            && let Some(err) = xrpc.error.as_deref()
            && (err == "RecordNotFound"
                || (err == "InvalidRequest" && body.contains("RecordNotFound")))
        {
            return Ok(None);
        }
        return Err(FetchError::PdsError(status.as_u16()));
    }
    let mut record = decode_record_json::<GetInventoryResponse>(resp)
        .await?
        .value;
    record.sanitize();
    Ok(Some(record))
}

// ---------------------------------------------------------------------------
// Publish
// ---------------------------------------------------------------------------

/// Build the `applyWrites` batch that turns the published stash (`stored`)
/// into the edited one (`live`): creates for new names, updates for changed
/// items, deletes for removed names - all in sorted-name order across every
/// kind so the batch is deterministic - plus the legacy-monolith delete when
/// `legacy_present`. Every written item is size-checked against the
/// [`super::record_size`] hard ceiling before any network I/O.
///
/// A world item's parts (#1839) wrap the item writes in the order that
/// keeps every read safe: the parts a written head names and the data
/// server does not hold (`existing_parts`, the probe's answer) come FIRST
/// ([`world::plan_part_creates`]), so a head never names a part that is not
/// there; the parts a stored head named, no live one does, and the data
/// server holds come LAST ([`world::plan_part_deletes`]), so a head is gone
/// before its parts are. A plan without worlds writes exactly what it did
/// before #1839.
///
/// An item that changed KIND under one name (deleted, then the name reused
/// by another kind) is one rkey, so it is an update - never a delete and a
/// create of the same key in one batch.
///
/// Pure so the diff/migration policy is unit-testable; the create-vs-update
/// choice trusts `stored` to mirror the PDS (single-writer assumption - on
/// drift the PDS rejects the batch atomically and the save stays retryable).
fn plan_item_writes(
    live: &InventoryRecord,
    stored: &InventoryRecord,
    legacy_present: bool,
    existing_parts: &HashSet<String>,
) -> Result<Vec<RepoWrite>, String> {
    let mut writes = world::plan_part_creates(&live.worlds, &stored.worlds, existing_parts)?;

    let mut live_names: Vec<&String> = live.names().collect();
    live_names.sort();
    for name in live_names {
        let exists = stored.has_name(name);
        let item = if let Some(generator) = live.generators.get(name) {
            let wear = live.wear.get(name);
            let unchanged = stored
                .generators
                .get(name)
                .is_some_and(|old| old == generator && stored.wear.get(name) == wear);
            if unchanged {
                continue;
            }
            InventoryItemRecord::new(name, generator, wear)
        } else if let Some(avatar) = live.avatars.get(name) {
            if stored.avatars.get(name) == Some(avatar) {
                continue;
            }
            InventoryItemRecord::for_avatar(name, avatar)
        } else if let Some(item) = live.worlds.get(name) {
            if stored.worlds.get(name) == Some(item) {
                continue;
            }
            InventoryItemRecord::for_world(name, item.head())
        } else {
            continue;
        };
        super::record_size::preflight(&item, &format!("inventory item \"{name}\""))?;
        let value = serde_json::to_value(&item).map_err(|e| format!("serialize: {e}"))?;
        let write = if exists {
            RepoWrite::Update {
                collection: INVENTORY_ITEM_COLLECTION.into(),
                rkey: item_rkey(name),
                value,
            }
        } else {
            RepoWrite::Create {
                collection: INVENTORY_ITEM_COLLECTION.into(),
                rkey: item_rkey(name),
                value,
            }
        };
        // The request cap, checked HERE because here is the only place that
        // knows the item's name (#1293).
        //
        // `preflight` above bounds a record against the 900 KiB hard ceiling
        // and `xrpc::chunk_writes` bounds a write against the 120 KiB the PDS
        // accepts in one request - so an item between the two passed the check
        // that could name it and failed the one that could not. What the owner
        // saw was "a single record is 127.2 KiB", on a save that then failed
        // for every later rename, delete and accepted gift, with nothing to say
        // WHICH of their items had done it. Same limit, same arithmetic, said
        // where the name is in scope.
        let bytes = write.wire_bytes();
        if bytes > super::xrpc::MAX_APPLY_WRITES_BYTES {
            return Err(format!(
                "\"{name}\" is {} - larger than the {} the PDS accepts in one request, and a \
                 record cannot be split across requests, so this item alone fails the whole \
                 save. Remove content from it, or remove it from your inventory, and save \
                 again.",
                super::record_size::human_bytes(bytes),
                super::record_size::human_bytes(super::xrpc::MAX_APPLY_WRITES_BYTES),
            ));
        }
        writes.push(write);
    }

    let mut removed: Vec<&String> = stored.names().filter(|name| !live.has_name(name)).collect();
    removed.sort();
    for name in removed {
        writes.push(RepoWrite::Delete {
            collection: INVENTORY_ITEM_COLLECTION.into(),
            rkey: item_rkey(name),
        });
    }

    if legacy_present {
        writes.push(RepoWrite::Delete {
            collection: INVENTORY_COLLECTION.into(),
            rkey: "self".into(),
        });
    }

    writes.extend(world::plan_part_deletes(
        &live.worlds,
        &stored.worlds,
        existing_parts,
    ));
    Ok(writes)
}

/// Publish the live stash to the signed-in user's PDS as per-item records
/// (#696): diff against `stored`, then commit creates / updates / deletes -
/// plus the legacy-monolith delete on first save after migration - as ONE
/// atomic `com.atproto.repo.applyWrites` batch. A failure leaves the
/// published stash exactly as it was, so the caller keeps `stored`
/// unchanged and the save stays dirty and retryable.
///
/// Neither the write COUNT nor the write BYTES bound the stash:
/// `xrpc::chunk_writes` splits on both. Count matters because
/// `applyWrites` commits at most `MAX_APPLY_WRITES`; bytes matter because a
/// stash of large items sums past the PDS's request-body cap long before
/// that (#1115). Atomicity is therefore per batch rather than over the
/// whole save, which the write order makes safe to observe torn: every put
/// precedes every delete, so a stash read mid-save holds the new item and
/// possibly also the old one, never neither.
///
/// This paragraph used to reason "at most `MAX_INVENTORY_ITEMS` puts + as
/// many deletes + one legacy delete = 101 writes, half the commit limit".
/// That arithmetic was a coincidence of the cap being 50, and #1292 raised
/// it to 500 - the chunking is what makes the count safe, and it already
/// did before the cap moved.
///
/// A save that writes or deletes a world item (#1839) from a sign-in whose
/// grant predates the parts collection is refused here, before anything is
/// read or written ([`world::world_grant_refusal`]). The parts it needs and
/// the parts it may delete are probed first ([`world::probe_parts`]): an
/// `#create` over a part that is there and an `#delete` of one that is not
/// each fail the batch, and a head must never name a part that is gone.
///
/// Every request is bounded on its own (`run_or`) - the data server's
/// resolution, the legacy check, each probe and each batch - because a save
/// writing a world is many requests, as a load reading one is
/// ([`world::fetch_parts`]): one bound over the whole would fail a world of
/// a few megabytes on a slow uplink every time. How long the caller waits
/// for the whole is [`SaveBound`].
pub async fn publish_inventory_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    live: &InventoryRecord,
    stored: &InventoryRecord,
) -> Result<(), String> {
    if let Some(reason) = world::world_grant_refusal(
        &live.worlds,
        &stored.worlds,
        &session.session.token_set().scope,
    ) {
        return Err(reason.into());
    }
    use crate::config::http::{run_or, timed_out};
    let pds = run_or(resolve_pds(client, &session.did), None)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;

    // The monolith's presence is checked per save rather than tracked as
    // session state: one public getRecord round-trip buys migration that
    // cannot go stale (e.g. a second device already migrated).
    let legacy_present = run_or(
        fetch_legacy_inventory_record(client, &pds, &session.did),
        Err(FetchError::Network(timed_out(
            "Checking for your old inventory",
        ))),
    )
    .await
    // `Display`, whole: this is the owner's own record, and `Debug`
    // leaves a decoder's message out (#1432).
    .map_err(|e| format!("legacy inventory check failed: {e}"))?
    .is_some();

    let probe = world::parts_to_probe(&live.worlds, &stored.worlds);
    let existing_parts = world::probe_parts(client, &pds, &session.did, &probe).await?;
    let writes = plan_item_writes(live, stored, legacy_present, &existing_parts)?;
    if writes.is_empty() {
        return Ok(());
    }
    let batches = super::xrpc::chunk_writes(writes)?;
    let count = batches.len();
    for (index, batch) in batches.into_iter().enumerate() {
        let what = if count == 1 {
            String::from("Saving your inventory")
        } else {
            format!("Saving your inventory (request {} of {count})", index + 1)
        };
        run_or(
            super::xrpc::apply_writes(&pds, session, refresh, batch),
            Err(timed_out(&what)),
        )
        .await?;
    }
    Ok(())
}

/// How long a save of `live` over `stored` may take, as its task is
/// spawned and polled (#1839).
///
/// A save that leaves every world alone is what it was before world items:
/// ONE bound over the whole (`run_or` around the task) and the fixed
/// [`crate::config::http::PUBLISH_TASK_DEADLINE`] behind it. A save that
/// writes or deletes a world is many requests - each bounded inside
/// [`publish_inventory_record`] - so it has no outer bound, and the poll's
/// deadline grows by a request's bound for every request it can make
/// ([`Self::PerRequest`], from [`Self::of`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveBound {
    /// One bound over the whole save.
    Whole,
    /// Each request bounded on its own; at most this many of them.
    PerRequest(usize),
}

impl SaveBound {
    /// The bound a save of `live` over `stored` runs under, estimated
    /// before it is spawned from what it can write and probe - an upper
    /// bound on its requests: the data server's resolution and the legacy
    /// check, every probe window, and every batch the plan can need with
    /// every probed part answered the costly way (each needed part absent,
    /// each part it may delete there). A part's create fills a request, and
    /// every item write is counted as one too; the deletes are small, so
    /// they fill batches by count.
    pub fn of(live: &InventoryRecord, stored: &InventoryRecord) -> Self {
        if !world::touches_worlds(&live.worlds, &stored.worlds) {
            return Self::Whole;
        }
        let probes =
            world::probe_windows(world::parts_to_probe(&live.worlds, &stored.worlds).len());
        let item_writes = live
            .names()
            .filter(|name| {
                if let Some(generator) = live.generators.get(*name) {
                    stored.generators.get(*name) != Some(generator)
                        || stored.wear.get(*name) != live.wear.get(*name)
                } else if let Some(avatar) = live.avatars.get(*name) {
                    stored.avatars.get(*name) != Some(avatar)
                } else if let Some(item) = live.worlds.get(*name) {
                    stored.worlds.get(*name) != Some(item)
                } else {
                    false
                }
            })
            .count();
        let deletes = stored.names().filter(|name| !live.has_name(name)).count()
            + 1
            + world::most_part_deletes(&live.worlds, &stored.worlds);
        let batches = world::most_part_creates(&live.worlds, &stored.worlds)
            + item_writes
            + deletes.div_ceil(super::xrpc::MAX_APPLY_WRITES)
            + 1;
        Self::PerRequest(2 + probes + batches)
    }

    /// Run `save` - a [`publish_inventory_record`] - under this bound, as
    /// its task does: inside one `run_or` over the whole for a
    /// [`Self::Whole`] save, and simply awaited for a [`Self::PerRequest`]
    /// one. The save bounds each of its requests either way, so a whole save
    /// nests one bounded call in another (#1853).
    pub async fn run<F>(self, save: F) -> Result<(), String>
    where
        F: std::future::Future<Output = Result<(), String>>,
    {
        use crate::config::http::{run_or, timed_out};
        match self {
            Self::Whole => run_or(save, Err(timed_out("Saving your inventory"))).await,
            Self::PerRequest(_) => save.await,
        }
    }

    /// How long the poll waits for the save before declaring it dead: the
    /// fixed deadline for a [`Self::Whole`] save, and beyond it a request's
    /// bound per request for a [`Self::PerRequest`] one - as a world load's
    /// grows with its windows of parts.
    pub fn deadline(self) -> std::time::Duration {
        use crate::config::http::{PUBLISH_TASK_DEADLINE, REQUEST_TIMEOUT};
        match self {
            Self::Whole => PUBLISH_TASK_DEADLINE,
            Self::PerRequest(requests) => {
                PUBLISH_TASK_DEADLINE
                    + REQUEST_TIMEOUT.saturating_mul(u32::try_from(requests).unwrap_or(u32::MAX))
            }
        }
    }
}

#[cfg(test)]
mod cap_tests {
    use super::*;
    use crate::config::state::{MAX_INVENTORY_FETCH_BYTES, MAX_INVENTORY_ITEMS};

    /// A full stash fits the budget the fetch walk is allowed to spend
    /// (#1292).
    ///
    /// The canary behind the raise from 50 to 500. `MAX_INVENTORY_ITEMS`
    /// stopped being a wire limit when #696 made the stash one record per
    /// item, but two things still scale with it, and this pins the one that
    /// can silently lose data: a stash the walk cannot afford to read is a
    /// stash that comes back SHORT at the next login, with the missing
    /// items looking to the owner exactly like items they never saved.
    ///
    /// Sized from the shipped catalogue rather than a guess - every entry
    /// built and measured, so a future entry heavy enough to move the
    /// distribution fails here instead of in somebody's stash.
    #[test]
    fn a_full_stash_fits_the_fetch_budget() {
        let mut sizes: Vec<usize> = crate::catalogue::ENTRIES
            .iter()
            .map(|e| {
                let g = e.build("did:plc:canary");
                serde_json::to_vec(&InventoryItemRecord::new(e.name(), &g, None))
                    .map(|v| v.len())
                    .unwrap_or(0)
            })
            .collect();
        sizes.sort_unstable();
        let median = sizes[sizes.len() / 2];
        let p90 = sizes[sizes.len() * 9 / 10];

        // The walk can carry a full cap of p90-heavy items. Not the MAX
        // item: the heaviest entry is several times the p90 and a stash of
        // 500 of those is not a case the budget is meant to serve - the
        // per-item record budget is what speaks to that, and
        // `every_catalogue_entry_fits_one_apply_writes_batch` is what keeps
        // the heaviest one savable at all.
        let heavy = MAX_INVENTORY_ITEMS * p90;
        assert!(
            heavy <= MAX_INVENTORY_FETCH_BYTES,
            "a full stash of p90 items is {heavy} B, past the {MAX_INVENTORY_FETCH_BYTES} B \
             the fetch walk may spend - it would load truncated"
        );
        // And the ordinary case has room to spare.
        assert!(MAX_INVENTORY_ITEMS * median * 4 <= MAX_INVENTORY_FETCH_BYTES);
    }

    /// Every catalogue entry, copied into a stash, still saves (#1293).
    ///
    /// The canary behind the Vertical Farm fix. One entry serialized to
    /// 130,207 B as an `InventoryItemRecord` - 7,327 B past the
    /// `MAX_APPLY_WRITES_BYTES` a record may occupy in one `applyWrites`
    /// request - and because a record cannot be split across requests and
    /// `publish_inventory_record` hands the whole diff to `chunk_writes` in
    /// one go, copying that one item out of the Catalogue stopped the owner's
    /// **entire inventory** from saving: every later rename, delete and
    /// accepted gift failed too.
    ///
    /// So this is not a budget on the catalogue, it is a wire limit on a
    /// record, and a second entry must not be able to do it again. The
    /// per-record `preflight` cannot stand in for it: that ceiling is 900 KiB
    /// and this limit is 120 KiB, which is exactly the "measured per record vs
    /// per request" split #1115 recorded.
    ///
    /// The failure names the slug and the size, because the refusal that
    /// shipped named a size and not an item, and that is the whole reason
    /// nobody could act on it.
    #[test]
    fn every_catalogue_entry_fits_one_apply_writes_batch() {
        let cap = crate::pds::xrpc::MAX_APPLY_WRITES_BYTES;
        let mut over: Vec<(&str, usize)> = Vec::new();
        let mut largest = ("", 0usize);
        for entry in crate::catalogue::ENTRIES {
            let g = entry.build("did:plc:canary");
            let item = InventoryItemRecord::new(entry.name(), &g, None);
            // The write, not the record: what the limit measures is the
            // record's bytes plus the `$type`/`collection`/`rkey` the write
            // wraps it in, which is what `chunk_writes` weighs.
            let write = RepoWrite::Create {
                collection: INVENTORY_ITEM_COLLECTION.into(),
                rkey: item_rkey(entry.name()),
                value: serde_json::to_value(&item).expect("catalogue entries serialize"),
            };
            let bytes = write.wire_bytes();
            if bytes > cap {
                over.push((entry.slug(), bytes));
            }
            if bytes > largest.1 {
                largest = (entry.slug(), bytes);
            }
        }
        assert!(
            over.is_empty(),
            "these catalogue entries cannot be saved to a stash at all - each is one \
             `applyWrites` write larger than the {cap} B the PDS accepts in one request, \
             so copying one out of the Catalogue breaks EVERY later inventory save: {over:?}"
        );
        // The catalogue is walked, not merely iterated over an empty list: a
        // guard that measures nothing passes forever (#1293).
        assert!(
            largest.1 > cap / 4,
            "the heaviest entry measured is {largest:?} - suspect the walk before believing \
             the catalogue got light"
        );
    }
}

/// Which generator kinds can be point-placed via drag-and-drop.
///
/// Terrain + water describe whole-room scope (one heightmap / one water
/// plane) so a ground-level placement of them is nonsensical; they stay
/// editable via the World Editor tabs.
///
/// Lives here rather than in `ui::inventory` (#1158). It reads as a
/// drag-and-drop affordance and it is one, but it is also the rule
/// `network::inbound` applies to a generator arriving in a STRANGER's
/// gift - so the check protecting a room from a hostile peer was owned by
/// a panel module, and the network layer imported the egui surface to ask
/// it. The rule is about the record, so it belongs with the record.
pub fn is_drop_placeable(generator: &Generator) -> bool {
    use super::GeneratorKind;
    !matches!(
        generator.kind,
        GeneratorKind::Terrain(_) | GeneratorKind::Water { .. } | GeneratorKind::Unknown
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stash(entries: &[&str]) -> InventoryRecord {
        let mut record = InventoryRecord::default();
        for name in entries {
            record
                .generators
                .insert(name.to_string(), Generator::default_cuboid());
        }
        record
    }

    #[test]
    fn item_rkey_is_deterministic_hex() {
        let a = item_rkey("My Fancy Tree!");
        assert_eq!(a, item_rkey("My Fancy Tree!"));
        assert_ne!(a, item_rkey("My Fancy Tree"));
        assert_eq!(a.len(), 16);
        assert!(
            a.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
    }

    #[test]
    fn plan_diffs_creates_updates_deletes_in_name_order() {
        let mut live = stash(&["b_changed", "c_new", "a_kept"]);
        let stored = stash(&["b_changed", "a_kept", "d_removed"]);
        // Change b's content so it becomes an update.
        if let Some(g) = live.generators.get_mut("b_changed") {
            g.transform.translation.0[0] = 5.0;
        }

        let writes = plan_item_writes(&live, &stored, false, &HashSet::new()).unwrap();
        assert_eq!(
            writes,
            vec![
                RepoWrite::Update {
                    collection: INVENTORY_ITEM_COLLECTION.into(),
                    rkey: item_rkey("b_changed"),
                    value: serde_json::to_value(InventoryItemRecord::new(
                        "b_changed",
                        &live.generators["b_changed"],
                        None
                    ))
                    .unwrap(),
                },
                RepoWrite::Create {
                    collection: INVENTORY_ITEM_COLLECTION.into(),
                    rkey: item_rkey("c_new"),
                    value: serde_json::to_value(InventoryItemRecord::new(
                        "c_new",
                        &live.generators["c_new"],
                        None
                    ))
                    .unwrap(),
                },
                RepoWrite::Delete {
                    collection: INVENTORY_ITEM_COLLECTION.into(),
                    rkey: item_rkey("d_removed"),
                },
            ],
        );
    }

    #[test]
    fn plan_appends_legacy_migration_delete() {
        let live = stash(&["a"]);
        let stored = stash(&[]);
        let writes = plan_item_writes(&live, &stored, true, &HashSet::new()).unwrap();
        assert_eq!(
            writes.last(),
            Some(&RepoWrite::Delete {
                collection: INVENTORY_COLLECTION.into(),
                rkey: "self".into(),
            })
        );
        // And with nothing else to do, the legacy delete alone still commits
        // (a Reset after migration ends with an empty collection).
        let none = plan_item_writes(&stash(&[]), &stash(&[]), false, &HashSet::new()).unwrap();
        assert!(none.is_empty());
    }

    /// A full-churn save chunks into batches the PDS will accept, and the
    /// chunking preserves the ordering that makes a torn read safe.
    ///
    /// This used to assert `writes.len() <= MAX_APPLY_WRITES` - that the
    /// whole plan fits ONE batch. That was never a requirement, only an
    /// arithmetic coincidence of the item cap being 50 (50 puts + 50
    /// deletes + 1 legacy = 101, under 200); `chunk_writes` has split by
    /// both count and bytes since #1115. #1292 raised the cap to 500, the
    /// coincidence ended, and the assertion failed - correctly identifying
    /// itself, not the code, as the thing encoding the old limit.
    ///
    /// What replaces it is the property the publish path actually
    /// promises: every batch is one the PDS accepts, no write is lost or
    /// reordered by the split, and **every put precedes every delete**, so
    /// a stash read midway through a multi-batch save holds the new item
    /// and possibly the old one, never neither.
    #[test]
    fn a_full_churn_chunks_into_batches_the_pds_accepts() {
        // Worst case: a full stash entirely replaced by a different full
        // stash, plus the legacy delete.
        let cap = crate::config::state::MAX_INVENTORY_ITEMS;
        let old_names: Vec<String> = (0..cap).map(|i| format!("old_{i}")).collect();
        let new_names: Vec<String> = (0..cap).map(|i| format!("new_{i}")).collect();
        let stored = stash(&old_names.iter().map(String::as_str).collect::<Vec<_>>());
        let live = stash(&new_names.iter().map(String::as_str).collect::<Vec<_>>());
        let writes = plan_item_writes(&live, &stored, true, &HashSet::new()).unwrap();
        assert_eq!(writes.len(), cap * 2 + 1);

        let batches = super::super::xrpc::chunk_writes(writes.clone()).unwrap();
        assert!(
            batches.len() > 1,
            "a full-churn plan at this cap is supposed to need splitting - \
             if it fits one batch the test has stopped exercising the split"
        );
        for batch in &batches {
            assert!(batch.len() <= super::super::xrpc::MAX_APPLY_WRITES);
            let bytes: usize = batch.iter().map(|w| w.wire_bytes()).sum();
            assert!(bytes <= super::super::xrpc::MAX_APPLY_WRITES_BYTES);
        }
        // Nothing lost, nothing reordered.
        let flat: Vec<_> = batches.into_iter().flatten().collect();
        assert_eq!(flat, writes);
        // And the read-safe order survives: no put after the first delete.
        let first_delete = flat
            .iter()
            .position(|w| matches!(w, RepoWrite::Delete { .. }));
        if let Some(at) = first_delete {
            assert!(
                flat[at..]
                    .iter()
                    .all(|w| matches!(w, RepoWrite::Delete { .. })),
                "a put after a delete makes a torn mid-save read able to show neither copy"
            );
        }
    }

    #[test]
    fn repo_write_serializes_with_lexicon_type_tags() {
        let write = RepoWrite::Delete {
            collection: INVENTORY_ITEM_COLLECTION.into(),
            rkey: "self".into(),
        };
        let v = serde_json::to_value(&write).unwrap();
        assert_eq!(
            v.get("$type").and_then(|t| t.as_str()),
            Some("com.atproto.repo.applyWrites#delete")
        );
    }

    #[test]
    fn fold_skips_foreign_records_and_keeps_last_duplicate() {
        let good = serde_json::to_value(InventoryItemRecord::new(
            "a",
            &Generator::default_cuboid(),
            None,
        ))
        .unwrap();
        let mut dup_gen = Generator::default_cuboid();
        dup_gen.transform.translation.0[1] = 3.0;
        let dup = serde_json::to_value(InventoryItemRecord::new("a", &dup_gen, None)).unwrap();
        let junk = serde_json::json!({"$type": "app.bsky.feed.post", "text": "hi"});

        let mut record = InventoryRecord::default();
        fold_listed_items(vec![good, junk, dup], &mut record);
        assert_eq!(record.generators.len(), 1);
        assert_eq!(record.generators["a"].transform.translation.0[1], 3.0);
    }

    /// Wear metadata (#1096) in all three schema directions on the per-item
    /// wire record, and the side-table invariants around it: an orphan is
    /// swept on sanitize, a rename carries it across, and a wear-only edit
    /// counts as a change the publish plan writes.
    #[test]
    fn wear_metadata_rides_the_item_record_and_follows_its_item() {
        let meta = WearMeta {
            socket: String::from("left-hip"),
            fit_band_mm: 0,
            fit_beast_mm: 0,
            offset: TransformData::default(),
        };
        let item = InventoryItemRecord::new("satchel", &Generator::default_cuboid(), Some(&meta));
        let json = serde_json::to_string(&item).expect("serializes");
        assert!(
            json.contains("\"wear\":{\"socket\":\"left-hip\"}"),
            "encode: {json}"
        );
        let back: InventoryItemRecord = serde_json::from_str(&json).expect("decodes");
        assert_eq!(back.wear.as_ref(), Some(&meta), "decode");
        let plain = InventoryItemRecord::new("box", &Generator::default_cuboid(), None);
        let plain_json = serde_json::to_string(&plain).expect("serializes");
        assert!(!plain_json.contains("wear"), "elides: {plain_json}");
        let old: InventoryItemRecord = serde_json::from_str(&plain_json).expect("decodes");
        assert_eq!(old.wear, None, "a pre-#1096 item is decor");

        // Side table follows the item.
        let mut record = InventoryRecord::default();
        record.put_item(
            String::from("satchel"),
            Generator::default_cuboid(),
            Some(meta.clone()),
        );
        record.wear.insert(String::from("ghost"), meta.clone());
        record.sanitize();
        assert!(
            !record.wear.contains_key("ghost"),
            "an orphan entry is swept"
        );
        assert!(record.rename_item("satchel", String::from("bag")));
        assert!(record.is_wearable("bag") && !record.is_wearable("satchel"));
        assert!(record.remove_item("bag"));
        assert!(record.wear.is_empty());

        // A wear-only edit is a change to publish.
        let stored = {
            let mut r = InventoryRecord::default();
            r.put_item(
                String::from("s"),
                Generator::default_cuboid(),
                Some(meta.clone()),
            );
            r
        };
        let mut live = stored.clone();
        assert!(
            plan_item_writes(&live, &stored, false, &HashSet::new())
                .unwrap()
                .is_empty()
        );
        live.wear.get_mut("s").unwrap().fit_band_mm = 178;
        let writes = plan_item_writes(&live, &stored, false, &HashSet::new()).unwrap();
        assert!(
            matches!(writes.as_slice(), [RepoWrite::Update { .. }]),
            "a fit change alone must republish the item: {writes:?}"
        );
    }

    /// #1205: an over-long name used to delete the item at the next
    /// login. Sequence: rename an item past the cap → publish (success
    /// toast) → log out → fetch → sanitize. The item must survive under
    /// the cut name, and its wear entry must follow it.
    #[test]
    fn sanitize_cuts_oversized_names_instead_of_dropping_the_item() {
        let cap = crate::config::state::MAX_INVENTORY_NAME_CHARS;
        let long = "x".repeat(cap + 1);
        let mut record = stash(&["ok", &long]);
        record.wear.insert(
            long.clone(),
            WearMeta {
                socket: "head".to_owned(),
                fit_band_mm: 170,
                fit_beast_mm: 0,
                offset: TransformData::default(),
            },
        );
        record.sanitize();
        assert_eq!(record.generators.len(), 2);
        assert!(record.generators.contains_key("ok"));
        let cut = "x".repeat(cap);
        assert!(
            record.generators.contains_key(&cut),
            "item must be cut, not dropped"
        );
        assert!(
            record.wear.contains_key(&cut),
            "wear must follow the renamed item"
        );
        assert!(!record.wear.contains_key(&long));
    }

    /// #1205: an invisible character in a stash name is stripped on load,
    /// and a name made only of invisibles is the one case that is dropped.
    #[test]
    fn sanitize_strips_invisible_characters_from_names() {
        let mut record = stash(&["Tree\u{200B}", "\u{200B}"]);
        record.sanitize();
        assert_eq!(record.generators.len(), 1);
        assert!(record.generators.contains_key("Tree"));
    }

    #[test]
    fn sanitize_preserves_over_cap_stashes_up_to_the_dos_bound() {
        // #841: an over-cap legacy stash must SURVIVE the load (the UI
        // surfaces it and blocks publish) - sanitize only truncates at
        // the hostile-PDS DoS bound, deterministically past it.
        let cap = crate::config::state::MAX_INVENTORY_ITEMS;
        let bound = crate::config::state::MAX_INVENTORY_SANITIZE_ITEMS;
        let names: Vec<String> = (0..bound + 10).map(|i| format!("item_{i:04}")).collect();
        let mut record = stash(&names.iter().map(String::as_str).collect::<Vec<_>>());
        record.sanitize();
        assert!(
            record.generators.len() > cap,
            "over-cap stash was truncated"
        );
        assert_eq!(record.generators.len(), bound);
        // Lexicographic survivors: the zero-padded low indices stay.
        assert!(record.generators.contains_key("item_0000"));
        assert!(
            !record
                .generators
                .contains_key(&format!("item_{:04}", bound + 5))
        );
    }

    /// #1207, finding 208 (the stash half). The readout names which item
    /// holds the number, in the words the publish refusal would use, so an
    /// over-ceiling stash says WHICH item to shrink before the click.
    #[test]
    fn measure_publish_names_the_largest_item() {
        let mut record = stash(&["pebble", "cathedral"]);
        let cathedral = record.generators.get_mut("cathedral").unwrap();
        cathedral.children = vec![Generator::default_cuboid(); 40];
        let readout = measure_publish(&record);
        assert_eq!(
            readout.largest.as_deref(),
            Some("inventory item \"cathedral\"")
        );
        assert_eq!(readout.bytes, max_item_bytes(&record));
        assert_eq!(readout.unserializable, None);
        assert_eq!(measure_publish(&InventoryRecord::default()).bytes, None);
    }

    /// #1207, finding 122. Sequence: a gift sent from a newer build lands
    /// in the stash and decodes to `GeneratorKind::Unknown`, which is
    /// `skip_serializing`. With the item in both live and stored, both
    /// sides of the dirty comparison serialized to `None`, dirty stayed
    /// false whatever else the owner edited, and Save never lit up - with
    /// no reason anywhere. The measurement now names the item and why.
    #[test]
    fn an_unreadable_item_is_reported_as_unserializable_by_name() {
        let mut record = stash(&["pebble"]);
        record.generators.insert(
            String::from("gift_from_the_future"),
            Generator {
                kind: crate::pds::generator::GeneratorKind::Unknown,
                ..Generator::default_cuboid()
            },
        );
        let readout = measure_publish(&record);
        let reason = readout
            .unserializable
            .expect("an Unknown item refuses the save");
        assert!(reason.contains("gift_from_the_future"), "{reason}");
        assert!(reason.contains("newer version of Overlands"), "{reason}");
        // The readable item is still measured, so the number stays honest.
        assert_eq!(
            readout.largest.as_deref(),
            Some("inventory item \"pebble\"")
        );
    }

    #[test]
    fn max_item_bytes_tracks_largest_entry() {
        assert_eq!(max_item_bytes(&InventoryRecord::default()), None);
        let record = stash(&["a", "a_much_longer_item_name_that_serializes_bigger"]);
        let max = max_item_bytes(&record).unwrap();
        let small = super::super::record_size::serialized_record_bytes(&InventoryItemRecord::new(
            "a",
            &record.generators["a"],
            None,
        ))
        .unwrap();
        assert!(max > small);
    }
}

/// #1838: avatar items on the wire and in the stash model.
#[cfg(test)]
pub(crate) mod avatar_item_tests {
    use super::*;
    use crate::pds::avatar::{AttachmentRecord, AvatarRecord, ResolvedAttachment, ResolvedRig};

    /// A rigged person wearing one prop, resolved as the editor holds it.
    pub(crate) fn rigged_record() -> AvatarRecord {
        let mut record = AvatarRecord::wearing("3jzfcijpj2z2a");
        let mut prop =
            AttachmentRecord::new(Generator::default_cuboid(), symbios_avatar::Socket::Crown);
        prop.source = Some(String::from("circlet"));
        if let Some(rig) = record.body.rigged_mut() {
            rig.attachments = vec![String::from("3jzfcijpj2z2b")];
            rig.resolved = Some(ResolvedRig {
                body: crate::pds::avatar::wardrobe::engine_default_for_did("did:plc:stash"),
                attachments: vec![ResolvedAttachment {
                    rkey: String::from("3jzfcijpj2z2b"),
                    record: prop,
                }],
            });
        }
        record
    }

    /// A seeded vehicle - a generator body.
    pub(crate) fn generator_record() -> AvatarRecord {
        let record = AvatarRecord::default_for_seed(42);
        assert!(record.body.visuals().is_some(), "seed 42 rolls a vehicle");
        record
    }

    fn rigged_item() -> AvatarItem {
        AvatarItem::from_record(&rigged_record()).expect("a resolved body copies")
    }

    fn generator_item() -> AvatarItem {
        AvatarItem::from_record(&generator_record()).expect("a vehicle copies")
    }

    fn value_of(record: &InventoryItemRecord) -> serde_json::Value {
        serde_json::to_value(record).expect("serializes")
    }

    /// A generator item's wire bytes did not move when `generator` became
    /// optional and `avatar` joined it. Pinned from the code BEFORE #1838
    /// changed it, as the record and as the `applyWrites` write wrapping it
    /// - both the JSON string and the `Value` form the write carries.
    #[test]
    fn a_generator_items_wire_bytes_are_unchanged() {
        let meta = WearMeta {
            socket: String::from("left-hip"),
            fit_band_mm: 178,
            fit_beast_mm: 0,
            offset: TransformData::default(),
        };
        let mut satchel = Generator::default_cuboid();
        satchel.transform.translation.0[1] = 0.25;
        satchel.children.push(Generator::default_cuboid());
        let worn = InventoryItemRecord::new("satchel", &satchel, Some(&meta));
        let decor = InventoryItemRecord::new("Ship's lantern", &Generator::default_cuboid(), None);
        assert_eq!(
            serde_json::to_string(&worn).expect("serializes"),
            r#"{"$type":"network.symbios.overlands.inventory.item","name":"satchel","generator":{"$type":"network.symbios.gen.cuboid","size":[10000,10000,10000],"solid":true,"transform":{"translation":[0,2500,0]},"children":[{"$type":"network.symbios.gen.cuboid","size":[10000,10000,10000],"solid":true}]},"wear":{"socket":"left-hip","fitBandMm":178}}"#
        );
        assert_eq!(
            serde_json::to_string(&decor).expect("serializes"),
            r#"{"$type":"network.symbios.overlands.inventory.item","name":"Ship's lantern","generator":{"$type":"network.symbios.gen.cuboid","size":[10000,10000,10000],"solid":true}}"#
        );
        let write = RepoWrite::Create {
            collection: INVENTORY_ITEM_COLLECTION.into(),
            rkey: item_rkey("satchel"),
            value: value_of(&worn),
        };
        assert_eq!(
            serde_json::to_string(&write).expect("serializes"),
            r#"{"$type":"com.atproto.repo.applyWrites#create","collection":"network.symbios.overlands.inventory.item","rkey":"198d49962c712695","value":{"$type":"network.symbios.overlands.inventory.item","generator":{"$type":"network.symbios.gen.cuboid","children":[{"$type":"network.symbios.gen.cuboid","size":[10000,10000,10000],"solid":true}],"size":[10000,10000,10000],"solid":true,"transform":{"translation":[0,2500,0]}},"name":"satchel","wear":{"fitBandMm":178,"socket":"left-hip"}}}"#
        );
    }

    /// An avatar item survives its own wire record, for both body kinds:
    /// serialized, listed, folded back and sanitized, it is the item it was
    /// - and it is the only payload on that record.
    #[test]
    fn an_avatar_item_round_trips_for_a_rigged_and_a_generator_body() {
        for (what, item) in [("rigged", rigged_item()), ("generator", generator_item())] {
            let value = value_of(&InventoryItemRecord::for_avatar("mine", &item));
            assert!(value.get("generator").is_none(), "{what}: one payload");
            assert!(value.get("wear").is_none(), "{what}: one payload");
            let tag = value["avatar"]["body"]["$type"].as_str().expect("tagged");
            assert_eq!(
                tag,
                format!("network.symbios.overlands.inventory.item#{what}")
            );
            let mut once = InventoryRecord::default();
            fold_listed_items(vec![value], &mut once);
            once.sanitize();
            let back = once
                .avatars
                .get("mine")
                .expect("folded as an avatar")
                .clone();
            // Quantised in serde, so the claim is grid -> grid.
            let mut twice = InventoryRecord::default();
            fold_listed_items(
                vec![value_of(&InventoryItemRecord::for_avatar("mine", &back))],
                &mut twice,
            );
            twice.sanitize();
            assert_eq!(twice.avatars.get("mine"), Some(&back), "{what}");
            assert!(once.generators.is_empty(), "{what}");
        }
        let rigged = rigged_item();
        assert_eq!(rigged.wearable_count(), 1, "the worn prop came along");
        assert_eq!(rigged.kind(), AvatarKind::Person);
        assert!(generator_item().visuals().is_some());
    }

    /// Today's decode, frozen (#1838): `generator` was required. Kept
    /// verbatim so the claim "an older client is safe" is tested against
    /// the client that is actually out there, not against this one.
    #[derive(Deserialize)]
    #[allow(dead_code)]
    struct FrozenItemRecord {
        #[serde(rename = "$type")]
        lex_type: String,
        name: String,
        generator: Generator,
        #[serde(default)]
        wear: Option<WearMeta>,
    }

    /// Today's fold, frozen with it.
    fn frozen_fold(values: Vec<serde_json::Value>) -> InventoryRecord {
        let mut into = InventoryRecord::default();
        for value in values {
            if let Ok(item) = serde_json::from_value::<FrozenItemRecord>(value) {
                into.put_item(item.name, item.generator, item.wear);
            }
        }
        into
    }

    /// Today's save plan (f62ef9f's `plan_item_writes`), frozen with it as
    /// the write KIND and rkey of each write: creates and updates for the
    /// changed live generators in name order, deletes for the stored names
    /// live lost, then the legacy delete. Frozen rather than calling the
    /// current plan, so the test keeps describing the deployed client
    /// whatever the plan becomes (critic of #1838).
    fn frozen_plan(
        live: &InventoryRecord,
        stored: &InventoryRecord,
        legacy_present: bool,
    ) -> Vec<(&'static str, String)> {
        let mut writes = Vec::new();
        let mut live_names: Vec<&String> = live.generators.keys().collect();
        live_names.sort();
        for name in live_names {
            let generator = &live.generators[name];
            let wear = live.wear.get(name);
            let (changed, exists) = match stored.generators.get(name) {
                Some(old) => (old != generator || stored.wear.get(name) != wear, true),
                None => (true, false),
            };
            if changed {
                writes.push((if exists { "update" } else { "create" }, item_rkey(name)));
            }
        }
        let mut removed: Vec<&String> = stored
            .generators
            .keys()
            .filter(|name| !live.generators.contains_key(*name))
            .collect();
        removed.sort();
        for name in removed {
            writes.push(("delete", item_rkey(name)));
        }
        if legacy_present {
            writes.push(("delete", String::from("self")));
        }
        writes
    }

    /// An older client never sees an avatar item and never deletes one
    /// (#1838). Its decode skips the record, so the item is in neither its
    /// `stored` nor its `live`; its save plan deletes only stored names
    /// absent from live - so whatever it edits, adds or deletes, no write
    /// names the avatar item's rkey.
    #[test]
    fn an_older_client_skips_an_avatar_item_and_never_deletes_it() {
        let lamp = value_of(&InventoryItemRecord::new(
            "lamp",
            &Generator::default_cuboid(),
            None,
        ));
        let boat = value_of(&InventoryItemRecord::for_avatar(
            "my boat",
            &generator_item(),
        ));
        let person = value_of(&InventoryItemRecord::for_avatar("me", &rigged_item()));
        assert!(serde_json::from_value::<FrozenItemRecord>(boat.clone()).is_err());

        let stored = frozen_fold(vec![lamp, boat, person]);
        assert_eq!(
            stored.generators.len(),
            1,
            "the old client sees the lamp alone"
        );
        let mut live = stored.clone();
        live.generators.remove("lamp");
        live.generators
            .insert(String::from("bench"), Generator::default_cuboid());
        let writes = frozen_plan(&live, &stored, true);
        assert_eq!(
            writes,
            vec![
                ("create", item_rkey("bench")),
                ("delete", item_rkey("lamp")),
                ("delete", String::from("self")),
            ],
            "a create, a delete and the legacy delete"
        );
        for avatar in ["my boat", "me"] {
            let rkey = item_rkey(avatar);
            assert!(
                writes.iter().all(|(_, r)| *r != rkey),
                "the old client's save touched {avatar:?}: {writes:?}"
            );
        }
    }

    /// A name cleaned in one map never lands on a name another map holds
    /// untouched (critic of #1838). The record under `item_rkey("x")` is the
    /// untouched item's: a stash that renamed the generator `"x\u{200B}"`
    /// onto `"x"` dropped the avatar item `"x"` from memory, and then aimed
    /// the owner's next delete or edit of "x" at the avatar's record. Both
    /// directions, since the passes run one map after the other.
    #[test]
    fn a_cleaned_name_never_displaces_another_kinds_untouched_item() {
        let mut stash = InventoryRecord::default();
        stash
            .generators
            .insert(String::from("x\u{200B}"), Generator::default_cuboid());
        stash.avatars.insert(String::from("x"), rigged_item());
        stash.sanitize();
        assert!(
            stash.avatars.contains_key("x"),
            "the untouched avatar keeps its name"
        );
        assert!(
            !stash.generators.contains_key("x"),
            "the cleaned generator never takes it"
        );

        let mut stash = InventoryRecord::default();
        stash
            .generators
            .insert(String::from("y"), Generator::default_cuboid());
        stash
            .avatars
            .insert(String::from("y\u{200B}"), rigged_item());
        stash.sanitize();
        assert!(stash.generators.contains_key("y"));
        assert!(
            !stash.avatars.contains_key("y"),
            "the cleaned avatar never takes the generator's name"
        );
    }

    /// This build skips a record with no payload, with two, or with one it
    /// cannot decode (#1838) - and, since such a record is in neither
    /// `stored` nor `live`, no save of this build deletes it either.
    #[test]
    fn this_build_skips_a_record_with_no_payload_two_or_one_it_cannot_decode() {
        let mut both = value_of(&InventoryItemRecord::new(
            "both",
            &Generator::default_cuboid(),
            None,
        ));
        both["avatar"] = serde_json::to_value(generator_item()).expect("serializes");
        let none = serde_json::json!({"$type": INVENTORY_ITEM_COLLECTION, "name": "none"});
        let garbled = serde_json::json!({
            "$type": INVENTORY_ITEM_COLLECTION,
            "name": "garbled",
            "avatar": {"body": 5, "locomotion": "fast"},
        });
        let good = value_of(&InventoryItemRecord::for_avatar("good", &generator_item()));

        let mut stored = InventoryRecord::default();
        fold_listed_items(vec![both, none, garbled, good], &mut stored);
        assert_eq!(stored.item_count(), 1, "only the well-formed record lands");
        assert!(stored.avatars.contains_key("good"));

        let mut live = stored.clone();
        live.remove_item("good");
        let writes = plan_item_writes(&live, &stored, false, &HashSet::new()).expect("plans");
        assert_eq!(
            writes,
            vec![RepoWrite::Delete {
                collection: INVENTORY_ITEM_COLLECTION.into(),
                rkey: item_rkey("good"),
            }],
            "the one delete is the item the owner deleted"
        );
    }

    /// An avatar from a newer client - a body kind this build has no arm
    /// for - lands as an unreadable item, never serializes, and so is
    /// never written back lossy: the size readout refuses the save by name,
    /// and an untouched copy plans no write at all.
    #[test]
    fn an_avatar_this_build_cannot_read_is_never_written_back() {
        let mut future = value_of(&InventoryItemRecord::for_avatar(
            "future",
            &generator_item(),
        ));
        future["avatar"]["body"] =
            serde_json::json!({"$type": "network.symbios.overlands.inventory.item#hologram"});
        let mut stored = InventoryRecord::default();
        fold_listed_items(vec![future], &mut stored);
        let item = stored
            .avatars
            .get("future")
            .expect("kept, as an unreadable item");
        assert!(!item.is_readable());
        assert_eq!(item.kind(), AvatarKind::Unreadable);
        let reason = measure_publish(&stored)
            .unserializable
            .expect("refuses the save");
        assert!(reason.contains("\"future\""), "{reason}");
        assert!(
            plan_item_writes(&stored.clone(), &stored, false, &HashSet::new())
                .expect("plans")
                .is_empty()
        );
    }

    /// One namespace (#1838): every insert, the rename, the free-name
    /// helper and sanitize answer a cross-kind duplicate - refused or
    /// resolved, never two items under one rkey.
    #[test]
    fn a_name_lives_in_one_map_only() {
        let mut stash = InventoryRecord::default();
        assert!(stash.put_item(String::from("lamp"), Generator::default_cuboid(), None));
        assert!(stash.put_avatar(String::from("me"), rigged_item()));

        assert!(
            !stash.put_avatar(String::from("lamp"), rigged_item()),
            "insert refuses"
        );
        assert!(!stash.put_item(String::from("me"), Generator::default_cuboid(), None));
        assert_eq!(stash.item_count(), 2);
        assert!(
            stash.avatars["me"].wearable_count() == 1,
            "the avatar was not replaced"
        );

        assert!(
            !stash.rename_item("lamp", String::from("me")),
            "rename refuses"
        );
        assert!(!stash.rename_item("me", String::from("lamp")));
        assert!(
            stash.rename_item("me", String::from("you")),
            "a free name is fine"
        );
        assert!(stash.avatars.contains_key("you") && !stash.has_name("me"));

        assert_eq!(stash.free_name("lamp"), "lamp_1");
        assert_eq!(stash.free_name("you"), "you_1", "across kinds");
        assert_eq!(stash.free_name("bench"), "bench");
        assert_eq!(stash.free_name("  "), "item", "never a name of nothing");
        assert_eq!(stash.free_name("\u{200B}"), "item");

        // A record set this client did not write can claim one name twice;
        // sanitize keeps the generator, deterministically.
        let mut hostile = InventoryRecord::default();
        hostile
            .generators
            .insert(String::from("x"), Generator::default_cuboid());
        hostile.avatars.insert(String::from("x"), rigged_item());
        hostile
            .avatars
            .insert(String::from("x\u{200B}"), generator_item());
        hostile.avatars.insert(String::from("y"), generator_item());
        hostile.sanitize();
        assert!(hostile.generators.contains_key("x"));
        assert_eq!(hostile.avatars.keys().collect::<Vec<_>>(), vec!["y"]);
        assert!(
            hostile
                .names()
                .all(|n| hostile.generators.contains_key(n) != hostile.avatars.contains_key(n))
        );
    }

    /// The cap counts every kind (#1838), and so does the DoS backstop.
    #[test]
    fn the_cap_and_the_backstop_count_every_kind() {
        let cap = crate::config::state::MAX_INVENTORY_ITEMS;
        let mut stash = InventoryRecord::default();
        for i in 0..cap {
            if i % 2 == 0 {
                stash.put_item(format!("g{i:04}"), Generator::default_cuboid(), None);
            } else {
                stash.put_avatar(format!("a{i:04}"), generator_item());
            }
        }
        assert_eq!(stash.item_count(), cap);
        assert!(stash.is_full());
        stash.remove_item("a0001");
        assert!(!stash.is_full(), "an avatar's slot frees a slot");

        let bound = crate::config::state::MAX_INVENTORY_SANITIZE_ITEMS;
        let mut huge = InventoryRecord::default();
        let item = generator_item();
        for i in 0..bound {
            huge.put_item(format!("b{i:05}"), Generator::default_cuboid(), None);
        }
        for i in 0..10 {
            huge.put_avatar(format!("a{i:05}"), item.clone());
        }
        huge.sanitize();
        assert_eq!(huge.item_count(), bound, "the backstop counts both maps");
        assert_eq!(
            huge.avatars.len(),
            10,
            "and keeps the names that sort first"
        );
    }

    /// A hostile avatar item is bounded by the avatar record's own
    /// sanitizers (#1838), not copies of them: an oversized outfit is cut
    /// to the attachment cap, a socket name to the attachment record's
    /// bound, and the engine record, each prop and the visuals tree come
    /// out exactly as their own records' sanitizers leave them.
    #[test]
    fn sanitize_bounds_a_hostile_avatar_item_with_the_shared_sanitizers() {
        use crate::pds::avatar::{AvatarBody, LocomotionPreset as _, MAX_AVATAR_ATTACHMENTS};
        let mut hostile_body = crate::pds::avatar::wardrobe::engine_default_for_did("did:plc:x");
        hostile_body.composites.femininity = 1.0e9;
        let mut prop = Generator::default_cuboid();
        prop.transform.translation.0[0] = 1.0e12;
        let wearable = Wearable {
            generator: prop.clone(),
            wear: WearMeta {
                socket: "s".repeat(500),
                fit_band_mm: 1,
                fit_beast_mm: 1,
                offset: TransformData::default(),
            },
        };
        let mut item = AvatarItem {
            body: AvatarItemBody::Rigged(Box::new(RiggedItem {
                record: hostile_body.clone(),
                wearables: vec![wearable.clone(); 40],
            })),
            locomotion: crate::pds::avatar::HumanoidParams::default_config(),
            gait: None,
        };
        item.sanitize();
        let AvatarItemBody::Rigged(rigged) = &item.body else {
            panic!("still rigged");
        };
        let mut expected_body = hostile_body.clone();
        expected_body.sanitize();
        assert_ne!(
            expected_body, hostile_body,
            "the engine's own clamp moved something"
        );
        assert_eq!(
            rigged.record, expected_body,
            "the engine record's own sanitize"
        );
        assert_eq!(rigged.wearables.len(), MAX_AVATAR_ATTACHMENTS);
        let mut expected_prop = wearable.to_attachment();
        expected_prop.sanitize();
        for worn in &rigged.wearables {
            assert_eq!(
                worn.to_attachment(),
                expected_prop,
                "the attachment record's clamp"
            );
            assert!(worn.wear.socket.chars().count() <= 32);
            assert!(worn.wear.fit_band_mm >= 50);
        }

        let mut visuals = Generator::default_cuboid();
        visuals.transform.translation.0[2] = -1.0e12;
        visuals.spin = Some(crate::pds::Spin::about(
            [0.0, 1.0, 0.0],
            crate::pds::SpinTerm::Constant {
                rate: crate::pds::Fp(30.0),
            },
        ));
        let mut item = AvatarItem::from_record(&AvatarRecord {
            body: AvatarBody::generator(visuals.clone()),
            ..generator_record()
        })
        .expect("copies");
        item.sanitize();
        let mut expected = AvatarBody::generator(visuals);
        expected.sanitize();
        assert_eq!(
            item.visuals(),
            expected.visuals(),
            "the generator body's own clamp"
        );
    }

    /// `plan_item_writes` creates, updates and deletes avatar items beside
    /// generator items, in one name order (#1838), and refuses one past the
    /// request cap by name, as #1293 does for a generator.
    #[test]
    fn plan_writes_avatar_items_in_name_order_and_refuses_one_past_the_cap() {
        let mut stored = InventoryRecord::default();
        stored.put_avatar(String::from("b_changed"), generator_item());
        stored.put_avatar(String::from("d_removed"), rigged_item());
        stored.put_item(String::from("a_kept"), Generator::default_cuboid(), None);
        let mut live = stored.clone();
        live.remove_item("d_removed");
        live.put_avatar(String::from("c_new"), rigged_item());
        live.avatars.get_mut("b_changed").expect("held").gait = None;

        let writes = plan_item_writes(&live, &stored, false, &HashSet::new()).expect("plans");
        let avatar_value =
            |name: &str| value_of(&InventoryItemRecord::for_avatar(name, &live.avatars[name]));
        assert_eq!(
            writes,
            vec![
                RepoWrite::Update {
                    collection: INVENTORY_ITEM_COLLECTION.into(),
                    rkey: item_rkey("b_changed"),
                    value: avatar_value("b_changed"),
                },
                RepoWrite::Create {
                    collection: INVENTORY_ITEM_COLLECTION.into(),
                    rkey: item_rkey("c_new"),
                    value: avatar_value("c_new"),
                },
                RepoWrite::Delete {
                    collection: INVENTORY_ITEM_COLLECTION.into(),
                    rkey: item_rkey("d_removed"),
                },
            ]
        );

        // A name that changed kind is one rkey, so it is one update.
        let mut swapped = stored.clone();
        swapped.remove_item("a_kept");
        swapped.put_avatar(String::from("a_kept"), generator_item());
        let writes = plan_item_writes(&swapped, &stored, false, &HashSet::new()).expect("plans");
        assert!(
            matches!(writes.as_slice(), [RepoWrite::Update { rkey, .. }] if *rkey == item_rkey("a_kept")),
            "{writes:?}"
        );

        let mut heavy = generator_item();
        if let AvatarItemBody::Generator(body) = &mut heavy.body {
            body.visuals.children = vec![Generator::default_cuboid(); 2_000];
        }
        let mut over = stored.clone();
        over.put_avatar(String::from("galleon"), heavy);
        let refusal = plan_item_writes(&over, &stored, false, &HashSet::new())
            .expect_err("past the request cap");
        assert!(refusal.starts_with("\"galleon\" is "), "{refusal}");
    }
}

/// #1839: world items on the wire, in the stash model and in the save plan.
#[cfg(test)]
mod world_item_tests {
    use super::*;
    use crate::pds::INVENTORY_PART_COLLECTION;
    use world::part_rkey;

    /// Slices of eight bytes, so a few short texts make worlds of several
    /// parts that share some.
    const BUDGET: usize = 8;

    fn world_of(text: &str) -> WorldItem {
        WorldItem::of_text(text, BUDGET)
    }

    /// "aaaaaaaa" is the first part of both.
    fn shared_pair() -> (WorldItem, WorldItem) {
        (world_of("aaaaaaaabbbbbbbb"), world_of("aaaaaaaacccccccc"))
    }

    fn value_of(record: &InventoryItemRecord) -> serde_json::Value {
        serde_json::to_value(record).expect("serializes")
    }

    fn part_create(data: &str) -> RepoWrite {
        RepoWrite::Create {
            collection: INVENTORY_PART_COLLECTION.into(),
            rkey: part_rkey(data),
            value: serde_json::to_value(world::PartRecord {
                lex_type: INVENTORY_PART_COLLECTION.into(),
                data: data.into(),
            })
            .expect("serializes"),
        }
    }

    fn part_delete(data: &str) -> RepoWrite {
        RepoWrite::Delete {
            collection: INVENTORY_PART_COLLECTION.into(),
            rkey: part_rkey(data),
        }
    }

    fn head_value(name: &str, item: &WorldItem) -> serde_json::Value {
        value_of(&InventoryItemRecord::for_world(name, item.head()))
    }

    /// A world head survives its item record: listed, folded and
    /// sanitised, it is the head it was, the record's one payload - and
    /// the listing holds no contents, only the head.
    #[test]
    fn a_world_head_round_trips_as_the_records_one_payload() {
        let (world, _) = shared_pair();
        let value = head_value("home", &world);
        assert!(value.get("generator").is_none() && value.get("avatar").is_none());
        let mut stash = InventoryRecord::default();
        fold_listed_items(vec![value], &mut stash);
        stash.sanitize();
        let back = stash.worlds.get("home").expect("folded as a world");
        assert_eq!(back.head(), world.head());
        assert!(back.slices().is_none(), "a listing holds heads only");
        assert_eq!(back, &world, "heads compare");
        assert!(stash.is_readable("home"));
        assert_eq!(stash.picture_of("home"), None, "a text card, no picture");
        // The dirty check serialises the head, never the contents.
        let json = serde_json::to_string(&stash).expect("serializes");
        assert!(!json.contains("bbbbbbbb"), "{json}");
    }

    /// The deployed client's decode (`generator` required) and #1838's
    /// (exactly one of `generator` and `avatar`), frozen: both skip a world
    /// head, so neither save ever names its rkey - whatever else it edits.
    #[test]
    fn an_older_client_skips_a_world_head_and_never_deletes_it() {
        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct PreAvatarRecord {
            name: String,
            generator: Generator,
        }
        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct AvatarBuildRecord {
            name: String,
            #[serde(default)]
            generator: Option<Generator>,
            #[serde(default)]
            avatar: Option<serde_json::Value>,
        }
        let (world, _) = shared_pair();
        let head = head_value("home", &world);
        let lamp = value_of(&InventoryItemRecord::new(
            "lamp",
            &Generator::default_cuboid(),
            None,
        ));
        assert!(serde_json::from_value::<PreAvatarRecord>(head.clone()).is_err());
        let avatar_build: AvatarBuildRecord =
            serde_json::from_value(head.clone()).expect("the #1838 build decodes the fields");
        assert!(
            avatar_build.generator.is_none() && avatar_build.avatar.is_none(),
            "and finds no payload it knows, so its fold skips the record"
        );

        // Either client's stash: the lamp alone. Its save of any edit plans
        // creates and updates for its own names and deletes for the names
        // it loaded - never the world's.
        let mut seen = InventoryRecord::default();
        for value in [lamp, head] {
            if let Ok(item) = serde_json::from_value::<PreAvatarRecord>(value) {
                seen.put_item(item.name, item.generator, None);
            }
        }
        assert_eq!(seen.item_count(), 1);
        let mut live = seen.clone();
        live.remove_item("lamp");
        live.put_item(String::from("bench"), Generator::default_cuboid(), None);
        let writes = plan_item_writes(&live, &seen, true, &HashSet::new()).expect("plans");
        let home = item_rkey("home");
        assert!(
            writes
                .iter()
                .all(|write| write.key().1 != home && write.key().0 != INVENTORY_PART_COLLECTION),
            "{writes:?}"
        );
    }

    /// The read-safe order (#1839): part creates, then heads (create and
    /// update) in name order with every other kind, then head deletes, the
    /// legacy delete, and last the part deletes. A part two heads share is
    /// neither created nor deleted.
    #[test]
    fn the_plan_creates_parts_then_heads_then_deletes_heads_then_parts() {
        let (old, new) = shared_pair();
        let mut stored = InventoryRecord::default();
        stored.put_world(String::from("b_world"), old.clone());
        stored.put_item(String::from("z_gone"), Generator::default_cuboid(), None);
        let mut live = InventoryRecord::default();
        live.put_world(String::from("b_world"), new.clone());
        live.put_item(String::from("a_new"), Generator::default_cuboid(), None);
        // The data server holds the stored head's parts.
        let on_server: HashSet<String> = old.head().parts.iter().cloned().collect();

        let writes = plan_item_writes(&live, &stored, true, &on_server).expect("plans");
        assert_eq!(
            writes,
            vec![
                part_create("cccccccc"),
                RepoWrite::Create {
                    collection: INVENTORY_ITEM_COLLECTION.into(),
                    rkey: item_rkey("a_new"),
                    value: value_of(&InventoryItemRecord::new(
                        "a_new",
                        &Generator::default_cuboid(),
                        None
                    )),
                },
                RepoWrite::Update {
                    collection: INVENTORY_ITEM_COLLECTION.into(),
                    rkey: item_rkey("b_world"),
                    value: head_value("b_world", &new),
                },
                RepoWrite::Delete {
                    collection: INVENTORY_ITEM_COLLECTION.into(),
                    rkey: item_rkey("z_gone"),
                },
                RepoWrite::Delete {
                    collection: INVENTORY_COLLECTION.into(),
                    rkey: "self".into(),
                },
                part_delete("bbbbbbbb"),
            ]
        );
        // Chunked, the order holds across every request.
        let flat: Vec<RepoWrite> = crate::pds::xrpc::chunk_writes(writes.clone())
            .expect("chunks")
            .into_iter()
            .flatten()
            .collect();
        assert_eq!(flat, writes);
    }

    /// Two items share a part: deleting one deletes its head and its own
    /// parts, and the shared part stays for the other.
    #[test]
    fn a_shared_part_survives_deleting_one_of_two_items() {
        let (one, two) = shared_pair();
        let mut stored = InventoryRecord::default();
        stored.put_world(String::from("one"), one.clone());
        stored.put_world(String::from("two"), two);
        let mut live = stored.clone();
        live.remove_item("one");
        let on_server: HashSet<String> = ["aaaaaaaa", "bbbbbbbb", "cccccccc"]
            .into_iter()
            .map(part_rkey)
            .collect();
        let writes = plan_item_writes(&live, &stored, false, &on_server).expect("plans");
        assert_eq!(
            writes,
            vec![
                RepoWrite::Delete {
                    collection: INVENTORY_ITEM_COLLECTION.into(),
                    rkey: item_rkey("one"),
                },
                part_delete("bbbbbbbb"),
            ]
        );
        // And the second delete takes the shared part with it.
        let mut empty = live.clone();
        empty.remove_item("two");
        let writes = plan_item_writes(&empty, &live, false, &on_server).expect("plans");
        assert!(writes.contains(&part_delete("aaaaaaaa")), "{writes:?}");
        assert!(writes.contains(&part_delete("cccccccc")), "{writes:?}");
    }

    /// A rename writes a new head over the same parts and deletes the old
    /// head: no part is written while the data server holds them all - and
    /// a head loaded without its contents renames as well as one that
    /// holds them.
    #[test]
    fn a_rename_moves_only_the_head() {
        let (world, _) = shared_pair();
        let on_server: HashSet<String> = world.head().parts.iter().cloned().collect();
        let mut listed = WorldItem::from_head(world.head().clone());
        listed.sanitize();
        for item in [world, listed] {
            let mut stored = InventoryRecord::default();
            stored.put_world(String::from("home"), item);
            let mut live = stored.clone();
            assert!(live.rename_item("home", String::from("cottage")));
            let writes = plan_item_writes(&live, &stored, false, &on_server).expect("plans");
            assert_eq!(
                writes,
                vec![
                    RepoWrite::Create {
                        collection: INVENTORY_ITEM_COLLECTION.into(),
                        rkey: item_rkey("cottage"),
                        value: head_value("cottage", &live.worlds["cottage"]),
                    },
                    RepoWrite::Delete {
                        collection: INVENTORY_ITEM_COLLECTION.into(),
                        rkey: item_rkey("home"),
                    },
                ]
            );
        }
    }

    /// A part a torn save left behind - named by no stored head, but on the
    /// data server - is not created again: an `#create` over it would fail
    /// the whole batch. The parts still missing are.
    #[test]
    fn an_existing_orphan_part_is_not_created_again() {
        let (world, _) = shared_pair();
        let stored = InventoryRecord::default();
        let mut live = InventoryRecord::default();
        live.put_world(String::from("home"), world.clone());
        assert_eq!(
            world::parts_to_probe(&live.worlds, &stored.worlds),
            {
                let mut keys = world.head().parts.clone();
                keys.sort();
                keys
            },
            "every part no stored head names is probed"
        );
        let orphan: HashSet<String> = [part_rkey("aaaaaaaa")].into_iter().collect();
        let writes = plan_item_writes(&live, &stored, false, &orphan).expect("plans");
        assert_eq!(writes.first(), Some(&part_create("bbbbbbbb")));
        assert!(!writes.contains(&part_create("aaaaaaaa")), "{writes:?}");
        assert_eq!(writes.len(), 2, "the part and the head: {writes:?}");
    }

    /// A part a stored head names that is not on the data server - a torn
    /// save, a world saved with one missing - is never deleted: an
    /// `#delete` of nothing fails the whole batch, and it would sit in
    /// every later plan. The doomed parts are probed, and a key of a stored
    /// head that is not a part key is neither asked about nor deleted.
    #[test]
    fn a_missing_part_is_never_deleted_and_a_bad_key_never_sent() {
        let (world, _) = shared_pair();
        let mut stored = InventoryRecord::default();
        stored.put_world(String::from("home"), world.clone());
        let mut hostile = world.head().clone();
        hostile
            .parts
            .push(String::from("x&collection=app.bsky.feed.post"));
        stored.put_world(String::from("odd"), WorldItem::from_head(hostile));
        let live = InventoryRecord::default();

        let probe = world::parts_to_probe(&live.worlds, &stored.worlds);
        let mut expected = world.head().parts.clone();
        expected.sort();
        assert_eq!(
            probe, expected,
            "the doomed parts are probed, the bad key is not"
        );

        // Only "aaaaaaaa" is still there.
        let on_server: HashSet<String> = [part_rkey("aaaaaaaa")].into_iter().collect();
        let writes = plan_item_writes(&live, &stored, false, &on_server).expect("plans");
        let parts: Vec<&RepoWrite> = writes
            .iter()
            .filter(|write| write.key().0 == INVENTORY_PART_COLLECTION)
            .collect();
        assert_eq!(parts, vec![&part_delete("aaaaaaaa")], "{writes:?}");
    }

    /// `stored` can be behind the data server: a save whose commit landed
    /// and whose answer was lost leaves it naming parts that save deleted.
    /// Every part a written head names is probed, whether or not a stored
    /// head names it: the absent ones are created from the contents this
    /// session holds, and without them the save is refused by the item's
    /// name - never a head written naming a part that is not there.
    #[test]
    fn a_stale_stored_head_never_leaves_a_head_naming_nothing() {
        let (world, _) = shared_pair();
        // As the login listing lands it: heads only.
        let mut stored = InventoryRecord::default();
        stored.put_world(
            String::from("home"),
            WorldItem::from_head(world.head().clone()),
        );
        let mut sorted = world.head().parts.clone();
        sorted.sort();

        // The same world saved again under another name, its contents held.
        let mut live = stored.clone();
        live.put_world(String::from("copy"), world.clone());
        assert_eq!(
            world::parts_to_probe(&live.worlds, &stored.worlds),
            sorted,
            "a stored head naming them is no proof they are there"
        );
        let writes = plan_item_writes(&live, &stored, false, &HashSet::new()).expect("plans");
        assert_eq!(
            &writes[..2],
            &[part_create("aaaaaaaa"), part_create("bbbbbbbb")],
            "the absent parts are written first: {writes:?}"
        );

        // The same, from a head without its contents: refused by name.
        let mut listed = stored.clone();
        listed.put_world(
            String::from("copy"),
            WorldItem::from_head(world.head().clone()),
        );
        let refusal = plan_item_writes(&listed, &stored, false, &HashSet::new())
            .expect_err("a head naming nothing is never written");
        assert!(refusal.starts_with("\"copy\""), "{refusal}");

        // And with the parts there, only the head.
        let on_server: HashSet<String> = world.head().parts.iter().cloned().collect();
        let writes = plan_item_writes(&listed, &stored, false, &on_server).expect("plans");
        assert_eq!(writes.len(), 1, "{writes:?}");
        assert_eq!(
            writes[0].key(),
            (INVENTORY_ITEM_COLLECTION, item_rkey("copy").as_str())
        );
    }

    /// A save that leaves every world alone is bounded as it always was:
    /// one bound over the whole, the fixed deadline. One that writes a world
    /// is bounded per request, its deadline grown by a request's bound for
    /// every request it can make - the resolution, the legacy check, the
    /// probe windows, one batch per part and per item write, the deletes.
    /// #1853: a save that touches no world runs under one bound over the
    /// whole, and the save bounds each of its requests inside it. On native
    /// both were `block_on` on the shared runtime: the inner one panicked
    /// ("Cannot start a runtime from within a runtime") and the panic took
    /// the client down with its task - Kairos's first save of a thing, live.
    /// Here the save gets as far as its first request, resolving the data
    /// server of a `did:key`, which answers at once with no network.
    #[test]
    fn a_save_without_a_world_runs_its_requests_under_the_whole_bound() {
        let mut live = InventoryRecord::default();
        assert!(live.put_item(
            String::from("Cat Cottage"),
            Generator::default_cuboid(),
            None
        ));
        let stored = InventoryRecord::default();
        let bound = SaveBound::of(&live, &stored);
        assert_eq!(bound, SaveBound::Whole);
        let session = crate::oauth::stand_in::stand_in_session(
            "did:key:zkairoswhosesavegoesnowhere",
            "kairos.example",
        )
        .expect("builds");
        let refresh = crate::oauth::stand_in::stand_in_refresh_ctx().expect("builds");
        let client = reqwest::Client::new();
        let save = publish_inventory_record(&client, &session, &refresh, &live, &stored);
        let answer = futures_lite::future::block_on(bound.run(save));
        assert_eq!(answer, Err(String::from("Failed to resolve PDS")));
    }

    #[test]
    fn a_save_writing_a_world_is_bounded_per_request() {
        use crate::config::http::{PUBLISH_TASK_DEADLINE, REQUEST_TIMEOUT};
        let mut stored = InventoryRecord::default();
        stored.put_item(String::from("lamp"), Generator::default_cuboid(), None);
        let mut live = stored.clone();
        live.put_item(String::from("bench"), Generator::default_cuboid(), None);
        assert_eq!(SaveBound::of(&live, &stored), SaveBound::Whole);
        assert_eq!(SaveBound::Whole.deadline(), PUBLISH_TASK_DEADLINE);

        // A world of 40 parts: 2 + 10 probe windows + 40 part creates + 2
        // item writes (the world's head and the bench) + one batch of
        // deletes (the legacy check's) + 1.
        let text: String = (0..40).map(|i| format!("{i:08}")).collect();
        live.put_world(String::from("home"), WorldItem::of_text(&text, BUDGET));
        assert_eq!(live.worlds["home"].head().parts.len(), 40);
        let bound = SaveBound::of(&live, &stored);
        assert_eq!(bound, SaveBound::PerRequest(2 + 10 + 40 + 2 + 1 + 1));
        assert_eq!(
            bound.deadline(),
            PUBLISH_TASK_DEADLINE + REQUEST_TIMEOUT * 56,
            "it grows with the requests"
        );

        // Deleting it probes its parts and deletes them: still per request.
        let bound = SaveBound::of(&stored, &live);
        assert_eq!(bound, SaveBound::PerRequest(2 + 10 + 1 + 1));
    }

    /// A world whose contents this session no longer holds cannot have its
    /// parts written: the plan refuses by the item's name rather than write
    /// a head naming records that are not there.
    #[test]
    fn a_world_without_its_contents_refuses_a_save_that_must_write_them() {
        let (world, _) = shared_pair();
        let mut live = InventoryRecord::default();
        live.put_world(
            String::from("home"),
            WorldItem::from_head(world.head().clone()),
        );
        let refusal = plan_item_writes(&live, &InventoryRecord::default(), false, &HashSet::new())
            .expect_err("nothing to write the parts from");
        assert!(refusal.starts_with("\"home\""), "{refusal}");
    }

    /// A sign-in whose grant predates the parts collection is refused BEFORE
    /// any write - before anything is read, even - wherever a world item
    /// would be written or deleted, and only there.
    #[test]
    fn a_sign_in_without_the_parts_grant_is_refused_before_any_write() {
        let (world, _) = shared_pair();
        let mut with_world = InventoryRecord::default();
        with_world.put_world(String::from("home"), world);
        let empty = InventoryRecord::default();
        let old_grant = format!("atproto repo:{INVENTORY_ITEM_COLLECTION}");
        let new_grant = format!("{old_grant} repo:{INVENTORY_PART_COLLECTION}");
        for (live, stored) in [(&with_world, &empty), (&empty, &with_world)] {
            assert_eq!(
                world::world_grant_refusal(&live.worlds, &stored.worlds, &old_grant),
                Some(world::SIGN_IN_AGAIN),
                "a world written or deleted"
            );
            assert_eq!(
                world::world_grant_refusal(&live.worlds, &stored.worlds, &new_grant),
                None
            );
            assert_eq!(
                world::world_grant_refusal(&live.worlds, &stored.worlds, "atproto"),
                None,
                "a broad grant is the data server's to judge"
            );
        }
        let mut generators_only = with_world.clone();
        generators_only.put_item(String::from("lamp"), Generator::default_cuboid(), None);
        assert_eq!(
            world::world_grant_refusal(&generators_only.worlds, &with_world.worlds, &old_grant),
            None,
            "a save that leaves every world alone needs no new grant"
        );

        // Through the save itself: it answers before resolving the data
        // server, so nothing is read and nothing is written.
        let mut session =
            crate::oauth::stand_in::stand_in_session("did:plc:worlds", "worlds.example")
                .expect("builds");
        let mut tokens = session.session.token_set();
        tokens.scope = old_grant.clone();
        session.session =
            std::sync::Arc::new(proto_blue_oauth::session::OAuthSession::with_fetch_handler(
                tokens,
                proto_blue_oauth::DpopKey::generate().expect("DPoP key"),
                proto_blue_oauth::DpopNonceCache::new(),
                std::sync::Arc::new(crate::oauth::capped_fetch::CappedFetcher::new()),
            ));
        let refresh = crate::oauth::stand_in::stand_in_refresh_ctx().expect("builds");
        let answer = futures_lite::future::block_on(publish_inventory_record(
            &reqwest::Client::new(),
            &session,
            &refresh,
            &with_world,
            &empty,
        ));
        assert_eq!(answer, Err(String::from(world::SIGN_IN_AGAIN)));
    }

    /// One namespace and one cap (#1839): a world's name is taken for every
    /// kind and every kind's for a world, and a world counts.
    #[test]
    fn a_world_shares_the_namespace_and_the_cap() {
        let (world, _) = shared_pair();
        let mut stash = InventoryRecord::default();
        assert!(stash.put_item(String::from("lamp"), Generator::default_cuboid(), None));
        assert!(stash.put_world(String::from("home"), world.clone()));
        assert!(!stash.put_world(String::from("lamp"), world.clone()));
        assert!(!stash.put_item(String::from("home"), Generator::default_cuboid(), None));
        assert_eq!(stash.item_count(), 2);
        assert_eq!(stash.free_name("home"), "home_1");
        assert!(!stash.rename_item("lamp", String::from("home")));
        assert!(stash.rename_item("home", String::from("cottage")));
        assert!(stash.worlds.contains_key("cottage") && !stash.has_name("home"));
        assert!(stash.remove_item("cottage"));
        assert_eq!(stash.item_count(), 1);

        // A record set no PDS produced, claiming one name twice: the
        // generator keeps it.
        let mut hostile = InventoryRecord::default();
        hostile
            .generators
            .insert(String::from("x"), Generator::default_cuboid());
        hostile.worlds.insert(String::from("x"), world.clone());
        hostile.worlds.insert(String::from("x\u{200b}"), world);
        hostile.sanitize();
        assert!(hostile.generators.contains_key("x"));
        assert!(hostile.worlds.is_empty(), "{:?}", hostile.worlds.keys());
    }

    /// After a save, a world's contents may go (wasm memory): only for the
    /// heads the save wrote.
    #[test]
    fn a_saved_worlds_contents_are_let_go() {
        let (one, two) = shared_pair();
        let mut live = InventoryRecord::default();
        live.put_world(String::from("one"), one.clone());
        live.put_world(String::from("two"), two);
        let mut published = InventoryRecord::default();
        published.put_world(String::from("one"), one);
        assert!(live.forget_saved_world_contents(&published));
        assert!(live.worlds["one"].slices().is_none());
        assert!(
            live.worlds["two"].slices().is_some(),
            "a world the save did not write keeps its contents"
        );
        assert!(live.holds_world_contents());
    }
}
