//! What kind of body a rigged avatar is (#1875): a person, a four-legged
//! beast, a winged body or one this build cannot read - [`RiggedKind`], asked
//! two ways.
//!
//! * **Of the record** ([`RiggedKind::of_record`]), where the editor and every
//!   other reader of the record ask what is stored: its archetype.
//! * **Of the rig** ([`RiggedKind::of_rig`]), where the player asks what is
//!   drawn: a rig carrying wings ([`symbios_avatar::Rig::wings`], which
//!   crosses the web's worker) is winged, one with a person's trunk a
//!   person, any other a beast. A browser holding an older cached
//!   gen-worker.js builds a winged record as the stand-in person it knew,
//!   and that body is a person to everything that moves or dresses it.
//!
//! Neither asks the anatomy or the leg count. A winged body's head and trunk
//! are both a beast's kind (`HeadKind::Beast`, `TrunkKind::Beast`), so every
//! "not a human trunk" test reads it as a beast; and the engine's leg count
//! (`ground_contacts().len() == 4`) reads a bat or a glider as a beast and a
//! bird as a person.

use symbios_avatar::{Archetype, Kinship, Rig, WingedFamily};

use super::EngineAvatarRecord;

/// The kind of body a rigged avatar is (#1875). A winged body carries the
/// family that leads it, which decides its posture: a bird stands on two
/// legs, a bat and a glider go on four (the owner's decision on #1859, a
/// cross's posture is its leading family's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiggedKind {
    /// A person: the humanoid plan, a person's trunk.
    Person,
    /// A four-legged beast: the quadruped plan.
    Beast,
    /// A winged body (bird, bat, glider or a cross), led by this family.
    Winged(WingedFamily),
    /// A body plan this build cannot read. Only a record says so: whatever
    /// draws one draws the canon person, which its rig reads as.
    Unknown,
}

impl RiggedKind {
    /// What `body` is stored as: its archetype, and a winged body's leading
    /// family as the engine reads it off the record ([`Kinship::of`]), a
    /// cross at an even blend led by its second family.
    pub fn of_record(body: &EngineAvatarRecord) -> Self {
        match &body.archetype {
            Archetype::Humanoid(_) => Self::Person,
            Archetype::Quadruped(_) => Self::Beast,
            Archetype::Winged(params) => Self::Winged(Kinship::of(params).leading),
            Archetype::Unknown { .. } => Self::Unknown,
        }
    }

    /// What `rig` draws: winged when it carries wings, led by the family its
    /// wings were grown for ([`symbios_avatar::Wings::kinship()`], read off
    /// its stance on a rig from a build older than the kinship); a person
    /// when its trunk is a person's; a beast otherwise. Never
    /// [`Self::Unknown`]: a rig is always drawn as something.
    pub fn of_rig(rig: &Rig) -> Self {
        if let Some(wings) = &rig.wings {
            Self::Winged(wings.kinship(rig).leading)
        } else if rig.anatomy.human_trunk() {
            Self::Person
        } else {
            Self::Beast
        }
    }

    /// Whether this is a winged body, whichever family leads it.
    pub fn is_winged(self) -> bool {
        matches!(self, Self::Winged(_))
    }

    /// Whether the body goes on four legs: a beast, and a winged body a bat
    /// or a glider leads (the owner's decision on #1859).
    pub fn on_four(self) -> bool {
        matches!(
            self,
            Self::Beast | Self::Winged(WingedFamily::Bat | WingedFamily::Glider)
        )
    }

