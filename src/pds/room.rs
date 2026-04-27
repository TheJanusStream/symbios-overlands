//! The root `RoomRecord` recipe plus its atmospheric `Environment` payload,
//! the deterministic `find_terrain_config` lookup shared across peers, and
//! the XRPC wrappers that fetch / publish / delete / reset the record on the
//! owner's PDS.

use super::COLLECTION;
use super::generator::{Generator, GeneratorKind, Placement, WaterSurface};
use super::sanitize::{limits, sanitize_generator};
use super::terrain::SovereignTerrainConfig;
use super::types::{Fp, Fp2, Fp3, Fp4, TransformData};
use super::xrpc::{FetchError, PutOutcome, XrpcError, resolve_pds};
use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Non-spatial environment state — directional sun, ambient light, sky
/// cuboid tint, and atmospheric distance fog. Every field is wrapped in a
/// fixed-point type so the record stays DAG-CBOR compliant.
///
/// `#[serde(default)]` lets pre-atmosphere records (which only carried
/// `sun_color`) round-trip: any missing field falls back to the canonical
/// constant via `Environment::default()` rather than failing the whole
/// decode and stranding the owner on the recovery banner.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Environment {
    pub sun_color: Fp3,
    pub sun_illuminance: Fp,
    pub ambient_brightness: Fp,
    pub sky_color: Fp3,

    pub fog_color: Fp4,
    pub fog_visibility: Fp,
    pub fog_extinction: Fp3,
    pub fog_inscattering: Fp3,
    pub fog_sun_color: Fp4,
    pub fog_sun_exponent: Fp,

    /// Tiling frequency for the close-distance scrolling detail normal map
    /// (world-unit reciprocal — higher = tighter tiling). Pairs with
    /// [`Self::water_normal_scale_far`] to kill the repeating-grid look on
    /// long camera sightlines.
    pub water_normal_scale_near: Fp,
    /// Tiling frequency for the far-distance scrolling detail normal map.
    pub water_normal_scale_far: Fp,
    /// Screen-space refraction distortion strength. Currently reserved — the
    /// shader honours the value but falls back to a no-op when no depth
    /// prepass is wired. Kept on the record so the field is stable.
    pub water_refraction_strength: Fp,
    /// Intensity of the sharp specular sun-glitter highlight on the water
    /// surface. `0` disables; ~2.0 is a pleasing default.
    pub water_sun_glitter: Fp,
    /// sRGB tint added to wave crests to simulate cheap subsurface scatter.
    pub water_scatter_color: Fp3,
    /// Width (m) of the procedural shoreline foam band. Reserved — requires
    /// a depth prepass to resolve shoreline distance; stored on the record
    /// so the UI field is stable across the feature's rollout.
    pub water_shore_foam_width: Fp,

    // ---- Cloud-deck (procedural FBM layer; see `crate::clouds`) -----------
    /// Fraction of sky covered by clouds. `0` = empty blue, `1` = totally
    /// overcast.
    pub cloud_cover: Fp,
    /// Opacity multiplier for the clouds that survive the cover threshold.
    pub cloud_density: Fp,
    /// Edge-softness band around the cover threshold. Larger ⇒ wispier.
    pub cloud_softness: Fp,
    /// Drift speed (m/s) along [`Self::cloud_wind_dir`].
    pub cloud_speed: Fp,
    /// World metres per UV unit for the cloud noise sampler.
    pub cloud_scale: Fp,
    /// Altitude (m) of the cloud-deck plane.
    pub cloud_height: Fp,
    /// 2D wind direction in world XZ. Need not be unit length — the shader
    /// normalises a small epsilon-padded copy.
    pub cloud_wind_dir: Fp2,
    /// sRGB tint for the sunlit top of the cloud layer.
    pub cloud_color: Fp3,
    /// sRGB tint for the underside / shadowed regions, mixed with
    /// [`Self::cloud_color`] by the dot of the sun direction with world Y.
    pub cloud_shadow_color: Fp3,
}

