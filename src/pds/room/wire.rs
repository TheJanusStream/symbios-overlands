//! The room record on the wire: how it is fetched from and published to a
//! PDS, and the #697 split-wire plan that decides what a publish actually
//! writes.
//!
//! A room that fits inside one record is written as one record. A room that
//! does not is written as a **manifest** — the `self` record carrying the
//! environment plus a map of generator name to child rkey — with one
//! content-addressed child record per generator ([`child_rkey`]). The read
//! side ([`fetch_room_record`]) joins them back into the single
//! [`RoomRecord`] the rest of the app knows, so nothing downstream can tell
//! which shape a room arrived in.
//!
//! Split out of `pds/room.rs` by #1159, which was the record lexicon, this
//! planner and a six-hundred-line seeded-world assembler in one file. The
//! record's shape and the transport that carries it are related; the
//! *derivation* of a seeded world is not, and it now lives in
//! [`crate::seeded_defaults::room::build`].
//!
//! The manifest is the source of truth for what a room references — a
//! reference set recomputed from `generators` would miss the opaque refs a
//! newer client wrote and this one cannot decode.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde::{Deserialize, Serialize};

use super::{DefaultLanding, Environment, RoomRecord};
use crate::pds::COLLECTION;
use crate::pds::contact_effects::ContactEffects;
use crate::pds::generator::{Generator, Placement};
use crate::pds::xrpc::{FetchError, RepoWrite, XrpcError, decode_record_json, resolve_pds};

#[derive(Deserialize)]
struct GetRecordResponse {
    /// Captured raw so both `room/self` shapes — the legacy monolith and
    /// the #697 manifest — decode through [`RoomSelfWire`].
    value: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Split wire format (#697): manifest + content-addressed child generators
// ---------------------------------------------------------------------------

/// One named generator on the wire: a record in
/// [`crate::pds::ROOM_GENERATOR_COLLECTION`] at `rkey =` [`child_rkey`].
/// Immutable by construction — the rkey is a hash of this exact content,
/// so editing a generator publishes a *new* child and retires the old one.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RoomGeneratorRecord {
    #[serde(rename = "$type")]
    pub lex_type: String,
    /// The manifest name pointing here. It is part of the hashed body, so
    /// it participates in [`child_rkey`]: two names holding identical
    /// generator content get two children, not one. That is deliberate —
    /// renaming a generator must re-key its child, or a peer that fetched
    /// the old manifest and the new child would render the room under
    /// stale names.
    pub name: String,
    pub generator: Generator,
}

impl RoomGeneratorRecord {
    fn new(name: &str, generator: &Generator) -> Self {
        Self {
            lex_type: crate::pds::ROOM_GENERATOR_COLLECTION.into(),
            name: name.into(),
            generator: generator.clone(),
        }
    }
}

/// Content-addressed record key for a child generator: lowercase hex of
/// `fnv1a_64` over the child's canonical serialized body.
///
/// "Canonical" is a property the wire types have to earn, not one
/// `serde_json` supplies: it emits struct fields in declaration order but
/// `HashMap` entries in iteration order, which `RandomState` re-seeds per
/// map. Until #1118 the map-bearing generators (LSystem's `materials` and
/// `prop_mappings`, Shape's `materials`) therefore hashed differently on
/// every decode. The fix belongs in the *serializers* — see
/// [`sorted_string_map`](crate::pds::types::sorted_string_map) — because
/// this function must hash the same bytes the batch actually writes; a hash
/// canonicalised on its own would address content nobody stored.
///
/// Content-addressing is what keeps non-atomic reads safe:
/// a manifest can only ever point at children whose bytes cannot change,
/// so a visitor racing a publish sees a fully consistent old or new room,
/// never a half-updated child. It also makes unchanged generators free to
/// republish — same content, same rkey, no write.
pub fn child_rkey(name: &str, generator: &Generator) -> String {
    let canonical =
        serde_json::to_string(&RoomGeneratorRecord::new(name, generator)).unwrap_or_default();
    format!("{:016x}", crate::seeded_defaults::fnv1a_64(&canonical))
}

/// Both shapes `room/self` takes on the wire (#697 version-by-shape): the
/// legacy monolith carries inline `generators`; the manifest instead
/// carries `generator_refs` (name → child rkey). Every field defaults so
/// either shape — or a forward-compat superset — decodes.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RoomSelfWire {
    environment: Environment,
    generators: HashMap<String, Generator>,
    generator_refs: HashMap<String, String>,
    placements: Vec<Placement>,
    traits: HashMap<String, Vec<String>>,
    contact_effects: ContactEffects,
    default_landing: Option<DefaultLanding>,
}

/// The manifest written to `room/self` since #697: the full record minus
/// generator bodies, which live in content-addressed child records.
/// `generator_refs` and `traits` are `BTreeMap`s so the manifest bytes are
/// canonical — the manifest is not content-addressed, but a peer diffing a
/// re-broadcast room should see byte-equality when nothing changed.
#[derive(Serialize)]
struct RoomManifestOut {
    #[serde(rename = "$type")]
    lex_type: String,
    environment: Environment,
    generator_refs: std::collections::BTreeMap<String, String>,
    placements: Vec<Placement>,
    traits: std::collections::BTreeMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "ContactEffects::is_default")]
    contact_effects: ContactEffects,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_landing: Option<DefaultLanding>,
}

