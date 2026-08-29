//! Wardrobe, profile and attachment records — the rigged body's PDS half
//! (#1056, epic #1054).
//!
//! Three collections, two of them adopted from the `symbios-avatar` sibling
//! project's lexicons rather than invented here:
//!
//!   - [`WARDROBE_COLLECTION`] (`network.symbios.avatar.avatar`, tid-keyed) —
//!     one engine [`EngineAvatarRecord`] per record, many per identity. The
//!     wardrobe is cross-app by design: any symbios application can read the
//!     same bodies, which is the reason these records do NOT live under an
//!     overlands NSID.
//!   - [`AVATAR_PROFILE_COLLECTION`] (`network.symbios.avatar.profile`,
//!     rkey = self) — the identity's default-body pointer. Overlands keeps
//!     it in step with the worn body so other symbios apps agree on what the
//!     identity looks like, and reads it as a fallback: an identity with no
//!     overlands avatar record but a wardrobe from another app spawns
//!     wearing their default body instead of a seeded vehicle.
//!   - [`AVATAR_ATTACHMENT_COLLECTION`]
//!     (`network.symbios.overlands.avatar.attachment`, tid-keyed) — one worn
//!     prop per record: an owned COPY of a [`Generator`] plus the rig socket
//!     it hangs from and its offset transform. A copy rather than an
//!     inventory reference (decision on epic #1054): editing or deleting the
//!     inventory item must not mutate an outfit that was already dressed.
//!
//! Publishing all of it — body, props, profile pointer and the overlands
//! avatar record — is ONE `com.atproto.repo.applyWrites` batch (#1117),
//! planned by [`plan_avatar_writes`] against what [`AvatarRepoState`] says
//! the repo already holds. It commits whole or not at all, which is what
//! stops a transient failure landing the props and losing the record that
//! references them, and its delete phase is also the only sweep that ever
//! retires an orphaned attachment.

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// The engine's parametric avatar record, under the name the rest of this
/// crate uses for it. `crate::pds::AvatarRecord` is the overlands record;
/// this is the body it references.
pub use symbios_avatar::AvatarRecord as EngineAvatarRecord;
pub use symbios_avatar::ProfileRecord as EngineProfileRecord;

use super::super::generator::Generator;
use super::super::sanitize::{Sanitize as _, sanitize_avatar_visuals};
use super::super::types::TransformData;
use super::super::xrpc::{
    FetchError, RepoWrite, XrpcError, chunk_writes, decode_record_json, record_exists, resolve_pds,
};
use super::super::{AVATAR_ATTACHMENT_COLLECTION, AVATAR_PROFILE_COLLECTION, WARDROBE_COLLECTION};
use super::body::{ResolvedAttachment, ResolvedRig, RiggedBody};

/// The longest socket name accepted off the wire. The engine's socket names
/// are short kebab strings ([`symbios_avatar::Socket::name`]); anything past
/// this is not one, but an *unknown short* name is deliberately kept — a
/// future socket from a newer client degrades to "prop not worn", the same
/// answer every open union here gives.
const MAX_SOCKET_NAME_CHARS: usize = 32;

/// Bounds a non-zero [`AttachmentRecord::fit_band_mm`] is clamped into.
/// 50 mm is a doll's circlet and 1000 mm a barrel hoop — both absurd but
/// harmlessly renderable; outside them the fit ratio itself becomes the
/// attack (a 1 mm band inflates a prop ~180×).
const MIN_FIT_BAND_MM: u32 = 50;
/// See [`MIN_FIT_BAND_MM`].
const MAX_FIT_BAND_MM: u32 = 1000;

/// How many `listRecords` pages a wardrobe walk will fetch (100 records a
/// page). A wardrobe past this is not a wardrobe, it is a DoS.
const MAX_WARDROBE_LIST_PAGES: usize = 4;

// ---------------------------------------------------------------------------
// AttachmentRecord
// ---------------------------------------------------------------------------

/// One worn prop: a record in [`AVATAR_ATTACHMENT_COLLECTION`] at a TID rkey.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AttachmentRecord {
    #[serde(rename = "$type")]
    pub lex_type: String,
    /// The prop itself — an owned copy of the item's `Generator` tree,
    /// sanitised with the avatar kind rules (no Terrain/Water/Portal).
    pub item: Generator,
    /// Which rig socket carries it, as the engine's stable kebab name
    /// ([`symbios_avatar::Socket::name`]). An unknown name is kept on the
    /// record and skipped at spawn.
    pub socket: String,
    /// Offset from the socket's joint, in the joint's rest-pose frame.
    /// Quantised like every transform on the wire; identity elides.
    #[serde(default, skip_serializing_if = "TransformData::is_identity")]
    pub offset: TransformData,
    /// Measurement-fit declaration (#1089), copied from the catalogue
    /// entry's [`WearFit`](crate::catalogue::WearFit) at Wear time: the
    /// item's **authored band inner diameter in whole millimetres**, which
    /// each client fits to the wearer's measured brow circumference at
    /// dress time (`src/player/attachments.rs`, `fit_scale`). Carried on
    /// the record — not looked up — because a peer dresses from the wire
    /// alone. `0` (elided) means no fit: the prop is worn at authored
    /// size. Integer millimetres because atproto records hold no floats.
    #[serde(default, rename = "fitBandMm", skip_serializing_if = "fit_elides")]
    pub fit_band_mm: u32,
    /// The inventory item this was worn from (#1096), by name — the
    /// provenance "Save to inventory" writes back to, and what lets the
    /// Inventory window show an item as worn. `None` for a prop attached
    /// from a bare generator (a legacy attach, a gift never stashed). A
    /// name that no longer exists in the stash is not an error: saving
    /// then creates the item afresh under that name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Serde elision for [`AttachmentRecord::fit_band_mm`]: `0` is "no fit"
/// and stays off the wire, so every pre-#1089 record round-trips
/// byte-identical.
fn fit_elides(mm: &u32) -> bool {
    *mm == 0
}

/// The shared clamp for the three fields an attachment record and an
/// inventory item's [`WearMeta`](crate::pds::inventory::WearMeta) both
/// carry (#1096): socket name length, the fit dimension's divisor floor
/// and ceiling, and the offset transform. One function so the two wire
/// forms can never drift apart in what they accept.
pub fn sanitize_wear_fields(
    socket: &mut String,
    fit_band_mm: &mut u32,
    offset: &mut TransformData,
) {
    if socket.chars().count() > MAX_SOCKET_NAME_CHARS {
        *socket = socket.chars().take(MAX_SOCKET_NAME_CHARS).collect();
    }
    offset.sanitize();
    // A fit dimension off the wire is a divisor (`fit_scale` divides the
    // measured head by it), so the non-zero floor is what keeps a hostile
    // 1 mm band from scaling a prop 500× onto everyone's screen. Zero
    // stays zero: it is the "no fit" sentinel, not a measurement.
    if *fit_band_mm != 0 {
        *fit_band_mm = (*fit_band_mm).clamp(MIN_FIT_BAND_MM, MAX_FIT_BAND_MM);
    }
}

impl AttachmentRecord {
    /// A new attachment of `item` at `socket`, with an identity offset.
    pub fn new(item: Generator, socket: symbios_avatar::Socket) -> Self {
        Self {
            lex_type: AVATAR_ATTACHMENT_COLLECTION.into(),
            item,
            socket: socket.name().into(),
            offset: TransformData::default(),
            fit_band_mm: 0,
            source: None,
        }
    }

    /// [`Self::new`] carrying a catalogue entry's fit declaration (#1089).
    /// `None` is exactly [`Self::new`]: worn at authored size.
    pub fn with_fit(
        item: Generator,
        socket: symbios_avatar::Socket,
        fit: Option<crate::catalogue::WearFit>,
    ) -> Self {
        let mut record = Self::new(item, socket);
        record.fit_band_mm = fit.map_or(0, |fit| fit.band_mm());
        record
    }

    /// An attachment worn **from the inventory** (#1096): the item's own
    /// wear metadata — socket, fit, the offset it was last saved with —
    /// and its name as provenance. The socket string is taken verbatim so
    /// a stash entry from a newer client degrades to "kept, not worn",
    /// exactly as a record off the wire does.
    pub fn from_inventory(
        name: &str,
        item: Generator,
        meta: &crate::pds::inventory::WearMeta,
    ) -> Self {
        Self {
            lex_type: AVATAR_ATTACHMENT_COLLECTION.into(),
            item,
            socket: meta.socket.clone(),
            offset: meta.offset.clone(),
            fit_band_mm: meta.fit_band_mm,
            source: Some(name.to_string()),
        }
    }

    /// The wear metadata a save-back to the inventory writes (#1096): the
    /// record's socket, fit and offset — its whole placement — so wearing
    /// the saved item again reproduces this exact look.
    pub fn wear_meta(&self) -> crate::pds::inventory::WearMeta {
        crate::pds::inventory::WearMeta {
            socket: self.socket.clone(),
            fit_band_mm: self.fit_band_mm,
            offset: self.offset.clone(),
        }
    }

    /// The socket this attachment names, when this build knows it.
    pub fn socket(&self) -> Option<symbios_avatar::Socket> {
        symbios_avatar::Socket::from_name(&self.socket)
    }