impl Default for Environment {
    fn default() -> Self {
        use crate::config::{
            camera::fog as f, lighting as l, lighting::clouds as c, terrain::water as w,
        };
        Self {
            sun_color: Fp3(l::SUN_COLOR),
            sun_illuminance: Fp(l::ILLUMINANCE),
            ambient_brightness: Fp(l::AMBIENT_BRIGHTNESS),
            sky_color: Fp3(l::SKY_COLOR),

            fog_color: Fp4(f::COLOR),
            fog_visibility: Fp(f::VISIBILITY),
            fog_extinction: Fp3(f::EXTINCTION_COLOR),
            fog_inscattering: Fp3(f::INSCATTERING_COLOR),
            fog_sun_color: Fp4(f::DIRECTIONAL_LIGHT_COLOR),
            fog_sun_exponent: Fp(f::DIRECTIONAL_LIGHT_EXPONENT),

            water_normal_scale_near: Fp(w::DEFAULT_NORMAL_SCALE_NEAR),
            water_normal_scale_far: Fp(w::DEFAULT_NORMAL_SCALE_FAR),
            water_refraction_strength: Fp(w::DEFAULT_REFRACTION_STRENGTH),
            water_sun_glitter: Fp(w::DEFAULT_SUN_GLITTER),
            water_scatter_color: Fp3(w::DEFAULT_SCATTER_COLOR),
            water_shore_foam_width: Fp(w::DEFAULT_SHORE_FOAM_WIDTH),

            cloud_cover: Fp(c::COVER),
            cloud_density: Fp(c::DENSITY),
            cloud_softness: Fp(c::SOFTNESS),
            cloud_speed: Fp(c::SPEED),
            cloud_scale: Fp(c::SCALE),
            cloud_height: Fp(c::HEIGHT),
            cloud_wind_dir: Fp2(c::WIND_DIR),
            cloud_color: Fp3(c::COLOR),
            cloud_shadow_color: Fp3(c::SHADOW_COLOR),
        }
    }
}

impl Environment {
    /// Clamp every field so a malicious or malformed record cannot crash
    /// the renderer with NaN, negative light values, or a zero visibility
    /// that makes `FogFalloff::from_visibility_colors` divide by zero.
    pub fn sanitize(&mut self) {
        let clamp_unit = |v: f32| v.clamp(0.0, 1.0);
        let clamp3 = |c: Fp3| Fp3([clamp_unit(c.0[0]), clamp_unit(c.0[1]), clamp_unit(c.0[2])]);
        let clamp4 = |c: Fp4| {
            Fp4([
                clamp_unit(c.0[0]),
                clamp_unit(c.0[1]),
                clamp_unit(c.0[2]),
                clamp_unit(c.0[3]),
            ])
        };

        self.sun_color = clamp3(self.sun_color);
        self.sky_color = clamp3(self.sky_color);
        self.fog_color = clamp4(self.fog_color);
        self.fog_extinction = clamp3(self.fog_extinction);
        self.fog_inscattering = clamp3(self.fog_inscattering);
        self.fog_sun_color = clamp4(self.fog_sun_color);

        self.sun_illuminance = Fp(self.sun_illuminance.0.clamp(0.0, 100_000.0));
        self.ambient_brightness = Fp(self.ambient_brightness.0.clamp(0.0, 10_000.0));
        // A zero visibility would make `FogFalloff::from_visibility_colors`
        // blow up (it divides by `visibility` internally). Floor at 10 m so
        // the falloff remains well-defined even under an adversarial record.
        self.fog_visibility = Fp(self.fog_visibility.0.clamp(10.0, 10_000.0));
        self.fog_sun_exponent = Fp(self.fog_sun_exponent.0.clamp(1.0, 100.0));

        // Water-environment fields. Keep every channel in a finite,
        // physically-sane range — a NaN or negative normal-tiling scale
        // would poison the water shader's UV math every frame.
        let clamp_finite_pos = |v: f32, lo: f32, hi: f32, default: f32| -> f32 {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                default
            }
        };
        self.water_normal_scale_near = Fp(clamp_finite_pos(
            self.water_normal_scale_near.0,
            0.0,
            64.0,
            0.85,
        ));
        self.water_normal_scale_far = Fp(clamp_finite_pos(
            self.water_normal_scale_far.0,
            0.0,
            64.0,
            0.08,
        ));
        self.water_refraction_strength = Fp(clamp_finite_pos(
            self.water_refraction_strength.0,
            0.0,
            4.0,
            0.0,
        ));
        self.water_sun_glitter = Fp(clamp_finite_pos(self.water_sun_glitter.0, 0.0, 16.0, 1.8));
        self.water_scatter_color = clamp3(self.water_scatter_color);
        self.water_shore_foam_width = Fp(clamp_finite_pos(
            self.water_shore_foam_width.0,
            0.0,
            50.0,
            0.0,
        ));