impl RoomManifestOut {
    fn from_record(record: &RoomRecord) -> Self {
        // Refs this build could not decode go in FIRST, so a live generator
        // authored under the same name wins (#1175). That collision is the
        // one case where dropping an opaque ref is right: the owner has
        // deliberately put something else at that name, and a manifest must
        // not point two ways at once. Everywhere else the opaque ref rides
        // through untouched, which is what keeps the child referenced and so
        // out of the orphan sweep.
        let mut generator_refs: std::collections::BTreeMap<String, String> =
            record.opaque_refs.clone();
        generator_refs.extend(
            record
                .generators
                .iter()
                .map(|(name, generator)| (name.clone(), child_rkey(name, generator))),
        );
        Self {
            lex_type: COLLECTION.into(),
            environment: record.environment.clone(),
            generator_refs,
            placements: record.placements.clone(),
            traits: record
                .traits
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            contact_effects: record.contact_effects.clone(),
            default_landing: record.default_landing,
        }
    }
}

/// Join a decoded `room/self` with the child-generator listing (rkey →
/// `Some(generator)` when this build decoded it, `None` when the record is
/// there but opaque to us) fetched from
/// [`crate::pds::ROOM_GENERATOR_COLLECTION`].
///
/// A ref resolves one of three ways, and the difference matters on the way
/// back out (#1175):
///
/// * **decoded** — merges into `generators` alongside the inline legacy ones.
/// * **listed but undecodable** — recorded in
///   [`RoomRecord::opaque_refs`] so the next publish keeps pointing at it.
///   The room loads without that generator, which is unavoidable; what is
///   avoidable is this client then deleting somebody else's content because
///   it could not read it.
/// * **absent from the listing** — skipped with a warning, as before. A ref
///   pointing at nothing is a torn historical write (or a hostile PDS
///   dropping records); preserving it would carry a dangling ref forever.
fn assemble_room(wire: RoomSelfWire, children: &HashMap<String, Option<Generator>>) -> RoomRecord {
    let RoomSelfWire {
        environment,
        mut generators,
        generator_refs,
        placements,
        traits,
        contact_effects,
        default_landing,
    } = wire;
    let mut opaque_refs = std::collections::BTreeMap::new();
    for (name, rkey) in generator_refs {
        match children.get(&rkey) {
            Some(Some(generator)) => {
                generators.insert(name, generator.clone());
            }
            Some(None) => {
                warn!(
                    "room child generator {rkey} for '{name}' is not decodable by \
                     this build — preserving the reference so a newer client can \
                     still read it"
                );
                opaque_refs.insert(name, rkey);
            }
            None => warn!(
                "room manifest references missing child generator {rkey} for '{name}' — skipping"
            ),
        }
    }
    RoomRecord {
        lex_type: COLLECTION.into(),
        environment,
        generators,
        placements,
        traits,
        contact_effects,
        default_landing,
        opaque_refs,
    }
}

/// `com.atproto.repo.listRecords` envelope for the child walk. Values stay
/// raw so one foreign / undecodable record degrades instead of failing a
/// page; the rkey is recovered from the record's `at://` URI tail and is
/// kept whether or not the value decodes (#1175).
#[derive(Deserialize)]
struct ListChildrenResponse {
    #[serde(default)]
    records: Vec<ListedChild>,
    cursor: Option<String>,
}

#[derive(Deserialize)]
struct ListedChild {
    uri: String,
    value: serde_json::Value,
}

/// Fold one page of listed children into the rkey → decode-outcome map.
///
/// Split out of the walk so the decode half is testable without a PDS: it
/// is the half that decides whether a child is content we can render or
/// content we must merely preserve, and that decision is the whole of
/// #1175.
///
/// The rkey is keyed from the listed `at://` URI, never from the value —
/// that is the point. `list_attachment_rkeys` (avatar/wardrobe.rs) takes
/// the same shape for the same reason: a record we cannot read still has
/// to count as PRESENT, or the publish planner will try to create over it
/// and the orphan sweep will delete it.
fn fold_listed_children(records: Vec<ListedChild>, out: &mut HashMap<String, Option<Generator>>) {
    for rec in records {
        let Some(rkey) = rec.uri.rsplit('/').next() else {
            continue;
        };
        let decoded = serde_json::from_value::<RoomGeneratorRecord>(rec.value)
            .ok()
            .map(|child| child.generator);
        out.insert(rkey.to_string(), decoded);
    }
}