    /// Clamp every numeric field, exactly as the room and avatar records do.
    ///
    /// The offset is a **full transform** (#1095): translation, a free
    /// rotation and per-axis scale, clamped exactly like a region
    /// placement's. It used to be forced uniform in scale on the argument
    /// that a non-uniform scale under an animated joint shears nested
    /// sub-assemblies — which is true, and equally true of a region asset's
    /// placement, where the editor has always offered the triad and left
    /// the judgement to the author. A worn item is edited with the same
    /// tools as a placed one now, so it gets the same freedom.
    pub fn sanitize(&mut self) {
        sanitize_avatar_visuals(&mut self.item);
        sanitize_wear_fields(&mut self.socket, &mut self.fit_band_mm, &mut self.offset);
        if let Some(source) = self.source.as_mut()
            && source.chars().count() > crate::config::state::MAX_INVENTORY_NAME_CHARS
        {
            *source = source
                .chars()
                .take(crate::config::state::MAX_INVENTORY_NAME_CHARS)
                .collect();
        }
    }
}

// ---------------------------------------------------------------------------
// Shared getRecord / putRecord plumbing
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct RecordEnvelope<T> {
    value: T,
}

/// `com.atproto.repo.getRecord` against an already-resolved PDS. `Ok(None)`
/// is the clean "no record" answer, matching every other fetch here.
async fn get_record_value<T: DeserializeOwned>(
    client: &reqwest::Client,
    pds: &str,
    did: &str,
    collection: &str,
    rkey: &str,
) -> Result<Option<T>, FetchError> {
    let url = format!("{pds}/xrpc/com.atproto.repo.getRecord");
    let resp = client
        .get(&url)
        .query(&[("repo", did), ("collection", collection), ("rkey", rkey)])
        .send()
        .await
        .map_err(|e| FetchError::Network(e.to_string()))?;
    let status = resp.status();
    if status.as_u16() == 404 {
        return Ok(None);
    }
    if !status.is_success() {
        // Capped (#1124) — this reads wardrobe, profile and attachment
        // records of OTHER identities.
        let body = super::super::xrpc::read_capped_text(resp).await;
        if let Ok(xrpc) = serde_json::from_str::<XrpcError>(&body)
            && let Some(err) = xrpc.error.as_deref()
            && (err == "RecordNotFound"
                || (err == "InvalidRequest" && body.contains("RecordNotFound")))
        {
            return Ok(None);
        }
        return Err(FetchError::PdsError(status.as_u16()));
    }
    let wrapper: RecordEnvelope<T> = decode_record_json(resp).await?;
    Ok(Some(wrapper.value))
}

#[derive(Serialize)]
struct DeleteRequest<'a> {
    repo: &'a str,
    collection: &'a str,
    rkey: &'a str,
}

/// `com.atproto.repo.deleteRecord`; a 404 counts as deleted.
async fn delete_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    collection: &str,
    rkey: &str,
) -> Result<(), String> {
    let pds = resolve_pds(client, &session.did)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;
    let url = format!("{pds}/xrpc/com.atproto.repo.deleteRecord");
    let body = DeleteRequest {
        repo: &session.did,
        collection,
        rkey,
    };
    let body_json = serde_json::to_value(&body).map_err(|e| e.to_string())?;
    let (status, body) =
        crate::oauth::oauth_post_with_refresh(&session.session, refresh, &url, &body_json).await?;
    if status.is_success() || status.as_u16() == 404 {
        Ok(())
    } else {
        Err(format!(
            "deleteRecord ({collection}/{rkey}) failed: {status} — {body}"
        ))
    }
}

// ---------------------------------------------------------------------------
// Wardrobe (engine avatar records)
// ---------------------------------------------------------------------------

/// The engine record as the JSON a PDS stores: its own camelCase form with
/// the collection's `$type` injected through the record's `extra` passthrough
/// map. The engine type deliberately has no `$type` field of its own — the
/// NSID belongs to the application storing it — and round-tripping keeps the
/// key harmlessly in `extra`.
pub fn engine_record_wire(record: &EngineAvatarRecord) -> Result<serde_json::Value, String> {
    let mut value = serde_json::to_value(record).map_err(|e| format!("serialize body: {e}"))?;
    if let Some(map) = value.as_object_mut() {
        map.insert(
            String::from("$type"),
            serde_json::Value::String(WARDROBE_COLLECTION.into()),
        );
    }
    Ok(value)
}

/// Fetch one wardrobe record by rkey. `Ok(None)` is a clean miss.
pub async fn fetch_wardrobe_record(
    client: &reqwest::Client,
    did: &str,
    rkey: &str,
) -> Result<Option<EngineAvatarRecord>, FetchError> {
    let pds = resolve_pds(client, did)
        .await
        .ok_or(FetchError::DidResolutionFailed)?;
    fetch_wardrobe_record_at(client, &pds, did, rkey).await
}

/// [`fetch_wardrobe_record`] against an already-resolved PDS — the resolution
/// fan-out uses this so one avatar fetch resolves the DID document once.
pub(crate) async fn fetch_wardrobe_record_at(
    client: &reqwest::Client,
    pds: &str,
    did: &str,
    rkey: &str,
) -> Result<Option<EngineAvatarRecord>, FetchError> {
    let record: Option<EngineAvatarRecord> =
        get_record_value(client, pds, did, WARDROBE_COLLECTION, rkey).await?;
    Ok(record.map(|mut r| {
        r.sanitize();
        r
    }))
}

/// Every wardrobe record the identity has, as `(rkey, record)` pairs in
/// rkey (= creation) order. Undecodable records are skipped one at a time,
/// like the inventory walk. An empty wardrobe is `Ok(vec![])`.
pub async fn list_wardrobe(
    client: &reqwest::Client,
    did: &str,
) -> Result<Vec<(String, EngineAvatarRecord)>, FetchError> {
    #[derive(Deserialize)]
    struct Page {
        #[serde(default)]
        records: Vec<Listed>,
        cursor: Option<String>,
    }
    #[derive(Deserialize)]
    struct Listed {
        uri: String,
        value: serde_json::Value,
    }

    let pds = resolve_pds(client, did)
        .await
        .ok_or(FetchError::DidResolutionFailed)?;
    let mut out = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_WARDROBE_LIST_PAGES {
        let url = format!("{pds}/xrpc/com.atproto.repo.listRecords");
        let mut query: Vec<(&str, String)> = vec![
            ("repo", did.to_string()),
            ("collection", WARDROBE_COLLECTION.to_string()),
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
        let page: Page = decode_record_json(resp).await?;
        let empty_page = page.records.is_empty();
        for listed in page.records {
            // at://did/collection/rkey — the rkey is the last path segment.
            let Some(rkey) = listed.uri.rsplit('/').next() else {
                continue;
            };
            if let Ok(mut record) = serde_json::from_value::<EngineAvatarRecord>(listed.value) {
                record.sanitize();
                out.push((rkey.to_string(), record));
            }
        }
        cursor = page.cursor;
        if cursor.is_none() || empty_page {
            break;
        }
    }
    Ok(out)
}

/// Delete one wardrobe record.
pub async fn delete_wardrobe_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    rkey: &str,
) -> Result<(), String> {
    delete_record(client, session, refresh, WARDROBE_COLLECTION, rkey).await
}

// ---------------------------------------------------------------------------
// Profile (default-body pointer)
// ---------------------------------------------------------------------------

/// The profile record as stored: the engine's shape plus the `$type` the
/// engine type does not carry.
#[derive(Serialize, Deserialize)]
struct WireProfile {
    #[serde(rename = "$type")]
    lex_type: String,
    #[serde(flatten)]
    profile: EngineProfileRecord,
}

/// Fetch the identity's avatar profile. `Ok(None)` is a clean miss.
pub async fn fetch_avatar_profile(
    client: &reqwest::Client,
    did: &str,
) -> Result<Option<EngineProfileRecord>, FetchError> {
    let pds = resolve_pds(client, did)
        .await
        .ok_or(FetchError::DidResolutionFailed)?;
    fetch_avatar_profile_at(client, &pds, did).await
}

/// [`fetch_avatar_profile`] against an already-resolved PDS.
pub(crate) async fn fetch_avatar_profile_at(
    client: &reqwest::Client,
    pds: &str,
    did: &str,
) -> Result<Option<EngineProfileRecord>, FetchError> {
    let wire: Option<WireProfile> =
        get_record_value(client, pds, did, AVATAR_PROFILE_COLLECTION, "self").await?;
    Ok(wire.map(|w| {
        let mut profile = w.profile;
        // The wire is another identity's PDS and nothing about the contents
        // can be assumed. `defaultAvatar` in particular is dereferenced into
        // an AT-URI by everything downstream of here, so an invalid pointer
        // is dropped at the seam (engine #51, overlands #1076); the rule and
        // its tests live upstream in `ProfileRecord::sanitize`, and this is
        // the one place overlands takes a profile off the network.
        //
        // Named when it happens (#1078), because the symptom is remote from
        // the cause: the wearer shows everyone their fallback body, this
        // client renders it without complaint, and nothing anywhere would
        // have said why.
        let had_pointer = profile.default_avatar.is_some();
        profile.sanitize();
        if had_pointer && profile.default_avatar.is_none() {
            warn!("{did}'s avatar profile named an invalid default-avatar record key — pointer dropped, wearing the fallback");
        }
        profile
    }))
}