        // Cloud-deck fields. Same NaN / range guarding as water — the cloud
        // shader divides by `cloud_scale` and reads `cloud_height` straight
        // into a `Transform.translation.y`, so a poisoned record must not
        // be allowed to feed Inf or negative values into either.
        self.cloud_cover = Fp(clamp_finite_pos(self.cloud_cover.0, 0.0, 1.0, 0.45));
        self.cloud_density = Fp(clamp_finite_pos(self.cloud_density.0, 0.0, 1.0, 0.85));
        self.cloud_softness = Fp(clamp_finite_pos(self.cloud_softness.0, 0.001, 1.0, 0.18));
        self.cloud_speed = Fp(clamp_finite_pos(self.cloud_speed.0, 0.0, 200.0, 4.0));
        self.cloud_scale = Fp(clamp_finite_pos(self.cloud_scale.0, 1.0, 10_000.0, 320.0));
        self.cloud_height = Fp(clamp_finite_pos(self.cloud_height.0, 5.0, 10_000.0, 250.0));
        let wd = self.cloud_wind_dir.0;
        let wd0 = if wd[0].is_finite() {
            wd[0].clamp(-100.0, 100.0)
        } else {
            1.0
        };
        let wd1 = if wd[1].is_finite() {
            wd[1].clamp(-100.0, 100.0)
        } else {
            0.3
        };
        // Reject the zero vector — the shader normalises wind_dir and a
        // bit-for-bit zero would NaN-out the noise sampling. A vanishingly
        // small magnitude falls back to the canonical default.
        let mag2 = wd0 * wd0 + wd1 * wd1;
        self.cloud_wind_dir = if mag2 > 1.0e-6 {
            Fp2([wd0, wd1])
        } else {
            Fp2([1.0, 0.3])
        };
        self.cloud_color = clamp3(self.cloud_color);
        self.cloud_shadow_color = clamp3(self.cloud_shadow_color);
    }
}

/// The full recipe: environment + generators + placements + traits. Acts as
/// a Bevy `Resource` so `world_builder.rs` can compile it into ECS entities.
#[derive(Serialize, Deserialize, Clone, Debug, Resource)]
pub struct RoomRecord {
    #[serde(rename = "$type")]
    pub lex_type: String,
    pub environment: Environment,
    pub generators: HashMap<String, Generator>,
    pub placements: Vec<Placement>,
    /// Maps a generator name to a list of trait strings (e.g.
    /// `"collider_heightfield"`, `"sensor"`) the world compiler should attach
    /// to every entity that generator spawns.
    pub traits: HashMap<String, Vec<String>>,
}

