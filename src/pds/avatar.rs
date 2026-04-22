//! Avatar record — player vessel / body definition.
//!
//! Each player's avatar is published to their own PDS at
//! `collection = network.symbios.overlands.avatar, rkey = self`. The body is
//! an open union tagged by `$type` so future vessels (e.g. submarine,
//! glider) can extend the schema without breaking older clients — unknown
//! tags deserialize to `AvatarBody::Unknown`, which the player-side fallback
//! converts to a default hover-rover.
//!
//! **Phenotype vs kinematics.** The body carries two disjoint sub-records:
//!   - `phenotype` — shape/scales/colours. Remote peers render this.
//!   - `kinematics` — physics tuning (spring stiffness, drive force, jump
//!     impulse). Remote peers *deserialize but ignore* these so a malicious
//!     PDS cannot crash guests by broadcasting pathological spring constants.

use super::AVATAR_COLLECTION;
use super::sanitize::sanitize_material_settings;
use super::texture::SovereignMaterialSettings;
use super::types::{Fp, Fp2, Fp3};
use super::xrpc::{FetchError, PutOutcome, XrpcError, resolve_pds};
use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde::{Deserialize, Serialize};

/// Rover chassis construction + material, DAG-CBOR safe via `Fp*` wrappers.
/// Each slot carries a full [`SovereignMaterialSettings`] so the hull,
/// pontoons, mast, struts, and sail can drive any `bevy_symbios_texture`
/// generator independently.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RoverPhenotype {
    pub hull_length: Fp,
    pub hull_width: Fp,
    pub hull_depth: Fp,
    pub pontoon_spread: Fp,
    pub pontoon_length: Fp,
    pub pontoon_width: Fp,
    pub pontoon_height: Fp,
    pub pontoon_shape: crate::protocol::PontoonShape,
    pub strut_drop: Fp,
    pub mast_height: Fp,
    pub mast_radius: Fp,
    pub mast_offset: Fp2,
    pub sail_size: Fp,
    pub hull_material: SovereignMaterialSettings,
    pub pontoon_material: SovereignMaterialSettings,
    pub mast_material: SovereignMaterialSettings,
    pub strut_material: SovereignMaterialSettings,
    pub sail_material: SovereignMaterialSettings,
}

impl Default for RoverPhenotype {
    fn default() -> Self {
        use crate::config::airship as cfg;
        let mat = |color: [f32; 3]| SovereignMaterialSettings {
            base_color: Fp3(color),
            metallic: Fp(cfg::METALLIC),
            roughness: Fp(cfg::ROUGHNESS),
            ..Default::default()
        };
        Self {
            hull_length: Fp(cfg::HULL_LENGTH),
            hull_width: Fp(cfg::HULL_WIDTH),
            hull_depth: Fp(cfg::HULL_DEPTH),
            pontoon_spread: Fp(cfg::PONTOON_SPREAD),
            pontoon_length: Fp(cfg::PONTOON_LENGTH),
            pontoon_width: Fp(cfg::PONTOON_WIDTH),
            pontoon_height: Fp(cfg::PONTOON_HEIGHT),
            pontoon_shape: crate::protocol::PontoonShape::default(),
            strut_drop: Fp(cfg::STRUT_DROP),
            mast_height: Fp(cfg::MAST_HEIGHT),
            mast_radius: Fp(cfg::MAST_RADIUS),
            mast_offset: Fp2(cfg::MAST_OFFSET),
            sail_size: Fp(cfg::SAIL_SIZE),
            hull_material: mat(cfg::HULL_COLOR),
            pontoon_material: mat(cfg::PONTOON_COLOR),
            mast_material: mat(cfg::MAST_COLOR),
            strut_material: mat(cfg::STRUT_COLOR),
            sail_material: SovereignMaterialSettings {
                base_color: Fp3([0.95, 0.95, 0.92]),
                metallic: Fp(0.0),
                roughness: Fp(0.85),
                ..Default::default()
            },
        }
    }
}

/// Rover physics tuning. Deserialized on remote peers but ignored at apply
/// time — only the local player's kinematics drive the rigid body.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RoverKinematics {
    pub suspension_rest_length: Fp,
    pub suspension_stiffness: Fp,
    pub suspension_damping: Fp,
    pub drive_force: Fp,
    pub turn_torque: Fp,
    pub lateral_grip: Fp,
    pub jump_force: Fp,
    pub uprighting_torque: Fp,
    pub linear_damping: Fp,
    pub angular_damping: Fp,
    pub mass: Fp,
    pub water_rest_length: Fp,
    pub buoyancy_strength: Fp,
    pub buoyancy_damping: Fp,
    pub buoyancy_max_depth: Fp,
}

