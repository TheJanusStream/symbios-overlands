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
use super::generator::{AlphaModeKind, Generator, GeneratorKind, SignSource};
use super::sanitize::sanitize_avatar_visuals;
use super::texture::SovereignMaterialSettings;
use super::types::{Fp, Fp2, Fp3, TransformData};
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
    /// Synthesise a starting avatar with a deterministic palette,
    /// body proportions, and vessel design derived from the owner's
    /// DID — every fresh player gets a unique-coloured,
    /// uniquely-shaped catamaran without ever touching the editor.
    ///
    /// The visual tree is a stylised steampunk/scifi catamaran: a
    /// thin deck cuboid bridging two longitudinal hull capsules, a
    /// central mast crowned by a glowing finial, a flag panel
    /// ([`SignSource::DidPfp`]) hanging behind the mast, an optional
    /// prow ornament, and either smokestacks (Steam / Hybrid
    /// archetypes) or a tilted solar panel + antenna (Solar /
    /// Hybrid). Colour assignments come from
    /// [`crate::seeded_defaults::AvatarPalette`]; dimensions from
    /// [`crate::seeded_defaults::AvatarBody`] +
    /// [`crate::seeded_defaults::VesselDesign`]; the archetype +
    /// bow-style enums in [`VesselDesign`] gate which ornament arms
    /// actually appear, so two peers spawning side-by-side differ in
    /// tint *and* silhouette *and* fitted ornaments.
    pub fn default_for_did(did: &str) -> Self {
        use crate::seeded_defaults::{AvatarBody, AvatarPalette, BowStyle, VesselDesign};

        let palette = AvatarPalette::for_did(did);
        let body = AvatarBody::for_did(did);
        let vessel = VesselDesign::for_did(did);

        // Colour assignments:
        //   deck            = primary_accent  (largest visible surface)
        //   hulls           = secondary_accent
        //   mast / ornaments= tertiary_accent
        //   bow jewel / finial = eye_color    (small "gem" slot)
        //   smokestacks / panel / antenna = hair_color
        //                     (a darker techy / brass tone — taken
        //                     from the curated hair-colour table so it
        //                     reads as metallic, not as accent paint)
        let deck_color = palette.primary_accent;
        let hull_color = palette.secondary_accent;
        let mast_color = palette.tertiary_accent;
        let jewel_color = palette.eye_color;
        let metal_color = palette.hair_color;

        // Two-level scaling: AvatarBody = avatar-wide size (humanoid-
        // tight band, ±15 %); VesselDesign = vessel-specific
        // proportions (wider band per knob).
        let h = body.height_scale;
        let w = body.shoulder_width_scale;
        let limb = body.limb_thickness_scale;
        let head = body.head_scale;

        // Vessel-level scales.
        let hull_r = 0.28 * limb * vessel.hull_radius_scale;
        let hull_len = 2.4 * h * vessel.hull_length_scale;
        let hull_x = 0.80 * w * vessel.hull_spread_scale;
        let hull_y = -0.30 * h * vessel.hull_drop_scale;

        let deck_x = 1.6 * w * vessel.hull_spread_scale;
        let deck_y = 0.12;
        let deck_z = 2.0 * h * vessel.hull_length_scale;
        let deck_half_z = deck_z * 0.5;

        let mast_r = 0.05 * limb * vessel.mast_radius_scale;
        let mast_h = 1.4 * h * vessel.mast_height_scale;
        // Mast cylinder sits with its centre at this Y so the base
        // rests on the deck top surface and the top is at deck_top +
        // mast_h. Used as the mast's `translation.y`.
        let mast_origin_y = 0.5 * deck_y + mast_h * 0.5;

        let metal_mat = |color: [f32; 3]| SovereignMaterialSettings {
            base_color: Fp3(color),
            metallic: Fp(0.4),
            roughness: Fp(0.45),
            ..Default::default()
        };
        let brass_mat = |color: [f32; 3]| SovereignMaterialSettings {
            base_color: Fp3(color),
            metallic: Fp(0.7),
            roughness: Fp(0.35),
            ..Default::default()
        };
        let glow_mat = |color: [f32; 3]| SovereignMaterialSettings {
            base_color: Fp3(color),
            metallic: Fp(0.4),
            roughness: Fp(0.40),
            emission_color: Fp3(color),
            emission_strength: Fp(5.0),
            ..Default::default()
        };

        // Identity rotation reused in every transform that doesn't
        // turn its child.
        let id_quat = quat_xyzw([0.0, 0.0, 0.0, 1.0]);
        let pontoon_lay_quat = quat_xyzw(quat_x(std::f32::consts::FRAC_PI_2));

        // ---- Catamaran hulls (two horizontal capsules along Z) ----
        let make_hull = |x: f32| Generator {
            kind: GeneratorKind::Capsule {
                radius: Fp(hull_r),
                length: Fp(hull_len),
                latitudes: 8,
                longitudes: 16,
                solid: false,
                material: metal_mat(hull_color),
                twist: Fp(0.0),
                taper: Fp(0.0),
                bend: Fp3([0.0, 0.0, 0.0]),
            },
            transform: TransformData {
                translation: Fp3([x, hull_y, 0.0]),
                rotation: pontoon_lay_quat,
                scale: Fp3([1.0, 1.0, 1.0]),
            },
            children: Vec::new(),
        };

        // ---- Mast subtree -----------------------------------------------
        let mut mast_children: Vec<Generator> = Vec::new();
        // Glowing finial on top.
        mast_children.push(Generator {
            kind: GeneratorKind::Sphere {
                radius: Fp(0.10 * head),
                resolution: 3,
                solid: false,
                material: glow_mat(jewel_color),
                twist: Fp(0.0),
                taper: Fp(0.0),
                bend: Fp3([0.0, 0.0, 0.0]),
            },
            transform: TransformData {
                translation: Fp3([0.0, mast_h * 0.5, 0.0]),
                rotation: id_quat,
                scale: Fp3([1.0, 1.0, 1.0]),
            },
            children: Vec::new(),
        });
        // Antenna (Solar / Hybrid only): thin spire above the finial.
        if vessel.archetype.has_antenna() {
            let antenna_h = 0.45 * mast_h;
            mast_children.push(Generator {
                kind: GeneratorKind::Cylinder {
                    radius: Fp(0.015 * limb),
                    height: Fp(antenna_h),
                    resolution: 8,
                    solid: false,
                    material: brass_mat(metal_color),
                    twist: Fp(0.0),
                    taper: Fp(0.0),
                    bend: Fp3([0.0, 0.0, 0.0]),
                },
                transform: TransformData {
                    translation: Fp3([0.0, mast_h * 0.5 + antenna_h * 0.5 + 0.08, 0.0]),
                    rotation: id_quat,
                    scale: Fp3([1.0, 1.0, 1.0]),
                },
                children: Vec::new(),
            });
        }
        // Flag — Sign panel showing the owner's bsky profile picture.
        // The Sign mesh is a plane in local XZ (normal +Y); rotating
        // the parent transform by π/2 around Z stands it up in YZ
        // (normal ±X), so the panel is visible from the boat's left
        // and right sides like a heraldic banner. `double_sided`
        // makes both views render. `unlit` keeps the pfp legible
        // regardless of sun angle.
        let flag_height = 0.55;
        let flag_width = 0.40;
        mast_children.push(Generator {
            kind: GeneratorKind::Sign {
                source: SignSource::DidPfp {
                    did: did.to_owned(),
                },
                size: Fp2([flag_height, flag_width]),
                uv_repeat: Fp2([1.0, 1.0]),
                uv_offset: Fp2([0.0, 0.0]),
                material: SovereignMaterialSettings {
                    base_color: Fp3([1.0, 1.0, 1.0]),
                    roughness: Fp(0.6),
                    metallic: Fp(0.0),
                    ..Default::default()
                },
                double_sided: true,
                alpha_mode: AlphaModeKind::Opaque,
                unlit: true,
            },
            transform: TransformData {
                translation: Fp3([0.0, mast_h * 0.2, flag_width * 0.5 + 0.05]),
                rotation: quat_xyzw(quat_z(std::f32::consts::FRAC_PI_2)),
                scale: Fp3([1.0, 1.0, 1.0]),
            },
            children: Vec::new(),
        });

        let mast = Generator {
            kind: GeneratorKind::Cylinder {
                radius: Fp(mast_r),
                height: Fp(mast_h),
                resolution: 16,
                solid: false,
                material: metal_mat(mast_color),
                twist: Fp(0.0),
                taper: Fp(0.0),
                bend: Fp3([0.0, 0.0, 0.0]),
            },
            transform: TransformData {
                translation: Fp3([0.0, mast_origin_y, 0.0]),
                rotation: id_quat,
                scale: Fp3([1.0, 1.0, 1.0]),
            },
            children: mast_children,
        };

        // ---- Bow ornament (conditional on BowStyle) ---------------------
        let bow_z = -deck_half_z - 0.10; // just past the deck front edge
        let bow_y = 0.5 * deck_y + 0.05;
        let bow_ornament: Option<Generator> = match vessel.bow_style {
            BowStyle::Spike => Some(Generator {
                kind: GeneratorKind::Cone {
                    radius: Fp(0.06 * vessel.bow_scale),
                    height: Fp(0.30 * vessel.bow_scale),
                    resolution: 12,
                    solid: false,
                    material: brass_mat(metal_color),
                    twist: Fp(0.0),
                    taper: Fp(0.0),
                    bend: Fp3([0.0, 0.0, 0.0]),
                },
                transform: TransformData {
                    translation: Fp3([0.0, bow_y + 0.05, bow_z]),
                    // Bevy `Cone` axis is +Y; rotate around X by
                    // -π/2 to point the apex along -Z (forward).
                    rotation: quat_xyzw(quat_x(-std::f32::consts::FRAC_PI_2)),
                    scale: Fp3([1.0, 1.0, 1.0]),
                },
                children: Vec::new(),
            }),
            BowStyle::Sphere => Some(Generator {
                kind: GeneratorKind::Sphere {
                    radius: Fp(0.10 * vessel.bow_scale),
                    resolution: 3,
                    solid: false,
                    material: glow_mat(jewel_color),
                    twist: Fp(0.0),
                    taper: Fp(0.0),
                    bend: Fp3([0.0, 0.0, 0.0]),
                },
                transform: TransformData {
                    translation: Fp3([0.0, bow_y, bow_z]),
                    rotation: id_quat,
                    scale: Fp3([1.0, 1.0, 1.0]),
                },
                children: Vec::new(),
            }),
            BowStyle::Beak => Some(Generator {
                kind: GeneratorKind::Cone {
                    radius: Fp(0.10 * vessel.bow_scale),
                    height: Fp(0.50 * vessel.bow_scale),
                    resolution: 12,
                    solid: false,
                    material: brass_mat(metal_color),
                    twist: Fp(0.0),
                    taper: Fp(0.0),
                    bend: Fp3([0.0, 0.0, 0.0]),
                },
                transform: TransformData {
                    translation: Fp3([0.0, bow_y, bow_z - 0.10]),
                    rotation: quat_xyzw(quat_x(-std::f32::consts::FRAC_PI_2)),
                    scale: Fp3([1.0, 1.0, 1.0]),
                },
                children: Vec::new(),
            }),
            BowStyle::None => None,
        };

        // ---- Smokestacks (Steam / Hybrid) -------------------------------
        // Symmetric placement around the deck stern. 1 → centred; 2 →
        // ±0.25 X; 3 → centre + ±0.30 X. The visual variety comes
        // from per-vessel count + height jitter (`smokestack_count`,
        // `smokestack_height_scale`) so two Steam vessels still read
        // as distinct configurations.
        let mut smokestacks: Vec<Generator> = Vec::new();
        if vessel.archetype.has_smokestacks() && vessel.smokestack_count > 0 {
            let stack_radius = 0.055 * limb;
            let stack_height = 0.40 * h * vessel.smokestack_height_scale;
            let stack_y = 0.5 * deck_y + stack_height * 0.5;
            let stack_z = deck_half_z - 0.30;
            let xs: &[f32] = match vessel.smokestack_count {
                1 => &[0.0],
                2 => &[-0.25, 0.25],
                _ => &[0.0, -0.30, 0.30],
            };
            for x in xs {
                smokestacks.push(Generator {
                    kind: GeneratorKind::Cylinder {
                        radius: Fp(stack_radius),
                        height: Fp(stack_height),
                        resolution: 12,
                        solid: false,
                        material: brass_mat(metal_color),
                        twist: Fp(0.0),
                        taper: Fp(0.0),
                        bend: Fp3([0.0, 0.0, 0.0]),
                    },
                    transform: TransformData {
                        translation: Fp3([*x * w, stack_y, stack_z]),
                        rotation: id_quat,
                        scale: Fp3([1.0, 1.0, 1.0]),
                    },
                    children: Vec::new(),
                });
            }
        }

        // ---- Solar panel (Solar / Hybrid) -------------------------------
        let solar_panel: Option<Generator> = if vessel.archetype.has_solar_panel() {
            Some(Generator {
                kind: GeneratorKind::Cuboid {
                    size: Fp3([0.65 * w, 0.03, 0.75 * h]),
                    solid: false,
                    material: brass_mat(metal_color),
                    twist: Fp(0.0),
                    taper: Fp(0.0),
                    bend: Fp3([0.0, 0.0, 0.0]),
                },
                transform: TransformData {
                    translation: Fp3([0.0, 0.5 * deck_y + 0.18, 0.25 * h]),
                    rotation: quat_xyzw(quat_x(vessel.solar_panel_tilt_rad)),
                    scale: Fp3([1.0, 1.0, 1.0]),
                },
                children: Vec::new(),
            })
        } else {
            None
        };

        // ---- Assemble the deck root and its children --------------------
        let mut children: Vec<Generator> = Vec::with_capacity(8);
        children.push(make_hull(-hull_x));
        children.push(make_hull(hull_x));
        if let Some(b) = bow_ornament {
            children.push(b);
        }
        children.push(mast);
        children.extend(smokestacks);
        if let Some(p) = solar_panel {
            children.push(p);
        }

        let deck = Generator {
            kind: GeneratorKind::Cuboid {
                size: Fp3([deck_x, deck_y, deck_z]),
                solid: false,
                material: metal_mat(deck_color),
                twist: Fp(0.0),
                taper: Fp(0.0),
                bend: Fp3([0.0, 0.0, 0.0]),
            },
            transform: TransformData::default(),
            children,
        };

        Self {
            lex_type: AVATAR_COLLECTION.into(),
            visuals: deck,
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

/// Build a normalised quaternion `[x, y, z, w]` from a half-angle rotation
/// around the X axis. Used by [`AvatarRecord::default_for_did`] to lay
/// hull capsules on their side and to point bow-ornament cones forward.
fn quat_x(angle_rad: f32) -> [f32; 4] {
    let half = angle_rad * 0.5;
    [half.sin(), 0.0, 0.0, half.cos()]
}

/// Sister of [`quat_x`] for rotations around the Z axis. Used by the
/// avatar default's flag panel to stand the Sign plane up in YZ
/// (normal along ±X) so the bsky pfp reads as a heraldic banner
/// from the boat's left and right.
fn quat_z(angle_rad: f32) -> [f32; 4] {
    let half = angle_rad * 0.5;
    [0.0, 0.0, half.sin(), half.cos()]
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