/// Walk the child-generator collection for `did`, returning rkey →
/// `Some(generator)` for every child this build decoded and `None` for
/// every child that is there but opaque to us (#1175). Bounded by
/// [`crate::config::state::MAX_ROOM_GENERATOR_PAGES`] pages of 100 so a
/// hostile PDS handing out endless cursors cannot keep the client paging.
///
/// The walk is not reported as complete-or-truncated the way the
/// attachment listing is (#1185), because the room never uses it as
/// evidence of ABSENCE for a delete: every delete in `plan_room_writes`
/// comes out of this listing, so a short walk can only under-delete. It
/// *is* evidence of absence for the create half, and a truncated walk
/// there would emit `#create` over a record that exists — an opaque 500
/// that fails the whole save (#1186). That cannot happen for a legal room:
/// four pages hold 400 records against `sanitize`'s
/// `MAX_GENERATORS = 256` cap, and the surplus is orphan-swept on every
/// publish. It would take a repo that had already accumulated >400
/// children — i.e. a GC that had already failed — to get there.
async fn list_room_children(
    client: &reqwest::Client,
    pds: &str,
    did: &str,
) -> Result<HashMap<String, Option<Generator>>, FetchError> {
    let mut children: HashMap<String, Option<Generator>> = HashMap::new();
    let mut cursor: Option<String> = None;
    for _ in 0..crate::config::state::MAX_ROOM_GENERATOR_PAGES {
        let url = format!("{}/xrpc/com.atproto.repo.listRecords", pds);
        let mut query: Vec<(&str, String)> = vec![
            ("repo", did.to_string()),
            (
                "collection",
                crate::pds::ROOM_GENERATOR_COLLECTION.to_string(),
            ),
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
        let page: ListChildrenResponse = decode_record_json(resp).await?;
        let empty_page = page.records.is_empty();
        fold_listed_children(page.records, &mut children);
        cursor = page.cursor;
        if cursor.is_none() || empty_page {
            break;
        }
    }
    Ok(children)
}

/// Fetch the room customisation record from the given DID's PDS.
///
/// * `Ok(Some(record))` — the owner has published a record.
/// * `Ok(None)` — the PDS reported there is no record yet (the caller may
///   substitute the default homeworld).
/// * `Err(FetchError)` — transient or permanent failure; the caller must
///   **not** fall through to the default, because doing so risks the user
///   publishing the blank default over their real room on the next save.
///
/// Shape-agnostic since #697: a legacy monolith is returned as-is, while a
/// manifest triggers one additional `listRecords` walk over the
/// child-generator collection before assembly.
///
/// Note: ATProto's `com.atproto.repo.getRecord` returns `400 RecordNotFound`
/// — NOT `404` — when the record does not exist. We detect that payload
/// explicitly and convert it to `Ok(None)` so the loading state can advance
/// onto the default homeworld instead of hammering the PDS with retries.
pub async fn fetch_room_record(
    client: &reqwest::Client,
    did: &str,
) -> Result<Option<RoomRecord>, FetchError> {
    let pds = resolve_pds(client, did)
        .await
        .ok_or(FetchError::DidResolutionFailed)?;
    let url = format!(
        "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection={}&rkey=self",
        pds, did, COLLECTION
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
        // Inspect the error body before surfacing as PdsError — ATProto
        // signals "no such record" via 400 + `error: "RecordNotFound"` in
        // the body, and we must not treat that as a transient retry case.
        // Capped (#1124): a room is fetched from its owner's PDS, which
        // for a portal or gateway visit is a stranger's.
        let body = crate::pds::xrpc::read_capped_text(resp).await;
        if let Ok(xrpc) = serde_json::from_str::<XrpcError>(&body)
            && let Some(err) = xrpc.error.as_deref()
            && (err == "RecordNotFound"
                || (err == "InvalidRequest" && body.contains("RecordNotFound")))
        {
            return Ok(None);
        }
        return Err(FetchError::PdsError(status.as_u16()));
    }
    let wrapper: GetRecordResponse = decode_record_json(resp).await?;
    let wire: RoomSelfWire =
        serde_json::from_value(wrapper.value).map_err(|e| FetchError::Decode(e.to_string()))?;
    let children = if wire.generator_refs.is_empty() {
        HashMap::new()
    } else {
        list_room_children(client, &pds, did).await?
    };
    let mut record = assemble_room(wire, &children);
    record.sanitize();
    Ok(Some(record))
}

// ---------------------------------------------------------------------------
// Write: publish room record to the authenticated user's PDS
// ---------------------------------------------------------------------------

/// Serialized size of the largest single record a publish of `record`
/// would write — the manifest or the biggest child generator. This is the
/// per-record figure the #694 size budget applies to now that the room is
/// split across records (#697).
pub fn max_publish_record_bytes(record: &RoomRecord) -> Option<usize> {
    let manifest =
        crate::pds::record_size::serialized_record_bytes(&RoomManifestOut::from_record(record));
    let biggest_child = record
        .generators
        .iter()
        .filter_map(|(name, generator)| {
            crate::pds::record_size::serialized_record_bytes(&RoomGeneratorRecord::new(
                name, generator,
            ))
        })
        .max();
    manifest.into_iter().chain(biggest_child).max()
}

/// Build the ordered `applyWrites` batches that publish `record` as a
/// manifest + content-addressed children (#697), given the child rkeys
/// currently on the PDS and whether `room/self` already exists.
///
/// The plan is: child creates, then the manifest put, then orphan deletes —
/// chunked by [`crate::pds::xrpc::chunk_writes`] to both the write-count
/// commit cap and the request-body byte budget, in that order, so a
/// visitor reading between commits always sees a manifest whose refs all
/// resolve (new children land before the manifest points at them; orphans
/// are only deleted after nothing references them). Unchanged generators
/// cost nothing: same content → same rkey → already in `existing`.
/// Every record written is size-checked against the hard ceiling first.
fn plan_room_writes(
    record: &RoomRecord,
    existing_children: &std::collections::HashSet<String>,
    manifest_exists: bool,
) -> Result<Vec<Vec<RepoWrite>>, String> {
    let manifest = RoomManifestOut::from_record(record);
    crate::pds::record_size::preflight(&manifest, "room manifest")?;

    // Desired child set, deduped by rkey (identical content under two
    // names shares one record) and sorted for a deterministic plan.
    let mut desired: std::collections::BTreeMap<String, RepoWrite> =
        std::collections::BTreeMap::new();
    for (name, generator) in &record.generators {
        let rkey = child_rkey(name, generator);
        if existing_children.contains(&rkey) || desired.contains_key(&rkey) {
            continue;
        }
        let child = RoomGeneratorRecord::new(name, generator);
        crate::pds::record_size::preflight(&child, &format!("room generator \"{name}\""))?;
        let value = serde_json::to_value(&child).map_err(|e| format!("serialize: {e}"))?;
        desired.insert(
            rkey.clone(),
            RepoWrite::Create {
                collection: crate::pds::ROOM_GENERATOR_COLLECTION.into(),
                rkey,
                value,
            },
        );
    }
    let creates: Vec<RepoWrite> = desired.into_values().collect();

    // Orphaned means "the manifest we are about to write does not point at
    // it" — read off `manifest.generator_refs`, not recomputed from
    // `record.generators`. The two differ by exactly the opaque refs
    // (#1175): children this build could not decode, which the manifest
    // still names and which must therefore survive the sweep. Deriving the
    // set from the thing actually being written means there is no second
    // place to keep in step.
    //
    // Deletes are still sourced purely from `existing_children`, so a short
    // listing can only leave an orphan for a later sweep — never delete a
    // record the repo does not have. That asymmetry is why this sweep needs
    // no completeness gate, unlike `attachment_retirements`, whose delete
    // set has a derived half the listing cannot vouch for.
    let referenced: std::collections::HashSet<&String> = manifest.generator_refs.values().collect();
    let mut orphans: Vec<&String> = existing_children
        .iter()
        .filter(|rkey| !referenced.contains(*rkey))
        .collect();
    orphans.sort();
    let deletes: Vec<RepoWrite> = orphans
        .into_iter()
        .map(|rkey| RepoWrite::Delete {
            collection: crate::pds::ROOM_GENERATOR_COLLECTION.into(),
            rkey: rkey.clone(),
        })
        .collect();

    let manifest_value = serde_json::to_value(&manifest).map_err(|e| format!("serialize: {e}"))?;
    let manifest_write = if manifest_exists {
        RepoWrite::Update {
            collection: COLLECTION.into(),
            rkey: "self".into(),
            value: manifest_value,
        }
    } else {
        RepoWrite::Create {
            collection: COLLECTION.into(),
            rkey: "self".into(),
            value: manifest_value,
        }
    };

    // Chunk in read-safe order: creates → manifest (+ deletes that fit) →
    // remaining deletes.
    let ordered: Vec<RepoWrite> = creates
        .into_iter()
        .chain(std::iter::once(manifest_write))
        .chain(deletes)
        .collect();
    // Sharing a batch across the create/manifest/delete phases is fine —
    // each applyWrites batch commits atomically, so only the ordering at
    // CHUNK boundaries matters, and the linear order above provides it.
    crate::pds::xrpc::chunk_writes(ordered)
}

/// `true` when `room/self` exists on the PDS in either shape. Publish uses
/// this to pick `applyWrites#create` vs `#update` for the manifest.
async fn room_self_exists(client: &reqwest::Client, pds: &str, did: &str) -> Result<bool, String> {
    let url = format!(
        "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection={}&rkey=self",
        pds, did, COLLECTION
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("existence check: {e}"))?;
    let status = resp.status();
    if status.is_success() {
        return Ok(true);
    }
    if status.as_u16() == 404 {
        return Ok(false);
    }
    let body = crate::pds::xrpc::read_capped_text(resp).await;
    if body.contains("RecordNotFound") {
        return Ok(false);
    }
    Err(format!("existence check failed: {} — {}", status, body))
}

/// Publish the room to the authenticated user's own PDS as a slim manifest
/// plus content-addressed child generator records (#697).
///
/// The write plan diffs the desired child set against a `listRecords` walk
/// of what is actually on the PDS (authoritative — orphans from older
/// writes or other devices are GC'd in the same plan) and commits via
/// `com.atproto.repo.applyWrites` in read-safe order: children first, then
/// the manifest, then orphan deletes. A plan that fits one batch (the
/// overwhelmingly common case) is fully atomic; a chunked plan is safe at
/// every commit boundary because manifests only ever reference
/// already-committed, immutable children.
///
/// This replaces the old `putRecord` + delete-then-put 5xx recovery: the
/// manifest is small and schema-stable, and `applyWrites` sidesteps the
/// stale-CID diffing that path worked around.
pub async fn publish_room_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    record: &RoomRecord,
) -> Result<(), String> {
    // Refused before any network I/O, and before `child_rkey` hashes a body
    // it could not write (#1111): a generator, placement or material this
    // build decoded as `Unknown` cannot be re-serialized, and saving anyway
    // would replace the owner's newer content with a husk — the split-wire
    // publish would even GC the original child as an orphan.
    crate::pds::record_size::wire_ready(record, "world")?;
    let pds = resolve_pds(client, &session.did)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;

    let existing: std::collections::HashSet<String> =
        list_room_children(client, &pds, &session.did)
            .await
            .map_err(|e| format!("child listing failed: {e:?}"))?
            .into_keys()
            .collect();
    let manifest_exists = room_self_exists(client, &pds, &session.did).await?;

    // Plan construction runs every per-record size preflight BEFORE any
    // write lands, so an oversized generator can never leave the room
    // half-published.
    let batches = plan_room_writes(record, &existing, manifest_exists)?;
    for batch in batches {
        crate::pds::xrpc::apply_writes(&pds, session, refresh, batch).await?;
    }
    Ok(())
}

