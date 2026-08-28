//! Avatar record — player vessel / body definition.
//!
//! Each player's avatar is published to their own PDS at
//! `collection = network.symbios.overlands.avatar, rkey = self`. The record
//! is split into disjoint halves:
//!
//!   - `body` — which body the avatar wears (#1056), as the open union
//!     [`AvatarBody`]: a **rigged** engine body referenced out of the
//!     cross-app wardrobe (see [`wardrobe`]) plus worn attachment records,
//!     or a **generator** body — a hierarchical
//!     [`super::generator::Generator`] tree (cuboids, capsules, lsystems, …)
//!     using identical machinery to room generators, with avatar-specific
//!     allowed kinds enforced by
//!     [`super::sanitize::sanitize_avatar_visuals`] (no Terrain/Water/
//!     Portal). Remote peers render this.
//!   - `locomotion` — a tagged-union [`LocomotionConfig`] selecting one of
//!     five physics presets (HoverBoat / Humanoid / Airplane / Helicopter /
//!     Car), each carrying its own collider dimensions + tuning. Remote
//!     peers *deserialize but ignore* this — only the local player's
//!     locomotion drives the rigid body.
//!   - `gait` — optional idle-motion tuning for generator bodies.
//!
//! Locomotion presets live in the [`locomotion`] submodule, one file per
//! preset; each parameter struct impls
//! [`locomotion::LocomotionPreset`] so the central enum's `kind_tag`,
//! `display_label`, `sanitize`, and `pickers` dispatch through the trait
//! rather than a hand-maintained `match` ladder.
//!
//! There is no automatic migration, for any generation of this schema: a
//! pre-#1056 record (whose looks lived in a `visuals` field) decodes with
//! [`AvatarBody::Absent`] and the fetch path treats it as "no record",
//! falling through to the cross-app profile and then to
//! [`AvatarRecord::default_for_did`]. Older still — the legacy
//! `network.symbios.avatar.hover_rover` / `…humanoid` era — lands the same
//! place. Old records require a manual republish.

pub mod body;
pub mod default_visuals;
pub mod gait;
pub mod locomotion;
pub mod parts;
pub mod wardrobe;

pub use body::{
    AvatarBody, GeneratorBody, MAX_AVATAR_ATTACHMENTS, ResolvedAttachment, ResolvedRig, RiggedBody,
};
pub use gait::GaitParams;
pub use locomotion::{
    AirplaneParams, CarParams, HelicopterParams, HoverBoatParams, HumanoidParams, LocomotionConfig,
    LocomotionPickerEntry, LocomotionPreset,
};
pub use wardrobe::{AttachmentRecord, EngineAvatarRecord, EngineProfileRecord};

