//! Instance sets (#1643, P2.1 of epic #1636): one generator placed at many
//! poses, each copy drawn with its own seed and named grammar values, and
//! all of them drawn merged ([`crate::world_builder::instances`]).
//!
//! A set is a placement ([`crate::pds::Placement::Instances`]): a
//! generator's name, an anchor, what a walker meets at a copy
//! ([`InstanceSolid`]), its variants and its copies. A **variant**
//! ([`InstanceVariant`]) is what a copy's grammars draw with: a seed in
//! place of every Shape node's own, as an absolute placement's seed is
//! (#1505), and named values every Shape node's grammar reads
//! ([`GrammarParams`]), so one street building's generator can stand at
//! every fit its kind has. A **copy** ([`InstanceCopy`]) is where one
//! stands and which variant it is.
//!
//! # A copy on the wire
//!
//! A Kiez is thousands of copies, so a copy is a list of whole numbers
//! rather than an object: `[x, z, yaw, variant, scale, y]` - centimetres
//! along the anchor's X and Z, its turn in tenths of a degree, the
//! variant's index, thousandths of the generator's own size, and
//! centimetres above the ground under it (above the anchor, in a set that
//! does not snap). A turn is clockwise seen from above, as the agent's
//! `yaw_deg` is: 0 faces the anchor's -Z, 900 its +X. Trailing defaults are
//! left out (variant 0, scale 1000, y 0), so most copies are three or four
//! numbers, about twenty bytes.
//!
//! A copy that is not a list of at least three whole numbers is dropped as
//! the record is read, and nothing else with it: bad data is never a bad
//! parse (#1119). A number past what its field holds saturates, for the
//! sanitiser to bound - a negative variant to one no set holds, so the
//! copy is dropped there - and numbers past the sixth, a newer build's, are
//! read past. A variant the record cannot read - a seed not written as a
//! quoted decimal, a value not a whole number - is read as one that draws
//! the generator as it is, keeping its place, so the copies of the
//! variants after it keep theirs.

use std::collections::BTreeMap;

use serde::de::IgnoredAny;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::generator::{Generator, Placement};
use super::room::RoomRecord;
use super::sanitize::limits;
use super::types::{FP_SCALE, Fp, Fp3, Fp4, option_u64_as_string};

/// Named values a grammar reads (#1643): each set on every Shape node of a
/// placed tree as a host attribute, over the grammar's own `attr` or
/// `const` of that name, so a grammar written at one fit draws another.
/// Names are grammar identifiers; values are the record's fixed point. The
/// map is sorted, so its bytes are canonical.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GrammarParams(pub BTreeMap<String, Fp>);

impl GrammarParams {
    /// No values: the skip predicate, so a placement that sets none keeps
    /// its bytes.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Each name with its value as the wire holds it: what a variant's
    /// identity is hashed from, so two maps that write the same bytes are
    /// one variant.
    pub fn wire(&self) -> impl Iterator<Item = (&str, i32)> + '_ {
        self.0
            .iter()
            .map(|(name, value)| (name.as_str(), (value.0 * FP_SCALE).round() as i32))
    }

    /// The values as a grammar reads them.
    pub fn attrs(&self) -> impl Iterator<Item = (&str, f64)> + '_ {
        self.0
            .iter()
            .map(|(name, value)| (name.as_str(), f64::from(value.0)))
    }
}

/// The key a Shape node's derivation is cached under when it draws with
/// `seed` and `params` (#1505, #1643): the seed alone where no values are
/// set - so a plain seed shares the derivation an absolute placement's seed
/// made - and a hash of the two where they are.
pub fn shape_variant_key(seed: u64, params: &GrammarParams) -> u64 {
    if params.is_empty() {
        return seed;
    }
    let mut text = seed.to_string();
    for (name, value) in params.wire() {
        text.push('|');
        text.push_str(name);
        text.push('=');
        text.push_str(&value.to_string());
    }
    crate::seeded_defaults::fnv1a_64(&text)
}

/// What a copy's grammars draw with (#1643): a seed in place of every Shape
/// node's own, as an absolute placement's is (#1505), and the values they
/// read. A variant with neither draws the generator as it is.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InstanceVariant {
    /// A quoted decimal on the wire, like every seed, and left out when
    /// `None`.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "option_u64_as_string"
    )]
    pub seed: Option<u64>,
    #[serde(default, skip_serializing_if = "GrammarParams::is_empty")]
    pub params: GrammarParams,
}

impl InstanceVariant {
    /// The key a Shape node whose own seed is `own` caches this variant's
    /// derivation under ([`shape_variant_key`]).
    pub fn shape_key(&self, own: u64) -> u64 {
        shape_variant_key(self.seed.unwrap_or(own), &self.params)
    }
}

/// What a walker meets at a copy of a set (#1643): copies are drawn merged,
/// so their parts carry no colliders of their own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum InstanceSolid {
    /// The copy's 1 m voxel shell: a building's.
    #[default]
    Shell,
    /// The box its parts fill: a prop's.
    Box,
    /// A trunk as wide as its base, up to 3 m tall, under a crown a walker
    /// passes through: a tree's.
    Trunk,
    /// Nothing: ground cover a walker wades through.
    None,
    /// A word this build does not know: written back as it was read, and
    /// met as [`Self::Shell`].
    Other(String),
}