// ---------------------------------------------------------------------------
// Attachments
// ---------------------------------------------------------------------------

/// Fetch one attachment record by rkey. `Ok(None)` is a clean miss.
pub async fn fetch_attachment_record(
    client: &reqwest::Client,
    did: &str,
    rkey: &str,
) -> Result<Option<AttachmentRecord>, FetchError> {
    let pds = resolve_pds(client, did)
        .await
        .ok_or(FetchError::DidResolutionFailed)?;
    fetch_attachment_record_at(client, &pds, did, rkey).await
}

/// [`fetch_attachment_record`] against an already-resolved PDS.
pub(crate) async fn fetch_attachment_record_at(
    client: &reqwest::Client,
    pds: &str,
    did: &str,
    rkey: &str,
) -> Result<Option<AttachmentRecord>, FetchError> {
    let record: Option<AttachmentRecord> =
        get_record_value(client, pds, did, AVATAR_ATTACHMENT_COLLECTION, rkey).await?;
    Ok(record.map(|mut r| {
        r.sanitize();
        r
    }))
}

// ---------------------------------------------------------------------------
// The orchestrated save (#1059)
// ---------------------------------------------------------------------------

/// Everything one avatar save writes, precomputed so the async half is a
/// straight walk. Built by [`plan_avatar_publish`] — pure, so the ordering
/// and the fill-ins are testable without a network.
#[derive(Clone, Debug, PartialEq)]
pub struct AvatarPublishPlan {
    /// The worn engine body, at its wardrobe rkey. `None` for a generator
    /// body — the classic single-record save.
    pub wardrobe: Option<(String, EngineAvatarRecord)>,
    /// Every worn attachment, at its rkey.
    pub attachments: Vec<(String, AttachmentRecord)>,
    /// Attachment records detached this session, deleted **after** the
    /// avatar record stops referencing them.
    pub attachment_deletes: Vec<String>,
    /// The cross-app default-body pointer, kept in step with the worn body
    /// so every symbios app agrees what this identity looks like.
    pub profile: Option<EngineProfileRecord>,
    /// The overlands avatar record itself.
    pub record: super::AvatarRecord,
}

/// Lay out one save. For a rigged body the engine record is made
/// publishable on the way — an empty name and a missing `createdAt` are
/// filled here (`now_iso` is the application's clock; the engine crate is
/// deliberately clock-free) — and the profile follows the worn body.
///
/// **The delete set is derived, never queued** (#1110). `stored_attachments`
/// is the attachment rkey list of the record the PDS currently holds; what
/// this save deletes is exactly that set minus what the record being
/// published still references. A session-scoped "detached this session"
/// queue cannot express this: it survived Undo, Load-from-PDS, Reset and
/// logout, so a take-off followed by an undo deleted a record the very same
/// bundle had just re-published — and, in the other direction, every path
/// that drops a reference *without* going through the queue ("Publish & log
/// out", Reset, re-roll, wearing a fresh body) orphaned its record in the
/// repo forever. Deriving from the two record states covers both, and can
/// never ask the PDS to delete an rkey it was never given (a prop worn and
/// taken off inside one session was never published).
pub fn plan_avatar_publish(
    record: &super::AvatarRecord,
    stored_attachments: &[String],
    now_iso: &str,
) -> AvatarPublishPlan {
    let mut plan = AvatarPublishPlan {
        wardrobe: None,
        attachments: Vec::new(),
        attachment_deletes: attachment_deletes(record, stored_attachments),
        profile: None,
        record: record.clone(),
    };
    if let Some(rig) = record.body.rigged_ref()
        && let Some(resolved) = rig.resolved.as_ref()
    {
        let mut body = resolved.body.clone();
        if body.name.trim().is_empty() {
            body.name = String::from("Wanderer");
        }
        if body
            .created_at
            .as_deref()
            .is_none_or(|at| at.trim().is_empty())
        {
            body.created_at = Some(now_iso.to_string());
        }
        plan.wardrobe = Some((rig.avatar.clone(), body));
        plan.attachments = resolved
            .attachments
            .iter()
            .map(|attachment| (attachment.rkey.clone(), attachment.record.clone()))
            .collect();
        plan.profile = Some(EngineProfileRecord::pointing_at(
            rig.avatar.clone(),
            now_iso,
        ));
    }
    plan
}

/// The attachment record keys an avatar record references, in record order.
///
/// The publish flow's view of "what the PDS holds for this identity": pass
/// the *stored* record's list to [`plan_avatar_publish`] and it retires
/// whatever the record being saved has since stopped referencing (#1110).
/// Empty for a generator body, which wears nothing.
pub fn attachment_rkeys(record: &super::AvatarRecord) -> Vec<String> {
    record
        .body
        .rigged_ref()
        .map(|rig| rig.attachments.clone())
        .unwrap_or_default()
}

/// Which attachment records this save retires: the ones the PDS holds that
/// the record being published no longer references.
///
/// Deliberately reads the record's own **reference list** (`rig.attachments`)
/// rather than the resolved outfit: the references are what a reader
/// follows, and a resolution that failed (a fetch error leaves `resolved`
/// short of a prop the record still names) must never be read as "the owner
/// took it off". Order follows `stored_attachments` so a plan is stable, and
/// duplicates collapse — a repeated rkey would delete twice and fail the
/// second time.
fn attachment_deletes(record: &super::AvatarRecord, stored_attachments: &[String]) -> Vec<String> {
    let kept: std::collections::HashSet<&str> = record
        .body
        .rigged_ref()
        .map(|rig| rig.attachments.iter().map(String::as_str).collect())
        .unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    stored_attachments
        .iter()
        .filter(|rkey| !kept.contains(rkey.as_str()))
        .filter(|rkey| seen.insert(rkey.as_str()))
        .cloned()
        .collect()
}

/// What the owner's repo already holds, read once before the plan is turned
/// into writes (#1117).
///
/// `applyWrites` has no upsert: `#create` and `#update` are different verbs
/// and the wrong one fails the whole atomic batch, so the plan has to know
/// which records are already there. The room learned this first — one
/// `room_self_exists` probe feeding `manifest_exists` into
/// `room::plan_room_writes` (private to that module) — and the bundle is the
/// same shape with four pointers instead of one.
///
/// The attachment listing does double duty: it decides create-vs-update for
/// every worn prop AND it is the only way to find attachment records nothing
/// references any more. Deliberately keyed on the rkeys in the listed
/// AT-URIs rather than on decoded values, so a record this build cannot
/// decode is still counted as present — the alternative re-`#create`s over
/// it and fails the batch.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AvatarRepoState {
    /// `network.symbios.overlands.avatar / self` is present.
    pub avatar_record: bool,
    /// The cross-app default-body pointer is present.
    pub profile: bool,
    /// The wardrobe rkey this save writes is present. Meaningless (and
    /// never read) when the plan carries no wardrobe body.
    pub worn_body: bool,
    /// Every attachment rkey the owner's attachment collection holds.
    pub attachments: std::collections::HashSet<String>,
    /// Whether [`Self::attachments`] is the WHOLE collection — the listing
    /// walk finished rather than stopping at its page cap (#1185).
    ///
    /// It decides whether the listing may be used as evidence of ABSENCE. A
    /// complete listing says a derived delete for an rkey it does not contain
    /// would be a delete of a record that is not there, which fails the whole
    /// atomic batch with an opaque 500; a truncated one says nothing of the
    /// sort. Defaults to `false` so anything that builds this state without
    /// walking the collection is treated as "unknown", never as "empty".
    pub attachments_complete: bool,
}

/// Read [`AvatarRepoState`] — three existence probes and one `listRecords`
/// walk, all before any write.
async fn read_avatar_repo_state(
    client: &reqwest::Client,
    pds: &str,
    did: &str,
    plan: &AvatarPublishPlan,
) -> Result<AvatarRepoState, String> {
    let worn_body = match &plan.wardrobe {
        Some((rkey, _)) => record_exists(client, pds, did, WARDROBE_COLLECTION, rkey).await?,
        None => false,
    };
    let (attachments, attachments_complete) = list_attachment_rkeys(client, pds, did)
        .await
        .map_err(|e| format!("attachment listing failed: {e:?}"))?;
    Ok(AvatarRepoState {
        avatar_record: record_exists(client, pds, did, super::super::AVATAR_COLLECTION, "self")
            .await?,
        profile: record_exists(client, pds, did, AVATAR_PROFILE_COLLECTION, "self").await?,
        worn_body,
        attachments,
        attachments_complete,
    })
}