impl Default for RoverKinematics {
    fn default() -> Self {
        use crate::config::rover as cfg;
        Self {
            suspension_rest_length: Fp(cfg::SUSPENSION_REST_LENGTH),
            suspension_stiffness: Fp(cfg::SUSPENSION_STIFFNESS),
            suspension_damping: Fp(cfg::SUSPENSION_DAMPING),
            drive_force: Fp(cfg::DRIVE_FORCE),
            turn_torque: Fp(cfg::TURN_TORQUE),
            lateral_grip: Fp(cfg::LATERAL_GRIP),
            jump_force: Fp(cfg::JUMP_FORCE),
            uprighting_torque: Fp(cfg::UPRIGHTING_TORQUE),
            linear_damping: Fp(cfg::LINEAR_DAMPING),
            angular_damping: Fp(cfg::ANGULAR_DAMPING),
            mass: Fp(cfg::MASS),
            water_rest_length: Fp(cfg::WATER_REST_LENGTH),
            buoyancy_strength: Fp(cfg::BUOYANCY_STRENGTH),
            buoyancy_damping: Fp(cfg::BUOYANCY_DAMPING),
            buoyancy_max_depth: Fp(cfg::BUOYANCY_MAX_DEPTH),
        }
    }
}

/// Humanoid body construction (blocky/robotic).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HumanoidPhenotype {
    /// Total standing height (m).
    pub height: Fp,
    /// Torso half-width in X (m).
    pub torso_half_width: Fp,
    /// Torso half-depth in Z (m).
    pub torso_half_depth: Fp,
    /// Head edge length (m).
    pub head_size: Fp,
    /// Limb thickness (m).
    pub limb_thickness: Fp,
    /// Arm length expressed as a ratio of torso height (≈0.5–1.5).
    #[serde(default = "default_arm_ratio")]
    pub arm_length_ratio: Fp,
    /// Leg length expressed as a ratio of total height (≈0.3–0.6).
    #[serde(default = "default_leg_ratio")]
    pub leg_length_ratio: Fp,
    /// Show the owner's ATProto profile picture on the chest.
    #[serde(default = "default_show_badge")]
    pub show_badge: bool,
    pub body_material: SovereignMaterialSettings,
    pub head_material: SovereignMaterialSettings,
    pub limb_material: SovereignMaterialSettings,
}

fn default_arm_ratio() -> Fp {
    Fp(0.9)
}
fn default_leg_ratio() -> Fp {
    Fp(0.45)
}
fn default_show_badge() -> bool {
    true
}

impl Default for HumanoidPhenotype {
    fn default() -> Self {
        let mat = |color: [f32; 3]| SovereignMaterialSettings {
            base_color: Fp3(color),
            metallic: Fp(0.2),
            roughness: Fp(0.7),
            ..Default::default()
        };
        Self {
            height: Fp(1.8),
            torso_half_width: Fp(0.28),
            torso_half_depth: Fp(0.18),
            head_size: Fp(0.28),
            limb_thickness: Fp(0.12),
            arm_length_ratio: default_arm_ratio(),
            leg_length_ratio: default_leg_ratio(),
            show_badge: default_show_badge(),
            body_material: mat([0.25, 0.45, 0.75]),
            head_material: mat([0.85, 0.75, 0.65]),
            limb_material: mat([0.20, 0.20, 0.25]),
        }
    }
}

/// Humanoid movement tuning.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HumanoidKinematics {
    /// Target linear speed when input is held (m/s).
    pub walk_speed: Fp,
    /// Per-second velocity correction applied toward the target (1/s).
    pub acceleration: Fp,
    /// Instantaneous upward impulse magnitude on jump (N·s).
    pub jump_impulse: Fp,
    pub mass: Fp,
    pub linear_damping: Fp,
}

impl Default for HumanoidKinematics {
    fn default() -> Self {
        Self {
            walk_speed: Fp(4.0),
            acceleration: Fp(12.0),
            jump_impulse: Fp(450.0),
            mass: Fp(80.0),
            linear_damping: Fp(0.3),
        }
    }
}

/// Open-union avatar body. Future vehicle types add new `#[serde(rename)]`
/// variants; older clients fall through to `Unknown`.
///
/// Phenotype + kinematics payloads are boxed so the enum's stack footprint
/// stays close to that of the zero-sized `Unknown` variant. Deref coercion
/// makes the boxing transparent at every access site.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "$type")]
pub enum AvatarBody {
    #[serde(rename = "network.symbios.avatar.hover_rover")]
    HoverRover {
        phenotype: Box<RoverPhenotype>,
        kinematics: Box<RoverKinematics>,
    },

