//! Avatar items (#1838, epic #1837): a whole avatar, stashed by value.
//!
//! The owner saves the avatar they are wearing to the Inventory from the
//! Avatar editor and later wears it back from the Inventory window. The
//! item is the avatar **by value** - never a reference into the wardrobe or
//! the attachment collection - so it survives the wardrobe entry and the
//! worn props it was copied from being edited, re-worn or retired:
//!
//!   - [`AvatarItemBody::Rigged`] carries the engine record itself and every
//!     prop the body wore, each as a [`Wearable`] - the prop's tree and the
//!     [`WearMeta`] an attachment record carries besides its `$type`.
//!   - [`AvatarItemBody::Generator`] carries the vehicle's (or a legacy
//!     figure's) visuals tree, as the avatar record's own [`GeneratorBody`].
//!   - [`AvatarItemBody::Unknown`] is a body kind from a newer client. It
//!     never serializes, so an item holding one can be deleted and nothing
//!     else - it is never written back lossy, which is how an unreadable
//!     generator item behaves.
//!
//! Locomotion and the idle motion travel exactly as the avatar record
//! carries them. Every bound is the avatar record's own: the generator arm
//! is sanitized by [`GeneratorBody::sanitize`], the engine record by its own
//! `sanitize`, each wearable by the attachment record's
//! [`sanitize_worn_prop`], and the wearables are capped at
//! [`MAX_AVATAR_ATTACHMENTS`] as a rigged body's attachments are.

use serde::{Deserialize, Serialize};

use super::WearMeta;
use crate::pds::AVATAR_ATTACHMENT_COLLECTION;
use crate::pds::avatar::wardrobe::sanitize_worn_prop;
use crate::pds::avatar::{
    AttachmentRecord, AvatarBody, AvatarRecord, EngineAvatarRecord, GaitParams, GeneratorBody,
    LocomotionConfig, MAX_AVATAR_ATTACHMENTS, ResolvedAttachment, ResolvedRig, RiggedBody,
    RiggedKind,
};
use crate::pds::generator::{Generator, GeneratorKind};

/// The whole avatar, by value: the payload of an avatar item's
/// `avatar` field on the wire.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AvatarItem {
    /// Which body the avatar wears, with everything it is made of.
    pub body: AvatarItemBody,
    /// The physics preset, as the avatar record carries it.
    pub locomotion: LocomotionConfig,
    /// Idle-motion tuning, as the avatar record carries it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gait: Option<GaitParams>,
}

/// An avatar item's body - an open union, like the avatar record's own
/// [`AvatarBody`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "$type")]
pub enum AvatarItemBody {
    /// A rigged engine body and what it wore.
    #[serde(rename = "network.symbios.overlands.inventory.item#rigged")]
    Rigged(Box<RiggedItem>),
    /// A generator-tree body: a vehicle, or a legacy figure.
    #[serde(rename = "network.symbios.overlands.inventory.item#generator")]
    Generator(Box<GeneratorBody>),
    /// A body kind this build does not know. Never re-serialized, so an
    /// older client cannot round-trip somebody's future avatar into
    /// nothing. (Last variant: serde requires `other` to close the union.)
    #[serde(other, skip_serializing)]
    Unknown,
}

/// The rigged arm's payload.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RiggedItem {
    /// The engine record itself, by value - what the wardrobe record held.
    pub record: EngineAvatarRecord,
    /// Every prop the body wore, in draw order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wearables: Vec<Wearable>,
}

/// One prop a stashed rigged body wore: exactly what an
/// [`AttachmentRecord`] carries besides its `$type` and its provenance.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Wearable {
    /// The prop's tree.
    pub generator: Generator,
    /// Its socket, fit and offset.
    pub wear: WearMeta,
}

/// What an avatar is, as far as its record can tell (#1838) - the kind
/// tag an avatar row shows and the default name it is saved under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AvatarKind {
    /// A rigged body with a person's trunk.
    Person,
    /// A rigged four-legged body.
    Beast,
    /// A rigged winged body (#1875): a bird, a bat, a glider or a cross.
    Winged,
    /// A generator body on the hover-boat preset.
    Boat,
    /// A generator body on the car preset - the land-skiff.
    Skiff,
    /// A generator body on the airplane preset.
    Airplane,
    /// A generator body on the helicopter preset, which an airship and a
    /// helicopter share: the record carries no craft type to tell them
    /// apart, so the label says both.
    Rotorcraft,
    /// A generator body that walks - a legacy construction-kit figure.
    Figure,
    /// A generator body on a preset this build does not know.
    Vehicle,
    /// A body this build cannot read.
    Unreadable,
}