/// Every attachment rkey in the owner's attachment collection, and whether
/// that is the WHOLE collection.
///
/// Bounded by [`MAX_WARDROBE_LIST_PAGES`] pages of 100 for the same reason
/// the wardrobe and room-child walks are: a hostile PDS handing out endless
/// cursors must not be able to keep the client paging. A truncated listing
/// under-reports, which can only make the orphan sweep miss a record — never
/// delete one it should not.
///
/// The completeness flag is what makes the listing usable as evidence of
/// ABSENCE (#1185): only a walk that ran out of records, rather than out of
/// pages, can say an rkey is not in the repo. Returned rather than inferred
/// from the set's size, because "fewer than the cap" and "the walk finished"
/// are different facts — a final page can be full.
async fn list_attachment_rkeys(
    client: &reqwest::Client,
    pds: &str,
    did: &str,
) -> Result<(std::collections::HashSet<String>, bool), FetchError> {
    #[derive(Deserialize)]
    struct Page {
        #[serde(default)]
        records: Vec<Listed>,
        cursor: Option<String>,
    }
    #[derive(Deserialize)]
    struct Listed {
        uri: String,
    }

    let mut out = std::collections::HashSet::new();
    let mut cursor: Option<String> = None;
    let mut complete = false;
    for _ in 0..MAX_WARDROBE_LIST_PAGES {
        let url = format!("{pds}/xrpc/com.atproto.repo.listRecords");
        let mut query: Vec<(&str, String)> = vec![
            ("repo", did.to_string()),
            ("collection", AVATAR_ATTACHMENT_COLLECTION.to_string()),
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
        let page: Page = decode_record_json(resp).await?;
        let empty_page = page.records.is_empty();
        for listed in page.records {
            // at://did/collection/rkey — the rkey is the last path segment.
            if let Some(rkey) = listed.uri.rsplit('/').next() {
                out.insert(rkey.to_string());
            }
        }
        cursor = page.cursor;
        if cursor.is_none() || empty_page {
            // Ran out of records rather than out of pages: the set is the
            // whole collection.
            complete = true;
            break;
        }
    }
    Ok((out, complete))
}

/// Turn a plan plus what the repo holds into `applyWrites` batches (#1117).
///
/// Pure, so the ordering, the create-vs-update choice, the orphan sweep and
/// the batch sizing are all testable without a network — the same reason
/// [`plan_avatar_publish`] is pure.
///
/// **Order: children before pointers.** Wardrobe body, then attachments,
/// then the profile and the avatar record that reference them, then the
/// deletes. Inside one atomic batch the order is immaterial; it starts
/// mattering the instant [`chunk_writes`] splits the plan, because then each
/// batch commits separately and a reader can land between two of them. A
/// pointer written before its target is a dangling reference for that
/// window; a delete issued before the record stops referencing it is the
/// same defect from the other end.
///
/// Every per-record size preflight runs here, before any write leaves — an
/// oversized attachment must never be able to leave the outfit half-saved.
pub(crate) fn plan_avatar_writes(
    plan: &AvatarPublishPlan,
    repo: &AvatarRepoState,
) -> Result<Vec<Vec<RepoWrite>>, String> {
    // Refused before anything else: a body carrying an Absent or Unknown
    // marker cannot be written back, and saving anyway would replace the
    // owner's newer content with a husk.
    plan.record.body.wire_ready()?;

    let mut ordered: Vec<RepoWrite> = Vec::new();

    if let Some((rkey, body)) = &plan.wardrobe {
        let value = engine_record_wire(body)?;
        crate::pds::record_size::preflight(&value, "wardrobe body")?;
        ordered.push(upsert(
            repo.worn_body,
            WARDROBE_COLLECTION,
            rkey.clone(),
            value,
        ));
    }
    for (rkey, attachment) in &plan.attachments {
        let value =
            serde_json::to_value(attachment).map_err(|e| format!("serialize attachment: {e}"))?;
        crate::pds::record_size::preflight(&value, &format!("attachment \"{rkey}\""))?;
        ordered.push(upsert(
            repo.attachments.contains(rkey),
            AVATAR_ATTACHMENT_COLLECTION,
            rkey.clone(),
            value,
        ));
    }
    if let Some(profile) = &plan.profile {
        let wire = WireProfile {
            lex_type: AVATAR_PROFILE_COLLECTION.into(),
            profile: profile.clone(),
        };
        let value = serde_json::to_value(&wire).map_err(|e| format!("serialize profile: {e}"))?;
        crate::pds::record_size::preflight(&value, "avatar profile")?;
        ordered.push(upsert(
            repo.profile,
            AVATAR_PROFILE_COLLECTION,
            String::from("self"),
            value,
        ));
    }
    let record_value =
        serde_json::to_value(&plan.record).map_err(|e| format!("serialize avatar: {e}"))?;
    crate::pds::record_size::preflight(&record_value, "avatar")?;
    ordered.push(upsert(
        repo.avatar_record,
        super::super::AVATAR_COLLECTION,
        String::from("self"),
        record_value,
    ));

    for rkey in attachment_retirements(plan, repo) {
        ordered.push(RepoWrite::Delete {
            collection: AVATAR_ATTACHMENT_COLLECTION.into(),
            rkey,
        });
    }

    chunk_writes(ordered)
}

/// `#update` when the record is already there, `#create` when it is not.
fn upsert(exists: bool, collection: &str, rkey: String, value: serde_json::Value) -> RepoWrite {
    if exists {
        RepoWrite::Update {
            collection: collection.into(),
            rkey,
            value,
        }
    } else {
        RepoWrite::Create {
            collection: collection.into(),
            rkey,
            value,
        }
    }
}

/// Which attachment records this save retires: #1110's derived delete set
/// **plus** every attachment record in the repo that nothing references.
///
/// The two halves answer different questions and neither subsumes the other
/// in every case. The derived set (`stored` refs − `live` refs) knows about
/// records this client published and can name them even when the listing
/// truncates at its page cap. The sweep knows about records the derived set
/// cannot see at all: orphans left by an interrupted save, by another
/// device, or — until this issue — by the old non-atomic bundle, which could
/// land the attachments and then fail before the record that referenced them
/// ever went out. Nothing else in the app ever walked this collection, so
/// those accumulated in the owner's repo forever.
///
/// Referenced means the record's own `attachments` **reference list**, not
/// the resolved outfit — the same distinction #1110 draws in
/// [`attachment_deletes`]: a resolution that failed leaves `resolved` short
/// of a prop the record still names, and that must never read as "the owner
/// took it off".
///
/// The sweep is attachments only. The wardrobe is a keep-all collection —
/// an identity accumulates bodies and the Body tab lists them for
/// re-wearing — so an unreferenced wardrobe record is a saved body, not an
/// orphan, and sweeping that collection would delete the owner's saved
/// selves. This asymmetry is exactly why #1110 derived attachment deletes
/// and left everything else alone.
fn attachment_retirements(plan: &AvatarPublishPlan, repo: &AvatarRepoState) -> Vec<String> {
    let referenced: std::collections::HashSet<&str> = plan
        .record
        .body
        .rigged_ref()
        .map(|rig| rig.attachments.iter().map(String::as_str).collect())
        .unwrap_or_default();
    let mut retire: Vec<String> = plan
        .attachment_deletes
        .iter()
        .chain(repo.attachments.iter())
        .filter(|rkey| !referenced.contains(rkey.as_str()))
        // **Only what the repo actually holds — when we know what it holds**
        // (#1185). An `applyWrites#delete` on a record that is not there throws
        // out of the PDS's MST and comes back as a bare 500 that fails the
        // WHOLE atomic batch: one stale rkey in the derived set loses the
        // entire save, body and props alike, with an error naming none of it.
        // The room path has always guarded this (`delete_room_record` deletes
        // "only what a listRecords walk + existence check say is actually
        // there"); the avatar path inherited the hazard without the guard.
        //
        // The risk lives in the DERIVED half, which comes from the local mirror
        // of the last-published record and can name an rkey the repo no longer
        // has — another device retired it, an earlier bundle landed the record
        // but not the attachment, or a partial publish left the two
        // disagreeing. The orphan half is read straight off the listing and was
        // never in doubt.
        //
        // Gated on `attachments_complete` rather than applied unconditionally,
        // because a truncated listing genuinely cannot see records the derived
        // set can — that is the whole point of keeping both halves in the union
        // (see `a_derived_delete_survives_a_listing_that_did_not_see_it`).
        // Where the listing is authoritative, trust it and drop the delete;
        // where it is not, keep the union and accept the older risk. Dropping a
        // delete only ever leaves an orphan for a later sweep; sending one the
        // repo cannot satisfy loses the save.
        .filter(|rkey| !repo.attachments_complete || repo.attachments.contains(rkey.as_str()))
        .cloned()
        .collect();
    // Sorted and deduped: a repeated rkey would be deleted twice and the
    // second delete fails the whole batch, and a stable order makes the
    // plan comparable in a test.
    retire.sort();
    retire.dedup();
    retire
}

/// Execute an [`AvatarPublishPlan`] as `applyWrites` batches (#1117).
///
/// Replaces a walk of four separate `putRecord`s, one of which reacted to
/// any `5xx` — a proxy's 502 included — by deleting `avatar/self` and
/// re-putting it. During an outage the re-put usually failed too, and the
/// owner's avatar record was simply gone. That path existed because some
/// PDS implementations choked on their own update-diff logic against a
/// stale stored CID; `applyWrites` does not do that diffing at all, and the
/// existence probes in [`AvatarRepoState`] name `#create` vs `#update`
/// explicitly, which is what the delete-then-put was crudely achieving. The
/// room retired the same path for the same reason in #697. (The per-record
/// size preflight is unrelated cover — it catches an oversized record, not a
/// stale CID.)
///
/// Atomicity comes free with it: a bundle that fits one batch commits whole
/// or not at all, so a transient failure can no longer land the attachments
/// and lose the record that references them.
pub async fn publish_avatar_bundle(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    plan: &AvatarPublishPlan,
) -> Result<(), String> {
    let pds = resolve_pds(client, &session.did)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;
    let repo = read_avatar_repo_state(client, &pds, &session.did, plan).await?;
    for batch in plan_avatar_writes(plan, &repo)? {
        crate::pds::xrpc::apply_writes(&pds, session, refresh, batch).await?;
    }
    Ok(())
}

/// The deterministic engine body for an identity: `fnv1a_64(did)` feeding
/// the engine's one-call record-from-seed roll (`AvatarRecord::rolled`,
/// symbios-avatar #233). Every client derives the same person from the same
/// DID, which is the whole contract — the rigged counterpart of
/// [`super::AvatarRecord::default_for_did`], used when #1057 gives seeded
/// defaults a rigged option. The archetype is the host's call and overlands
/// says humanoid; the name is a placeholder the identity's handle replaces
/// in UI, not on the record.
pub fn engine_default_for_did(did: &str) -> EngineAvatarRecord {
    engine_default_for_seed(crate::seeded_defaults::fnv1a_64(did))
}

/// [`engine_default_for_did`] from a pre-computed seed — the re-roll path,
/// and the one the seeded humanoid chassis builds through (#1060).
///
/// **Stature is held to the engine's conservative range, and the wider
/// exploration envelope is deliberately not used here.** The engine draws
/// each shape axis with a rare wildcard over a range stretched about the
/// default — right for an editor, where a 3-metre body is somebody
/// exploring and one drag undoes it, and wrong for the avatar an identity
/// is *given* before they have ever opened one: roughly one seed in thirty
/// would hand a new arrival a 20 cm or 3 m body they never asked for, in a
/// world whose doorways, seats and camera are cut for people. Every other
/// axis is a bounded offset and rolls untouched; stature is the one that
/// sets world scale and the physics capsule.
pub fn engine_default_for_seed(seed: u64) -> EngineAvatarRecord {
    let mut record = EngineAvatarRecord::rolled(
        "Wanderer",
        symbios_avatar::Archetype::default(),
        seed as i64,
    );
    let (low, high) = symbios_avatar::plan::humanoid_height_range();
    if let symbios_avatar::Archetype::Humanoid(params) = &mut record.archetype {
        params.height = params.height.clamp(low, high);
    }
    // Re-quantise: the clamp above can land off the wire's grid, and this
    // record is compared by value against what a peer decoded.
    record.sanitize();
    record
}

/// Dress `record` in a fresh engine body (#1059): a new wardrobe rkey
/// minted from the DID's entropy, resolved locally so the editor and the
/// spawn pipeline see the body immediately — nothing exists on the PDS
/// until the next save publishes the bundle.
///
/// Humanoid locomotion tuning survives the switch; any other preset is
/// replaced by the humanoid default, because a rigged body walks.
pub fn wear_new_engine_body(record: &mut super::AvatarRecord, did: &str) {
    let rkey = crate::pds::tid::tid_now(crate::seeded_defaults::fnv1a_64(did));
    let mut fresh = super::AvatarRecord::wearing(rkey);
    if let Some(rig) = fresh.body.rigged_mut() {
        rig.resolved = Some(ResolvedRig {
            body: engine_default_for_did(did),
            attachments: Vec::new(),
        });
    }
    if matches!(record.locomotion, super::LocomotionConfig::Humanoid(_)) {
        fresh.locomotion = record.locomotion.clone();
    }
    *record = fresh;
}

// ---------------------------------------------------------------------------
// Resolution fan-out
// ---------------------------------------------------------------------------

/// What one [`resolve_rigged_body`] pass could and could not fetch (#1144).
///
/// Deliberately NOT a field on [`ResolvedRig`]: that type is compared by value
/// to decide whether the avatar record is dirty
/// ([`avatar_is_dirty`](crate::pds::avatar::avatar_is_dirty)), so a transient
/// list of what failed would make two identical bodies read as unsaved work.
/// The outcome travels beside the resolution instead.
///
/// The gap this closes: the whole wardrobe + attachment chain (#1086-#1108)
/// reported NotFound and transport failures through `warn!` only, and the
/// caller then `continue`d on a `None`. With gifting (#1108) making attachment
/// records cross-owner, a guest fetching a peer depends on N+2 records — and
/// "why is Bob a bare chassis for me but not for Alice" was unanswerable from
/// a captured log.
#[derive(Default, Clone, Debug)]
pub(crate) struct ResolveReport {
    /// Why the wardrobe record itself did not install. `None` = it did.
    pub body_error: Option<String>,
    /// Attachment rkeys that were not installed, each with its reason.
    pub skipped: Vec<(String, String)>,
}

impl ResolveReport {
    /// The report for a resolution that never ran to completion — a timeout
    /// or a DID that would not resolve. Distinguished from a missing wardrobe
    /// record, which is a real answer.
    pub(crate) fn aborted(reason: &str) -> Self {
        ResolveReport {
            body_error: Some(reason.to_owned()),
            skipped: Vec::new(),
        }
    }
}

/// Resolve a rigged body's references against the owner's (already resolved)
/// PDS, writing the result onto [`RiggedBody::resolved`].
///
/// Degradation is graded, never all-or-nothing: a wardrobe reference that
/// does not resolve leaves `resolved = None` (a bare chassis — there is no
/// body worth building around missing geometry), while an attachment that
/// does not resolve is skipped with a warning (a barer avatar beats a
/// missing one). Every kept record arrives sanitised.
pub(crate) async fn resolve_rigged_body(
    client: &reqwest::Client,
    pds: &str,
    did: &str,
    rig: &mut RiggedBody,
) -> ResolveReport {
    let mut report = ResolveReport::default();
    let body = match fetch_wardrobe_record_at(client, pds, did, &rig.avatar).await {
        Ok(Some(body)) => body,
        Ok(None) => {
            warn!(
                "wardrobe record {}/{} not found for {did} — spawning a bare chassis",
                WARDROBE_COLLECTION, rig.avatar
            );
            rig.resolved = None;
            report.body_error = Some(String::from("wardrobe record not found"));
            return report;
        }
        Err(err) => {
            warn!(
                "wardrobe fetch {}/{} failed for {did}: {err:?} — spawning a bare chassis",
                WARDROBE_COLLECTION, rig.avatar
            );
            rig.resolved = None;
            report.body_error = Some(format!("wardrobe fetch failed: {err:?}"));
            return report;
        }
    };
    let mut attachments = Vec::new();
    for rkey in &rig.attachments {
        match fetch_attachment_record_at(client, pds, did, rkey).await {
            Ok(Some(record)) => attachments.push(ResolvedAttachment {
                rkey: rkey.clone(),
                record,
            }),
            Ok(None) => {
                warn!("attachment record {rkey} not found for {did} — prop skipped");
                report
                    .skipped
                    .push((rkey.clone(), String::from("record not found")));
            }
            Err(err) => {
                warn!("attachment fetch {rkey} failed for {did}: {err:?} — prop skipped");
                report
                    .skipped
                    .push((rkey.clone(), format!("fetch failed: {err:?}")));
            }
        }
    }
    rig.resolved = Some(ResolvedRig { body, attachments });
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::types::Fp3;

    #[test]
    fn a_rigged_publish_plan_fills_the_lexicons_requirements_and_orders_the_pointer() {
        let mut record = super::super::AvatarRecord::wearing("3jzfcijpj2z2a");
        if let Some(rig) = record.body.rigged_mut() {
            rig.attachments.push(String::from("3jzfcijpj2z2b"));
            let mut body = engine_default_for_did("did:plc:plan-test");
            body.name = String::from("   ");
            body.created_at = None;
            rig.resolved = Some(ResolvedRig {
                body,
                attachments: vec![ResolvedAttachment {
                    rkey: String::from("3jzfcijpj2z2b"),
                    record: AttachmentRecord::new(
                        Generator::default(),
                        symbios_avatar::Socket::Crown,
                    ),
                }],
            });
        }
        let plan = plan_avatar_publish(
            &record,
            &[String::from("3jzfcijpj2z2c")],
            "2026-08-14T00:00:00Z",
        );
        let (rkey, body) = plan
            .wardrobe
            .as_ref()
            .expect("a rigged plan carries its body");
        assert_eq!(rkey, "3jzfcijpj2z2a");
        // The lexicon's two required fields, filled on the way out rather
        // than rejected at the PDS.
        assert_eq!(body.name, "Wanderer");
        assert_eq!(body.created_at.as_deref(), Some("2026-08-14T00:00:00Z"));
        assert_eq!(plan.attachments.len(), 1);
        assert_eq!(
            plan.profile
                .as_ref()
                .and_then(|p| p.default_avatar.as_deref()),
            Some("3jzfcijpj2z2a"),
            "the cross-app pointer follows the worn body"
        );
        assert_eq!(plan.attachment_deletes, vec![String::from("3jzfcijpj2z2c")]);
    }

    /// A rigged record wearing exactly `rkeys`, resolved so the plan has a
    /// body to publish.
    fn wearing(rkeys: &[&str]) -> super::super::AvatarRecord {
        let mut record = super::super::AvatarRecord::wearing("3jzfcijpj2z2a");
        if let Some(rig) = record.body.rigged_mut() {
            rig.attachments = rkeys.iter().map(|k| (*k).to_string()).collect();
            rig.resolved = Some(ResolvedRig {
                body: engine_default_for_did("did:plc:deletes"),
                attachments: rkeys
                    .iter()
                    .map(|k| ResolvedAttachment {
                        rkey: (*k).to_string(),
                        record: AttachmentRecord::new(
                            Generator::default(),
                            symbios_avatar::Socket::Crown,
                        ),
                    })
                    .collect(),
            });
        }
        record
    }

    #[test]
    fn a_worn_rkey_is_never_deleted_by_the_save_that_publishes_it() {
        // #1110, the sequence that lost data: take a prop off (the queue
        // holds its rkey), press Ctrl+Z or Load-from-PDS so the record wears
        // it again, then Save. The old queue-driven plan re-published the
        // record AND deleted it, leaving the avatar pointing at nothing.
        // Deriving the delete set from the two record states cannot express
        // that: what the record references is never retired.
        let stored = [String::from("rkey-worn"), String::from("rkey-gone")];
        let plan = plan_avatar_publish(&wearing(&["rkey-worn"]), &stored, "2026-08-28T00:00:00Z");

        assert_eq!(
            plan.attachment_deletes,
            vec![String::from("rkey-gone")],
            "only the reference the record dropped is retired"
        );
        assert!(
            plan.attachments.iter().any(|(rkey, _)| rkey == "rkey-worn"),
            "and the worn one is published, not deleted"
        );
    }

    #[test]
    fn dropping_a_reference_retires_its_record_however_it_was_dropped() {
        // The other direction (#1110): "Publish & log out", Reset,
        // re-roll and wearing a fresh body all drop references without
        // going through any take-off path. The old plan was handed an empty
        // queue on those routes and orphaned every record in the repo.
        let stored = [
            String::from("rkey-a"),
            String::from("rkey-b"),
            String::from("rkey-c"),
        ];
        let plan = plan_avatar_publish(&wearing(&[]), &stored, "2026-08-28T00:00:00Z");
        assert_eq!(plan.attachment_deletes, stored.to_vec());

        // A body swapped for a generator one keeps the same rule.
        let plan = plan_avatar_publish(
            &super::super::AvatarRecord::default_for_seed(7),
            &stored,
            "2026-08-28T00:00:00Z",
        );
        assert_eq!(plan.attachment_deletes, stored.to_vec());
    }

    #[test]
    fn a_prop_worn_and_taken_off_before_any_save_is_never_deleted() {
        // Its record was never published, so asking the PDS to delete it
        // would fail the whole bundle. Nothing the PDS was not given can
        // appear in the delete set, because the set is derived from what it
        // holds.
        let plan = plan_avatar_publish(&wearing(&[]), &[], "2026-08-28T00:00:00Z");
        assert!(plan.attachment_deletes.is_empty());
    }

    #[test]
    fn a_repeated_stored_rkey_is_deleted_once() {
        // A duplicate would delete twice and fail on the second call.
        let stored = [String::from("rkey-dup"), String::from("rkey-dup")];
        let plan = plan_avatar_publish(&wearing(&[]), &stored, "2026-08-28T00:00:00Z");
        assert_eq!(plan.attachment_deletes, vec![String::from("rkey-dup")]);
    }

    #[test]
    fn a_failed_resolution_does_not_read_as_taking_the_prop_off() {
        // `resolved` is short of a prop the record still names (a fetch
        // error at load). The delete set reads the RECORD's reference list,
        // so the record survives; reading the resolved outfit would have
        // deleted somebody's prop because their PDS blipped.
        let mut record = wearing(&["rkey-x", "rkey-y"]);
        if let Some(rig) = record.body.rigged_mut()
            && let Some(resolved) = rig.resolved.as_mut()
        {
            resolved.attachments.retain(|a| a.rkey != "rkey-y");
        }
        let plan = plan_avatar_publish(
            &record,
            &[String::from("rkey-x"), String::from("rkey-y")],
            "2026-08-28T00:00:00Z",
        );
        assert!(plan.attachment_deletes.is_empty());
    }

    #[test]
    fn a_generator_publish_plan_is_the_classic_single_record_save() {
        let record = super::super::AvatarRecord::default_for_seed(7);
        let plan = plan_avatar_publish(&record, &[], "2026-08-14T00:00:00Z");
        assert!(plan.wardrobe.is_none());
        assert!(plan.attachments.is_empty());
        assert!(plan.profile.is_none());
        assert_eq!(plan.record, record);
    }

    #[test]
    fn wearing_a_fresh_engine_body_resolves_locally_and_keeps_humanoid_tuning() {
        let mut record = super::super::AvatarRecord::default_for_seed(7);
        let walked = matches!(
            record.locomotion,
            super::super::LocomotionConfig::Humanoid(_)
        );
        wear_new_engine_body(&mut record, "did:plc:wear-test");
        let rig = record.body.rigged_ref().expect("rigged now");
        assert_eq!(rig.avatar.len(), 13, "a freshly minted TID rkey");
        let resolved = rig.resolved.as_ref().expect("resolved locally");
        assert_eq!(resolved.body, engine_default_for_did("did:plc:wear-test"));
        assert!(
            matches!(
                record.locomotion,
                super::super::LocomotionConfig::Humanoid(_)
            ),
            "a rigged body walks"
        );
        let _ = walked;
    }

    #[test]
    fn a_seeded_body_is_always_a_plausible_height() {
        // The engine's exploration envelope reaches roughly 0.1 m to 3.1 m
        // with a rare wildcard draw — about one seed in thirty. That is the
        // right distribution for somebody dragging a slider and the wrong
        // one for the body an identity is handed on arrival.
        let (low, high) = symbios_avatar::plan::humanoid_height_range();
        for seed in 0u64..600 {
            let record = engine_default_for_seed(seed);
            let symbios_avatar::Archetype::Humanoid(params) = &record.archetype else {
                panic!("seed {seed} rolled a non-humanoid default");
            };
            assert!(
                params.height >= low && params.height <= high,
                "seed {seed} stands {:.2} m, outside {low:.2}..{high:.2}",
                params.height
            );
        }
    }

    #[test]
    fn the_engine_default_is_deterministic_per_did() {
        // The contract the whole peer-rendering story rests on: every client
        // derives the same person from the same DID, with nothing exchanged.
        let a = engine_default_for_did("did:plc:vpkhqolt662uhesyj6nxm7ys");
        let b = engine_default_for_did("did:plc:vpkhqolt662uhesyj6nxm7ys");
        let other = engine_default_for_did("did:plc:aaaaaaaaaaaaaaaaaaaaaaaa");
        assert_eq!(a, b);
        assert_ne!(a, other, "two identities should not share a face");
        assert!(matches!(
            a.archetype,
            symbios_avatar::Archetype::Humanoid(_)
        ));
    }

    #[test]
    fn an_attachment_record_round_trips_and_names_its_collection() {
        let record = AttachmentRecord::new(Generator::default(), symbios_avatar::Socket::LeftHand);
        let json = serde_json::to_string(&record).expect("serializes");
        assert!(json.contains("network.symbios.overlands.avatar.attachment"));
        assert!(json.contains("left-hand"));
        let back: AttachmentRecord = serde_json::from_str(&json).expect("decodes");
        assert_eq!(back.socket(), Some(symbios_avatar::Socket::LeftHand));
    }

    #[test]
    fn sanitize_keeps_a_per_axis_offset_scale() {
        // Full transform parity with a region placement (#1095): the
        // sanitiser clamps each axis but no longer collapses them to one —
        // the editor offers the triad and the author owns the judgement.
        let mut record = AttachmentRecord::new(Generator::default(), symbios_avatar::Socket::Crown);
        record.offset.scale = Fp3([2.0, 5.0, 0.5]);
        record.sanitize();
        assert_eq!(record.offset.scale.0, [2.0, 5.0, 0.5]);
        // And still clamps: a zero or negative axis is not a scale.
        record.offset.scale = Fp3([0.0, -3.0, f32::NAN]);
        record.sanitize();
        assert!(
            record
                .offset
                .scale
                .0
                .iter()
                .all(|s| *s > 0.0 && s.is_finite())
        );
    }

    #[test]
    fn an_unknown_socket_survives_the_record_and_resolves_to_nothing() {
        let mut record = AttachmentRecord::new(Generator::default(), symbios_avatar::Socket::Crown);
        record.socket = String::from("third-elbow");
        record.sanitize();
        assert_eq!(record.socket, "third-elbow", "kept for the future client");
        assert_eq!(record.socket(), None, "and worn by nobody today");
    }

    #[test]
    fn the_engine_record_wire_form_carries_the_wardrobe_type() {
        let record = EngineAvatarRecord::default();
        let wire = engine_record_wire(&record).expect("serializes");
        assert_eq!(
            wire.get("$type").and_then(|v| v.as_str()),
            Some(WARDROBE_COLLECTION)
        );
        // And the injected key survives a round trip through the engine's
        // own decoder via its `extra` passthrough, so a fetched record
        // republishes without this crate having to strip anything.
        let back: EngineAvatarRecord = serde_json::from_value(wire).expect("decodes");
        assert_eq!(
            back.extra.get("$type").and_then(|v| v.as_str()),
            Some(WARDROBE_COLLECTION)
        );
    }

    /// The #1089 fit field, in all three schema directions (the #211/#212
    /// lesson: encode, decode, and absence each fail independently): a set
    /// fit reaches the wire under its lexicon name, comes back through the
    /// decoder, and a pre-fit record decodes to the "no fit" default with
    /// nothing added to its wire form on the way back out.
    #[test]
    fn the_fit_dimension_survives_the_wire_in_all_three_directions() {
        let mut record = AttachmentRecord::with_fit(
            Generator::default(),
            symbios_avatar::Socket::Crown,
            Some(crate::catalogue::WearFit::HeadBand {
                inner_diameter: 0.178,
            }),
        );
        record.sanitize();
        let json = serde_json::to_string(&record).expect("serializes");
        assert!(json.contains("\"fitBandMm\":178"), "encode: {json}");
        let back: AttachmentRecord = serde_json::from_str(&json).expect("decodes");
        assert_eq!(back.fit_band_mm, 178, "decode");

        // Absence: a record from before #1089 decodes as unfitted, and an
        // unfitted record puts nothing new on the wire.
        let plain = AttachmentRecord::new(Generator::default(), symbios_avatar::Socket::Crown);
        let plain_json = serde_json::to_string(&plain).expect("serializes");
        assert!(!plain_json.contains("fitBandMm"), "elides: {plain_json}");
        let old: AttachmentRecord = serde_json::from_str(&plain_json).expect("decodes");
        assert_eq!(old.fit_band_mm, 0, "an old record is worn at authored size");
    }

    /// The #1096 provenance field in all three schema directions, plus the
    /// inventory round trip it exists for: worn from a stash item, the
    /// record carries the item's socket / fit / offset and its name; saved
    /// back, `wear_meta()` reproduces the metadata exactly.
    #[test]
    fn a_record_worn_from_the_inventory_carries_its_provenance_and_round_trips() {
        use crate::pds::inventory::WearMeta;
        let meta = WearMeta {
            socket: String::from("crown"),
            fit_band_mm: 178,
            offset: TransformData {
                translation: Fp3([0.0, 0.05, 0.0]),
                ..Default::default()
            },
        };
        let mut record =
            AttachmentRecord::from_inventory("Gilded Circlet", Generator::default(), &meta);
        record.sanitize();
        assert_eq!(record.socket(), Some(symbios_avatar::Socket::Crown));
        assert_eq!(
            record.wear_meta(),
            meta,
            "save-back reproduces the wear metadata"
        );

        let json = serde_json::to_string(&record).expect("serializes");
        assert!(
            json.contains("\"source\":\"Gilded Circlet\""),
            "encode: {json}"
        );
        let back: AttachmentRecord = serde_json::from_str(&json).expect("decodes");
        assert_eq!(back.source.as_deref(), Some("Gilded Circlet"), "decode");

        let plain = AttachmentRecord::new(Generator::default(), symbios_avatar::Socket::Crown);
        let plain_json = serde_json::to_string(&plain).expect("serializes");
        assert!(!plain_json.contains("source"), "elides: {plain_json}");
        let old: AttachmentRecord = serde_json::from_str(&plain_json).expect("decodes");
        assert_eq!(
            old.source, None,
            "a record with no provenance stays that way"
        );
    }

    // -----------------------------------------------------------------
    // #1117: the bundle as one atomic applyWrites plan
    // -----------------------------------------------------------------

    /// A repo holding exactly `attachments` and nothing else.
    /// A repo holding `attachments`, from a listing that did NOT finish —
    /// so the set is not evidence of absence. The conservative default, and
    /// the one the pre-#1185 tests were written against.
    fn empty_repo_with(attachments: &[&str]) -> AvatarRepoState {
        AvatarRepoState {
            avatar_record: false,
            profile: false,
            worn_body: false,
            attachments: attachments.iter().map(|k| (*k).to_string()).collect(),
            attachments_complete: false,
        }
    }

    /// The same, from a listing that ran to the end: the set now says what the
    /// repo does NOT have, as well as what it does.
    fn fully_listed_repo_with(attachments: &[&str]) -> AvatarRepoState {
        AvatarRepoState {
            attachments_complete: true,
            ..empty_repo_with(attachments)
        }
    }

    fn collections(writes: &[RepoWrite]) -> Vec<(&'static str, &str, &str)> {
        writes
            .iter()
            .map(|w| match w {
                RepoWrite::Create {
                    collection, rkey, ..
                } => ("create", collection.as_str(), rkey.as_str()),
                RepoWrite::Update {
                    collection, rkey, ..
                } => ("update", collection.as_str(), rkey.as_str()),
                RepoWrite::Delete { collection, rkey } => {
                    ("delete", collection.as_str(), rkey.as_str())
                }
            })
            .collect()
    }

    /// Children before pointers, deletes last. Inside one atomic batch the
    /// order is immaterial — it becomes load-bearing the moment
    /// `chunk_writes` splits the plan, because then a reader can land
    /// between two commits.
    #[test]
    fn the_plan_writes_children_before_pointers_and_deletes_last() {
        let plan = plan_avatar_publish(
            &wearing(&["rkey-worn"]),
            &[String::from("rkey-worn"), String::from("rkey-gone")],
            "2026-08-28T00:00:00Z",
        );
        let repo = empty_repo_with(&["rkey-worn", "rkey-gone"]);
        let batches = plan_avatar_writes(&plan, &repo).expect("plans");
        assert_eq!(batches.len(), 1, "a normal outfit is one atomic commit");
        let order = collections(&batches[0]);

        assert_eq!(order[0].1, WARDROBE_COLLECTION, "body first");
        assert_eq!(order[1].1, AVATAR_ATTACHMENT_COLLECTION, "then its props");
        assert_eq!(order[2].1, AVATAR_PROFILE_COLLECTION, "then the pointers");
        assert_eq!(order[3].1, super::super::AVATAR_COLLECTION);
        assert_eq!(
            order[4],
            ("delete", AVATAR_ATTACHMENT_COLLECTION, "rkey-gone"),
            "and the retirement last, after nothing references it"
        );
        assert_eq!(order.len(), 5);
    }

    /// `applyWrites` has no upsert. A record already in the repo must be an
    /// `#update` and a fresh one a `#create`; the wrong verb fails the whole
    /// atomic batch, which is why the plan takes a repo state at all.
    #[test]
    fn existing_records_update_and_fresh_ones_create() {
        let plan = plan_avatar_publish(&wearing(&["rkey-worn"]), &[], "2026-08-28T00:00:00Z");

        let fresh = plan_avatar_writes(&plan, &empty_repo_with(&[])).expect("plans");
        assert!(
            collections(&fresh[0])
                .iter()
                .all(|(verb, ..)| *verb == "create"),
            "a first save creates everything"
        );

        let established = AvatarRepoState {
            avatar_record: true,
            profile: true,
            worn_body: true,
            attachments: [String::from("rkey-worn")].into_iter().collect(),
            attachments_complete: true,
        };
        let again = plan_avatar_writes(&plan, &established).expect("plans");
        assert!(
            collections(&again[0])
                .iter()
                .all(|(verb, ..)| *verb == "update"),
            "a re-save updates everything"
        );
    }

    /// The orphan sweep. Nothing in the app ever walked the attachment
    /// collection before #1117, so a record the old non-atomic bundle landed
    /// and then failed to reference — the attachments went out first, and a
    /// 5xx on `avatar/self` could delete the record that named them — stayed
    /// in the owner's repo forever, invisible and un-deletable through any
    /// UI.
    ///
    /// The sequence: attachment `rkey-stray` lands; the avatar record write
    /// fails; the session ends. `stored_attachments` never learned about it,
    /// so #1110's derived delete set cannot see it. Only the listing can.
    #[test]
    fn the_sweep_retires_an_orphan_the_derived_delete_set_cannot_see() {
        let plan = plan_avatar_publish(&wearing(&["rkey-worn"]), &[], "2026-08-28T00:00:00Z");
        assert!(
            plan.attachment_deletes.is_empty(),
            "the derived set knows of no detachment here"
        );

        let repo = empty_repo_with(&["rkey-worn", "rkey-stray"]);
        let writes = plan_avatar_writes(&plan, &repo).expect("plans");
        let deletes: Vec<&str> = collections(&writes[0])
            .into_iter()
            .filter(|(verb, ..)| *verb == "delete")
            .map(|(_, _, rkey)| rkey)
            .collect();
        assert_eq!(deletes, vec!["rkey-stray"]);
    }

    /// **A save must not ask the PDS to delete a record it does not have**
    /// (#1185).
    ///
    /// The sequence, and it is an ordinary one: a prop is worn and published,
    /// so the local mirror of the stored record names `rkey-gone`. The record
    /// is then retired somewhere this client cannot see — another device, or
    /// an earlier bundle that landed the avatar record and lost the
    /// attachment. The owner takes the prop off here and saves.
    ///
    /// The derived delete set names `rkey-gone` because the stored record did.
    /// Before this, the batch carried `applyWrites#delete` for it, the PDS's
    /// MST threw on a key that is not there, and the whole ATOMIC batch came
    /// back as a bare `500 Internal Server Error` — so the body, every worn
    /// prop and the avatar record were all lost with it, behind an error that
    /// named none of them. Losing an entire save to a record that is already
    /// gone is the worst possible trade.
    ///
    /// A complete listing is the evidence that lets us drop it.
    #[test]
    fn a_delete_is_dropped_when_a_complete_listing_says_the_record_is_not_there() {
        let plan = plan_avatar_publish(
            &wearing(&[]),
            &[String::from("rkey-gone")],
            "2026-08-28T00:00:00Z",
        );
        assert_eq!(
            plan.attachment_deletes,
            vec![String::from("rkey-gone")],
            "the stored record named it, so the derived set does too"
        );

        let writes = plan_avatar_writes(&plan, &fully_listed_repo_with(&[])).expect("plans");
        let deletes: Vec<&str> = writes
            .iter()
            .flat_map(|batch| collections(batch))
            .filter(|(verb, ..)| *verb == "delete")
            .map(|(_, _, rkey)| rkey)
            .collect();
        assert!(
            deletes.is_empty(),
            "a delete the repo cannot satisfy would fail the whole batch: {deletes:?}"
        );
    }

    /// And the guard is evidence-based, not blanket: a record the complete
    /// listing DOES hold is still retired. Dropping every derived delete would
    /// trade one bug for the orphan leak #1110 exists to prevent.
    #[test]
    fn a_delete_the_complete_listing_confirms_still_goes_out() {
        let plan = plan_avatar_publish(
            &wearing(&[]),
            &[String::from("rkey-detached")],
            "2026-08-28T00:00:00Z",
        );
        let writes =
            plan_avatar_writes(&plan, &fully_listed_repo_with(&["rkey-detached"])).expect("plans");
        let deletes: Vec<&str> = collections(&writes[0])
            .into_iter()
            .filter(|(verb, ..)| *verb == "delete")
            .map(|(_, _, rkey)| rkey)
            .collect();
        assert_eq!(deletes, vec!["rkey-detached"]);
    }

    /// The other half: a listing that truncated at its page cap cannot see
    /// a record the derived set can, so both halves stay in the union.
    #[test]
    fn a_derived_delete_survives_a_listing_that_did_not_see_it() {
        let plan = plan_avatar_publish(
            &wearing(&[]),
            &[String::from("rkey-detached")],
            "2026-08-28T00:00:00Z",
        );
        let writes = plan_avatar_writes(&plan, &empty_repo_with(&[])).expect("plans");
        let deletes: Vec<&str> = collections(&writes[0])
            .into_iter()
            .filter(|(verb, ..)| *verb == "delete")
            .map(|(_, _, rkey)| rkey)
            .collect();
        assert_eq!(deletes, vec!["rkey-detached"]);
    }

    /// #1110's rule survives the sweep: what the record still *references*
    /// is never retired, even when the resolution that would have published
    /// it failed and the plan therefore carries no write for it.
    #[test]
    fn the_sweep_never_retires_a_referenced_rkey() {
        let mut record = wearing(&["rkey-worn"]);
        // A fetch failure: the record still names the prop, but resolution
        // came back without it. This must not read as "took it off".
        if let Some(rig) = record.body.rigged_mut()
            && let Some(resolved) = rig.resolved.as_mut()
        {
            resolved.attachments.clear();
        }
        let plan = plan_avatar_publish(
            &record,
            &[String::from("rkey-worn")],
            "2026-08-28T00:00:00Z",
        );
        let repo = empty_repo_with(&["rkey-worn"]);
        let writes = plan_avatar_writes(&plan, &repo).expect("plans");
        assert!(
            !collections(&writes[0])
                .iter()
                .any(|(verb, _, rkey)| *verb == "delete" && *rkey == "rkey-worn"),
            "a prop the record still names is not an orphan"
        );
    }

    /// The wardrobe is a keep-all collection — an identity accumulates
    /// bodies and the Body tab lists them for re-wearing — so the sweep must
    /// never touch it. A saved body the owner is not currently wearing is
    /// not an orphan, and deleting it would delete a self.
    #[test]
    fn the_sweep_is_attachments_only_and_never_touches_the_wardrobe() {
        let plan = plan_avatar_publish(&wearing(&[]), &[], "2026-08-28T00:00:00Z");
        let writes = plan_avatar_writes(&plan, &empty_repo_with(&[])).expect("plans");
        assert!(
            !collections(&writes[0]).iter().any(
                |(verb, collection, _)| *verb == "delete" && *collection == WARDROBE_COLLECTION
            ),
            "the wardrobe is never swept"
        );
    }

    /// A body this build cannot serialize is refused before any write is
    /// built, not discovered mid-batch (#1111).
    #[test]
    fn an_unwritable_body_is_refused_at_plan_time() {
        let plan = plan_avatar_publish(
            &super::super::AvatarRecord::default_for_seed(3),
            &[],
            "2026-08-28T00:00:00Z",
        );
        let mut absent = plan.clone();
        absent.record.body = super::super::AvatarBody::Absent;
        assert!(plan_avatar_writes(&absent, &empty_repo_with(&[])).is_err());
    }

    /// The batch respects the request-body budget rather than the write
    /// count alone (#1115) — the reason this routes through `chunk_writes`
    /// instead of building one `Vec` and hoping.
    #[test]
    fn a_heavy_outfit_chunks_instead_of_exceeding_the_request_budget() {
        let rkeys: Vec<String> = (0..40).map(|i| format!("rkey-{i:04}")).collect();
        let refs: Vec<&str> = rkeys.iter().map(String::as_str).collect();
        let mut record = wearing(&refs);
        // Fatten each attachment so the bundle cannot fit one request.
        if let Some(rig) = record.body.rigged_mut()
            && let Some(resolved) = rig.resolved.as_mut()
        {
            for attachment in &mut resolved.attachments {
                attachment.record.item.children =
                    (0..60).map(|_| Generator::default_cuboid()).collect();
            }
        }
        let plan = plan_avatar_publish(&record, &[], "2026-08-28T00:00:00Z");
        let batches = plan_avatar_writes(&plan, &empty_repo_with(&[])).expect("plans");
        assert!(batches.len() > 1, "a heavy outfit must split");
        for batch in &batches {
            let bytes: usize = batch.iter().map(RepoWrite::wire_bytes).sum();
            assert!(
                bytes <= crate::pds::xrpc::MAX_APPLY_WRITES_BYTES,
                "a batch of {bytes} B exceeds the request budget"
            );
        }
        // Split or not, the children still precede the pointers that name
        // them: flatten the batches and check the avatar record comes after
        // every attachment write.
        let flat: Vec<RepoWrite> = batches.into_iter().flatten().collect();
        let order = collections(&flat);
        let last_attachment = order
            .iter()
            .rposition(|(verb, collection, _)| {
                *verb != "delete" && *collection == AVATAR_ATTACHMENT_COLLECTION
            })
            .expect("attachments present");
        let record_at = order
            .iter()
            .position(|(_, collection, _)| *collection == super::super::AVATAR_COLLECTION)
            .expect("the record is written");
        assert!(
            record_at > last_attachment,
            "the pointer must never be committed before what it points at"
        );
    }

    #[test]
    fn sanitize_bounds_a_hostile_fit_dimension_and_keeps_the_sentinel() {
        // The field is a divisor: a 1 mm band would scale a prop ~180× onto
        // every peer's screen. Zero must survive untouched — it is "no fit",
        // not a small band.
        let mut tiny = AttachmentRecord::new(Generator::default(), symbios_avatar::Socket::Crown);
        tiny.fit_band_mm = 1;
        tiny.sanitize();
        assert_eq!(tiny.fit_band_mm, 50);
        let mut huge = AttachmentRecord::new(Generator::default(), symbios_avatar::Socket::Crown);
        huge.fit_band_mm = 40_000;
        huge.sanitize();
        assert_eq!(huge.fit_band_mm, 1000);
        let mut unfitted =
            AttachmentRecord::new(Generator::default(), symbios_avatar::Socket::Crown);
        unfitted.sanitize();
        assert_eq!(unfitted.fit_band_mm, 0);
    }

    #[test]
    fn fp_offsets_quantise_like_every_other_transform() {
        let mut record = AttachmentRecord::new(Generator::default(), symbios_avatar::Socket::Back);
        record.offset.translation = Fp3([0.123_456_78, 0.0, 0.0]);
        let json = serde_json::to_string(&record).expect("serializes");
        let back: AttachmentRecord = serde_json::from_str(&json).expect("decodes");
        // (0.12345678 * 10000).round() / 10000 — the wire's thousandth-of-a
        // -percent grid, same as every other transform.
        assert!((back.offset.translation.0[0] - 0.1235).abs() < 1e-6);
    }
}