impl InstanceSolid {
    /// Its word on the wire.
    pub fn word(&self) -> &str {
        match self {
            InstanceSolid::Shell => "shell",
            InstanceSolid::Box => "box",
            InstanceSolid::Trunk => "trunk",
            InstanceSolid::None => "none",
            InstanceSolid::Other(word) => word,
        }
    }

    /// The skip predicate: a shell, the default, is left out.
    pub fn is_shell(&self) -> bool {
        *self == InstanceSolid::Shell
    }

    fn from_word(word: String) -> Self {
        match word.as_str() {
            "shell" => InstanceSolid::Shell,
            "box" => InstanceSolid::Box,
            "trunk" => InstanceSolid::Trunk,
            "none" => InstanceSolid::None,
            _ => InstanceSolid::Other(word),
        }
    }
}

impl Serialize for InstanceSolid {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.word())
    }
}

impl<'de> Deserialize<'de> for InstanceSolid {
    /// Any word is read; anything that is not a word is a shell (#1119).
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Word(String),
            Other(IgnoredAny),
        }
        Ok(match Wire::deserialize(d)? {
            Wire::Word(word) => InstanceSolid::from_word(word),
            Wire::Other(_) => InstanceSolid::Shell,
        })
    }
}

/// The scale a copy is drawn at when its list leaves it out (thousandths).
pub const COPY_SCALE_ONE: u32 = 1_000;

/// One copy of a set (#1643): where it stands, turned and sized, and which
/// variant it is. See the module docs for its wire form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InstanceCopy {
    /// Along the anchor's X (cm).
    pub x_cm: i32,
    /// Along the anchor's Z (cm).
    pub z_cm: i32,
    /// Its turn about the anchor's up axis, clockwise seen from above, in
    /// tenths of a degree.
    pub yaw_ddeg: i32,
    /// Which of the set's variants it draws.
    pub variant: u32,
    /// Its size, in thousandths of the generator's own.
    pub scale_pm: u32,
    /// Above the ground under it, or above the anchor in a set that does
    /// not snap (cm).
    pub y_cm: i32,
}

impl InstanceCopy {
    /// A copy of variant 0 at `(x, z)` metres turned `yaw_deg` clockwise,
    /// at the generator's own size, on the ground: each quantised as the
    /// wire holds it.
    pub fn at(x: f32, z: f32, yaw_deg: f32) -> Self {
        InstanceCopy {
            x_cm: centimetres(x),
            z_cm: centimetres(z),
            yaw_ddeg: tenths_of_degree(yaw_deg),
            variant: 0,
            scale_pm: COPY_SCALE_ONE,
            y_cm: 0,
        }
    }

    /// Where it stands in the anchor's frame (m): `y` is its height above
    /// the ground, or above the anchor in a set that does not snap.
    pub fn position(&self) -> [f32; 3] {
        [
            self.x_cm as f32 / 100.0,
            self.y_cm as f32 / 100.0,
            self.z_cm as f32 / 100.0,
        ]
    }

    /// Its turn about the anchor's up axis (see [`Self::yaw_ddeg`]).
    pub fn rotation(&self) -> bevy::prelude::Quat {
        bevy::prelude::Quat::from_rotation_y(-(self.yaw_ddeg as f32 / 10.0).to_radians())
    }

    /// Its size as a share of the generator's own.
    pub fn scale(&self) -> f32 {
        self.scale_pm as f32 / COPY_SCALE_ONE as f32
    }

    /// The numbers it is written as, its trailing defaults left out.
    fn wire(&self) -> Vec<i64> {
        let mut numbers = vec![
            i64::from(self.x_cm),
            i64::from(self.z_cm),
            i64::from(self.yaw_ddeg),
            i64::from(self.variant),
            i64::from(self.scale_pm),
            i64::from(self.y_cm),
        ];
        let defaults = [0, 0, 0, 0, i64::from(COPY_SCALE_ONE), 0];
        while numbers.len() > 3 && numbers.last() == defaults.get(numbers.len() - 1) {
            numbers.pop();
        }
        numbers
    }

    /// The copy a list of numbers is, or `None` for fewer than three.
    /// Each saturates into its field; numbers past the sixth are read past.
    fn from_wire(numbers: &[i64]) -> Option<Self> {
        let [x, z, yaw, rest @ ..] = numbers else {
            return None;
        };
        let signed = |v: i64| v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        let unsigned = |v: i64| v.clamp(0, i64::from(u32::MAX)) as u32;
        Some(InstanceCopy {
            x_cm: signed(*x),
            z_cm: signed(*z),
            yaw_ddeg: signed(*yaw),
            // A negative index is no variant at all, not the first one.
            variant: rest
                .first()
                .map_or(0, |v| if *v < 0 { u32::MAX } else { unsigned(*v) }),
            scale_pm: rest.get(1).map_or(COPY_SCALE_ONE, |v| unsigned(*v)),
            y_cm: rest.get(2).map_or(0, |v| signed(*v)),
        })
    }
}

