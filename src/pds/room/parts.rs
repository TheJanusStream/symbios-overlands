//! A long instance set's copies in records of their own (#1645, P2.3 of
//! epic #1636).
//!
//! A Berlin Kiez seeded into the record is thousands of copies - 56 to 125
//! KiB of them a square kilometre, past what the room's manifest may carry
//! in one write - so a set of more than [`INLINE_COPIES`] copies is written
//! to the manifest as a placement of its own type, [`INSTANCES_IN_PARTS`]:
//! the set's fields as an inline set writes them, with `parts`, an ordered
//! list of record keys, in place of `copies`. Each part
//! ([`RoomCopiesRecord`]) is a record in
//! [`crate::pds::ROOM_COPIES_COLLECTION`] keyed by a hash of its own bytes,
//! as a child generator is ([`super::child_rkey`]): immutable, shared by
//! identical parts, and free to write again. A set of no more copies keeps
//! them inline, its bytes as they always were - unless the room's inline
//! sets would hold more than [`INLINE_ROOM_COPIES`] between them, when its
//! biggest small sets go into parts too ([`kept_in_parts`]), so copies never
//! fill the manifest however many sets hold them.
//!
//! **Where a part ends is read off the copies** ([`cut`]): after a copy
//! whose numbers hash to a multiple of [`PART_SPREAD`], once the part holds
//! [`PART_MIN`]. An edit to one copy rewrites the part that holds it - and
//! the one beside it, where the edit moves a cut - and the manifest; a copy
//! added or taken out moves the cuts only as far as the next one the copies
//! pick, and the parts after it keep their keys. No part passes
//! [`PART_MAX`] copies, under the record budget at the widest numbers a copy
//! can have; where a part runs that long before the copies pick a cut (about
//! one in six), an added or removed copy shifts the next part too, and a set
//! of identical copies, which never picks one, is cut by count alone.
//!
//! The read side joins the parts back in order ([`join`]), so the rest of
//! the app only ever sees [`Placement::Instances`]. It reads no more copies
//! than the sanitiser keeps, however many parts a manifest names. A part
//! the listing lacks is skipped with a warning, as a missing child
//! generator is; a part it holds that this build cannot read makes the
//! whole set [`Placement::Unknown`], which a save refuses (#1111) rather
//! than writing the set back without its copies. Every build since #1111
//! reads the new type as unknown too, and so refuses to save the room rather
//! than losing them.

use std::collections::{HashMap, HashSet};

use bevy::log::warn;
use serde::{Deserialize, Serialize};

use crate::pds::generator::Placement;
use crate::pds::instances::InstanceCopy;
use crate::pds::sanitize::limits;

/// The manifest type of a set whose copies are kept in parts.
pub(crate) const INSTANCES_IN_PARTS: &str = "network.symbios.place.instancesInParts";

/// The type [`Placement::Instances`] is written with.
const INSTANCES: &str = "network.symbios.place.instances";

/// The most copies a set keeps inline in the manifest: a hand-placed row of
/// trees stays as it is.
pub(crate) const INLINE_COPIES: usize = 64;

/// The most copies the sets a room keeps inline may hold between them: past
/// it, its biggest small sets keep theirs in parts too ([`kept_in_parts`]).
/// At most about 46 KiB of copies at the widest numbers a copy can have,
/// 24 KiB as a Kiez's are written.
pub(crate) const INLINE_ROOM_COPIES: usize = 1_024;

/// The fewest copies a part holds before the copies may end it.
pub(crate) const PART_MIN: usize = 256;

/// The most copies a part holds: about 92 KiB at the widest numbers a copy
/// can be written with, under the 100 KiB record budget.
pub(crate) const PART_MAX: usize = 2_048;

/// One copy in this many ends a part ([`cut`]), so a part holds about
/// [`PART_MIN`] and this many copies more.
pub(crate) const PART_SPREAD: u64 = 1_024;

/// The most part keys a set is read with: one a copy, past which no set the
/// sanitiser keeps could need another.
const MAX_KEYS_READ: usize = limits::MAX_INSTANCE_COPIES;

