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
//! Publishing here is plain `putRecord` upsert per record with no
//! delete-then-put recovery — these are fresh collections with none of the
//! legacy-record 5xx history the avatar record's publish path carries.

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
use super::super::xrpc::{FetchError, PutOutcome, XrpcError, decode_record_json, resolve_pds};
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
        let body = resp.text().await.unwrap_or_default();
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
struct PutRequest<'a> {
    repo: &'a str,
    collection: &'a str,
    rkey: &'a str,
    record: &'a serde_json::Value,
}

/// `com.atproto.repo.putRecord` upsert of an already-serialized value.
async fn put_record_json(
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    pds: &str,
    collection: &str,
    rkey: &str,
    record: &serde_json::Value,
) -> PutOutcome {
    let url = format!("{pds}/xrpc/com.atproto.repo.putRecord");
    let body = PutRequest {
        repo: &session.did,
        collection,
        rkey,
        record,
    };
    let body_json = match serde_json::to_value(&body) {
        Ok(v) => v,
        Err(e) => return PutOutcome::Transport(format!("serialize: {e}")),
    };
    let (status, body) =
        match crate::oauth::oauth_post_with_refresh(&session.session, refresh, &url, &body_json)
            .await
        {
            Ok(pair) => pair,
            Err(e) => return PutOutcome::Transport(e),
        };
    if status.is_success() {
        return PutOutcome::Ok;
    }
    let msg = format!("putRecord ({collection}/{rkey}) failed: {status} — {body}");
    if status.is_server_error() {
        PutOutcome::ServerError(msg)
    } else {
        PutOutcome::ClientError(msg)
    }
}

/// Resolve the PDS and upsert, mapping the outcome to the `Result` shape the
/// publish tasks report through the status line.
async fn resolve_and_put(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    collection: &str,
    rkey: &str,
    record: &serde_json::Value,
) -> Result<(), String> {
    let pds = resolve_pds(client, &session.did)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;
    match put_record_json(session, refresh, &pds, collection, rkey, record).await {
        PutOutcome::Ok => Ok(()),
        PutOutcome::ClientError(m) | PutOutcome::ServerError(m) | PutOutcome::Transport(m) => {
            Err(m)
        }
    }
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

/// Upsert one wardrobe record at `rkey` (a TID from
/// [`crate::pds::tid::tid_now`] for a new body, the existing rkey for an
/// edit).
pub async fn publish_wardrobe_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    rkey: &str,
    record: &EngineAvatarRecord,
) -> Result<(), String> {
    let wire = engine_record_wire(record)?;
    crate::pds::record_size::preflight(&wire, "wardrobe body")?;
    resolve_and_put(client, session, refresh, WARDROBE_COLLECTION, rkey, &wire).await
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

/// Upsert the identity's avatar profile (rkey = self). Overlands calls this
/// whenever the worn body changes so other symbios apps stay in agreement.
pub async fn publish_avatar_profile(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    profile: &EngineProfileRecord,
) -> Result<(), String> {
    let wire = WireProfile {
        lex_type: AVATAR_PROFILE_COLLECTION.into(),
        profile: profile.clone(),
    };
    let value = serde_json::to_value(&wire).map_err(|e| format!("serialize profile: {e}"))?;
    crate::pds::record_size::preflight(&value, "avatar profile")?;
    resolve_and_put(
        client,
        session,
        refresh,
        AVATAR_PROFILE_COLLECTION,
        "self",
        &value,
    )
    .await
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

/// Upsert one attachment record at `rkey`.
pub async fn publish_attachment_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    rkey: &str,
    record: &AttachmentRecord,
) -> Result<(), String> {
    let value = serde_json::to_value(record).map_err(|e| format!("serialize attachment: {e}"))?;
    crate::pds::record_size::preflight(&value, "attachment")?;
    resolve_and_put(
        client,
        session,
        refresh,
        AVATAR_ATTACHMENT_COLLECTION,
        rkey,
        &value,
    )
    .await
}

/// Delete one attachment record — the detach half of the outfit editor.
pub async fn delete_attachment_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    rkey: &str,
) -> Result<(), String> {
    delete_record(client, session, refresh, AVATAR_ATTACHMENT_COLLECTION, rkey).await
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
pub fn plan_avatar_publish(
    record: &super::AvatarRecord,
    deleted_attachments: &[String],
    now_iso: &str,
) -> AvatarPublishPlan {
    let mut plan = AvatarPublishPlan {
        wardrobe: None,
        attachments: Vec::new(),
        attachment_deletes: deleted_attachments.to_vec(),
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

/// Execute a [`AvatarPublishPlan`], children before pointers: the wardrobe
/// body and the attachments land first, then the profile and the avatar
/// record that reference them, and detached attachment records are deleted
/// last — so no reader ever resolves a dangling reference, and a failure
/// partway leaves at worst an unreferenced record, never a broken outfit.
pub async fn publish_avatar_bundle(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    plan: &AvatarPublishPlan,
) -> Result<(), String> {
    if let Some((rkey, body)) = &plan.wardrobe {
        publish_wardrobe_record(client, session, refresh, rkey, body)
            .await
            .map_err(|e| format!("wardrobe body: {e}"))?;
    }
    for (rkey, attachment) in &plan.attachments {
        publish_attachment_record(client, session, refresh, rkey, attachment)
            .await
            .map_err(|e| format!("attachment {rkey}: {e}"))?;
    }
    if let Some(profile) = &plan.profile {
        publish_avatar_profile(client, session, refresh, profile)
            .await
            .map_err(|e| format!("profile: {e}"))?;
    }
    super::publish_avatar_record(client, session, refresh, &plan.record).await?;
    for rkey in &plan.attachment_deletes {
        delete_attachment_record(client, session, refresh, rkey)
            .await
            .map_err(|e| format!("detach {rkey}: {e}"))?;
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
) {
    let body = match fetch_wardrobe_record_at(client, pds, did, &rig.avatar).await {
        Ok(Some(body)) => body,
        Ok(None) => {
            warn!(
                "wardrobe record {}/{} not found for {did} — spawning a bare chassis",
                WARDROBE_COLLECTION, rig.avatar
            );
            rig.resolved = None;
            return;
        }
        Err(err) => {
            warn!(
                "wardrobe fetch {}/{} failed for {did}: {err:?} — spawning a bare chassis",
                WARDROBE_COLLECTION, rig.avatar
            );
            rig.resolved = None;
            return;
        }
    };
    let mut attachments = Vec::new();
    for rkey in &rig.attachments {
        match fetch_attachment_record_at(client, pds, did, rkey).await {
            Ok(Some(record)) => attachments.push(ResolvedAttachment {
                rkey: rkey.clone(),
                record,
            }),
            Ok(None) => warn!("attachment record {rkey} not found for {did} — prop skipped"),
            Err(err) => warn!("attachment fetch {rkey} failed for {did}: {err:?} — prop skipped"),
        }
    }
    rig.resolved = Some(ResolvedRig { body, attachments });
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