impl AvatarKind {
    /// The kind as a person reads it - the row's tag and a search facet.
    pub fn label(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Beast => "beast",
            Self::Winged => "winged",
            Self::Boat => "boat",
            Self::Skiff => "skiff",
            Self::Airplane => "airplane",
            Self::Rotorcraft => "airship or helicopter",
            Self::Figure => "figure",
            Self::Vehicle => "vehicle",
            Self::Unreadable => "avatar",
        }
    }

    /// The name an item of this kind is saved under when it has none of
    /// its own - the label, except where the label hedges between two.
    pub fn default_name(self) -> &'static str {
        match self {
            Self::Rotorcraft => "aircraft",
            other => other.label(),
        }
    }
}

/// Why the live avatar cannot be copied into an item: a rigged body whose
/// wardrobe record never resolved has nothing to copy.
pub const UNRESOLVED_BODY: &str = "This body has not loaded, so there is nothing to copy - \
     the Body tab can wear a fresh one.";

impl AvatarItem {
    /// Copy `record` - the live avatar, unsaved edits included - into an
    /// item. A rigged body is copied from its resolution: the engine record
    /// and every prop that resolved, which is exactly what the owner sees.
    /// Refuses, with the sentence to show, a body with nothing to copy and
    /// one this build cannot write back.
    pub fn from_record(record: &AvatarRecord) -> Result<Self, String> {
        if let Some(reason) = Self::copy_refused(record) {
            return Err(reason);
        }
        let body = match &record.body {
            AvatarBody::Rigged(rig) => {
                let resolved = rig.resolved.as_ref().ok_or(UNRESOLVED_BODY)?;
                AvatarItemBody::Rigged(Box::new(RiggedItem {
                    record: resolved.body.clone(),
                    wearables: resolved
                        .attachments
                        .iter()
                        .map(|worn| Wearable::from_attachment(&worn.record))
                        .collect(),
                }))
            }
            AvatarBody::Generator(body) => AvatarItemBody::Generator(body.clone()),
            // `wire_ready` above refused both markers.
            AvatarBody::Unknown | AvatarBody::Absent => return Err(UNRESOLVED_BODY.into()),
        };
        let mut item = Self {
            body,
            locomotion: record.locomotion.clone(),
            gait: record.gait.clone(),
        };
        // The live record is sanitized only when the editor's debounce
        // drains, and the copy may be taken before it does.
        item.sanitize();
        Ok(item)
    }

    /// Why [`Self::from_record`] would refuse `record`, without copying it
    /// - the question a disabled Save to Inventory asks every frame.
    pub fn copy_refused(record: &AvatarRecord) -> Option<String> {
        if let Err(reason) = record.body.wire_ready() {
            return Some(reason);
        }
        if matches!(record.locomotion, LocomotionConfig::Unknown) {
            return Some(String::from(
                "this avatar's locomotion was made by a newer client; pick a preset on the \
                 Locomotion tab first",
            ));
        }
        if record
            .body
            .rigged_ref()
            .is_some_and(|rig| rig.resolved.is_none())
        {
            return Some(UNRESOLVED_BODY.to_string());
        }
        None
    }

    /// The avatar record that wears this item, or `None` for a body this
    /// build cannot read.
    ///
    /// A rigged item wears at the wardrobe key `wardrobe_rkey` names for its
    /// engine record (the caller reuses one only where it knows the record
    /// there is equal) and at a FRESH attachment key per wearable, from
    /// `mint`; the resolution is filled locally, so the body builds at once
    /// and the next avatar save publishes the bundle. The props are copies
    /// with no provenance: the stash items they were once worn from have
    /// nothing to do with this one.
    pub fn wear(
        &self,
        wardrobe_rkey: impl FnOnce(&EngineAvatarRecord) -> String,
        mut mint: impl FnMut() -> String,
    ) -> Option<AvatarRecord> {
        let body = match &self.body {
            AvatarItemBody::Rigged(rigged) => {
                let attachments: Vec<ResolvedAttachment> = rigged
                    .wearables
                    .iter()
                    .take(MAX_AVATAR_ATTACHMENTS)
                    .map(|wearable| ResolvedAttachment {
                        rkey: mint(),
                        record: wearable.to_attachment(),
                    })
                    .collect();
                AvatarBody::Rigged(Box::new(RiggedBody {
                    avatar: wardrobe_rkey(&rigged.record),
                    attachments: attachments.iter().map(|worn| worn.rkey.clone()).collect(),
                    resolved: Some(ResolvedRig {
                        body: rigged.record.clone(),
                        attachments,
                    }),
                }))
            }
            AvatarItemBody::Generator(body) => AvatarBody::Generator(body.clone()),
            AvatarItemBody::Unknown => return None,
        };
        Some(AvatarRecord {
            lex_type: crate::pds::AVATAR_COLLECTION.into(),
            body,
            locomotion: self.locomotion.clone(),
            gait: self.gait.clone(),
        })
    }