use super::AVATAR_COLLECTION;
use super::xrpc::{FetchError, XrpcError, decode_record_json, resolve_pds};
use bevy::prelude::*;
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
    /// Which body the avatar wears (#1056): a rigged engine body by
    /// wardrobe reference, or a `Generator` tree. Field-level default so a
    /// pre-#1056 record (which has `visuals` instead) decodes to
    /// [`AvatarBody::Absent`] and the fetch path treats it as "no record" —
    /// the standing no-automatic-migration rule.
    #[serde(default)]
    pub body: AvatarBody,
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
        let (body, locomotion) = default_visuals::build_for_seed(seed);
        Self {
            lex_type: AVATAR_COLLECTION.into(),
            body,
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
        self.body.sanitize();
        self.locomotion.sanitize();
        if let Some(gait) = &mut self.gait {
            gait.sanitize();
        }
    }

    /// Whether this record would publish differently from `other` (#1059).
    ///
    /// [`crate::state::records_differ`] compares serialised forms, which is
    /// right for every other record — but a rigged body's payload lives on
    /// `resolved`, which is deliberately `serde(skip)`. Sculpting a body or
    /// nudging a prop's offset would therefore look *clean* to a plain
    /// serde compare, and the Save button would sit disabled over unsaved
    /// work. This asks the wire question about the record and a value
    /// question about the resolution the bundle publishes alongside it.
    pub fn publishes_differently_from(&self, other: &Self) -> bool {
        if crate::state::records_differ(self, other) {
            return true;
        }
        let resolved = |record: &Self| {
            record
                .body
                .rigged_ref()
                .and_then(|rig| rig.resolved.clone())
        };
        resolved(self) != resolved(other)
    }

    /// A record wearing the identity's cross-app default body (#1056): the
    /// fallback for an identity with no overlands avatar record but a
    /// wardrobe published by another symbios application. Locomotion is the
    /// humanoid preset — a rigged body walks — and gait is `None` because a
    /// skinned body's motion comes from the engine's procedural layer, not
    /// the generator-visual bobber.
    pub fn wearing(wardrobe_rkey: impl Into<String>) -> Self {
        Self {
            lex_type: AVATAR_COLLECTION.into(),
            body: AvatarBody::rigged(wardrobe_rkey),
            locomotion: locomotion::HumanoidParams::default_config(),
            gait: None,
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

/// Fetch a player's avatar record from their PDS, resolving a rigged body's
/// wardrobe + attachment references in the same pass (#1056). Result
/// semantics mirror [`super::fetch_room_record`]: `Ok(None)` is a clean
/// "nothing anywhere" (no record, no cross-app profile either) and the
/// caller synthesises the seeded default; any other failure returns an `Err`
/// the caller distinguishes so it does not silently overwrite a live record
/// with the default.
///
/// Two shapes fall through to the profile lookup: a true 404, and a record
/// that decodes with [`AvatarBody::Absent`] — the pre-#1056 schema, which
/// this module's standing rule leaves unmigrated. An identity that published
/// a wardrobe through another symbios app comes back as a record
/// [`AvatarRecord::wearing`] their profile's default body.
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
        return fallback_from_profile(client, &pds, did).await;
    }
    if !status.is_success() {
        // Capped (#1124): peer avatar fetches fire automatically when
        // anyone joins the room, so this body is chosen by a stranger.
        let body = super::xrpc::read_capped_text(resp).await;
        if let Ok(xrpc) = serde_json::from_str::<XrpcError>(&body)
            && let Some(err) = xrpc.error.as_deref()
            && (err == "RecordNotFound"
                || (err == "InvalidRequest" && body.contains("RecordNotFound")))
        {
            return fallback_from_profile(client, &pds, did).await;
        }
        return Err(FetchError::PdsError(status.as_u16()));
    }
    let wrapper: GetAvatarResponse = decode_record_json(resp).await?;
    let mut record = wrapper.value;
    record.sanitize();
    if record.body.is_absent() {
        return fallback_from_profile(client, &pds, did).await;
    }
    if let Some(rig) = record.body.rigged_mut() {
        wardrobe::resolve_rigged_body(client, &pds, did, rig).await;
    }
    Ok(Some(record))
}