    #[serde(rename = "network.symbios.avatar.humanoid")]
    Humanoid {
        phenotype: Box<HumanoidPhenotype>,
        kinematics: Box<HumanoidKinematics>,
    },

    #[serde(other)]
    Unknown,
}

impl AvatarBody {
    /// Stable string tag used by hot-swap detection so a variant change
    /// (HoverRover → Humanoid) can be seen without a full `==` compare.
    pub fn kind_tag(&self) -> &'static str {
        match self {
            AvatarBody::HoverRover { .. } => "hover_rover",
            AvatarBody::Humanoid { .. } => "humanoid",
            AvatarBody::Unknown => "unknown",
        }
    }
}

/// The top-level avatar record. Stored at
/// `network.symbios.overlands.avatar / self` on the player's PDS.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Resource)]
pub struct AvatarRecord {
    #[serde(rename = "$type")]
    pub lex_type: String,
    pub body: AvatarBody,
}

impl AvatarRecord {
    /// Synthesise a starting hover-rover with a deterministic palette derived
    /// from the owner's DID — every fresh player gets a unique-coloured
    /// vessel without ever touching the editor.
    pub fn default_for_did(did: &str) -> Self {
        // FNV-1a 64-bit, identical to `RoomRecord::default_for_did`.
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in did.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        // Derive three hue-shifted colours from the hash by taking 8-bit
        // slots in HSV-ish space — any deterministic expansion works, the
        // only requirement is stability across peers.
        let hue = |n: u32| -> [f32; 3] {
            let r = ((hash.rotate_left(n) & 0xFF) as f32) / 255.0;
            let g = ((hash.rotate_left(n + 8) & 0xFF) as f32) / 255.0;
            let b = ((hash.rotate_left(n + 16) & 0xFF) as f32) / 255.0;
            // Bias away from near-black so new players aren't invisible.
            [0.25 + r * 0.70, 0.25 + g * 0.70, 0.25 + b * 0.70]
        };

        let mut phenotype = RoverPhenotype::default();
        phenotype.hull_material.base_color = Fp3(hue(0));
        phenotype.pontoon_material.base_color = Fp3(hue(3));
        phenotype.mast_material.base_color = Fp3(hue(7));
        phenotype.strut_material.base_color = Fp3(hue(11));

        Self {
            lex_type: AVATAR_COLLECTION.into(),
            body: AvatarBody::HoverRover {
                phenotype: Box::new(phenotype),
                kinematics: Box::new(RoverKinematics::default()),
            },
        }
    }

