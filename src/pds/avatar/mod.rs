//! Avatar record — player vessel / body definition.
//!
//! Each player's avatar is published to their own PDS at
//! `collection = network.symbios.overlands.avatar, rkey = self`. The record
//! is split into two disjoint halves:
//!
//!   - `visuals` — a hierarchical [`Generator`] tree describing the cosmetic
//!     mesh (cuboids, capsules, lsystems, …). Identical machinery to room
//!     generators, with avatar-specific allowed kinds enforced by
//!     [`super::sanitize::sanitize_avatar_visuals`] (no Terrain/Water/Portal).
//!     Remote peers render this.
//!   - `locomotion` — a tagged-union [`LocomotionConfig`] selecting one of
//!     five physics presets (HoverBoat / Humanoid / Airplane / Helicopter /
//!     Car), each carrying its own collider dimensions + tuning. Remote
//!     peers *deserialize but ignore* this — only the local player's
//!     locomotion drives the rigid body.
//!
//! Locomotion presets live in the [`locomotion`] submodule, one file per
//! preset; each parameter struct impls
//! [`locomotion::LocomotionPreset`] so the central enum's `kind_tag`,
//! `display_label`, `sanitize`, and `pickers` dispatch through the trait
//! rather than a hand-maintained `match` ladder.
//!
//! Legacy `network.symbios.avatar.hover_rover` / `…humanoid` body records
//! published before this schema land deserialize to
//! [`LocomotionConfig::Unknown`] / [`super::generator::GeneratorKind::Unknown`]
//! respectively, and the fetch path falls through to
//! [`AvatarRecord::default_for_did`]. There is no automatic migration —
//! old records require a manual republish.

pub mod default_visuals;
pub mod gait;
pub mod locomotion;
pub mod parts;

pub use gait::GaitParams;
pub use locomotion::{
    AirplaneParams, CarParams, HelicopterParams, HoverBoatParams, HumanoidParams, LocomotionConfig,
    LocomotionPickerEntry, LocomotionPreset,
};

use super::AVATAR_COLLECTION;
use super::generator::Generator;
use super::sanitize::sanitize_avatar_visuals;
use super::xrpc::{FetchError, PutOutcome, XrpcError, decode_record_json, resolve_pds};
use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// AvatarRecord
// ---------------------------------------------------------------------------

/// The top-level avatar record. Stored at
/// `network.symbios.overlands.avatar / self` on the player's PDS.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Resource)]
pub struct AvatarRecord {
    #[serde(rename = "$type")]
    pub lex_type: String,
    /// Hierarchical visuals — the cosmetic mesh tree. Sanitised by
    /// [`super::sanitize::sanitize_avatar_visuals`] which excludes
    /// Terrain/Water/Portal kinds.
    pub visuals: Generator,
    /// Physics preset selecting the player's chassis collider + control
    /// scheme + tuning. Local-only — remote peers ignore this.
    pub locomotion: LocomotionConfig,
    /// Idle-motion tuning (bounce / sway / head-turn amplitudes). Optional
    /// on the wire: records published before the field existed (or by
    /// clients that never touched the sliders) omit it, and every peer
    /// falls back to the DID-seeded [`GaitParams::for_seed`] derivation —
    /// identical to the pre-#874 behavior. Field-level `default` (not a
    /// container default) so a present-but-partial record still fails
    /// loudly instead of half-deserializing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gait: Option<GaitParams>,
}

impl AvatarRecord {
    /// Synthesise a starting avatar derived entirely from the owner's
    /// DID — every fresh player gets a unique chassis without ever
    /// touching the editor.
    ///
    /// The DID first resolves to the [`crate::seeded_defaults::AvatarCharacter`]
    /// anchor — one of four visual families
    /// ([`crate::seeded_defaults::ChassisFamily`]: hover-boat, airship,
    /// humanoid figure, land-skiff) plus a style + ornateness / wear. The
    /// assembler in [`default_visuals`] composes the silhouette from the
    /// tagged part catalogue ([`parts`]) via the seeded
    /// [`crate::seeded_defaults::AvatarOutfit`], colouring each part from
    /// [`crate::seeded_defaults::AvatarPalette`] and finishing it with
    /// [`crate::seeded_defaults::MaterialKit`]. Locomotion follows the family
    /// (boat → HoverBoat, airship → Helicopter, humanoid → Humanoid, skiff →
    /// Car) so the chassis drives like it looks.
    pub fn default_for_did(did: &str) -> Self {
        Self::default_for_seed(crate::seeded_defaults::fnv1a_64(did))
    }