/// Metres as the whole centimetres a copy is written in, saturating.
pub fn centimetres(metres: f32) -> i32 {
    let cm = (metres * 100.0).round();
    if cm.is_finite() {
        cm.clamp(i32::MIN as f32, i32::MAX as f32) as i32
    } else {
        0
    }
}

/// Degrees as the whole tenths a copy's turn is written in, in `0..3600`.
pub fn tenths_of_degree(degrees: f32) -> i32 {
    let tenths = (degrees * 10.0).round();
    if tenths.is_finite() {
        (tenths.rem_euclid(3_600.0) as i32).rem_euclid(3_600)
    } else {
        0
    }
}

impl Serialize for InstanceCopy {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.wire().serialize(s)
    }
}

/// One number of a copy's list, or anything else.
#[derive(Deserialize)]
#[serde(untagged)]
enum CopyNumber {
    Whole(i64),
    /// Past `i64`: saturated, as every number past its field is.
    Huge(u64),
    Other(IgnoredAny),
}

/// One copy as written: a list, or anything else.
#[derive(Deserialize)]
#[serde(untagged)]
enum CopyWire {
    List(Vec<CopyNumber>),
    Other(IgnoredAny),
}

/// The copies `copies` reads as: each list of at least three whole numbers
/// a copy, anything else dropped (see the module docs).
fn read_copies(copies: Vec<CopyWire>) -> Vec<InstanceCopy> {
    copies
        .into_iter()
        .filter_map(|copy| {
            let CopyWire::List(numbers) = copy else {
                return None;
            };
            let whole: Option<Vec<i64>> = numbers
                .into_iter()
                .map(|n| match n {
                    CopyNumber::Whole(v) => Some(v),
                    CopyNumber::Huge(v) => Some(i64::try_from(v).unwrap_or(i64::MAX)),
                    CopyNumber::Other(_) => None,
                })
                .collect();
            InstanceCopy::from_wire(&whole?)
        })
        .collect()
}

/// A set's copies as the record reads them: each list of at least three
/// whole numbers a copy, anything else dropped (see the module docs).
pub(crate) fn deserialize_copies<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<InstanceCopy>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Copies {
        List(Vec<CopyWire>),
        Other(IgnoredAny),
    }
    let Copies::List(copies) = Copies::deserialize(d)? else {
        return Ok(Vec::new());
    };
    Ok(read_copies(copies))
}

/// The copies of one part of a long set (#1645), as a set's are read - but
/// they must be a list: a part whose copies are anything else is one this
/// build cannot read, and the set it belongs to is not read either, so a
/// save cannot write it back without them.
pub(crate) fn deserialize_part_copies<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<InstanceCopy>, D::Error> {
    Ok(read_copies(Vec::<CopyWire>::deserialize(d)?))
}

/// A set's variants as the record reads them: each it cannot read is read
/// as the default, in its place (see the module docs).
pub(crate) fn deserialize_variants<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<InstanceVariant>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Wire {
        Read(InstanceVariant),
        Other(IgnoredAny),
    }
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Variants {
        List(Vec<Wire>),
        Other(IgnoredAny),
    }
    let Variants::List(variants) = Variants::deserialize(d)? else {
        return Ok(Vec::new());
    };
    Ok(variants
        .into_iter()
        .map(|variant| match variant {
            Wire::Read(variant) => variant,
            Wire::Other(_) => InstanceVariant::default(),
        })
        .collect())
}

/// How many copies the sets of `record` hold in all.
pub fn room_copies(record: &RoomRecord) -> usize {
    record
        .placements
        .iter()
        .map(|placement| match placement {
            Placement::Instances { copies, .. } => copies.len(),
            _ => 0,
        })
        .sum()
}

/// Why no copy can be added to the set `placement` of `record`, or `None`
/// where one can: the set holds all a set may, or the room's sets all a
/// room's may - the sanitiser would drop a copy past either.
pub fn copy_refusal(record: &RoomRecord, placement: usize) -> Option<&'static str> {
    let Some(Placement::Instances { copies, .. }) = record.placements.get(placement) else {
        return Some("That is no set.");
    };
    if copies.len() >= limits::MAX_INSTANCE_COPIES {
        Some("A set holds no more copies.")
    } else if room_copies(record) >= limits::MAX_ROOM_INSTANCE_COPIES {
        Some("The world's sets hold no more copies.")
    } else {
        None
    }
}

/// No placement can be added.
const PLACEMENTS_FULL: &str = "The world holds no more placements.";
/// No item can be added for a copy of another size.
const ITEMS_FULL: &str =
    "A copy of another size needs an item of its own, and the world holds no more items.";

/// Why copy `copy` of the set `placement` cannot be detached
/// ([`detach_copy`]), or `None` where it can: what the World Editor paints
/// beside a greyed Detach.
pub fn detach_refusal(record: &RoomRecord, placement: usize, copy: usize) -> Option<&'static str> {
    detachable(record, placement, copy).err()
}

/// A copy as it will be detached.
struct Detached {
    numbers: InstanceCopy,
    variant: InstanceVariant,
    /// The item it is drawn from.
    item: String,
    /// The item to make under that name, where one must be ([`sized_item`]).
    make: Option<Generator>,
}