    /// Clamp every field with the avatar record's own bounds - see the
    /// module header for which sanitizer answers each part.
    pub fn sanitize(&mut self) {
        match &mut self.body {
            AvatarItemBody::Generator(body) => body.sanitize(),
            AvatarItemBody::Rigged(rigged) => {
                rigged.record.sanitize();
                rigged.wearables.truncate(MAX_AVATAR_ATTACHMENTS);
                for wearable in &mut rigged.wearables {
                    wearable.sanitize();
                }
            }
            AvatarItemBody::Unknown => {}
        }
        self.locomotion.sanitize();
        if let Some(gait) = &mut self.gait {
            gait.sanitize();
        }
    }

    /// Whether this build can read the item - wear it, rename it, write it
    /// back. An unreadable item is deleted and nothing else, as an
    /// unreadable generator item is: and a tree whose ROOT is a kind of a
    /// newer build is exactly such a generator item, whether it is the
    /// body's visuals or a wearable (critic of #1838) - worn, it would dress
    /// the live avatar in something its Save cannot write.
    pub fn is_readable(&self) -> bool {
        let readable_root =
            |generator: &Generator| !matches!(generator.kind, GeneratorKind::Unknown);
        let body = match &self.body {
            AvatarItemBody::Rigged(rigged) => rigged
                .wearables
                .iter()
                .all(|wearable| readable_root(&wearable.generator)),
            AvatarItemBody::Generator(body) => readable_root(&body.visuals),
            AvatarItemBody::Unknown => false,
        };
        body && !matches!(self.locomotion, LocomotionConfig::Unknown)
    }

    /// What this avatar is, as far as the record can tell.
    pub fn kind(&self) -> AvatarKind {
        match &self.body {
            // A body plan this build cannot read is drawn as a person, and
            // labelled one, as it always was.
            AvatarItemBody::Rigged(rigged) => match RiggedKind::of_record(&rigged.record) {
                RiggedKind::Beast => AvatarKind::Beast,
                RiggedKind::Winged(_) => AvatarKind::Winged,
                RiggedKind::Person | RiggedKind::Unknown => AvatarKind::Person,
            },
            AvatarItemBody::Generator(_) => match self.locomotion {
                LocomotionConfig::HoverBoat(_) => AvatarKind::Boat,
                LocomotionConfig::Car(_) => AvatarKind::Skiff,
                LocomotionConfig::Airplane(_) => AvatarKind::Airplane,
                LocomotionConfig::Helicopter(_) => AvatarKind::Rotorcraft,
                LocomotionConfig::Humanoid(_) => AvatarKind::Figure,
                LocomotionConfig::Unknown => AvatarKind::Vehicle,
            },
            AvatarItemBody::Unknown => AvatarKind::Unreadable,
        }
    }

    /// The name this item is saved under by default: the engine record's
    /// own name when it has one, else what the avatar is.
    pub fn default_name(&self) -> String {
        if let AvatarItemBody::Rigged(rigged) = &self.body {
            let name = rigged.record.name.trim();
            if !name.is_empty() {
                return name.to_string();
            }
        }
        self.kind().default_name().to_string()
    }

    /// The visuals tree a generator body is pictured from; `None` for a
    /// rigged body - whose picture is the body itself, built by the game's
    /// own job (#1842) - and an unreadable one.
    pub fn visuals(&self) -> Option<&Generator> {
        match &self.body {
            AvatarItemBody::Generator(body) => Some(&body.visuals),
            _ => None,
        }
    }

    /// How many props the stashed body wears.
    pub fn wearable_count(&self) -> usize {
        match &self.body {
            AvatarItemBody::Rigged(rigged) => rigged.wearables.len(),
            _ => 0,
        }
    }
}

impl Wearable {
    /// The by-value half of a worn prop's record.
    pub fn from_attachment(record: &AttachmentRecord) -> Self {
        Self {
            generator: record.item.clone(),
            wear: record.wear_meta(),
        }
    }

    /// A fresh attachment record wearing this prop, with no provenance.
    pub fn to_attachment(&self) -> AttachmentRecord {
        AttachmentRecord {
            lex_type: AVATAR_ATTACHMENT_COLLECTION.into(),
            item: self.generator.clone(),
            socket: self.wear.socket.clone(),
            offset: self.wear.offset.clone(),
            fit_band_mm: self.wear.fit_band_mm,
            fit_beast_mm: self.wear.fit_beast_mm,
            source: None,
        }
    }

    /// The attachment record's own clamp, over the same fields.
    pub fn sanitize(&mut self) {
        sanitize_worn_prop(
            &mut self.generator,
            &mut self.wear.socket,
            &mut self.wear.fit_band_mm,
            &mut self.wear.fit_beast_mm,
            &mut self.wear.offset,
        );
    }
}
