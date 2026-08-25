//! The record's body half: which kind of body an avatar wears (#1056).
//!
//! Until epic #1054 an avatar's looks were exactly one thing — a [`Generator`]
//! tree in the record's `visuals` field. The rigged-avatar integration makes
//! the body a choice, spelled as the same `$type`-tagged open union the
//! locomotion half already uses:
//!
//!   - [`AvatarBody::Rigged`] — a parametric skinned body from the
//!     `symbios-avatar` engine, stored as a *reference*: the record key of a
//!     wardrobe record (`network.symbios.avatar.avatar`, tid-keyed) plus the
//!     record keys of any attachment records. The referenced records are
//!     fetched in the same pass as the avatar record and carried on
//!     [`RiggedBody::resolved`], which never touches the wire.
//!   - [`AvatarBody::Generator`] — the classic tree. Post-#1060 this is the
//!     vehicle chassis' variant (boat / airship / skiff); until the legacy
//!     humanoid builder is deleted it carries seeded humanoids too, which is
//!     why the variant is named for its payload and not for a role.
//!   - [`AvatarBody::Unknown`] — a body kind from a newer client, kept as a
//!     bare chassis rather than replaced with a guess.
//!
//! A record published before this union has no `body` field at all and lands
//! on [`AvatarBody::Absent`] through the field-level default — distinct from
//! `Unknown` on purpose: an *old* record is treated as "no record" (the fetch
//! path falls through to the seeded default, matching this module's standing
//! no-automatic-migration rule), while a *future* record is honoured as far
//! as it can be. Neither marker variant serializes; the publish preflight
//! refuses a record that still carries one.

use serde::{Deserialize, Serialize};

use super::super::generator::Generator;
use super::super::sanitize::sanitize_avatar_visuals;
use super::wardrobe::AttachmentRecord;

/// The most attachment references a rigged body may carry.
///
/// A bound, not a budget: each reference is one PDS fetch on every peer that
/// renders the wearer, so an unbounded list is a fan-out amplifier. Sixteen
/// covers every socket the rig offers ([`symbios_avatar::Socket::ALL`]) —
/// one prop per socket — with no way for a hostile record to demand hundreds
/// of fetches.
pub const MAX_AVATAR_ATTACHMENTS: usize = 16;

/// The longest record key accepted in a body or attachment reference.
/// ATProto caps rkeys at 512 chars; ours are 13-char TIDs, and anything
/// longer than this is not a key either side of the integration mints.
const MAX_REF_RKEY_CHARS: usize = 64;

/// Which kind of body the avatar wears — an open union, like
/// [`super::LocomotionConfig`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(tag = "$type")]
pub enum AvatarBody {
    /// A parametric rigged body from the `symbios-avatar` engine, by
    /// reference into the identity's wardrobe.
    #[serde(rename = "network.symbios.overlands.avatar#rigged")]
    Rigged(Box<RiggedBody>),

    /// A `Generator`-tree body — vehicles, and (until #1060) the legacy
    /// seeded humanoids.
    #[serde(rename = "network.symbios.overlands.avatar#generator")]
    Generator(Box<GeneratorBody>),

    /// No `body` field on the wire at all: a pre-#1056 record. The fetch
    /// path treats this as "no record" and synthesises the seeded default.
    #[default]
    #[serde(skip)]
    Absent,

    /// A body kind this build does not know. Kept verbatim in spirit — the
    /// peer renders a bare chassis — but never re-serialized, so an old
    /// client cannot round-trip somebody's future body into nothing.
    /// (Last variant: serde requires `other` to close the union.)
    #[serde(other, skip_serializing)]
    Unknown,
}

/// The rigged variant's payload: references plus their fetched resolution.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct RiggedBody {
    /// Record key (TID) of the worn body in the identity's wardrobe
    /// (`network.symbios.avatar.avatar`).
    pub avatar: String,
    /// Record keys (TIDs) of worn attachment records
    /// (`network.symbios.overlands.avatar.attachment`), in draw-order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<String>,
    /// The referenced records, fetched alongside the avatar record. Never on
    /// the wire: a reference's whole point is that the payload lives in its
    /// own record, and a peer resolves against the owner's PDS rather than
    /// trusting a copy embedded here.
    #[serde(skip)]
    pub resolved: Option<ResolvedRig>,
}

