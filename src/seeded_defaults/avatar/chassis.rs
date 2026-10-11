//! Top-level chassis-family pick for the default avatar.
//!
//! Sampled into the [`AvatarCharacter`](super::AvatarCharacter) anchor so the
//! rest of the avatar pipeline can branch: a DID resolves to exactly one
//! visual family (hover-boat, airship, humanoid, land-skiff, helicopter), and
//! only that family's build path runs. Locomotion follows the family
//! (boat → HoverBoat, airship and helicopter → Helicopter, humanoid →
//! Humanoid, skiff → Car) so the default chassis *feels* like what it looks
//! like.
//!
//! # The pick is two stages (#1611, owner decision 1)
//!
//! The first stage asks whether a seed is an AIRCRAFT, on a salt of its own:
//! a uniform draw over six equal shares, one for the helicopter, one for the
//! airplane and four for everything else. The second stage is the four-way
//! pick that was the whole draw before the aircraft came: the same salt, the
//! same table ([`ChassisFamily::PRE_AIRCRAFT`]), the same uniform draw - so a
//! seed outside the aircraft third keeps exactly the family it always had.
//! Appending two families to one six-way table would have moved about 75 %
//! of the fleet; this moves a third of it, and only into the new families.
//!
//! The airplane share is drawn but not yet flown: until its family is built
//! (#1617) it falls through to the second stage, so those seeds keep their
//! family too, and no avatar changes family twice on the way (decision 7).
//! Today one seed in six is a helicopter and the other four families share
//! the rest.
//!
//! The first-stage salt was HUNTED, which is why it reads as it does: its
//! high half spells the stage and its low half is the first value under
//! which every stand-in the tests, the docs and the agents lean on keeps its
//! family - see `the_stand_ins_keep_their_families`.
//!
//! Diversity inside a family is NOT one mechanism. A boat, a skiff or a
//! helicopter rolls a [`CraftType`](super::CraftType) and is drawn by that
//! type's own builder over one profile (#1363, #1364, #1614); an airship is
//! composed by the seeded [`AvatarOutfit`](super::AvatarOutfit) filling slots
//! from the tagged part catalogue ([`crate::pds::avatar::parts`]) - the last
//! family that is; a humanoid is a seeded `symbios-avatar` engine record
//! (#1060). See [`super`]'s header for the three routes.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::seeded_defaults::hash::fnv1a_64;
use crate::seeded_defaults::scene::pick;

/// The second stage's salt - the whole pick's, before the aircraft.
const AVATAR_CHASSIS_SALT: u64 = 0xC4A5_51F0_C4A5_51F0;

/// The first stage's salt (#1614): the high half spells AIRCRAFT, the low
/// half is the first under which every stand-in keeps its family (hunted;
/// see the module header).
const AVATAR_AIRCRAFT_SALT: u64 = 0xA14C_4AF7_00E6_3FFB;

/// One sixth of the seed space, as the first stage deals it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Share {
    Helicopter,
    /// Not flown until #1617 - it falls through to the second stage.
    Airplane,
    /// One of the four sixths the second stage draws over.
    Rest,
}

/// The first stage's six equal shares.
const SHARES: [Share; 6] = [
    Share::Helicopter,
    Share::Airplane,
    Share::Rest,
    Share::Rest,
    Share::Rest,
    Share::Rest,
];

/// Discrete visual family of the default avatar. Picked first; how the
/// silhouette follows from it is the family's own affair - see the module
/// header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChassisFamily {
    /// Hover-boat - one of six seeded craft types, from a sloop to a scow
    /// ([`BoatType`](super::BoatType)).
    Boat,
    /// Lighter-than-air - envelope + gondola, composed from parts.
    Airship,
    /// Rigged figure from the `symbios-avatar` engine, consuming skin / hair
    /// / eye / gait (#1060).
    Humanoid,
    /// Land vehicle - one of six seeded craft types, from a roadster to a
    /// six-wheeled rover ([`SkiffType`](super::SkiffType)).
    Skiff,
    /// Rotorcraft - a light utility helicopter on skids, the first of the
    /// aircraft families (#1614; [`HelicopterType`](super::HelicopterType)).
    Helicopter,
}

impl ChassisFamily {
    /// Every family, for a picker or a census.
    pub const ALL: [Self; 5] = [
        Self::Boat,
        Self::Airship,
        Self::Humanoid,
        Self::Skiff,
        Self::Helicopter,
    ];