/// Delete the room from the authenticated user's PDS — the `room/self`
/// manifest (whichever shape it holds) **and** every record in the
/// child-generator collection, so a reset cannot strand orphaned children
/// (#697). Deletes only what a `listRecords` walk + existence check say is
/// actually there (an `applyWrites#delete` on a missing record fails the
/// whole batch), and deletes the manifest FIRST so an interrupted wipe
/// never leaves a manifest pointing at removed children. A repo with
/// nothing to delete is a clean no-op.
pub async fn delete_room_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
) -> Result<(), String> {
    let pds = resolve_pds(client, &session.did)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;

    let mut deletes: Vec<RepoWrite> = Vec::new();
    if room_self_exists(client, &pds, &session.did).await? {
        deletes.push(RepoWrite::Delete {
            collection: COLLECTION.into(),
            rkey: "self".into(),
        });
    }
    let mut children: Vec<String> = list_room_children(client, &pds, &session.did)
        .await
        .map_err(|e| format!("child listing failed: {e:?}"))?
        .into_keys()
        .collect();
    children.sort();
    deletes.extend(children.into_iter().map(|rkey| RepoWrite::Delete {
        collection: crate::pds::ROOM_GENERATOR_COLLECTION.into(),
        rkey,
    }));

    for batch in crate::pds::xrpc::chunk_writes(deletes)? {
        crate::pds::xrpc::apply_writes(&pds, session, refresh, batch).await?;
    }
    Ok(())
}