    /// Build the seeded default avatar from a pre-computed seed — the
    /// manual re-roll path. `seed` chooses the chassis family and drives
    /// every derived value (avatars carry no identity sign since #733).
    /// `default_for_did` is exactly `default_for_seed(fnv1a_64(did))`.
    pub fn default_for_seed(seed: u64) -> Self {
        let (visuals, locomotion) = default_visuals::build_for_seed(seed);
        Self {
            lex_type: AVATAR_COLLECTION.into(),
            visuals,
            locomotion,
            // Explicit rather than None so a re-roll re-rolls the idle
            // motion with the same seed as the visuals — peers rendering
            // the published record see the identical gait.
            gait: Some(GaitParams::for_seed(seed)),
        }
    }

    /// Clamp every numeric field so a malicious PDS (or a forward-compat
    /// client shipping a record we cannot fully model) cannot weaponise the
    /// record to panic Bevy primitive constructors.
    pub fn sanitize(&mut self) {
        sanitize_avatar_visuals(&mut self.visuals);
        self.locomotion.sanitize();
        if let Some(gait) = &mut self.gait {
            gait.sanitize();
        }
    }
}

// ---------------------------------------------------------------------------
// Avatar record fetch / publish
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GetAvatarResponse {
    value: AvatarRecord,
}

/// Fetch a player's avatar record from their PDS. Result semantics mirror
/// [`super::fetch_room_record`]: `Ok(None)` is a clean 404 ("no record
/// yet"), and any other failure returns an `Err` the caller distinguishes
/// so it does not silently overwrite a live record with the default.
pub async fn fetch_avatar_record(
    client: &reqwest::Client,
    did: &str,
) -> Result<Option<AvatarRecord>, FetchError> {
    let pds = resolve_pds(client, did)
        .await
        .ok_or(FetchError::DidResolutionFailed)?;
    let url = format!(
        "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection={}&rkey=self",
        pds, did, AVATAR_COLLECTION
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
    let wrapper: GetAvatarResponse = decode_record_json(resp).await?;
    let mut record = wrapper.value;
    record.sanitize();
    Ok(Some(record))
}

#[derive(Serialize)]
struct PutAvatarRequest<'a> {
    repo: &'a str,
    collection: &'a str,
    rkey: &'a str,
    record: &'a AvatarRecord,
}

async fn try_put_avatar(
    _client: &reqwest::Client,
    pds: &str,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    record: &AvatarRecord,
) -> PutOutcome {
    let url = format!("{}/xrpc/com.atproto.repo.putRecord", pds);
    let body = PutAvatarRequest {
        repo: &session.did,
        collection: AVATAR_COLLECTION,
        rkey: "self",
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
    let msg = format!("putRecord (avatar) failed: {} — {}", status, body);
    if status.is_server_error() {
        PutOutcome::ServerError(msg)
    } else {
        PutOutcome::ClientError(msg)
    }
}

#[derive(Serialize)]
struct DeleteAvatarRequest<'a> {
    repo: &'a str,
    collection: &'a str,
    rkey: &'a str,
}

async fn delete_avatar_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
) -> Result<(), String> {
    let pds = resolve_pds(client, &session.did)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;
    let url = format!("{}/xrpc/com.atproto.repo.deleteRecord", pds);
    let body = DeleteAvatarRequest {
        repo: &session.did,
        collection: AVATAR_COLLECTION,
        rkey: "self",
    };
    let body_json = serde_json::to_value(&body).map_err(|e| e.to_string())?;
    let (status, body) =
        crate::oauth::oauth_post_with_refresh(&session.session, refresh, &url, &body_json).await?;
    if status.is_success() || status.as_u16() == 404 {
        Ok(())
    } else {
        Err(format!(
            "deleteRecord (avatar) failed: {} — {}",
            status, body
        ))
    }
}

/// Upsert the avatar record to the authenticated user's own PDS. Uses the
/// same 5xx → delete-then-put recovery path as
/// [`super::publish_room_record`].
pub async fn publish_avatar_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    record: &AvatarRecord,
) -> Result<(), String> {
    // Pre-flight size guard BEFORE any network I/O — the 5xx fallback below
    // deletes the stored record, so an oversized record must be refused
    // before it can trigger that delete-without-replace sequence.
    crate::pds::record_size::preflight(record, "avatar")?;
    let pds = resolve_pds(client, &session.did)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;
    match try_put_avatar(client, &pds, session, refresh, record).await {
        PutOutcome::Ok => Ok(()),
        PutOutcome::ClientError(msg) => Err(msg),
        PutOutcome::Transport(msg) => Err(msg),
        PutOutcome::ServerError(first_err) => {
            warn!("{first_err} — retrying via delete+put for avatar");
            delete_avatar_record(client, session, refresh)
                .await
                .map_err(|e| format!("{first_err}; fallback delete failed: {e}"))?;
            match try_put_avatar(client, &pds, session, refresh, record).await {
                PutOutcome::Ok => Ok(()),
                PutOutcome::ClientError(m)
                | PutOutcome::ServerError(m)
                | PutOutcome::Transport(m) => Err(format!("{first_err}; fallback put failed: {m}")),
            }
        }
    }
}