/// A [`RiggedBody`]'s references, fetched. `None` fields never occur — a
/// reference that fails to resolve leaves the whole `resolved` slot `None`
/// (no body worth building) or drops just its attachment (partial failure
/// degrades to a barer avatar, not a missing one).
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedRig {
    /// The engine record the wardrobe reference names, sanitised.
    pub body: symbios_avatar::AvatarRecord,
    /// Every attachment reference that resolved, in the record's order.
    pub attachments: Vec<ResolvedAttachment>,
}

/// One resolved attachment reference.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedAttachment {
    /// The record key the record listed — kept so an edit can write back to
    /// the same record.
    pub rkey: String,
    /// The attachment record itself, sanitised.
    pub record: AttachmentRecord,
}

/// The generator variant's payload.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GeneratorBody {
    /// Hierarchical visuals — the cosmetic mesh tree. Sanitised by
    /// [`sanitize_avatar_visuals`], which excludes Terrain/Water/Portal.
    pub visuals: Generator,
}

impl AvatarBody {
    /// A generator body wrapping `visuals` — the seeded-default constructor.
    pub fn generator(visuals: Generator) -> Self {
        Self::Generator(Box::new(GeneratorBody { visuals }))
    }

    /// The seeded default's rigged body (#1060): the engine record rolled
    /// from `seed`, resolved **locally** at a deterministic wardrobe key.
    ///
    /// Nothing is fetched, and nothing needs to exist on a PDS — the
    /// engine's roll is deterministic, so every peer derives the same
    /// person for a DID with nothing on the wire. That is what lets an
    /// identity who has never opened the editor still look like themselves
    /// to everyone in the room.
    pub fn rigged_seeded(seed: u64) -> Self {
        Self::Rigged(Box::new(RiggedBody {
            avatar: crate::pds::tid::tid_for_seed(seed),
            attachments: Vec::new(),
            resolved: Some(ResolvedRig {
                body: super::wardrobe::engine_default_for_seed(seed),
                attachments: Vec::new(),
            }),
        }))
    }

    /// A rigged body wearing the wardrobe record at `rkey`, unresolved.
    pub fn rigged(rkey: impl Into<String>) -> Self {
        Self::Rigged(Box::new(RiggedBody {
            avatar: rkey.into(),
            attachments: Vec::new(),
            resolved: None,
        }))
    }

    /// The generator visuals, when this body has any. `None` for rigged,
    /// unknown and absent bodies — the callers that walk or edit the tree
    /// (spawn, gizmo, the Visuals tab) skip instead of guessing.
    pub fn visuals(&self) -> Option<&Generator> {
        match self {
            Self::Generator(body) => Some(&body.visuals),
            _ => None,
        }
    }

    /// Mutable access to the generator visuals, on the same terms as
    /// [`Self::visuals`].
    pub fn visuals_mut(&mut self) -> Option<&mut Generator> {
        match self {
            Self::Generator(body) => Some(&mut body.visuals),
            _ => None,
        }
    }

    /// The rigged payload, when this body is one.
    pub fn rigged_ref(&self) -> Option<&RiggedBody> {
        match self {
            Self::Rigged(body) => Some(body),
            _ => None,
        }
    }

    /// Mutable rigged payload — the resolution pass writes through this.
    pub fn rigged_mut(&mut self) -> Option<&mut RiggedBody> {
        match self {
            Self::Rigged(body) => Some(body),
            _ => None,
        }
    }

    /// This body with nothing worn: the part of it that decides what the
    /// chassis's visual children look like (#1104). The rigged variant
    /// keeps its wardrobe reference and resolved engine record but drops
    /// both attachment lists; every other variant is returned verbatim.
    ///
    /// `player::hotswap::rebuild_local_visuals` compares snapshots of this
    /// to decide whether a record change owes a visual respawn: worn props
    /// are dressed by `player::attachments::sync_rigged_attachments` from
    /// the record directly, so an attachment-only edit must never tear the
    /// body down.
    pub fn sans_attachments(&self) -> Self {
        match self {
            Self::Rigged(body) => {
                let mut stripped = body.clone();
                stripped.attachments.clear();
                if let Some(resolved) = stripped.resolved.as_mut() {
                    resolved.attachments.clear();
                }
                Self::Rigged(stripped)
            }
            other => other.clone(),
        }
    }