/// Force-overwrite the room by wiping manifest + children first, then
/// publishing fresh. Used by the recovery banner's "Reset PDS to default"
/// button, which must work even when the stored record is
/// schema-incompatible with the current build.
pub async fn reset_room_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    record: &RoomRecord,
) -> Result<(), String> {
    // Size-guard BEFORE the delete: this path removes the stored record
    // first, so an oversized replacement refused only at publish time would
    // already have destroyed the owner's saved room. The publish below
    // re-checks per record; this early manifest-level check just fails fast
    // on the worst case.
    crate::pds::record_size::preflight(&RoomManifestOut::from_record(record), "room manifest")?;
    delete_room_record(client, session, refresh).await?;
    publish_room_record(client, session, refresh, record).await
}

#[cfg(test)]
mod split_wire_tests {
    //! #697 manifest + content-addressed children: the write plan and the
    //! read-side join must be exact inverses, in read-safe order, across
    //! both wire shapes.
    use super::*;
    use crate::pds::generator::GeneratorKind;
    use crate::pds::sanitize::limits;
    use crate::pds::types::{Fp, Fp3};
    use std::collections::HashSet;

    fn cuboid_at(x: f32) -> Generator {
        let mut g = Generator::default_cuboid();
        g.transform.translation.0[0] = x;
        g
    }