/// One part of a long set's copies on the wire: a record in
/// [`crate::pds::ROOM_COPIES_COLLECTION`] at `rkey =` [`part_rkey`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RoomCopiesRecord {
    #[serde(rename = "$type")]
    pub lex_type: String,
    /// Required, and a list: a part without one is not a part this build
    /// can read.
    #[serde(deserialize_with = "crate::pds::instances::deserialize_part_copies")]
    pub copies: Vec<InstanceCopy>,
}

impl RoomCopiesRecord {
    fn new(copies: &[InstanceCopy]) -> Self {
        Self {
            lex_type: crate::pds::ROOM_COPIES_COLLECTION.into(),
            copies: copies.to_vec(),
        }
    }
}

/// The record key of `part`: lowercase hex of `fnv1a_64` over its bytes, as
/// a child generator's ([`super::child_rkey`]).
pub(crate) fn part_rkey(part: &RoomCopiesRecord) -> Result<String, String> {
    let canonical = serde_json::to_string(part).map_err(|e| format!("serialize: {e}"))?;
    Ok(format!(
        "{:016x}",
        crate::seeded_defaults::fnv1a_64(&canonical)
    ))
}

/// Whether a part ends after `copy` (see the module docs): its numbers,
/// FNV-1a hashed and mixed so the low bits carry the whole, a multiple of
/// [`PART_SPREAD`].
fn ends_a_part(copy: &InstanceCopy) -> bool {
    let words = [
        copy.x_cm as u32,
        copy.z_cm as u32,
        copy.yaw_ddeg as u32,
        copy.variant,
        copy.scale_pm,
        copy.y_cm as u32,
    ];
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in words.iter().flat_map(|word| word.to_le_bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    // MurmurHash3's finaliser: FNV's low bits follow its last bytes alone.
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    hash ^= hash >> 33;
    hash.is_multiple_of(PART_SPREAD)
}

/// `copies` cut into the parts they are kept in (see the module docs): in
/// order, together the whole list.
pub(crate) fn cut(copies: &[InstanceCopy]) -> Vec<&[InstanceCopy]> {
    let mut parts = Vec::new();
    let mut start = 0;
    for (i, copy) in copies.iter().enumerate() {
        let held = i + 1 - start;
        if held >= PART_MAX || (held >= PART_MIN && ends_a_part(copy)) {
            parts.push(&copies[start..=i]);
            start = i + 1;
        }
    }
    if start < copies.len() {
        parts.push(&copies[start..]);
    }
    parts
}

/// The copies `placement` holds, if it is a set.
fn copies_held(placement: &Placement) -> usize {
    match placement {
        Placement::Instances { copies, .. } => copies.len(),
        _ => 0,
    }
}

/// Which of `placements` keep their copies in parts: every set of more than
/// [`INLINE_COPIES`] copies, and, where the sets left inline would hold more
/// than [`INLINE_ROOM_COPIES`] between them, the biggest of those - the
/// first in the room among sets as big - until the rest fit.
pub(crate) fn kept_in_parts(placements: &[Placement]) -> Vec<bool> {
    let mut in_parts: Vec<bool> = placements
        .iter()
        .map(|placement| copies_held(placement) > INLINE_COPIES)
        .collect();
    let mut inline: Vec<(usize, usize)> = placements
        .iter()
        .enumerate()
        .map(|(index, placement)| (copies_held(placement), index))
        .filter(|(held, index)| *held > 0 && !in_parts[*index])
        .collect();
    let mut total: usize = inline.iter().map(|(held, _)| held).sum();
    inline.sort_by(|(a, i), (b, j)| b.cmp(a).then(i.cmp(j)));
    for (held, index) in inline {
        if total <= INLINE_ROOM_COPIES {
            break;
        }
        in_parts[index] = true;
        total -= held;
    }
    in_parts
}

/// A long set's parts, each with its record key, in order.
pub(crate) type Parts = Vec<(String, RoomCopiesRecord)>;

/// A placement as the manifest writes it.
#[derive(Serialize, Debug)]
#[serde(untagged)]
pub(crate) enum ManifestPlacement {
    /// As it is.
    Inline(Placement),
    /// A long set, its copies in parts: the set's fields as it writes
    /// them, its type [`INSTANCES_IN_PARTS`] and `parts` for `copies`.
    InParts(serde_json::Value),
}

/// `placement` as the manifest writes it - its copies in parts where
/// `in_parts` ([`kept_in_parts`]) and it holds any - with those parts, each
/// with its record key.
pub(crate) fn split(
    placement: &Placement,
    in_parts: bool,
) -> Result<(ManifestPlacement, Parts), String> {
    let Placement::Instances {
        generator_ref,
        transform,
        snap_to_terrain,
        solid,
        variants,
        copies,
    } = placement
    else {
        return Ok((ManifestPlacement::Inline(placement.clone()), Vec::new()));
    };
    if !in_parts || copies.is_empty() {
        return Ok((ManifestPlacement::Inline(placement.clone()), Vec::new()));
    }
    let parts = cut(copies)
        .into_iter()
        .map(|part| {
            let record = RoomCopiesRecord::new(part);
            Ok((part_rkey(&record)?, record))
        })
        .collect::<Result<Parts, String>>()?;
    // The set's other fields as it writes them, its copies left out.
    let bare = Placement::Instances {
        generator_ref: generator_ref.clone(),
        transform: transform.clone(),
        snap_to_terrain: *snap_to_terrain,
        solid: solid.clone(),
        variants: variants.clone(),
        copies: Vec::new(),
    };
    let mut value = serde_json::to_value(&bare).map_err(|e| format!("serialize: {e}"))?;
    let Some(fields) = value.as_object_mut() else {
        return Err("an instance set is not written as an object".into());
    };
    fields.insert("$type".into(), INSTANCES_IN_PARTS.into());
    fields.remove("copies");
    fields.insert(
        "parts".into(),
        parts
            .iter()
            .map(|(rkey, _)| serde_json::Value::from(rkey.as_str()))
            .collect(),
    );
    Ok((ManifestPlacement::InParts(value), parts))
}

/// Whether the manifest placement `value` keeps its copies in parts.
pub(crate) fn is_in_parts(value: &serde_json::Value) -> bool {
    value.get("$type").and_then(serde_json::Value::as_str) == Some(INSTANCES_IN_PARTS)
}

/// The record keys the manifest placements `placements` name, each set's
/// first [`MAX_KEYS_READ`]: the parts a fetch reads, and no others.
pub(crate) fn named_parts(placements: &[serde_json::Value]) -> HashSet<String> {
    placements
        .iter()
        .filter(|placement| is_in_parts(placement))
        .filter_map(|placement| placement.get("parts")?.as_array())
        .flat_map(|keys| {
            keys.iter()
                .take(MAX_KEYS_READ)
                .filter_map(|key| key.as_str().map(str::to_owned))
        })
        .collect()
}

/// A manifest placement read back (see the module docs): a set kept in
/// parts joined from `parts` - record key to its copies, or `None` where
/// the record is there but this build cannot read it - to no more copies
/// than the sanitiser keeps of a set, nor than `room_left`, what it keeps
/// of the room after the sets before; anything else read as it is.
pub(crate) fn join(
    mut value: serde_json::Value,
    parts: &HashMap<String, Option<Vec<InstanceCopy>>>,
    room_left: usize,
) -> Result<Placement, serde_json::Error> {
    if !is_in_parts(&value) {
        return serde_json::from_value(value);
    }
    let Some(fields) = value.as_object_mut() else {
        return Ok(Placement::Unknown);
    };
    // A list of keys, or a set this build cannot read.
    let Some(keys) = fields
        .remove("parts")
        .and_then(|keys| serde_json::from_value::<Vec<String>>(keys).ok())
    else {
        return Ok(Placement::Unknown);
    };
    // Copies past these the sanitiser would drop, so they are never read:
    // a key named a million times reads no more than a set holds.
    let kept = limits::MAX_INSTANCE_COPIES.min(room_left);
    let mut copies = Vec::new();
    let mut missing = 0;
    for key in keys.iter().take(MAX_KEYS_READ) {
        if copies.len() >= kept {
            break;
        }
        match parts.get(key) {
            Some(Some(part)) => {
                let take = part.len().min(kept - copies.len());
                copies.extend_from_slice(&part[..take]);
            }
            Some(None) => return Ok(Placement::Unknown),
            None => missing += 1,
        }
    }
    if missing > 0 {
        warn!("room manifest names {missing} copies parts the repo lacks - skipping their copies");
    }
    fields.insert("$type".into(), INSTANCES.into());
    fields.remove("copies");
    let mut placement: Placement = serde_json::from_value(value)?;
    if let Placement::Instances { copies: joined, .. } = &mut placement {
        *joined = copies;
    }
    Ok(placement)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{InstanceSolid, InstanceVariant, TransformData};

    /// `n` copies, each its own, as a seeded square's are.
    fn copies(n: usize, salt: i32) -> Vec<InstanceCopy> {
        (0..n as i32)
            .map(|i| InstanceCopy {
                x_cm: i * 97 + salt,
                z_cm: -i * 31,
                yaw_ddeg: (i * 7) % 3_600,
                variant: (i % 5) as u32,
                scale_pm: 900 + (i % 300) as u32,
                y_cm: 0,
            })
            .collect()
    }

    fn set_of(copies: Vec<InstanceCopy>) -> Placement {
        Placement::Instances {
            generator_ref: "oak".into(),
            transform: TransformData::default(),
            snap_to_terrain: true,
            solid: InstanceSolid::Trunk,
            variants: vec![InstanceVariant {
                seed: Some(7),
                params: Default::default(),
            }],
            copies,
        }
    }

    /// The parts of `copies` as their record keys.
    fn keys(copies: &[InstanceCopy]) -> Vec<String> {
        cut(copies)
            .into_iter()
            .map(|part| part_rkey(&RoomCopiesRecord::new(part)).expect("a key"))
            .collect()
    }

    /// The parts are the copies in order, none under PART_MIN but the last
    /// and none over PART_MAX, and the copies pick the cuts at about the
    /// spread asked.
    #[test]
    fn copies_are_cut_where_they_say_within_the_bounds() {
        let all = copies(40_000, 0);
        let parts = cut(&all);
        assert_eq!(parts.concat(), all, "together the whole list, in order");
        let (last, whole) = parts.split_last().expect("parts");
        assert!(!last.is_empty());
        for part in whole {
            assert!(
                (PART_MIN..=PART_MAX).contains(&part.len()),
                "{}",
                part.len()
            );
        }
        let mean = all.len() / parts.len();
        assert!(
            (PART_MIN + 500..PART_MIN + 1_600).contains(&mean),
            "about PART_MIN + PART_SPREAD a part: {mean}"
        );
        assert!(cut(&[]).is_empty());
    }

    /// An edit rewrites the part that holds it; a copy taken out or added
    /// moves the cuts only to the next one the copies pick, so every part
    /// after that keeps its key.
    #[test]
    fn an_edit_rewrites_the_parts_near_it_alone() {
        let all = copies(20_000, 3);
        let before = keys(&all);
        assert!(before.len() >= 8, "{}", before.len());

        let mut moved = all.clone();
        moved[5_000].x_cm += 100;
        let after = keys(&moved);
        let changed = before.iter().filter(|key| !after.contains(key)).count();
        assert!(changed <= 2, "a moved copy rewrote {changed} parts");

        let edits: [fn(&mut Vec<InstanceCopy>); 2] = [
            |list| {
                list.remove(5_000);
            },
            |list| list.insert(5_000, InstanceCopy::at(1.0, 2.0, 3.0)),
        ];
        for edit in edits {
            let mut edited = all.clone();
            edit(&mut edited);
            let after = keys(&edited);
            let kept = before.iter().filter(|key| after.contains(key)).count();
            assert!(
                kept + 3 >= before.len(),
                "kept {kept} of {} parts",
                before.len()
            );
            // The tail is untouched.
            assert_eq!(before.last(), after.last());
        }
    }

    /// A set of INLINE_COPIES or fewer is written as it always was; a longer
    /// one as INSTANCES_IN_PARTS with its parts' keys for its copies, which
    /// joins back to the set it was.
    #[test]
    fn a_long_set_splits_into_parts_and_joins_back() {
        let short = set_of(copies(INLINE_COPIES, 0));
        let (written, parts) = split(&short, false).expect("splits");
        assert!(parts.is_empty());
        assert_eq!(
            serde_json::to_value(&written).expect("serialises"),
            serde_json::to_value(&short).expect("serialises"),
            "a short set keeps its bytes"
        );

        let long = set_of(copies(3_000, 0));
        let (written, parts) = split(&long, true).expect("splits");
        assert!(parts.len() >= 2, "{}", parts.len());
        let value = serde_json::to_value(&written).expect("serialises");
        assert!(is_in_parts(&value));
        assert!(value.get("copies").is_none());
        assert_eq!(value["generator_ref"], "oak");
        assert_eq!(value["solid"], "trunk");
        assert_eq!(value["parts"].as_array().map(Vec::len), Some(parts.len()));

        let listed: HashMap<String, Option<Vec<InstanceCopy>>> = parts
            .iter()
            .map(|(key, part)| {
                // Through the wire, as the listing reads it.
                let json = serde_json::to_value(part).expect("serialises");
                let read: RoomCopiesRecord = serde_json::from_value(json).expect("reads");
                (key.clone(), Some(read.copies))
            })
            .collect();
        let joined = join(value, &listed, usize::MAX).expect("joins");
        assert_eq!(
            serde_json::to_value(&joined).expect("serialises"),
            serde_json::to_value(&long).expect("serialises")
        );
    }

    /// A part the listing lacks loses its copies alone; a part this build
    /// cannot read - no copies, or copies not a list - makes the set one it
    /// cannot read, so no save writes it back short.
    #[test]
    fn a_missing_part_is_skipped_and_an_unreadable_one_keeps_the_set_unread() {
        let long = set_of(copies(3_000, 9));
        let (written, parts) = split(&long, true).expect("splits");
        let value = serde_json::to_value(&written).expect("serialises");
        let mut listed: HashMap<String, Option<Vec<InstanceCopy>>> = parts
            .iter()
            .map(|(key, part)| (key.clone(), Some(part.copies.clone())))
            .collect();

        let (first_key, first) = &parts[0];
        listed.remove(first_key);
        let Placement::Instances { copies: left, .. } =
            join(value.clone(), &listed, usize::MAX).expect("joins")
        else {
            panic!("a set");
        };
        assert_eq!(left.len(), 3_000 - first.copies.len());

        for unreadable in [
            serde_json::json!({ "$type": crate::pds::ROOM_COPIES_COLLECTION }),
            serde_json::json!({ "$type": crate::pds::ROOM_COPIES_COLLECTION, "copies": "packed" }),
        ] {
            assert!(serde_json::from_value::<RoomCopiesRecord>(unreadable).is_err());
        }
        listed.insert(first_key.clone(), None);
        assert!(matches!(
            join(value, &listed, usize::MAX).expect("joins"),
            Placement::Unknown
        ));
    }

    /// A manifest naming one part a million times - or naming more parts
    /// than a set may hold - reads no more copies than the sanitiser keeps
    /// of a set, nor more than the room has left after the sets before.
    #[test]
    fn a_set_reads_no_more_copies_than_it_may_keep() {
        let part = copies(PART_MAX, 1);
        let key = String::from("0123456789abcdef");
        let listed: HashMap<String, Option<Vec<InstanceCopy>>> =
            HashMap::from([(key.clone(), Some(part))]);
        let (written, _) = split(&set_of(copies(3_000, 1)), true).expect("splits");
        let mut value = serde_json::to_value(&written).expect("serialises");
        value["parts"] = serde_json::Value::from(vec![key; 1_000_000]);
        let held = |placement: Placement| match placement {
            Placement::Instances { copies, .. } => copies.len(),
            _ => panic!("a set"),
        };
        let all = join(value.clone(), &listed, usize::MAX).expect("joins");
        assert_eq!(held(all), limits::MAX_INSTANCE_COPIES);
        assert_eq!(held(join(value, &listed, 100).expect("joins")), 100);
    }

    /// Sets of more than INLINE_COPIES keep their copies in parts; small
    /// sets stay inline until together they would pass INLINE_ROOM_COPIES,
    /// and then the biggest of them go into parts, first first, until the
    /// rest fit. An empty set and a placement of another kind stay as they
    /// are.
    #[test]
    fn a_room_of_many_small_sets_keeps_its_biggest_in_parts() {
        let room = |sizes: &[usize]| -> Vec<Placement> {
            sizes.iter().map(|n| set_of(copies(*n, 0))).collect()
        };
        assert_eq!(kept_in_parts(&room(&[64, 65, 0])), [false, true, false]);
        // Twenty sets of 60: 1,200 inline, so the first three of the
        // biggest - all equal - go, leaving 1,020.
        let many = room(&[60; 20]);
        let kept = kept_in_parts(&many);
        assert_eq!(kept.iter().filter(|in_parts| **in_parts).count(), 3);
        assert!(kept[..3].iter().all(|in_parts| *in_parts));
        // The biggest small set goes first.
        let mixed = room(&[
            10, 64, 40, 60, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64,
        ]);
        let kept = kept_in_parts(&mixed);
        let inline: usize = mixed
            .iter()
            .zip(&kept)
            .filter(|(_, in_parts)| !**in_parts)
            .map(|(placement, _)| copies_held(placement))
            .sum();
        assert!(inline <= INLINE_ROOM_COPIES, "{inline}");
        assert!(!kept[0] && !kept[2], "the smallest stay");
        let mut other = many.clone();
        other.push(Placement::Unknown);
        assert!(!kept_in_parts(&other)[20]);
    }

    /// A fetch reads the parts the manifest's sets in parts name, and no
    /// others.
    #[test]
    fn only_the_parts_a_set_names_are_read() {
        let (written, parts) = split(&set_of(copies(3_000, 2)), true).expect("splits");
        let inline = serde_json::to_value(set_of(copies(10, 0))).expect("serialises");
        let named = named_parts(&[serde_json::to_value(&written).expect("serialises"), inline]);
        assert_eq!(named.len(), parts.len());
        assert!(parts.iter().all(|(key, _)| named.contains(key)));
    }

    /// A build that knows no sets kept in parts - any from before #1645 -
    /// reads one as a placement it does not know, which it refuses to save
    /// (#1111), never as a set with no copies it would save back so.
    #[test]
    fn a_set_in_parts_is_a_placement_an_older_build_does_not_know() {
        let (written, _) = split(&set_of(copies(3_000, 0)), true).expect("splits");
        let value = serde_json::to_value(&written).expect("serialises");
        assert!(matches!(
            serde_json::from_value::<Placement>(value).expect("reads"),
            Placement::Unknown
        ));
    }

    /// A part's copies are read as a set's are: a malformed copy is dropped
    /// alone (#1119).
    #[test]
    fn a_parts_copies_are_read_leniently_one_by_one() {
        let part: RoomCopiesRecord = serde_json::from_value(serde_json::json!({
            "$type": crate::pds::ROOM_COPIES_COLLECTION,
            "copies": [[100, 200, 900], "nonsense", [1, 2], [300, 400, 0, 2, 1500]],
        }))
        .expect("reads");
        assert_eq!(part.copies.len(), 2);
        assert_eq!(part.copies[1].scale_pm, 1_500);
    }
}