/// Copy `copy` of the set `placement` as it will be detached, or why it
/// cannot be.
fn detachable(
    record: &RoomRecord,
    placement: usize,
    copy: usize,
) -> Result<Detached, &'static str> {
    if record.placements.len() >= limits::MAX_PLACEMENTS {
        return Err(PLACEMENTS_FULL);
    }
    let Some(Placement::Instances {
        generator_ref,
        variants,
        copies,
        ..
    }) = record.placements.get(placement)
    else {
        return Err("That is no set.");
    };
    let Some(numbers) = copies.get(copy).copied() else {
        return Err("The set holds no such copy.");
    };
    let variant = variants
        .get(numbers.variant as usize)
        .cloned()
        .unwrap_or_default();
    let (item, make) = if numbers.scale_pm == COPY_SCALE_ONE {
        (generator_ref.clone(), None)
    } else {
        sized_item(record, generator_ref, numbers.scale_pm)?
    };
    Ok(Detached {
        numbers,
        variant,
        item,
        make,
    })
}

/// Detach copy `copy` of the set `placement` into an absolute placement of
/// its own (#1644), drawn as it was: where it stood - on the ground, raised
/// by its own lift, where the set snaps - turned as it was, with its
/// variant's seed and values. A copy of another size than its item's is
/// drawn from an item of its own at that size ([`sized_item`]): an absolute
/// placement draws its item at its own size (#1454). The new placement's
/// index, or why it could not be made ([`detach_refusal`]).
pub fn detach_copy(
    record: &mut RoomRecord,
    placement: usize,
    copy: usize,
) -> Result<usize, &'static str> {
    let Detached {
        numbers,
        variant,
        item,
        make,
    } = detachable(record, placement, copy)?;
    let Some(Placement::Instances {
        generator_ref,
        transform,
        snap_to_terrain,
        copies,
        ..
    }) = record.placements.get_mut(placement)
    else {
        return Err("That is no set.");
    };
    let anchor = bevy::prelude::Transform::from(&*transform).with_scale(bevy::prelude::Vec3::ONE);
    let [x, y, z] = numbers.position();
    let mut at = anchor.translation + anchor.rotation * bevy::prelude::Vec3::new(x, y, z);
    if *snap_to_terrain {
        // Snapped, an absolute placement's height is its lift, as a copy's.
        at = anchor.translation + anchor.rotation * bevy::prelude::Vec3::new(x, 0.0, z);
        at.y = y;
    }
    let rotation = anchor.rotation * numbers.rotation();
    let snap = *snap_to_terrain;
    let base = generator_ref.clone();
    copies.remove(copy);
    if let Some(sized) = make {
        // The traits its item has, and no others a name once held.
        match record.traits.get(&base).cloned() {
            Some(traits) => record.traits.insert(item.clone(), traits),
            None => record.traits.remove(&item),
        };
        record.generators.insert(item.clone(), sized);
    }
    record.placements.push(Placement::Absolute {
        generator_ref: item,
        transform: super::types::TransformData {
            translation: Fp3(at.to_array()),
            rotation: Fp4(rotation.to_array()),
            scale: Fp3([1.0; 3]),
        },
        snap_to_terrain: snap,
        avoid_water: false,
        avoid_water_clearance: Fp(0.0),
        seed: variant.seed,
        params: variant.params,
    });
    Ok(record.placements.len() - 1)
}