    /// The four families of the pick before the aircraft, in its order -
    /// the table the second stage still draws over, unchanged, which is
    /// what keeps every seed outside the aircraft third in its family.
    pub const PRE_AIRCRAFT: [Self; 4] = [Self::Boat, Self::Airship, Self::Humanoid, Self::Skiff];

    /// Human-readable display name - used by the pinned re-roll readout.
    pub fn label(self) -> &'static str {
        match self {
            Self::Boat => "Hover-boat",
            Self::Airship => "Airship",
            Self::Humanoid => "Humanoid",
            Self::Skiff => "Land-skiff",
            Self::Helicopter => "Helicopter",
        }
    }

    pub fn for_did(did: &str) -> Self {
        Self::for_seed(fnv1a_64(did))
    }

    /// Derive from a pre-computed seed - the manual re-roll path.
    /// `for_did(did)` is exactly `for_seed(fnv1a_64(did))`.
    pub fn for_seed(seed: u64) -> Self {
        let mut first = ChaCha8Rng::seed_from_u64(seed ^ AVATAR_AIRCRAFT_SALT);
        match pick(&SHARES, &mut first) {
            Share::Helicopter => Self::Helicopter,
            Share::Airplane | Share::Rest => {
                let mut rng = ChaCha8Rng::seed_from_u64(seed ^ AVATAR_CHASSIS_SALT);
                pick(&Self::PRE_AIRCRAFT, &mut rng)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        assert_eq!(
            ChassisFamily::for_did("did:plc:abc"),
            ChassisFamily::for_did("did:plc:abc")
        );
    }

    #[test]
    fn all_families_reachable() {
        // 300 seeds: the rarest family is a sixth of them, so the odds of
        // one never appearing are (5/6)^300, about 10^-24 - a miss means the
        // sampler is broken, not unlucky.
        let mut seen = [false; 5];
        for s in 0u64..300 {
            let f = ChassisFamily::for_did(&format!("did:test:{s}"));
            let i = ChassisFamily::ALL.iter().position(|x| *x == f).unwrap();
            seen[i] = true;
        }
        assert_eq!(seen, [true; 5], "some chassis family never sampled");
    }

    /// The pick before the aircraft, written out: the second stage alone.
    /// It reads the same salt and table as the code under test, so on its
    /// own it can only say the two stages agree with each other;
    /// [`PRE_AIRCRAFT_0_TO_39`] is what ties it to the pick as it was.
    fn pre_aircraft(seed: u64) -> ChassisFamily {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ AVATAR_CHASSIS_SALT);
        pick(&ChassisFamily::PRE_AIRCRAFT, &mut rng)
    }

    /// The families seeds 0 to 39 had before the aircraft came (#1614),
    /// read off the four-way pick at c3e676b: Boat, Airship, Humanoid,
    /// Skiff. A literal, so a salt edited or a table reordered under both
    /// stages at once still fails here.
    const PRE_AIRCRAFT_0_TO_39: &str = "AHSSSBASASSAABHSBHHBHSSBSHSSBAHAHAAHSSHA";

    /// Every seed under 40 is a helicopter now or the family it had then.
    #[test]
    fn the_first_forty_seeds_keep_the_families_they_had() {
        let mut helicopters = 0;
        for (seed, was) in (0u64..).zip(PRE_AIRCRAFT_0_TO_39.chars()) {
            let was = match was {
                'B' => ChassisFamily::Boat,
                'A' => ChassisFamily::Airship,
                'H' => ChassisFamily::Humanoid,
                'S' => ChassisFamily::Skiff,
                other => panic!("no family {other:?}"),
            };
            assert_eq!(
                pre_aircraft(seed),
                was,
                "seed {seed}: the second stage moved"
            );
            match ChassisFamily::for_seed(seed) {
                ChassisFamily::Helicopter => helicopters += 1,
                now => assert_eq!(now, was, "seed {seed} changed family"),
            }
        }
        assert!(
            (3..=12).contains(&helicopters),
            "{helicopters} of the first forty are helicopters"
        );
    }

    /// Decision 1's promise: a seed is either a helicopter or EXACTLY the
    /// family it was before the aircraft came - never a different one of the
    /// four - and the helicopter takes a sixth of the fleet. Over 24 000
    /// seeds a sixth is 4 000 with a standard deviation of about 58, so the
    /// band is five of them wide. (CI runs this at opt-level 0: kept small.)
    #[test]
    fn a_seed_is_a_helicopter_or_the_family_it_always_was() {
        const N: u64 = 24_000;
        let mut helicopters = 0u64;
        for s in 0..N {
            match ChassisFamily::for_seed(s) {
                ChassisFamily::Helicopter => helicopters += 1,
                f => assert_eq!(f, pre_aircraft(s), "seed {s} changed family"),
            }
        }
        assert!(
            (3_700..=4_300).contains(&helicopters),
            "{helicopters} helicopters in {N} seeds, expected about a sixth"
        );
    }

    /// The airplane's sixth is DRAWN today, and falls through to the four:
    /// so when #1617 flies it, it takes a sixth that no helicopter seed
    /// holds, and the four keep two thirds of the fleet between them.
    #[test]
    fn the_airplane_share_is_a_sixth_of_its_own() {
        const N: u64 = 24_000;
        let mut airplanes = 0u64;
        for s in 0..N {
            let mut first = ChaCha8Rng::seed_from_u64(s ^ AVATAR_AIRCRAFT_SALT);
            if pick(&SHARES, &mut first) == Share::Airplane {
                airplanes += 1;
                assert_eq!(ChassisFamily::for_seed(s), pre_aircraft(s), "seed {s}");
            }
        }
        assert!(
            (3_700..=4_300).contains(&airplanes),
            "{airplanes} airplane draws in {N} seeds, expected about a sixth"
        );
    }

    /// The stand-ins keep their families, and the salt was hunted for it:
    /// the agent accounts and the offline agent's four bodies (their docs
    /// name what each is), every literal seed a test builds a seeded default
    /// from or pins a hunt result to, and every seed a doc or a comment names
    /// as a family - the standing comparison line-up `12,40,7`, the heaviest
    /// skiff, the seeds earlier reviews were judged on. Two thirds of seeds
    /// fall outside both aircraft shares under any salt, so a salt that
    /// keeps all forty is rare - about one in eleven million, the 15 089 660th
    /// tried - and changing it is a hunt, not an edit. Neither aircraft share
    /// holds any of them, so the airplane (#1617) will not move them either.
    #[test]
    fn the_stand_ins_keep_their_families() {
        use ChassisFamily::*;
        let dids = [
            // The offline agent's stand-in walks; `--stand-in` names the
            // other three (config::agent::OFFLINE_DID).
            ("did:plc:agentofflinestandin22222", Humanoid),
            ("did:plc:agentofflinecar22222222e", Skiff),
            ("did:plc:agentofflineboat2222222d", Boat),
            ("did:plc:agentofflineair222222222", Airship),
            // Jink, Hypha, Eigen and Reeve (docs/agent/accounts/).
            ("did:plc:lr2ocunor73lfqpwtgvt274o", Skiff),
            ("did:plc:ghkcajvgtvvfxavllty3pr57", Airship),
            ("did:plc:7po3qshysor5djaxoy7zme4s", Airship),
            ("did:plc:dwhgmzz27dllc3k2nmlznv3n", Boat),
        ];
        for (did, family) in dids {
            assert_eq!(ChassisFamily::for_did(did), family, "{did}");
            let mut first = ChaCha8Rng::seed_from_u64(fnv1a_64(did) ^ AVATAR_AIRCRAFT_SALT);
            assert_eq!(pick(&SHARES, &mut first), Share::Rest, "{did}");
        }
        let seeds = [
            (1, Humanoid),
            (2, Skiff),
            (3, Skiff),
            (7, Skiff),
            (13, Boat),
            (40, Boat),
            (42, Skiff),
            (4242, Skiff),
            // The four-axis hunt's sixteen (character.rs).
            (4906, Boat),
            (841, Boat),
            (439, Boat),
            (493, Boat),
            (1173, Skiff),
            (239, Skiff),
            (786, Skiff),
            (830, Skiff),
            (944, Airship),
            (862, Airship),
            (504, Airship),
            (53, Airship),
            (365, Humanoid),
            (215, Humanoid),
            (1003, Humanoid),
            // Named by a doc or a comment: the standing comparison's airship
            // (docs/building.md) and the window light's target look, the
            // heaviest skiff, and the seeds earlier reviews were judged on.
            (12, Airship),
            (1731, Skiff),
            (0, Airship),
            (29, Airship),
            (45, Airship),
            (48, Airship),
            (96, Skiff),
            (134, Skiff),
            (240, Skiff),
        ];
        for (seed, family) in seeds {
            assert_eq!(ChassisFamily::for_seed(seed), family, "seed {seed}");
            let mut first = ChaCha8Rng::seed_from_u64(seed ^ AVATAR_AIRCRAFT_SALT);
            assert_eq!(pick(&SHARES, &mut first), Share::Rest, "seed {seed}");
        }
    }
}