impl RoomRecord {
    /// Zero-configuration homeworld. When a client visits a DID whose owner
    /// has never saved a custom record, this builds the canonical default
    /// recipe on the fly — a base terrain plus a base water plane — so the
    /// world builder always has something valid to compile.
    pub fn default_for_did(did: &str) -> Self {
        // Synthesise a per-owner terrain seed from the DID so every freshly
        // visited overland has unique topography without requiring the owner
        // to touch their record. FNV-1a 64-bit — deterministic across peers.
        let did_seed = {
            let mut hash: u64 = 0xcbf29ce484222325;
            for byte in did.bytes() {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
            hash
        };
        let terrain_cfg = SovereignTerrainConfig {
            seed: did_seed,
            ..SovereignTerrainConfig::default()
        };

        // Strict scheme: a single named generator describes the whole
        // region. Terrain sits at the root (only valid position for
        // Terrain) and the room's water is a child of it (only valid
        // position for Water). Saving `base_terrain` to inventory now
        // captures the entire homeworld — heightmap + water — as one
        // portable blueprint.
        let mut base_region = Generator::from_kind(GeneratorKind::Terrain(terrain_cfg));
        base_region
            .children
            .push(Generator::from_kind(GeneratorKind::Water {
                level_offset: Fp(0.0),
                surface: WaterSurface::default(),
            }));

        let mut generators = HashMap::new();
        generators.insert("base_terrain".to_string(), base_region);

        let placements = vec![Placement::Absolute {
            generator_ref: "base_terrain".to_string(),
            transform: TransformData::default(),
            snap_to_terrain: false,
        }];

        let mut traits = HashMap::new();
        traits.insert(
            "base_terrain".to_string(),
            vec!["collider_heightfield".to_string(), "ground".to_string()],
        );

        Self {
            lex_type: COLLECTION.into(),
            environment: Environment::default(),
            generators,
            placements,
            traits,
        }
    }

    /// Clamp every numeric field to a safe upper bound. Every path that
    /// accepts a `RoomRecord` from the network (PDS fetch and peer-broadcast
    /// `RoomStateUpdate`) calls this before handing the record to the world
    /// compiler, so an attacker cannot weaponise an unbounded field to crash
    /// or OOM the victim.
    pub fn sanitize(&mut self) {
        // Clamp atmospheric fields first — cheap and independent of everything
        // else, and guarantees the world compiler never hands NaN or a zero
        // visibility to `FogFalloff::from_visibility_colors`.
        self.environment.sanitize();
        // Bound the total number of generators before touching any of them.
        // Drop entries in lexicographic key order so the survivor set is
        // deterministic across peers — otherwise a record with 1000
        // generators and `MAX_GENERATORS = 256` would resolve to a
        // different 256 on every client (HashMap iteration is SipHash
        // randomised) and fracture the shared world.
        if self.generators.len() > limits::MAX_GENERATORS {
            let mut keys: Vec<String> = self.generators.keys().cloned().collect();
            keys.sort();
            for key in keys.into_iter().skip(limits::MAX_GENERATORS) {
                self.generators.remove(&key);
            }
        }
        // Snapshot the names of generators whose root kind is Terrain or
        // Water *before* `sanitize_generator` rewrites them. Any
        // `Scatter`/`Grid` placement targeting one of these is positionally
        // invalid: a Scatter of a Terrain root would spawn duplicate
        // heightfield colliders (Avian forbids that), and Water can never
        // legally be a root. We capture the snapshot first because the
        // generator pass overwrites root Water with a default cuboid — if
        // we filtered after, a Scatter pointing at the now-cuboid would
        // silently spawn N copies of an unrelated shape instead of being
        // dropped outright.
        let ineligible_targets: std::collections::HashSet<String> = self
            .generators
            .iter()
            .filter(|(_, g)| {
                matches!(
                    g.kind,
                    GeneratorKind::Terrain(_) | GeneratorKind::Water { .. }
                )
            })
            .map(|(name, _)| name.clone())
            .collect();
        for generator in self.generators.values_mut() {
            sanitize_generator(generator);
        }
        // Drop offending Scatter/Grid placements before applying the
        // count cap, so 1024 ineligible entries can't push valid ones
        // past `MAX_PLACEMENTS`. Absolute is left alone — pointing it
        // at a Terrain root is the canonical home-world placement, and
        // a hostile Water-rooted Absolute is already neutralised by
        // the generator-level overwrite above.
        self.placements.retain(|p| match p {
            Placement::Scatter { generator_ref, .. } | Placement::Grid { generator_ref, .. } => {
                !ineligible_targets.contains(generator_ref)
            }
            _ => true,
        });
        // Drop excess placements so a 1M-entry array can't force
        // `compile_room_record` to spawn tens of millions of entities in
        // a single frame. Keeping a prefix is order-stable (serde
        // round-trips `Vec` in order) so every peer truncates to the
        // same survivor set.
        if self.placements.len() > limits::MAX_PLACEMENTS {
            self.placements.truncate(limits::MAX_PLACEMENTS);
        }
        for placement in self.placements.iter_mut() {
            match placement {
                Placement::Scatter { count, .. } => {
                    *count = (*count).min(limits::MAX_SCATTER_COUNT);
                }
                Placement::Grid { counts, gaps, .. } => {
                    counts[0] = counts[0].clamp(1, 100);
                    counts[1] = counts[1].clamp(1, 100);
                    counts[2] = counts[2].clamp(1, 100);
                    let total = (counts[0] as usize)
                        .saturating_mul(counts[1] as usize)
                        .saturating_mul(counts[2] as usize);
                    if total > 10_000 {
                        counts[0] = counts[0].min(21);
                        counts[1] = counts[1].min(21);
                        counts[2] = counts[2].min(21);
                    }
                    gaps.0[0] = gaps.0[0].clamp(0.01, 1000.0);
                    gaps.0[1] = gaps.0[1].clamp(0.01, 1000.0);
                    gaps.0[2] = gaps.0[2].clamp(0.01, 1000.0);
                }
                _ => {}
            }
        }
    }
}

impl Default for RoomRecord {
    fn default() -> Self {
        Self::default_for_did("")
    }
}

/// Return the terrain generator with the lexicographically smallest key.
///
/// `HashMap::values()` iteration order is randomised per execution (SipHash),
/// so a record with more than one `Generator::Terrain` entry would otherwise
/// have every client picking a different one and landing on a different
/// heightmap — instantly fracturing the shared world. Every site that needs
/// "the terrain" for a record must go through this function (or its sibling)
/// so the choice is deterministic across peers.
pub fn find_terrain_config(record: &RoomRecord) -> Option<&SovereignTerrainConfig> {
    let mut keys: Vec<&String> = record.generators.keys().collect();
    keys.sort();
    for k in keys {
        if let Some(generator) = record.generators.get(k)
            && let GeneratorKind::Terrain(cfg) = &generator.kind
        {
            return Some(cfg);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Read: fetch room record from the room owner's PDS
// ---------------------------------------------------------------------------

/// Wrapper for the `getRecord` XRPC response.
#[derive(Deserialize)]
struct GetRecordResponse {
    value: RoomRecord,
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
    let wrapper: GetRecordResponse = resp
        .json()
        .await
        .map_err(|e| FetchError::Decode(e.to_string()))?;
    let mut record = wrapper.value;
    record.sanitize();
    Ok(Some(record))
}

// ---------------------------------------------------------------------------
// Write: publish room record to the authenticated user's PDS
// ---------------------------------------------------------------------------

/// Payload for `com.atproto.repo.putRecord`.
#[derive(Serialize)]
struct PutRecordRequest<'a> {
    repo: &'a str,
    collection: &'a str,
    rkey: &'a str,
    record: &'a RoomRecord,
}

async fn try_put_record(
    _client: &reqwest::Client,
    pds: &str,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    record: &RoomRecord,
) -> PutOutcome {
    let url = format!("{}/xrpc/com.atproto.repo.putRecord", pds);
    let body = PutRecordRequest {
        repo: &session.did,
        collection: COLLECTION,
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
    let msg = format!("putRecord failed: {} — {}", status, body);
    if status.is_server_error() {
        PutOutcome::ServerError(msg)
    } else {
        PutOutcome::ClientError(msg)
    }
}

/// Write (upsert) the room record to the authenticated user's own PDS.
///
/// Tries `com.atproto.repo.putRecord` first (the fast-path upsert). If the
/// PDS responds with a `5xx`, some implementations are choking on their
/// own update-diff logic against a stale or incompatible stored CID — we
/// recover by transparently falling back to `delete_room_record` followed
/// by a fresh `putRecord`. Client (`4xx`) errors are surfaced directly
/// because retrying won't help.
pub async fn publish_room_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    record: &RoomRecord,
) -> Result<(), String> {
    let pds = resolve_pds(client, &session.did)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;

    match try_put_record(client, &pds, session, refresh, record).await {
        PutOutcome::Ok => Ok(()),
        PutOutcome::ClientError(msg) => Err(msg),
        PutOutcome::Transport(msg) => Err(msg),
        PutOutcome::ServerError(first_err) => {
            // Fall back to the hard-reset path. This recovers the common
            // failure mode where the PDS's putRecord update path crashes on
            // a stale CID/commit but can still handle a fresh create.
            warn!("{first_err} — retrying via delete_room_record + putRecord");
            delete_room_record(client, session, refresh)
                .await
                .map_err(|e| format!("{first_err}; fallback delete failed: {e}"))?;
            match try_put_record(client, &pds, session, refresh, record).await {
                PutOutcome::Ok => Ok(()),
                PutOutcome::ClientError(m)
                | PutOutcome::ServerError(m)
                | PutOutcome::Transport(m) => Err(format!("{first_err}; fallback put failed: {m}")),
            }
        }
    }
}

/// Payload for `com.atproto.repo.deleteRecord`.
#[derive(Serialize)]
struct DeleteRecordRequest<'a> {
    repo: &'a str,
    collection: &'a str,
    rkey: &'a str,
}

/// Delete the room record from the authenticated user's PDS. A 404 response
/// is reported as `Ok(())` because the caller usually just wants to know the
/// row is gone — whether it was never there or just removed is immaterial.
pub async fn delete_room_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
) -> Result<(), String> {
    let pds = resolve_pds(client, &session.did)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;

    let url = format!("{}/xrpc/com.atproto.repo.deleteRecord", pds);
    let body = DeleteRecordRequest {
        repo: &session.did,
        collection: COLLECTION,
        rkey: "self",
    };

    let body_json = serde_json::to_value(&body).map_err(|e| e.to_string())?;
    let (status, body) =
        crate::oauth::oauth_post_with_refresh(&session.session, refresh, &url, &body_json).await?;

    if status.is_success() || status.as_u16() == 404 {
        Ok(())
    } else {
        Err(format!("deleteRecord failed: {} — {}", status, body))
    }
}

/// Force-overwrite the room record by deleting first, then creating fresh.
///
/// The plain `putRecord` upsert path can trip on an incompatible stored
/// record: some PDS implementations try to diff the prior CID and return
/// `500 InternalServerError` when the old blob can't be validated against
/// the current lexicon. Deleting first gives the PDS a clean slate, so the
/// subsequent create is a simple new-record path with no diff logic.
pub async fn reset_room_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    record: &RoomRecord,
) -> Result<(), String> {
    delete_room_record(client, session, refresh).await?;
    publish_room_record(client, session, refresh, record).await
}