/// The cross-app fallback behind every "no overlands record" answer: an
/// identity whose `network.symbios.avatar.profile` names a default body
/// spawns wearing it instead of a seeded vehicle. Only a *fully resolved*
/// body is returned — a profile pointing at a deleted wardrobe record falls
/// through to `Ok(None)` (seeded default), because a bare humanoid chassis
/// with no geometry says less about the identity than the vehicle would.
/// Transport errors propagate as `Err`, keeping the caller's
/// don't-overwrite-on-transient-failure rule intact.
async fn fallback_from_profile(
    client: &reqwest::Client,
    pds: &str,
    did: &str,
) -> Result<Option<AvatarRecord>, FetchError> {
    let profile = wardrobe::fetch_avatar_profile_at(client, pds, did).await?;
    let Some(rkey) = profile.and_then(|p| p.default_avatar) else {
        return Ok(None);
    };
    let mut record = AvatarRecord::wearing(rkey);
    if let Some(rig) = record.body.rigged_mut() {
        wardrobe::resolve_rigged_body(client, pds, did, rig).await;
        if rig.resolved.is_none() {
            return Ok(None);
        }
    }
    Ok(Some(record))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seeded_default_wears_a_generator_body_and_round_trips() {
        let rolled = AvatarRecord::default_for_seed(42);
        assert!(
            rolled.body.visuals().is_some(),
            "seeded defaults stay Generator until #1060"
        );
        assert!(rolled.body.wire_ready().is_ok());
        // Quantisation happens IN serde (scaled-int wire types), so a fresh
        // roll only lands on the wire grid after one pass; the stability
        // claim is grid → grid.
        let json = serde_json::to_string(&rolled).expect("serializes");
        let mut once: AvatarRecord = serde_json::from_str(&json).expect("decodes");
        once.sanitize();
        let again = serde_json::to_string(&once).expect("re-serializes");
        let mut twice: AvatarRecord = serde_json::from_str(&again).expect("re-decodes");
        twice.sanitize();
        assert_eq!(once, twice, "the record did not survive its own JSON");
    }

    #[test]
    fn a_pre_1056_record_decodes_with_an_absent_body() {
        // The old schema had `visuals` where `body` now stands. The field is
        // simply unknown to the current shape, so the record decodes — with
        // an Absent body the fetch path converts to "no record". This is the
        // no-automatic-migration rule made testable.
        let mut value =
            serde_json::to_value(AvatarRecord::default_for_seed(7)).expect("serializes");
        let object = value.as_object_mut().expect("an object");
        let body = object.remove("body").expect("current shape has a body");
        object.insert(
            String::from("visuals"),
            body.get("visuals").cloned().expect("generator body"),
        );
        let record: AvatarRecord = serde_json::from_value(value).expect("old records still parse");
        assert!(record.body.is_absent());
        assert!(
            record.body.wire_ready().is_err(),
            "and cannot be republished as-is"
        );
    }

    #[test]
    fn a_sculpted_body_reads_dirty_even_though_the_wire_form_is_identical() {
        // The trap #1059 walked into: `resolved` is serde-skipped, so the
        // shared serde dirty-check calls a sculpted body clean and the Save
        // button sits disabled over unsaved work.
        let mut saved = AvatarRecord::wearing("3jzfcijpj2z2a");
        if let Some(rig) = saved.body.rigged_mut() {
            rig.resolved = Some(body::ResolvedRig {
                body: wardrobe::engine_default_for_did("did:plc:dirty-test"),
                attachments: Vec::new(),
            });
        }
        let mut edited = saved.clone();
        if let Some(resolved) = edited
            .body
            .rigged_mut()
            .and_then(|rig| rig.resolved.as_mut())
        {
            resolved.body.composites.femininity += 0.25;
        }

        assert!(
            !crate::state::records_differ(&saved, &edited),
            "the wire form is identical — which is exactly why the plain check is not enough"
        );
        assert!(
            edited.publishes_differently_from(&saved),
            "a sculpted body is unsaved work"
        );
        assert!(!saved.publishes_differently_from(&saved.clone()));
    }

    #[test]
    fn a_rigged_record_round_trips_its_references() {
        let mut record = AvatarRecord::wearing("3jzfcijpj2z2a");
        if let Some(rig) = record.body.rigged_mut() {
            rig.attachments.push(String::from("3jzfcijpj2z2b"));
        }
        record.sanitize();
        let json = serde_json::to_string(&record).expect("serializes");
        assert!(json.contains("#rigged"));
        let mut back: AvatarRecord = serde_json::from_str(&json).expect("decodes");
        back.sanitize();
        assert_eq!(record, back);
        assert!(
            matches!(back.locomotion, LocomotionConfig::Humanoid(_)),
            "a rigged body walks"
        );
    }
}