    /// Clamp every numeric field so a malicious PDS (or forward-compat
    /// client shipping a record we cannot fully model) cannot weaponise the
    /// record to panic Bevy primitive constructors.
    pub fn sanitize(&mut self) {
        const MIN_DIM: f32 = 0.01;
        const MAX_DIM: f32 = 50.0;
        let clamp = |v: f32| {
            if v.is_finite() {
                v.clamp(MIN_DIM, MAX_DIM)
            } else {
                MIN_DIM
            }
        };
        let clamp_unit = |v: f32| {
            if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let clamp_offset = |v: f32| {
            if v.is_finite() {
                v.clamp(-MAX_DIM, MAX_DIM)
            } else {
                0.0
            }
        };
        let clamp_pos = |v: f32, hi: f32| {
            if v.is_finite() { v.clamp(0.0, hi) } else { 0.0 }
        };

        match &mut self.body {
            AvatarBody::HoverRover {
                phenotype: p,
                kinematics: k,
            } => {
                p.hull_length = Fp(clamp(p.hull_length.0));
                p.hull_width = Fp(clamp(p.hull_width.0));
                p.hull_depth = Fp(clamp(p.hull_depth.0));
                p.pontoon_spread = Fp(clamp(p.pontoon_spread.0));
                p.pontoon_length = Fp(clamp(p.pontoon_length.0));
                p.pontoon_width = Fp(clamp(p.pontoon_width.0));
                p.pontoon_height = Fp(clamp(p.pontoon_height.0));
                p.strut_drop = Fp(clamp_unit(p.strut_drop.0));
                p.mast_height = Fp(clamp(p.mast_height.0));
                p.mast_radius = Fp(clamp(p.mast_radius.0));
                p.mast_offset = Fp2([
                    clamp_offset(p.mast_offset.0[0]),
                    clamp_offset(p.mast_offset.0[1]),
                ]);
                p.sail_size = Fp(clamp(p.sail_size.0));
                sanitize_material_settings(&mut p.hull_material);
                sanitize_material_settings(&mut p.pontoon_material);
                sanitize_material_settings(&mut p.mast_material);
                sanitize_material_settings(&mut p.strut_material);
                sanitize_material_settings(&mut p.sail_material);

                k.suspension_rest_length = Fp(clamp_pos(k.suspension_rest_length.0, 5.0));
                k.suspension_stiffness = Fp(clamp_pos(k.suspension_stiffness.0, 50_000.0));
                k.suspension_damping = Fp(clamp_pos(k.suspension_damping.0, 5_000.0));
                k.drive_force = Fp(clamp_pos(k.drive_force.0, 50_000.0));
                k.turn_torque = Fp(clamp_pos(k.turn_torque.0, 50_000.0));
                k.lateral_grip = Fp(clamp_pos(k.lateral_grip.0, 50_000.0));
                k.jump_force = Fp(clamp_pos(k.jump_force.0, 50_000.0));
                k.uprighting_torque = Fp(clamp_pos(k.uprighting_torque.0, 50_000.0));
                k.linear_damping = Fp(clamp_pos(k.linear_damping.0, 100.0));
                k.angular_damping = Fp(clamp_pos(k.angular_damping.0, 100.0));
                k.mass = Fp(k.mass.0.clamp(0.1, 10_000.0));
                k.water_rest_length = Fp(clamp_pos(k.water_rest_length.0, 10.0));
                k.buoyancy_strength = Fp(clamp_pos(k.buoyancy_strength.0, 100_000.0));
                k.buoyancy_damping = Fp(clamp_pos(k.buoyancy_damping.0, 10_000.0));
                k.buoyancy_max_depth = Fp(clamp_pos(k.buoyancy_max_depth.0, 50.0));
            }
            AvatarBody::Humanoid {
                phenotype: p,
                kinematics: k,
            } => {
                p.height = Fp(p.height.0.clamp(0.5, 5.0));
                p.torso_half_width = Fp(clamp(p.torso_half_width.0));
                p.torso_half_depth = Fp(clamp(p.torso_half_depth.0));
                p.head_size = Fp(clamp(p.head_size.0));
                p.limb_thickness = Fp(clamp(p.limb_thickness.0));
                p.arm_length_ratio = Fp(if p.arm_length_ratio.0.is_finite() {
                    p.arm_length_ratio.0.clamp(0.5, 1.5)
                } else {
                    default_arm_ratio().0
                });
                p.leg_length_ratio = Fp(if p.leg_length_ratio.0.is_finite() {
                    p.leg_length_ratio.0.clamp(0.3, 0.6)
                } else {
                    default_leg_ratio().0
                });
                sanitize_material_settings(&mut p.body_material);
                sanitize_material_settings(&mut p.head_material);
                sanitize_material_settings(&mut p.limb_material);

                k.walk_speed = Fp(clamp_pos(k.walk_speed.0, 50.0));
                k.acceleration = Fp(clamp_pos(k.acceleration.0, 200.0));
                k.jump_impulse = Fp(clamp_pos(k.jump_impulse.0, 50_000.0));
                k.mass = Fp(k.mass.0.clamp(0.1, 10_000.0));
                k.linear_damping = Fp(clamp_pos(k.linear_damping.0, 100.0));
            }
            AvatarBody::Unknown => {}
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
        match crate::oauth::oauth_post_with_nonce_retry(&session.session, &url, &body_json).await {
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
        crate::oauth::oauth_post_with_nonce_retry(&session.session, &url, &body_json).await?;
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
    record: &AvatarRecord,
) -> Result<(), String> {
    let pds = resolve_pds(client, &session.did)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;
    match try_put_avatar(client, &pds, session, record).await {
        PutOutcome::Ok => Ok(()),
        PutOutcome::ClientError(msg) => Err(msg),
        PutOutcome::Transport(msg) => Err(msg),
        PutOutcome::ServerError(first_err) => {
            warn!("{first_err} — retrying via delete+put for avatar");
            delete_avatar_record(client, session)
                .await
                .map_err(|e| format!("{first_err}; fallback delete failed: {e}"))?;
            match try_put_avatar(client, &pds, session, record).await {
                PutOutcome::Ok => Ok(()),
                PutOutcome::ClientError(m)
                | PutOutcome::ServerError(m)
                | PutOutcome::Transport(m) => Err(format!("{first_err}; fallback put failed: {m}")),
            }
        }
    }
}