    /// Whether this is the pre-#1056 no-field marker.
    pub fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }

    /// Whether a record carrying this body may be published or broadcast.
    /// The two marker variants cannot serialize, and refusing them here with
    /// a sentence beats a serde error string surfacing in the status line.
    pub fn wire_ready(&self) -> Result<(), String> {
        match self {
            Self::Rigged(_) | Self::Generator(_) => Ok(()),
            Self::Unknown => Err(String::from(
                "this avatar's body was made by a newer client; re-roll or edit it before saving",
            )),
            Self::Absent => Err(String::from(
                "this avatar predates the body schema; re-roll or edit it before saving",
            )),
        }
    }

    /// Clamp every numeric field and bound every list, exactly as the rest
    /// of the record sanitises.
    pub fn sanitize(&mut self) {
        match self {
            Self::Generator(body) => sanitize_avatar_visuals(&mut body.visuals),
            Self::Rigged(body) => body.sanitize(),
            Self::Unknown | Self::Absent => {}
        }
    }
}

impl RiggedBody {
    fn sanitize(&mut self) {
        clamp_rkey(&mut self.avatar);
        self.attachments.truncate(MAX_AVATAR_ATTACHMENTS);
        for rkey in &mut self.attachments {
            clamp_rkey(rkey);
        }
        if let Some(resolved) = &mut self.resolved {
            resolved.body.sanitize();
            resolved.attachments.truncate(MAX_AVATAR_ATTACHMENTS);
            for attachment in &mut resolved.attachments {
                clamp_rkey(&mut attachment.rkey);
                attachment.record.sanitize();
            }
        }
    }
}

/// Truncate a reference rkey to something that can only be a record key.
fn clamp_rkey(rkey: &mut String) {
    if rkey.chars().count() > MAX_REF_RKEY_CHARS {
        *rkey = rkey.chars().take(MAX_REF_RKEY_CHARS).collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_body_field_lands_on_absent_and_a_future_kind_on_unknown() {
        // The two directions of the compat story, kept distinct on purpose:
        // an old record synthesises a default, a future record keeps flying
        // as a bare chassis.
        #[derive(Deserialize)]
        struct Probe {
            #[serde(default)]
            body: AvatarBody,
        }
        let old: Probe = serde_json::from_str("{}").expect("decodes");
        assert!(old.body.is_absent());

        let future: Probe = serde_json::from_str(
            r#"{"body":{"$type":"network.symbios.overlands.avatar#holographic","shine":1}}"#,
        )
        .expect("decodes");
        assert_eq!(future.body, AvatarBody::Unknown);
    }

    #[test]
    fn neither_marker_variant_serializes() {
        assert!(serde_json::to_string(&AvatarBody::Unknown).is_err());
        assert!(serde_json::to_string(&AvatarBody::Absent).is_err());
        assert!(AvatarBody::Unknown.wire_ready().is_err());
        assert!(AvatarBody::Absent.wire_ready().is_err());
    }

    #[test]
    fn a_rigged_body_round_trips_without_its_resolution() {
        let mut body = AvatarBody::rigged("3jzfcijpj2z2a");
        if let Some(rig) = body.rigged_mut() {
            rig.attachments.push(String::from("3jzfcijpj2z2b"));
            rig.resolved = Some(ResolvedRig {
                body: symbios_avatar::AvatarRecord::default(),
                attachments: Vec::new(),
            });
        }
        let json = serde_json::to_string(&body).expect("serializes");
        assert!(
            !json.contains("resolved"),
            "resolution leaked onto the wire: {json}"
        );
        let back: AvatarBody = serde_json::from_str(&json).expect("decodes");
        let rig = back.rigged_ref().expect("still rigged");
        assert_eq!(rig.avatar, "3jzfcijpj2z2a");
        assert_eq!(rig.attachments, vec![String::from("3jzfcijpj2z2b")]);
        assert!(rig.resolved.is_none(), "resolution is re-fetched, not read");
    }

    #[test]
    fn sanitize_bounds_the_attachment_fan_out() {
        let mut body = AvatarBody::rigged("a");
        if let Some(rig) = body.rigged_mut() {
            rig.attachments = (0..100).map(|i| format!("rkey{i}")).collect();
        }
        body.sanitize();
        assert_eq!(
            body.rigged_ref().expect("rigged").attachments.len(),
            MAX_AVATAR_ATTACHMENTS
        );
    }
}