    #[test]
    fn child_rkey_is_content_addressed_hex() {
        let a = cuboid_at(1.0);
        let key = child_rkey("tree", &a);
        assert_eq!(key, child_rkey("tree", &a), "deterministic");
        assert_ne!(key, child_rkey("bush", &a), "name participates");
        assert_ne!(
            key,
            child_rkey("tree", &cuboid_at(2.0)),
            "content participates"
        );
        assert_eq!(key.len(), 16);
        assert!(
            key.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
    }

    /// Build an LSystem generator whose two maps are filled in the order
    /// `rotation` dictates. Same content every time; only the `HashMap`
    /// insertion sequence — and therefore its iteration order — differs.
    fn lsystem_with_maps(rotation: usize) -> Generator {
        let slots: Vec<u16> = vec![7, 1, 900, 42, 3, 250, 11, 65535];
        let props: Vec<u16> = vec![5, 2, 9, 1];
        let mut materials = HashMap::new();
        for i in 0..slots.len() {
            let slot = slots[(i + rotation) % slots.len()];
            materials.insert(
                slot,
                crate::pds::texture::SovereignMaterialSettings {
                    roughness: Fp(slot as f32 / 65535.0),
                    ..Default::default()
                },
            );
        }
        let mut prop_mappings = HashMap::new();
        for i in 0..props.len() {
            let id = props[(i + rotation) % props.len()];
            prop_mappings.insert(id, crate::pds::prim::PropMeshType::Leaf);
        }
        Generator::from_kind(GeneratorKind::LSystem {
            source_code: "A --> F[+A][-A]".into(),
            finalization_code: String::new(),
            iterations: 3,
            seed: 12,
            angle: Fp(25.0),
            step: Fp(1.0),
            width: Fp(0.1),
            elasticity: Fp(0.0),
            tropism: None,
            materials,
            prop_mappings,
            prop_scale: Fp(1.0),
            mesh_resolution: 4,
        })
    }

    /// The whole split-wire scheme rests on same-content-same-rkey, and
    /// before #1118 that silently failed for every generator carrying a map:
    /// `RandomState` re-seeds per map, so two equal `HashMap`s built in one
    /// process iterate differently, serialize to different bytes, and hash
    /// to different rkeys. The room would then re-create every tree on every
    /// save and GC the identical child it had just replaced.
    ///
    /// The sequence that produced it: decode a room, edit anything, publish
    /// — the decode rebuilt `materials` as a fresh map, so no LSystem child
    /// ever matched `existing_children`.
    #[test]
    fn child_rkey_ignores_hashmap_insertion_order() {
        let baseline = child_rkey("tree", &lsystem_with_maps(0));
        for rotation in 0..50 {
            assert_eq!(
                baseline,
                child_rkey("tree", &lsystem_with_maps(rotation)),
                "rotation {rotation} minted a different rkey for identical content"
            );
        }
    }

    /// The rkey is only an address if it addresses the bytes the batch
    /// actually writes. Assert the serialized child — the exact value
    /// `plan_room_writes` hands to `applyWrites` — is byte-identical across
    /// independently built maps, not merely equal-hashing.
    #[test]
    fn child_bytes_are_byte_identical_across_rebuilds() {
        let canonical =
            serde_json::to_string(&RoomGeneratorRecord::new("tree", &lsystem_with_maps(0)))
                .unwrap();
        for rotation in 0..50 {
            let again = serde_json::to_string(&RoomGeneratorRecord::new(
                "tree",
                &lsystem_with_maps(rotation),
            ))
            .unwrap();
            assert_eq!(
                canonical, again,
                "rotation {rotation} wrote different bytes"
            );
        }
        // And the keys really are sorted, not merely stable.
        let mats = canonical
            .split("\"materials\":{")
            .nth(1)
            .expect("materials object");
        let order: Vec<u16> = mats
            .split('}')
            .next()
            .unwrap()
            .split(',')
            .filter_map(|entry| entry.split(':').next())
            .filter_map(|k| k.trim().trim_matches('"').parse::<u16>().ok())
            .collect();
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(order, sorted, "material slots must serialize in key order");
    }

    /// The same defect in the `String`-keyed half: `Shape::materials` is
    /// keyed on the grammar's `Mat("...")` names and derives its serializer,
    /// so it needs the explicit `sorted_string_map` rather than the
    /// `map_u16_as_string` module.
    #[test]
    fn shape_material_names_serialize_sorted() {
        let names = ["stone", "glass", "trim", "roof", "brick"];
        let build = |rotation: usize| {
            let mut materials = HashMap::new();
            for i in 0..names.len() {
                let name = names[(i + rotation) % names.len()];
                materials.insert(
                    name.to_string(),
                    crate::pds::texture::SovereignMaterialSettings::default(),
                );
            }
            Generator::from_kind(GeneratorKind::Shape {
                grammar_source: "Lot --> Extrude(3) Roof".into(),
                root_rule: "Lot".into(),
                footprint: Fp3([4.0, 0.0, 4.0]),
                seed: 3,
                materials,
                round_meshes: Vec::new(),
            })
        };
        let baseline = child_rkey("hut", &build(0));
        for rotation in 0..names.len() {
            assert_eq!(baseline, child_rkey("hut", &build(rotation)));
        }
    }

    /// The manifest is not content-addressed, but a peer diffing a
    /// re-broadcast room compares bytes, so `traits` must not shuffle either.
    #[test]
    fn manifest_traits_serialize_sorted() {
        let mut record = RoomRecord::default_for_did("did:plc:traits");
        for name in ["zeta", "alpha", "mid", "beta", "omega"] {
            record
                .traits
                .insert(name.into(), vec![format!("{name}-trait")]);
        }
        let once = serde_json::to_string(&RoomManifestOut::from_record(&record)).unwrap();
        let traits = once.split("\"traits\":{").nth(1).expect("traits object");
        let order: Vec<&str> = traits
            .split('}')
            .next()
            .unwrap()
            .split("],")
            .filter_map(|entry| entry.split(':').next())
            .map(|k| k.trim().trim_matches('"'))
            .collect();
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(order, sorted, "trait keys must serialize in key order");
    }

    #[test]
    fn plan_creates_new_skips_existing_and_deletes_orphans() {
        let record = RoomRecord::default_for_did("did:plc:plan");
        // Make one generator "already published": seed the existing set
        // with its content-addressed rkey.
        let (unchanged_name, unchanged_gen) = record
            .generators
            .iter()
            .next()
            .map(|(n, g)| (n.clone(), g.clone()))
            .unwrap();
        let unchanged_rkey = child_rkey(&unchanged_name, &unchanged_gen);
        let orphan_rkey = "00000000deadbeef".to_string();
        let existing: HashSet<String> = [unchanged_rkey.clone(), orphan_rkey.clone()]
            .into_iter()
            .collect();

        let batches = plan_room_writes(&record, &existing, true).unwrap();
        assert_eq!(batches.len(), 1, "default room fits one atomic batch");
        let writes = &batches[0];

        // Creates for every generator EXCEPT the unchanged one.
        let creates: Vec<_> = writes
            .iter()
            .filter(|w| matches!(w, RepoWrite::Create { collection, .. } if collection == crate::pds::ROOM_GENERATOR_COLLECTION))
            .collect();
        assert_eq!(creates.len(), record.generators.len() - 1);
        assert!(
            !writes
                .iter()
                .any(|w| matches!(w, RepoWrite::Create { rkey, .. } if *rkey == unchanged_rkey)),
            "unchanged content is free — no rewrite"
        );

        // Manifest is an update (room/self exists) and sits after every
        // create and before every delete.
        let manifest_idx = writes
            .iter()
            .position(|w| matches!(w, RepoWrite::Update { collection, rkey, .. } if collection == COLLECTION && rkey == "self"))
            .expect("manifest update present");
        let last_create = writes
            .iter()
            .rposition(|w| matches!(w, RepoWrite::Create { .. }))
            .unwrap();
        let first_delete = writes
            .iter()
            .position(|w| matches!(w, RepoWrite::Delete { .. }))
            .expect("orphan delete present");
        assert!(last_create < manifest_idx && manifest_idx < first_delete);
        assert!(
            writes
                .iter()
                .any(|w| matches!(w, RepoWrite::Delete { rkey, .. } if *rkey == orphan_rkey)),
            "orphan is GC'd"
        );

        // Fresh repo → the manifest write is a create instead.
        let batches = plan_room_writes(&record, &HashSet::new(), false).unwrap();
        assert!(batches[0].iter().any(|w| matches!(
            w,
            RepoWrite::Create { collection, rkey, .. } if collection == COLLECTION && rkey == "self"
        )));
    }

    #[test]
    fn plan_dedups_identical_content_under_two_names() {
        let mut record = RoomRecord::default_for_did("did:plc:dedup");
        record.generators.clear();
        // The child body embeds the name, so true dedup needs identical
        // (name, content) — which two map keys can't produce. What CAN
        // happen is the same rkey appearing twice via map iteration of
        // equal content+name pairs after a merge; assert the guard holds
        // for the reachable case: one name, one child, and the ref map
        // still covers every generator.
        record.generators.insert("a".into(), cuboid_at(1.0));
        record.generators.insert("b".into(), cuboid_at(1.0));
        let batches = plan_room_writes(&record, &HashSet::new(), false).unwrap();
        let creates = batches[0]
            .iter()
            .filter(|w| matches!(w, RepoWrite::Create { collection, .. } if collection == crate::pds::ROOM_GENERATOR_COLLECTION))
            .count();
        // Different names → different child bodies → two creates.
        assert_eq!(creates, 2);
    }

    #[test]
    fn plan_chunks_past_the_commit_cap_in_read_safe_order() {
        let mut record = RoomRecord::default_for_did("did:plc:chunk");
        record.generators.clear();
        for i in 0..limits::MAX_GENERATORS {
            record
                .generators
                .insert(format!("g{i:03}"), cuboid_at(i as f32));
        }
        // 256 stale children on the PDS, none referenced.
        let existing: HashSet<String> = (0..limits::MAX_GENERATORS)
            .map(|i| format!("{i:016x}"))
            .collect();

        let batches = plan_room_writes(&record, &existing, true).unwrap();
        assert!(
            batches
                .iter()
                .all(|b| b.len() <= crate::pds::xrpc::MAX_APPLY_WRITES)
        );
        let flat: Vec<&RepoWrite> = batches.iter().flatten().collect();
        assert_eq!(flat.len(), limits::MAX_GENERATORS * 2 + 1);

        let manifest_idx = flat
            .iter()
            .position(|w| matches!(w, RepoWrite::Update { rkey, .. } if rkey == "self"))
            .unwrap();
        let last_create = flat
            .iter()
            .rposition(|w| matches!(w, RepoWrite::Create { .. }))
            .unwrap();
        let first_delete = flat
            .iter()
            .position(|w| matches!(w, RepoWrite::Delete { .. }))
            .unwrap();
        assert!(
            last_create < manifest_idx && manifest_idx < first_delete,
            "creates → manifest → deletes even across chunk boundaries"
        );
    }

    #[test]
    fn split_publish_reassembles_to_the_same_record() {
        for seed in [0u64, 1, 42, 0xDEAD_BEEF] {
            let record = RoomRecord::default_for_seed(seed, "did:plc:split");
            // Write side.
            let manifest = RoomManifestOut::from_record(&record);
            let children: HashMap<String, Option<Generator>> = record
                .generators
                .iter()
                .map(|(name, g)| (child_rkey(name, g), Some(g.clone())))
                .collect();
            // Read side: decode the manifest bytes and join the children.
            let wire: RoomSelfWire =
                serde_json::from_value(serde_json::to_value(&manifest).unwrap()).unwrap();
            assert!(wire.generators.is_empty(), "manifest carries no bodies");
            assert_eq!(wire.generator_refs.len(), record.generators.len());
            let assembled = assemble_room(wire, &children);
            assert_eq!(
                serde_json::to_value(&assembled).unwrap(),
                serde_json::to_value(&record).unwrap(),
                "seed {seed}: split round-trip diverged"
            );
        }
    }

    #[test]
    fn legacy_monolith_decodes_through_the_wire_shape() {
        let record = RoomRecord::default_for_did("did:plc:legacy");
        // `RoomRecord::serialize` still emits the legacy inline shape — the
        // in-memory model IS the old wire format.
        let wire: RoomSelfWire =
            serde_json::from_value(serde_json::to_value(&record).unwrap()).unwrap();
        assert!(wire.generator_refs.is_empty());
        let assembled = assemble_room(wire, &HashMap::new());
        assert_eq!(
            serde_json::to_value(&assembled).unwrap(),
            serde_json::to_value(&record).unwrap(),
        );
    }

    #[test]
    fn missing_child_skips_that_generator_only() {
        let mut record = RoomRecord::default_for_did("did:plc:missing");
        record.generators.clear();
        record.generators.insert("kept".into(), cuboid_at(1.0));
        record.generators.insert("lost".into(), cuboid_at(2.0));
        let manifest = RoomManifestOut::from_record(&record);
        let children: HashMap<String, Option<Generator>> = [(
            child_rkey("kept", &record.generators["kept"]),
            Some(record.generators["kept"].clone()),
        )]
        .into_iter()
        .collect();
        let wire: RoomSelfWire =
            serde_json::from_value(serde_json::to_value(&manifest).unwrap()).unwrap();
        let assembled = assemble_room(wire, &children);
        assert_eq!(assembled.generators.len(), 1);
        assert!(assembled.generators.contains_key("kept"));
        assert_eq!(assembled.placements.len(), record.placements.len());
    }

    /// #1175 — the whole sequence, end to end: a newer client's child
    /// record lands in the listing in a shape this build cannot decode, and
    /// the next publish from this build must leave it exactly where it is.
    ///
    /// Before the fix the listing dropped the rkey entirely, so
    /// `existing_children` never held it, `assemble_room` never mentioned
    /// it, the manifest was rewritten without its ref, and the owner's
    /// generator was gone from the room with the bytes stranded on the PDS
    /// — invisible to the very orphan sweep that would have tidied them.
    #[test]
    fn an_undecodable_child_is_preserved_across_a_publish() {
        let mut record = RoomRecord::default_for_did("did:plc:opaque");
        record.generators.clear();
        record.generators.insert("mine".into(), cuboid_at(1.0));

        // What the PDS hands back: our own child, plus one written by a
        // client whose child schema this build cannot parse (here: no
        // `name`, which `RoomGeneratorRecord` requires). This is a listing
        // response, not a hand-built map — the decode is the defect.
        let ours = child_rkey("mine", &record.generators["mine"]);
        let theirs = "0123456789abcdef".to_string();
        let did = "did:plc:opaque";
        let coll = crate::pds::ROOM_GENERATOR_COLLECTION;
        let listed = vec![
            ListedChild {
                uri: format!("at://{did}/{coll}/{ours}"),
                value: serde_json::to_value(RoomGeneratorRecord::new(
                    "mine",
                    &record.generators["mine"],
                ))
                .unwrap(),
            },
            ListedChild {
                uri: format!("at://{did}/{coll}/{theirs}"),
                value: serde_json::json!({
                    "$type": coll,
                    "generator": { "$type": "network.symbios.gen.future2027" },
                }),
            },
        ];
        let mut children: HashMap<String, Option<Generator>> = HashMap::new();
        fold_listed_children(listed, &mut children);
        assert_eq!(
            children.get(&theirs),
            Some(&None),
            "the undecodable child is PRESENT with no body, not absent"
        );

        // The manifest the newer client wrote names both.
        let mut manifest = RoomManifestOut::from_record(&record);
        manifest
            .generator_refs
            .insert("theirs".into(), theirs.clone());
        let wire: RoomSelfWire =
            serde_json::from_value(serde_json::to_value(&manifest).unwrap()).unwrap();
        let assembled = assemble_room(wire, &children);

        // The room loads with one renderable generator and still REFERENCES
        // the other. Losing it from `generators` is unavoidable; losing it
        // from the record is not.
        assert_eq!(assembled.generators.len(), 1);
        assert_eq!(assembled.opaque_refs.get("theirs"), Some(&theirs));
        assert_eq!(
            RoomManifestOut::from_record(&assembled)
                .generator_refs
                .get("theirs"),
            Some(&theirs),
            "the republished manifest still points at the child we could not read"
        );

        // Now publish it back. `existing_children` is every LISTED rkey,
        // opaque ones included — that is what the fix keys on.
        let existing: HashSet<String> = children.keys().cloned().collect();
        let batches = plan_room_writes(&assembled, &existing, true).unwrap();
        let writes: Vec<&RepoWrite> = batches.iter().flatten().collect();
        assert!(
            !writes.iter().any(|w| matches!(
                w,
                RepoWrite::Delete { rkey, .. } if *rkey == theirs
            )),
            "the orphan sweep must not delete a child it merely failed to read"
        );
        assert!(
            !writes.iter().any(|w| matches!(
                w,
                RepoWrite::Create { rkey, .. } if *rkey == theirs
            )),
            "and must not #create over it — the record is already there, and \
             applyWrites answers that with an opaque 500 that fails the save"
        );
        // Nothing else regressed: our own unchanged child is still free.
        assert!(!writes.iter().any(|w| matches!(
            w,
            RepoWrite::Create { rkey, .. } if *rkey == ours
        )));
    }

    /// A live generator authored under an opaque ref's name wins, and the
    /// now-unreferenced child becomes a normal orphan. This is the one case
    /// where #1175 deliberately lets the child go: the owner has put
    /// something else at that name, and a manifest cannot point two ways.
    #[test]
    fn a_reused_name_overrides_its_opaque_ref() {
        let mut record = RoomRecord::default_for_did("did:plc:reuse");
        record.generators.clear();
        record.generators.insert("shared".into(), cuboid_at(3.0));
        record
            .opaque_refs
            .insert("shared".into(), "deadbeefdeadbeef".into());

        let refs = RoomManifestOut::from_record(&record).generator_refs;
        assert_eq!(
            refs.get("shared"),
            Some(&child_rkey("shared", &record.generators["shared"])),
            "the live generator's rkey, not the opaque one"
        );
        let existing: HashSet<String> = ["deadbeefdeadbeef".to_string()].into_iter().collect();
        let batches = plan_room_writes(&record, &existing, true).unwrap();
        assert!(batches.iter().flatten().any(|w| matches!(
            w,
            RepoWrite::Delete { rkey, .. } if rkey == "deadbeefdeadbeef"
        )));
    }

    #[test]
    fn max_publish_record_bytes_covers_manifest_and_children() {
        let record = RoomRecord::default_for_did("did:plc:bytes");
        let max = max_publish_record_bytes(&record).unwrap();
        let manifest_bytes = crate::pds::record_size::serialized_record_bytes(
            &RoomManifestOut::from_record(&record),
        )
        .unwrap();
        assert!(max >= manifest_bytes);
        for (name, g) in &record.generators {
            let child = crate::pds::record_size::serialized_record_bytes(
                &RoomGeneratorRecord::new(name, g),
            )
            .unwrap();
            assert!(max >= child);
        }
    }
}