    /// Whether a body of this kind and one of `other` are the same kind of
    /// body, a winged body's leading family aside: a bird made a bat is still
    /// winged.
    pub fn same_body(self, other: Self) -> bool {
        std::mem::discriminant(&self) == std::mem::discriminant(&other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use symbios_avatar::{QuadrupedParams, WingedParams};

    /// A rolled body of each plan: `seeds` rolls of each, as the engine's own
    /// re-roll draws them (a winged roll may land on any family or a cross),
    /// and a winged roll made each family, which so few rolls may not reach.
    fn rolled_bodies(seeds: std::ops::Range<i64>) -> Vec<EngineAvatarRecord> {
        let plans = [
            Archetype::Humanoid(Default::default()),
            Archetype::Quadruped(QuadrupedParams::default()),
            Archetype::Winged(WingedParams::default()),
        ];
        let mut bodies = Vec::new();
        for plan in plans {
            for seed in seeds.clone() {
                let mut body = EngineAvatarRecord::rolled("Kind", plan.clone(), seed);
                body.sanitize();
                bodies.push(body);
            }
        }
        for family in [WingedFamily::Bird, WingedFamily::Bat, WingedFamily::Glider] {
            let mut body =
                EngineAvatarRecord::rolled("Kind", Archetype::Winged(WingedParams::default()), 40);
            if let Archetype::Winged(params) = &mut body.archetype {
                params.set_kind(family, None, 0.0);
            }
            body.sanitize();
            bodies.push(body);
        }
        bodies
    }

    /// **What is stored and what is drawn agree** (#1875): over rolled bodies
    /// of all three plans, the kind read off the record is the kind read off
    /// the rig the body builds - here, across the web's worker boundary over
    /// a self-describing codec (whose `serde(skip)`s the worker's share), and
    /// over the worker's own msgpack. Every winged family is among the rolls,
    /// so a rig read by its anatomy or its leg count - a bird as a person, a
    /// bat or a glider as a beast - fails here.
    #[test]
    fn the_record_and_the_rig_give_the_same_kind() {
        let mut families = std::collections::BTreeSet::new();
        for body in rolled_bodies(0..8) {
            let stored = RiggedKind::of_record(&body);
            if let RiggedKind::Winged(family) = stored {
                families.insert(format!("{family:?}"));
            }
            let built = crate::offload::GenJob::AvatarBuild {
                record: Box::new(body.clone()),
                atlas: 16,
                far_hair: false,
                measure_head: false,
            }
            .run();
            let crate::offload::GenResult::Avatar(Some(here)) = &built else {
                panic!("{stored:?} does not build");
            };
            let json = serde_json::to_vec(&built).expect("a result encodes");
            let crate::offload::GenResult::Avatar(Some(json)) =
                serde_json::from_slice(&json).expect("a result decodes")
            else {
                panic!("no body came back over JSON");
            };
            let packed = rmp_serde::to_vec_named(&built).expect("a result packs");
            let crate::offload::GenResult::Avatar(Some(packed)) =
                rmp_serde::from_slice(&packed).expect("a result unpacks")
            else {
                panic!("no body came back over msgpack");
            };
            for (how, avatar) in [("here", &**here), ("json", &*json), ("msgpack", &*packed)] {
                assert_eq!(
                    RiggedKind::of_rig(&avatar.rig),
                    stored,
                    "seed {} drawn {how}",
                    body.seed
                );
                // The web's worker measures a person's head for the dressing
                // (#1815) by its own copy of this answer, which it cannot
                // import: it must be this one, or a browser's person goes
                // unfitted or its beast is measured as one.
                assert_eq!(
                    gen_jobs::reads_a_persons_head(&avatar.rig),
                    stored == RiggedKind::Person,
                    "seed {} drawn {how}: the worker measures another set of heads",
                    body.seed
                );
            }
        }
        assert_eq!(
            families.len(),
            3,
            "the premise: every family rolled, {families:?}"
        );
    }

    /// **Persons and beasts read as they always did** (#1875): what a beast
    /// was asked by its leg count and a person by their trunk, each still
    /// answers, and only a winged body moved.
    #[test]
    fn a_person_and_a_beast_read_as_before() {
        for body in rolled_bodies(0..4) {
            let rig = Rig::from_skeleton(&body.skeleton()).expect("rigs");
            match &body.archetype {
                Archetype::Humanoid(_) => {
                    assert_eq!(RiggedKind::of_rig(&rig), RiggedKind::Person);
                    assert_ne!(rig.ground_contacts().len(), 4);
                }
                Archetype::Quadruped(_) => {
                    assert_eq!(RiggedKind::of_rig(&rig), RiggedKind::Beast);
                    assert_eq!(rig.ground_contacts().len(), 4);
                }
                _ => {}
            }
        }
        let unknown = EngineAvatarRecord {
            archetype: Archetype::Unknown {
                type_name: String::from("network.symbios.avatar.archetype#kite"),
                fields: serde_json::Map::new(),
            },
            ..EngineAvatarRecord::default()
        };
        assert_eq!(RiggedKind::of_record(&unknown), RiggedKind::Unknown);
    }

    /// **The leading family decides the posture, a cross at an even blend
    /// its second family's** (#1875): a bird stands on two legs and a bat or
    /// a glider goes on four, as the engine draws them; and one kind is the
    /// same body as another only within its own plan.
    #[test]
    fn a_winged_body_goes_on_its_leading_familys_legs() {
        let cross = |family, other, blend| {
            let mut params = WingedParams::default();
            params.set_kind(family, Some(other), blend);
            params.blend = blend;
            RiggedKind::of_record(&EngineAvatarRecord {
                archetype: Archetype::Winged(params),
                ..EngineAvatarRecord::default()
            })
        };
        use WingedFamily::{Bat, Bird, Glider};
        assert_eq!(cross(Bird, Bat, 0.49), RiggedKind::Winged(Bird));
        assert_eq!(cross(Bird, Bat, 0.5), RiggedKind::Winged(Bat));
        assert_eq!(cross(Bird, Bat, 0.51), RiggedKind::Winged(Bat));
        assert_eq!(cross(Bat, Bird, 0.51), RiggedKind::Winged(Bird));
        assert_eq!(cross(Bird, Glider, 0.5), RiggedKind::Winged(Glider));
        assert!(!RiggedKind::Winged(Bird).on_four());
        assert!(RiggedKind::Winged(Bat).on_four() && RiggedKind::Winged(Glider).on_four());
        assert!(RiggedKind::Beast.on_four() && !RiggedKind::Person.on_four());
        assert!(RiggedKind::Winged(Bird).same_body(RiggedKind::Winged(Bat)));
        for (a, b) in [
            (RiggedKind::Person, RiggedKind::Winged(Bird)),
            (RiggedKind::Winged(Bat), RiggedKind::Beast),
            (RiggedKind::Person, RiggedKind::Beast),
        ] {
            assert!(!a.same_body(b) && !b.same_body(a), "{a:?} and {b:?}");
        }
    }
}
