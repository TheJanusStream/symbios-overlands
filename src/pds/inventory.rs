//! Personal stash of `Generator` blueprints, stored as one record **per
//! item** in `collection = network.symbios.overlands.inventory.item` (#696,
//! Stage 2 of the single-record-boundary plan).
//!
//! The in-world editor lets the owner tuck any generator they like into
//! their inventory, rename it, and later spawn it into whichever room they
//! happen to be editing — so a hand-authored L-system, region blueprint, or
//! deeply-nested generator hierarchy survives across rooms the same way an
//! avatar does.
//!
//! # Wire layout
//!
//! Each stash entry is its own [`InventoryItemRecord`] at
//! `rkey = hex(fnv1a_64(name))` — deterministic, clock-free (wasm-safe),
//! stable across content edits, and unique because item names are the
//! stash's `HashMap` key. The collection **is** the stash: reads walk
//! `com.atproto.repo.listRecords` (no manifest record), and writes commit
//! the live-vs-stored diff as ONE atomic `com.atproto.repo.applyWrites`
//! batch, so a failed save leaves the published stash untouched.
//!
//! [`InventoryRecord`] remains the in-memory model (`Live` / `Stored`
//! resources, editor UI, offer flow) — only the PDS boundary changed shape.
//!
//! # Legacy migration
//!
//! Records published before #696 live in a single
//! `network.symbios.overlands.inventory / self` monolith. The fetch falls
//! back to it whenever the item collection is empty, and the next
//! successful save migrates: the publish plan writes every live item and
//! deletes the monolith in the same atomic commit.

use std::collections::HashMap;

use super::generator::Generator;
use super::sanitize::sanitize_generator;
use super::types::TransformData;
use super::xrpc::{FetchError, RepoWrite, XrpcError, decode_record_json, resolve_pds};
use super::{INVENTORY_COLLECTION, INVENTORY_ITEM_COLLECTION};
use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde::{Deserialize, Serialize};

/// Per-owner stash of `Generator` blueprints — the **in-memory** model the
/// editor mutates. On the wire this is exploded into one
/// [`InventoryItemRecord`] per entry (#696); the legacy single-record form
/// under `INVENTORY_COLLECTION / self` is still read for migration.
#[derive(Serialize, Deserialize, Clone, Debug, Resource)]
pub struct InventoryRecord {
    #[serde(rename = "$type")]
    pub lex_type: String,
    pub generators: HashMap<String, Generator>,
    /// How the items that can be **worn** are worn (#1096), keyed by item
    /// name exactly like [`Self::generators`]. An item with an entry here
    /// offers Wear in the Inventory window; one without is plain decor.
    /// Kept as a side table rather than folded into a per-item struct so
    /// every existing reader of `generators` — drops, gifts, the room
    /// editor's From-Inventory menu — stays untouched. Sanitize drops
    /// entries whose item is gone.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub wear: HashMap<String, WearMeta>,
}

/// How an inventory item is worn (#1096): the socket it lands on, the
/// measurement-fit it declares, and the offset it was last saved with —
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
            offset: TransformData::default(),
        }
    }

    /// The socket this metadata names, when this build knows it.
    pub fn socket(&self) -> Option<symbios_avatar::Socket> {
        symbios_avatar::Socket::from_name(&self.socket)
    }

    /// Clamp exactly as the attachment record clamps the same three fields
    /// — the two share a wire vocabulary and must share bounds.
    pub fn sanitize(&mut self) {
        super::avatar::wardrobe::sanitize_wear_fields(
            &mut self.socket,
            &mut self.fit_band_mm,
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
        }
    }
}

impl InventoryRecord {
    /// Clamp every stored generator to the same bounds the room record
    /// enforces, drop items with oversized names, and bound the overall
    /// stash size so a hostile PDS can't force the owner's client into a
    /// multi-megabyte allocation on login. Both the name filter and the
    /// count bound run in lexicographic key order so the survivor set is
    /// deterministic (HashMap iteration is SipHash-randomised).
    ///
    /// The count bound is [`MAX_INVENTORY_SANITIZE_ITEMS`] — the DoS
    /// backstop — NOT the 50-item gameplay cap (#841): sanitize used to
    /// truncate an over-cap legacy stash straight to 50, silently
    /// deleting items with the alphabet deciding which. Over-cap stashes
    /// now survive the load; the Inventory window surfaces them red and
    /// blocks publishing until the user prunes.
    ///
    /// [`MAX_INVENTORY_SANITIZE_ITEMS`]: crate::config::state::MAX_INVENTORY_SANITIZE_ITEMS
    pub fn sanitize(&mut self) {
        self.generators.retain(|name, _| {
            name.chars().count() <= crate::config::state::MAX_INVENTORY_NAME_CHARS
        });
        let bound = crate::config::state::MAX_INVENTORY_SANITIZE_ITEMS;
        if self.generators.len() > bound {
            let mut keys: Vec<String> = self.generators.keys().cloned().collect();
            keys.sort();
            for key in keys.into_iter().skip(bound) {
                self.generators.remove(&key);
            }
        }
        for generator in self.generators.values_mut() {
            sanitize_generator(generator);
        }
        // Wear metadata follows its item: an orphan entry names nothing
        // wearable, and every kept one is clamped like an attachment.
        let generators = &self.generators;
        self.wear.retain(|name, _| generators.contains_key(name));
        for meta in self.wear.values_mut() {
            meta.sanitize();
        }
    }

