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

pub mod locomotion;

pub use locomotion::{
    AirplaneParams, CarParams, HelicopterParams, HoverBoatParams, HumanoidParams, LocomotionConfig,
    LocomotionPickerEntry, LocomotionPreset,
};

use super::AVATAR_COLLECTION;
use super::generator::{Generator, GeneratorKind};
use super::sanitize::sanitize_avatar_visuals;
use super::texture::SovereignMaterialSettings;
use super::types::{Fp, Fp3, TransformData};
use super::xrpc::{FetchError, PutOutcome, XrpcError, resolve_pds};
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
}

impl AvatarRecord {
    /// Synthesise a starting avatar with a deterministic palette derived
    /// from the owner's DID — every fresh player gets a unique-coloured
    /// hover-boat without ever touching the editor.
    ///
    /// The visual tree mirrors the spirit of the legacy hover-rover: a
    /// cuboid hull with two capsule pontoons, an upright cylinder mast
    /// crowned by a sphere finial, and a flat sail. Materials carry the
    /// DID-hashed palette (`hue(0)` hull, `hue(3)` pontoons, `hue(7)` mast,
    /// `hue(11)` accents) so two peers spawning side-by-side never look
    /// identical.
    pub fn default_for_did(did: &str) -> Self {
        let hash = fnv1a_64(did);
        let hue = |n: u32| -> [f32; 3] {
            let r = ((hash.rotate_left(n) & 0xFF) as f32) / 255.0;
            let g = ((hash.rotate_left(n + 8) & 0xFF) as f32) / 255.0;
            let b = ((hash.rotate_left(n + 16) & 0xFF) as f32) / 255.0;
            // Bias away from near-black so new players aren't invisible.
            [0.25 + r * 0.70, 0.25 + g * 0.70, 0.25 + b * 0.70]
        };
        let hull_color = hue(0);
        let pontoon_color = hue(3);
        let mast_color = hue(7);
        let accent_color = hue(11);

        let metal_mat = |color: [f32; 3]| SovereignMaterialSettings {
            base_color: Fp3(color),
            metallic: Fp(0.4),
            roughness: Fp(0.45),
            ..Default::default()
        };
        let cloth_mat = |color: [f32; 3]| SovereignMaterialSettings {
            base_color: Fp3(color),
            metallic: Fp(0.0),
            roughness: Fp(0.85),
            ..Default::default()
        };

        let hull = Generator {
            kind: GeneratorKind::Cuboid {
                size: Fp3([1.6, 0.4, 2.4]),
                solid: false,
                material: metal_mat(hull_color),
                twist: Fp(0.0),
                taper: Fp(0.0),
                bend: Fp3([0.0, 0.0, 0.0]),
            },
            transform: TransformData::default(),
            children: vec![
                // Left pontoon — capsule lying on its side.
                Generator {
                    kind: GeneratorKind::Capsule {
                        radius: Fp(0.18),
                        length: Fp(2.0),
                        latitudes: 8,
                        longitudes: 16,
                        solid: false,
                        material: metal_mat(pontoon_color),
                        twist: Fp(0.0),
                        taper: Fp(0.0),
                        bend: Fp3([0.0, 0.0, 0.0]),
                    },
                    transform: TransformData {
                        translation: Fp3([-0.85, -0.25, 0.0]),
                        rotation: quat_xyzw(quat_x(std::f32::consts::FRAC_PI_2)),
                        scale: Fp3([1.0, 1.0, 1.0]),
                    },
                    children: Vec::new(),
                },
                // Right pontoon.
                Generator {
                    kind: GeneratorKind::Capsule {
                        radius: Fp(0.18),
                        length: Fp(2.0),
                        latitudes: 8,
                        longitudes: 16,
                        solid: false,
                        material: metal_mat(pontoon_color),
                        twist: Fp(0.0),
                        taper: Fp(0.0),
                        bend: Fp3([0.0, 0.0, 0.0]),
                    },
                    transform: TransformData {
                        translation: Fp3([0.85, -0.25, 0.0]),
                        rotation: quat_xyzw(quat_x(std::f32::consts::FRAC_PI_2)),
                        scale: Fp3([1.0, 1.0, 1.0]),
                    },
                    children: Vec::new(),
                },
                // Mast — vertical cylinder rising from the deck.
                Generator {
                    kind: GeneratorKind::Cylinder {
                        radius: Fp(0.06),
                        height: Fp(1.4),
                        resolution: 16,
                        solid: false,
                        material: metal_mat(mast_color),
                        twist: Fp(0.0),
                        taper: Fp(0.0),
                        bend: Fp3([0.0, 0.0, 0.0]),
                    },
                    transform: TransformData {
                        translation: Fp3([0.0, 0.9, 0.0]),
                        rotation: quat_xyzw([0.0, 0.0, 0.0, 1.0]),
                        scale: Fp3([1.0, 1.0, 1.0]),
                    },
                    children: vec![
                        // Sphere finial perched at the very top. Centred at
                        // the mast's local +Y so it caps the cylinder.
                        Generator {
                            kind: GeneratorKind::Sphere {
                                radius: Fp(0.12),
                                resolution: 3,
                                solid: false,
                                material: metal_mat(accent_color),
                                twist: Fp(0.0),
                                taper: Fp(0.0),
                                bend: Fp3([0.0, 0.0, 0.0]),
                            },
                            transform: TransformData {
                                translation: Fp3([0.0, 0.7, 0.0]),
                                rotation: quat_xyzw([0.0, 0.0, 0.0, 1.0]),
                                scale: Fp3([1.0, 1.0, 1.0]),
                            },
                            children: Vec::new(),
                        },
                    ],
                },
                // Sail — flat plane hanging beside the mast, cloth-like.
                Generator {
                    kind: GeneratorKind::Cuboid {
                        size: Fp3([0.05, 0.9, 0.9]),
                        solid: false,
                        material: cloth_mat([0.95, 0.95, 0.92]),
                        twist: Fp(0.0),
                        taper: Fp(0.0),
                        bend: Fp3([0.0, 0.0, 0.0]),
                    },
                    transform: TransformData {
                        translation: Fp3([0.0, 1.05, -0.5]),
                        rotation: quat_xyzw([0.0, 0.0, 0.0, 1.0]),
                        scale: Fp3([1.0, 1.0, 1.0]),
                    },
                    children: Vec::new(),
                },
            ],
        };

        Self {
            lex_type: AVATAR_COLLECTION.into(),
            visuals: hull,
            locomotion: HoverBoatParams::default_config(),
        }
    }

    /// Clamp every numeric field so a malicious PDS (or a forward-compat
    /// client shipping a record we cannot fully model) cannot weaponise the
    /// record to panic Bevy primitive constructors.
    pub fn sanitize(&mut self) {
        sanitize_avatar_visuals(&mut self.visuals);
        self.locomotion.sanitize();
    }
}

/// FNV-1a 64-bit hash of a string. Matches the hash used by
/// [`crate::pds::room::RoomRecord::default_for_did`] so peer rooms and
/// avatars derive their colour palettes from the same DID-derived seed.
fn fnv1a_64(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Build a normalised quaternion `[x, y, z, w]` from a half-angle rotation
/// around the X axis. Used by [`AvatarRecord::default_for_did`] to lay
/// pontoon capsules on their side without re-deriving the math at every
/// call site.
fn quat_x(angle_rad: f32) -> [f32; 4] {
    let half = angle_rad * 0.5;
    [half.sin(), 0.0, 0.0, half.cos()]
}

fn quat_xyzw(q: [f32; 4]) -> super::types::Fp4 {
    super::types::Fp4(q)
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
    let wrapper: GetAvatarResponse = resp
        .json()
        .await
        .map_err(|e| FetchError::Decode(e.to_string()))?;
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