/// The item a copy of `item` at `scale_pm` thousandths of its size is
/// drawn from once detached: `item` with its root's whole frame scaled
/// about the copy's own origin. It is named `<item>_x<scale_pm>` - or that
/// with `_2`, `_3`... where an item of the name holds something else - and
/// an item of the name holding just that, with `item`'s traits, is shared,
/// so every copy detached at one size draws from one item. Its name, and
/// the item to make under it, `None` where the world holds it already.
fn sized_item(
    record: &RoomRecord,
    item: &str,
    scale_pm: u32,
) -> Result<(String, Option<Generator>), &'static str> {
    let Some(mut sized) = record.generators.get(item).cloned() else {
        return Err("The set's item is missing.");
    };
    let s = scale_pm as f32 / COPY_SCALE_ONE as f32;
    let t = &mut sized.transform;
    t.translation = Fp3(t.translation.0.map(|v| v * s));
    t.scale = Fp3(t.scale.0.map(|v| v * s));
    let traits = record.traits.get(item);
    // One name of these is free past every item the world holds.
    for n in 1..=record.generators.len() + 1 {
        let suffix = if n == 1 {
            format!("_x{scale_pm}")
        } else {
            format!("_x{scale_pm}_{n}")
        };
        // Cut to the longest name an item may have, as the sanitiser would
        // cut it, keeping the size.
        let room = limits::MAX_GENERATOR_NAME_CHARS.saturating_sub(suffix.chars().count());
        let name = format!("{}{suffix}", item.chars().take(room).collect::<String>());
        match record.generators.get(&name) {
            Some(held) if *held == sized && record.traits.get(&name) == traits => {
                return Ok((name, None));
            }
            Some(_) => {}
            None if record.generators.len() >= limits::MAX_GENERATORS => return Err(ITEMS_FULL),
            None => return Ok((name, Some(sized))),
        }
    }
    Err(ITEMS_FULL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{TransformData, sanitize::limits};
    use bevy::prelude::{Quat, Transform, Vec3};

    /// A room of one set of `post`, anchored at `anchor`, its copies
    /// `copies`, with a variant giving a seed and a value.
    fn room_with_set(snap: bool, anchor: TransformData, copies: Vec<InstanceCopy>) -> RoomRecord {
        let mut record = RoomRecord::default();
        record.generators.clear();
        record.placements.clear();
        record.traits.clear();
        let mut post = crate::pds::Generator::default_cuboid();
        post.transform.translation = Fp3([0.0, 0.5, 0.0]);
        record.generators.insert("post".into(), post);
        record.traits.insert("post".into(), vec!["sensor".into()]);
        record.placements.push(Placement::Instances {
            generator_ref: "post".into(),
            transform: anchor,
            snap_to_terrain: snap,
            solid: InstanceSolid::Shell,
            variants: vec![
                InstanceVariant::default(),
                InstanceVariant {
                    seed: Some(42),
                    params: GrammarParams(std::collections::BTreeMap::from([(
                        "Storeys".to_string(),
                        Fp(5.0),
                    )])),
                },
            ],
            copies,
        });
        record
    }

    /// #1644: a detached copy is a placement of its own where it stood - a
    /// snapped one's height its lift - turned as it was, with its variant's
    /// seed and values, and gone from its set.
    #[test]
    fn a_detached_copy_stands_where_it_stood_with_its_variant() {
        let turned = TransformData {
            translation: Fp3([10.0, 3.0, -2.0]),
            rotation: crate::pds::Fp4(Quat::from_rotation_y(0.5).to_array()),
            ..TransformData::default()
        };
        let copy = InstanceCopy {
            variant: 1,
            y_cm: -35,
            ..InstanceCopy::at(4.0, 1.0, 90.0)
        };
        for snap in [true, false] {
            let mut record = room_with_set(
                snap,
                turned.clone(),
                vec![copy, InstanceCopy::at(0.0, 0.0, 0.0)],
            );
            let detached = detach_copy(&mut record, 0, 0).expect("detached");
            assert_eq!(detached, 1);
            let Placement::Instances { copies, .. } = &record.placements[0] else {
                panic!("a set");
            };
            assert_eq!(copies.len(), 1, "the copy left its set");
            let Placement::Absolute {
                generator_ref,
                transform,
                snap_to_terrain,
                seed,
                params,
                ..
            } = &record.placements[1]
            else {
                panic!("an absolute placement");
            };
            assert_eq!(generator_ref, "post");
            assert_eq!(*snap_to_terrain, snap);
            assert_eq!(*seed, Some(42));
            assert_eq!(params.0.get("Storeys"), Some(&Fp(5.0)));
            let anchor = Transform::from(&turned);
            let on_plan = anchor.translation + anchor.rotation * Vec3::new(4.0, 0.0, 1.0);
            let [x, y, z] = transform.translation.0;
            assert!((x - on_plan.x).abs() < 1e-4 && (z - on_plan.z).abs() < 1e-4);
            let expected_y = if snap {
                -0.35
            } else {
                (anchor.translation + anchor.rotation * Vec3::new(4.0, -0.35, 1.0)).y
            };
            assert!((y - expected_y).abs() < 1e-4, "{y} {expected_y}");
            let rotation = Quat::from_array(transform.rotation.0);
            assert!(rotation.abs_diff_eq(anchor.rotation * copy.rotation(), 1e-5));
        }
    }

    /// A copy of another size is given an item of its own at that size -
    /// its whole frame scaled, its traits with it - shared by every copy
    /// detached at that size; with the world's items full, it is refused.
    #[test]
    fn a_copy_of_another_size_is_given_an_item_at_that_size() {
        let big = InstanceCopy {
            scale_pm: 2_000,
            ..InstanceCopy::at(1.0, 1.0, 0.0)
        };
        let mut record = room_with_set(true, TransformData::default(), vec![big, big]);
        detach_copy(&mut record, 0, 0).expect("detached");
        detach_copy(&mut record, 0, 0).expect("detached");
        let sized = record
            .generators
            .get("post_x2000")
            .expect("an item at that size");
        assert_eq!(sized.transform.translation.0, [0.0, 1.0, 0.0]);
        assert_eq!(sized.transform.scale.0, [2.0, 2.0, 2.0]);
        assert_eq!(
            record.traits.get("post_x2000"),
            Some(&vec!["sensor".to_string()])
        );
        assert_eq!(record.generators.len(), 2, "shared by both");

        let mut full = room_with_set(true, TransformData::default(), vec![big]);
        for i in full.generators.len()..limits::MAX_GENERATORS {
            full.generators.insert(
                format!("filler{i}"),
                crate::pds::Generator::default_cuboid(),
            );
        }
        assert_eq!(detach_refusal(&full, 0, 0), Some(ITEMS_FULL));
        assert!(detach_copy(&mut full, 0, 0).is_err());
        let Placement::Instances { copies, .. } = &full.placements[0] else {
            panic!("a set");
        };
        assert_eq!(copies.len(), 1, "a refused detach leaves the copy");
    }

    /// An item of a copy's size is shared only while it holds just what the
    /// copy is drawn from: once its set's item changed, the next copy
    /// detached at that size gets an item of its own under the next name,
    /// and a world full of items still detaches a copy whose item it holds.
    #[test]
    fn a_sized_item_is_shared_only_while_it_holds_the_same() {
        let big = InstanceCopy {
            scale_pm: 1_500,
            ..InstanceCopy::at(1.0, 1.0, 0.0)
        };
        let mut record = room_with_set(true, TransformData::default(), vec![big; 4]);
        detach_copy(&mut record, 0, 0).expect("detached");
        // The set's item edited: the sized item made before no longer is
        // the copies' item at that size.
        if let Some(post) = record.generators.get_mut("post") {
            post.transform.translation = Fp3([0.0, 2.0, 0.0]);
        }
        let second = detach_copy(&mut record, 0, 0).expect("detached");
        assert_eq!(
            record.placements[second].generator_ref(),
            Some("post_x1500_2")
        );
        assert_eq!(
            record.generators["post_x1500"].transform.translation.0,
            [0.0, 0.75, 0.0],
            "the first is left as it was"
        );
        assert_eq!(
            record.generators["post_x1500_2"].transform.translation.0,
            [0.0, 3.0, 0.0]
        );
        let third = detach_copy(&mut record, 0, 0).expect("detached");
        assert_eq!(
            record.placements[third].generator_ref(),
            Some("post_x1500_2"),
            "shared while it holds the same"
        );
        for i in record.generators.len()..limits::MAX_GENERATORS {
            record.generators.insert(
                format!("filler{i}"),
                crate::pds::Generator::default_cuboid(),
            );
        }
        assert_eq!(detach_refusal(&record, 0, 0), None, "its item is held");
        detach_copy(&mut record, 0, 0).expect("detached into the held item");
    }

    /// No copy is added past a set's cap, nor past the room's across all
    /// its sets (#1644): the sanitiser would drop it.
    #[test]
    fn a_copy_is_refused_past_the_set_or_the_room() {
        let one = InstanceCopy::at(0.0, 0.0, 0.0);
        let mut record = room_with_set(false, TransformData::default(), vec![one]);
        assert_eq!(copy_refusal(&record, 0), None);
        assert!(copy_refusal(&record, 1).is_some(), "no set there");
        let full_set = room_with_set(
            false,
            TransformData::default(),
            vec![one; limits::MAX_INSTANCE_COPIES],
        );
        assert!(copy_refusal(&full_set, 0).is_some());
        // More sets bring the room to its cap, the first set far from its
        // own: full ones, and one holding what is left.
        let full = full_set.placements[0].clone();
        while room_copies(&record) + limits::MAX_INSTANCE_COPIES <= limits::MAX_ROOM_INSTANCE_COPIES
        {
            record.placements.push(full.clone());
        }
        let mut rest = full;
        if let Placement::Instances { copies, .. } = &mut rest {
            copies.truncate(limits::MAX_ROOM_INSTANCE_COPIES - room_copies(&record));
        }
        record.placements.push(rest);
        assert_eq!(room_copies(&record), limits::MAX_ROOM_INSTANCE_COPIES);
        assert!(copy_refusal(&record, 0).is_some());
    }

    fn wire_of(copy: InstanceCopy) -> String {
        serde_json::to_string(&copy).expect("serialise")
    }

    fn read(json: &str) -> Vec<InstanceCopy> {
        #[derive(Deserialize)]
        struct Set {
            #[serde(deserialize_with = "deserialize_copies")]
            copies: Vec<InstanceCopy>,
        }
        serde_json::from_str::<Set>(json).expect("a set").copies
    }

    #[test]
    fn a_copy_leaves_its_trailing_defaults_out() {
        let mut copy = InstanceCopy::at(12.34, -5.0, 90.0);
        assert_eq!(wire_of(copy), "[1234,-500,900]");
        copy.variant = 3;
        assert_eq!(wire_of(copy), "[1234,-500,900,3]");
        copy.y_cm = -35;
        assert_eq!(wire_of(copy), "[1234,-500,900,3,1000,-35]");
        copy.variant = 0;
        copy.y_cm = 0;
        copy.scale_pm = 1_250;
        assert_eq!(wire_of(copy), "[1234,-500,900,0,1250]");
    }

    #[test]
    fn a_copy_reads_back_as_it_was_written() {
        let copies = [
            InstanceCopy::at(0.0, 0.0, 0.0),
            InstanceCopy {
                x_cm: -98_765,
                z_cm: 43_210,
                yaw_ddeg: 3_599,
                variant: 41,
                scale_pm: 740,
                y_cm: 12,
            },
        ];
        let json = serde_json::to_string(&serde_json::json!({ "copies": copies })).unwrap();
        assert_eq!(read(&json), copies);
    }

    /// #1119: a copy the record cannot read is dropped, never the record;
    /// a number past its field saturates; a newer build's seventh number is
    /// read past.
    #[test]
    fn a_copy_that_cannot_be_read_drops_alone() {
        let copies = read(
            r#"{"copies": [[100, 200, 300], [1, 2], "a copy", [1.5, 2, 3], {"x": 1},
                [99999999999, -5, 10, 2, -7, 3, 77], [4, 5, 6]]}"#,
        );
        assert_eq!(
            copies,
            [
                InstanceCopy {
                    x_cm: 100,
                    z_cm: 200,
                    yaw_ddeg: 300,
                    variant: 0,
                    scale_pm: COPY_SCALE_ONE,
                    y_cm: 0,
                },
                InstanceCopy {
                    x_cm: i32::MAX,
                    z_cm: -5,
                    yaw_ddeg: 10,
                    variant: 2,
                    scale_pm: 0,
                    y_cm: 3,
                },
                InstanceCopy {
                    x_cm: 4,
                    z_cm: 5,
                    yaw_ddeg: 6,
                    variant: 0,
                    scale_pm: COPY_SCALE_ONE,
                    y_cm: 0,
                },
            ]
        );
        assert!(read(r#"{"copies": "none"}"#).is_empty());
    }

    /// A number past `i64` saturates as any number past its field does, and
    /// a negative variant is none the set holds, not its first.
    #[test]
    fn a_number_past_its_field_saturates_and_a_negative_variant_is_none() {
        let copies = read(r#"{"copies": [[18446744073709551615, 0, 0, -1]]}"#);
        assert_eq!(copies.len(), 1);
        assert_eq!(copies[0].x_cm, i32::MAX);
        assert_eq!(copies[0].variant, u32::MAX);
    }

    /// #1119: a variant the record cannot read is read as the default in
    /// its place - the copies of the variants after it keep theirs - and
    /// the placement around it is read, never refused.
    #[test]
    fn a_variant_that_cannot_be_read_draws_the_generator_as_it_is() {
        let set: crate::pds::Placement = serde_json::from_value(serde_json::json!({
            "$type": "network.symbios.place.instances",
            "generator_ref": "house",
            "variants": [{"seed": 11}, {"params": {"W": 4.5}}, {"seed": "12"}],
            "copies": [[0, 0, 0, 2]],
        }))
        .expect("the set reads");
        let crate::pds::Placement::Instances {
            variants, copies, ..
        } = set
        else {
            panic!("a set");
        };
        assert_eq!(
            variants,
            [
                InstanceVariant::default(),
                InstanceVariant::default(),
                InstanceVariant {
                    seed: Some(12),
                    params: GrammarParams::default(),
                },
            ]
        );
        assert_eq!(copies[0].variant, 2);
    }

    #[test]
    fn a_turn_is_held_in_tenths_of_a_degree_round_the_circle() {
        assert_eq!(tenths_of_degree(90.0), 900);
        assert_eq!(tenths_of_degree(-90.0), 2_700);
        assert_eq!(tenths_of_degree(359.96), 0);
        assert_eq!(tenths_of_degree(f32::NAN), 0);
        assert_eq!(centimetres(f32::INFINITY), 0);
        assert_eq!(centimetres(-1.004), -100);
    }

    #[test]
    fn a_solid_word_this_build_does_not_know_is_kept() {
        let read = |json: &str| serde_json::from_str::<InstanceSolid>(json).expect("read");
        assert_eq!(read(r#""trunk""#), InstanceSolid::Trunk);
        assert_eq!(read(r#""hull""#), InstanceSolid::Other("hull".into()));
        assert_eq!(read("7"), InstanceSolid::Shell);
        assert_eq!(
            serde_json::to_string(&InstanceSolid::Other("hull".into())).unwrap(),
            r#""hull""#
        );
    }

    /// A placement as a client built before #1643 had it: an absolute
    /// placement with its seed and no values, and no instance sets - every
    /// other kind it read as `Unknown`, as it reads this one.
    mod before_1643 {
        use crate::pds::types::{Fp, TransformData, default_true, is_false, is_true};
        use serde::{Deserialize, Serialize};

        #[derive(Serialize, Deserialize, Debug)]
        #[serde(tag = "$type")]
        pub(super) enum Placement {
            #[serde(rename = "network.symbios.place.absolute")]
            Absolute {
                generator_ref: String,
                #[serde(default, skip_serializing_if = "TransformData::is_identity")]
                transform: TransformData,
                #[serde(default = "default_true", skip_serializing_if = "is_true")]
                snap_to_terrain: bool,
                #[serde(default, skip_serializing_if = "is_false")]
                avoid_water: bool,
                #[serde(default)]
                avoid_water_clearance: Fp,
                #[serde(
                    default,
                    skip_serializing_if = "Option::is_none",
                    with = "crate::pds::types::option_u64_as_string"
                )]
                seed: Option<u64>,
            },
            #[serde(other, skip_serializing)]
            Unknown,
        }
    }

    fn a_set() -> crate::pds::Placement {
        crate::pds::Placement::Instances {
            generator_ref: "house".into(),
            transform: crate::pds::TransformData::default(),
            snap_to_terrain: true,
            solid: InstanceSolid::Shell,
            variants: Vec::new(),
            copies: vec![InstanceCopy::at(1.0, 2.0, 0.0)],
        }
    }

    /// A set at its defaults is its kind, its generator and its copies.
    #[test]
    fn a_set_leaves_its_defaults_out() {
        assert_eq!(
            serde_json::to_string(&a_set()).expect("serialises"),
            r#"{"$type":"network.symbios.place.instances","generator_ref":"house","copies":[[100,200,0]]}"#
        );
    }

    /// Every field set reads back as it was written: the anchor, snapping,
    /// the solid, a variant's quoted seed and scaled values, and the copies.
    #[test]
    fn a_set_reads_back_as_it_was_written() {
        let set = crate::pds::Placement::Instances {
            generator_ref: "street_house".into(),
            transform: crate::pds::TransformData {
                translation: crate::pds::Fp3([12.0, 0.0, -4.5]),
                ..Default::default()
            },
            snap_to_terrain: false,
            solid: InstanceSolid::Trunk,
            variants: vec![
                InstanceVariant::default(),
                InstanceVariant {
                    seed: Some(u64::MAX),
                    params: GrammarParams(BTreeMap::from([
                        ("Depth".to_string(), Fp(14.0)),
                        ("Storeys".to_string(), Fp(5.0)),
                    ])),
                },
            ],
            copies: vec![
                InstanceCopy::at(0.0, 0.0, 0.0),
                InstanceCopy {
                    variant: 1,
                    scale_pm: 1_500,
                    y_cm: -35,
                    ..InstanceCopy::at(-3.0, 7.25, 271.5)
                },
            ],
        };
        let wire = serde_json::to_value(&set).expect("serialises");
        assert_eq!(wire["solid"], serde_json::json!("trunk"));
        assert_eq!(wire["snap_to_terrain"], serde_json::json!(false));
        assert_eq!(
            wire["variants"],
            serde_json::json!([{}, {"seed": u64::MAX.to_string(), "params": {"Depth": 140000, "Storeys": 50000}}])
        );
        assert_eq!(
            wire["copies"],
            serde_json::json!([[0, 0, 0], [-300, 725, 2715, 1, 1500, -35]])
        );
        let back: crate::pds::Placement = serde_json::from_value(wire).expect("reads back");
        assert_eq!(
            serde_json::to_string(&back).unwrap(),
            serde_json::to_string(&set).unwrap()
        );
    }

    /// An absolute placement's values are written after its seed, as the
    /// record's fixed point, and left out when there are none - so every
    /// record written before them keeps its bytes (the corpus check is
    /// `an_unseeded_placement_keeps_the_bytes_it_had`).
    #[test]
    fn an_absolute_placements_values_follow_its_seed() {
        let placed = |params: GrammarParams| crate::pds::Placement::Absolute {
            generator_ref: "house".into(),
            transform: crate::pds::TransformData::default(),
            snap_to_terrain: true,
            avoid_water: false,
            avoid_water_clearance: Fp(0.0),
            seed: Some(9),
            params,
        };
        assert_eq!(
            serde_json::to_string(&placed(GrammarParams::default())).unwrap(),
            r#"{"$type":"network.symbios.place.absolute","generator_ref":"house","avoid_water_clearance":0,"seed":"9"}"#
        );
        let valued = placed(GrammarParams(BTreeMap::from([("Storeys".into(), Fp(3.0))])));
        assert_eq!(
            serde_json::to_string(&valued).unwrap(),
            r#"{"$type":"network.symbios.place.absolute","generator_ref":"house","avoid_water_clearance":0,"seed":"9","params":{"Storeys":30000}}"#
        );
        let back: crate::pds::Placement =
            serde_json::from_str(&serde_json::to_string(&valued).unwrap()).expect("reads");
        assert_eq!(
            back.shape_params().map(|params| params.0.clone()),
            Some(BTreeMap::from([("Storeys".to_string(), Fp(3.0))]))
        );
    }

    /// A client built before #1643 reads a set as a kind it does not know,
    /// and an absolute placement's values as a key it does not know: the
    /// record is read, never refused.
    #[test]
    fn a_client_before_sets_reads_past_them() {
        let set = serde_json::to_value(a_set()).expect("serialises");
        let read: before_1643::Placement = serde_json::from_value(set).expect("reads");
        assert!(matches!(read, before_1643::Placement::Unknown), "{read:?}");
        let valued = serde_json::json!({
            "$type": "network.symbios.place.absolute",
            "generator_ref": "house",
            "avoid_water_clearance": 0,
            "params": {"Storeys": 30000},
        });
        let read: before_1643::Placement = serde_json::from_value(valued).expect("reads");
        assert!(
            matches!(&read, before_1643::Placement::Absolute { generator_ref, .. } if generator_ref == "house"),
            "{read:?}"
        );
    }

    /// A plain seed keys as itself, so an absolute placement with that seed
    /// and a copy of that variant share a derivation; values make another
    /// key, the same for the same written values.
    #[test]
    fn a_variant_keys_by_its_seed_and_its_written_values() {
        let plain = InstanceVariant {
            seed: Some(7),
            params: GrammarParams::default(),
        };
        assert_eq!(plain.shape_key(99), 7);
        assert_eq!(InstanceVariant::default().shape_key(99), 99);
        let fit = |storeys: f32| InstanceVariant {
            seed: Some(7),
            params: GrammarParams(BTreeMap::from([("Storeys".to_string(), Fp(storeys))])),
        };
        assert_ne!(fit(5.0).shape_key(99), 7);
        assert_ne!(fit(5.0).shape_key(99), fit(6.0).shape_key(99));
        assert_eq!(fit(5.0).shape_key(99), fit(5.00001).shape_key(99));
    }
}