    /// Insert or replace an item together with its wear metadata (#1096) —
    /// the one write path that keeps the side table in step. `None` makes
    /// (or leaves) the item plain decor.
    pub fn put_item(&mut self, name: String, generator: Generator, wear: Option<WearMeta>) {
        match wear {
            Some(meta) => {
                self.wear.insert(name.clone(), meta);
            }
            None => {
                self.wear.remove(&name);
            }
        }
        self.generators.insert(name, generator);
    }

    /// Remove an item and its wear metadata together. Returns the item.
    pub fn remove_item(&mut self, name: &str) -> Option<Generator> {
        self.wear.remove(name);
        self.generators.remove(name)
    }

    /// Rename an item, carrying its wear metadata across. A no-op when the
    /// old name is missing or the new one is taken.
    pub fn rename_item(&mut self, old: &str, new: String) -> bool {
        if old == new || self.generators.contains_key(&new) {
            return false;
        }
        let Some(generator) = self.generators.remove(old) else {
            return false;
        };
        let wear = self.wear.remove(old);
        self.put_item(new, generator, wear);
        true
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
    /// Display name — also the in-memory `HashMap` key, so unique per
    /// stash. Kept inside the record; the rkey carries only its hash
    /// (arbitrary user strings are not valid record keys).
    pub name: String,
    pub generator: Generator,
    /// Wear metadata (#1096) when the item can be worn; absent for decor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wear: Option<WearMeta>,
}

impl InventoryItemRecord {
    fn new(name: &str, generator: &Generator, wear: Option<&WearMeta>) -> Self {
        Self {
            lex_type: INVENTORY_ITEM_COLLECTION.into(),
            name: name.into(),
            generator: generator.clone(),
            wear: wear.cloned(),
        }
    }
}

/// Record key for an item: lowercase hex of `fnv1a_64(name)` — 16 chars of
/// `[0-9a-f]`, always a valid ATProto rkey. Deterministic and clock-free
/// (no TID clock needed on wasm), stable across content edits so editing an
/// item is an update rather than a delete+create, and derivable from the
/// stored snapshot alone so no name→rkey state has to be persisted. A
/// rename naturally becomes create-new + delete-old.
pub fn item_rkey(name: &str) -> String {
    format!("{:016x}", crate::seeded_defaults::fnv1a_64(name))
}

/// Serialized size of the largest single item record the live stash would
/// publish — the per-record figure the size-budget readout and gauge track
/// now that the stash is one record *per item* (#694/#696). `None` for an
/// empty stash.
pub fn max_item_bytes(record: &InventoryRecord) -> Option<usize> {
    record
        .generators
        .iter()
        .filter_map(|(name, generator)| {
            super::record_size::serialized_record_bytes(&InventoryItemRecord::new(
                name,
                generator,
                record.wear.get(name),
            ))
        })
        .max()
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
/// anything that does not decode as an [`InventoryItemRecord`]. Duplicate
/// names keep the last occurrence in listRecords order (rkey order), which
/// is deterministic.
fn fold_listed_items(values: Vec<serde_json::Value>, into: &mut InventoryRecord) {
    for value in values {
        if let Ok(item) = serde_json::from_value::<InventoryItemRecord>(value) {
            into.put_item(item.name, item.generator, item.wear);
        }
    }
}

/// Fetch the inventory for `did`. Walks the item collection first (up to
/// [`crate::config::state::MAX_INVENTORY_LIST_PAGES`] pages of 100); when
/// that yields nothing, falls back to the pre-#696 monolith record.
/// `Ok(None)` signals "no stash at all", which the caller must treat as a
/// clean empty stash — the same convention as [`super::fetch_room_record`].
pub async fn fetch_inventory_record(
    client: &reqwest::Client,
    did: &str,
) -> Result<Option<InventoryRecord>, FetchError> {
    let pds = resolve_pds(client, did)
        .await
        .ok_or(FetchError::DidResolutionFailed)?;

    let mut record = InventoryRecord::default();
    let mut cursor: Option<String> = None;
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
        let page: ListRecordsResponse = decode_record_json(resp).await?;
        let empty_page = page.records.is_empty();
        fold_listed_items(
            page.records.into_iter().map(|r| r.value).collect(),
            &mut record,
        );
        cursor = page.cursor;
        if cursor.is_none() || empty_page {
            break;
        }
    }

    if !record.generators.is_empty() {
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
/// generators, deletes for removed names — all in sorted-name order so the
/// batch is deterministic — plus the legacy-monolith delete when
/// `legacy_present`. Every written item is size-checked against the
/// [`super::record_size`] hard ceiling before any network I/O.
///
/// Pure so the diff/migration policy is unit-testable; the create-vs-update
/// choice trusts `stored` to mirror the PDS (single-writer assumption — on
/// drift the PDS rejects the batch atomically and the save stays retryable).
fn plan_item_writes(
    live: &InventoryRecord,
    stored: &InventoryRecord,
    legacy_present: bool,
) -> Result<Vec<RepoWrite>, String> {
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
        if !changed {
            continue;
        }
        let item = InventoryItemRecord::new(name, generator, wear);
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
        writes.push(write);
    }

    let mut removed: Vec<&String> = stored
        .generators
        .keys()
        .filter(|name| !live.generators.contains_key(*name))
        .collect();
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

    Ok(writes)
}

/// Publish the live stash to the signed-in user's PDS as per-item records
/// (#696): diff against `stored`, then commit creates / updates / deletes —
/// plus the legacy-monolith delete on first save after migration — as ONE
/// atomic `com.atproto.repo.applyWrites` batch. A failure leaves the
/// published stash exactly as it was, so the caller keeps `stored`
/// unchanged and the save stays dirty and retryable.
///
/// The write COUNT is safe by construction: at most
/// [`crate::config::state::MAX_INVENTORY_ITEMS`] puts + as many deletes +
/// one legacy delete = 101 writes, half the `applyWrites` commit limit.
/// The write *bytes* are not — a stash of large items sums past the PDS's
/// 150 KiB request-body cap long before that (#1115) — so the plan is
/// chunked by size. Atomicity is therefore per batch rather than over the
/// whole save, which the write order makes safe to observe torn: every put
/// precedes every delete, so a stash read mid-save holds the new item and
/// possibly also the old one, never neither.
pub async fn publish_inventory_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    live: &InventoryRecord,
    stored: &InventoryRecord,
) -> Result<(), String> {
    let pds = resolve_pds(client, &session.did)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;

    // The monolith's presence is checked per save rather than tracked as
    // session state: one public getRecord round-trip buys migration that
    // cannot go stale (e.g. a second device already migrated).
    let legacy_present = fetch_legacy_inventory_record(client, &pds, &session.did)
        .await
        .map_err(|e| format!("legacy inventory check failed: {e:?}"))?
        .is_some();

    let writes = plan_item_writes(live, stored, legacy_present)?;
    if writes.is_empty() {
        return Ok(());
    }
    for batch in super::xrpc::chunk_writes(writes)? {
        super::xrpc::apply_writes(&pds, session, refresh, batch).await?;
    }
    Ok(())
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

        let writes = plan_item_writes(&live, &stored, false).unwrap();
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
        let writes = plan_item_writes(&live, &stored, true).unwrap();
        assert_eq!(
            writes.last(),
            Some(&RepoWrite::Delete {
                collection: INVENTORY_COLLECTION.into(),
                rkey: "self".into(),
            })
        );
        // And with nothing else to do, the legacy delete alone still commits
        // (a Reset after migration ends with an empty collection).
        let none = plan_item_writes(&stash(&[]), &stash(&[]), false).unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn plan_stays_under_the_apply_writes_cap_at_full_churn() {
        // Worst case: a full stash entirely replaced by a different full
        // stash, plus the legacy delete.
        let cap = crate::config::state::MAX_INVENTORY_ITEMS;
        let old_names: Vec<String> = (0..cap).map(|i| format!("old_{i}")).collect();
        let new_names: Vec<String> = (0..cap).map(|i| format!("new_{i}")).collect();
        let stored = stash(&old_names.iter().map(String::as_str).collect::<Vec<_>>());
        let live = stash(&new_names.iter().map(String::as_str).collect::<Vec<_>>());
        let writes = plan_item_writes(&live, &stored, true).unwrap();
        assert_eq!(writes.len(), cap * 2 + 1);
        assert!(writes.len() <= super::super::xrpc::MAX_APPLY_WRITES);
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
        assert!(record.remove_item("bag").is_some());
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
        assert!(plan_item_writes(&live, &stored, false).unwrap().is_empty());
        live.wear.get_mut("s").unwrap().fit_band_mm = 178;
        let writes = plan_item_writes(&live, &stored, false).unwrap();
        assert!(
            matches!(writes.as_slice(), [RepoWrite::Update { .. }]),
            "a fit change alone must republish the item: {writes:?}"
        );
    }

    #[test]
    fn sanitize_drops_oversized_names_deterministically() {
        let long = "x".repeat(crate::config::state::MAX_INVENTORY_NAME_CHARS + 1);
        let mut record = stash(&["ok", &long]);
        record.sanitize();
        assert_eq!(record.generators.len(), 1);
        assert!(record.generators.contains_key("ok"));
    }

    #[test]
    fn sanitize_preserves_over_cap_stashes_up_to_the_dos_bound() {
        // #841: an over-cap legacy stash must SURVIVE the load (the UI
        // surfaces it and blocks publish) — sanitize only truncates at
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
