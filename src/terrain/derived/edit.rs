//! The owner's edits over the walkable ground's derived items (#1590; the
//! owner's decisions are in `docs/geodata.md`).
//!
//! With the World Editor open, the owner clicks one of Berlin's buildings,
//! trees or items of street furniture in the world ([`DerivedPick`]), and
//! the editor names it ([`describe`]) and offers two edits, each kept in the
//! record's [`crate::pds::GeoSource`] by the item's source id:
//!
//! - **Remove** ([`remove`]): the item is drawn no more.
//! - **Make it this world's own** ([`adopt`]): a copy as drawn becomes
//!   ordinary record content - the same catalogue items at the same place,
//!   turn and size, as absolute placements of generators named after the
//!   item, which copies of one catalogue item share - and the original is
//!   drawn no more. A footprint's row comes over whole. Past the record's
//!   budgets the copy is refused, and the refusal says why.
//!
//! Either is undone by **Restore** ([`restore`]), from the Region source
//! section's list: the item is drawn from Berlin again, and an adopted
//! item's copy goes. The ring's lots carry no stable ids, and stay as
//! drawn.

use std::collections::HashSet;

use bevy::prelude::*;
use geodata::berlin::{BuildingUse, FurnitureKind};

use crate::pds::geo_source::{Edit, adopted_generator_name, adopted_source_of};
use crate::pds::sanitize::limits;
use crate::pds::{Fp, Fp3, Fp4, Generator, Placement, RoomRecord, TransformData};
use crate::terrain::geo::street_level::StreetLevel;

use super::{DerivedBuilds, SourceId, SourceLayer};

/// Why an item cannot be copied yet: its plan has not grown or baked it.
const STILL_DRAWING: &str =
    "It is still being drawn: its copy can be made once the world around it has finished.";

/// The derived item the World Editor has picked in the world, and why the
/// last edit asked of it could not be made.
#[derive(Resource, Default, Debug)]
pub(crate) struct DerivedPick {
    /// The item, by its source id.
    pub picked: Option<SourceId>,
    /// Why the last edit was refused, for the panel to say until the next
    /// pick.
    pub refused: Option<String>,
}

impl DerivedPick {
    /// Pick `id`, or nothing.
    pub(crate) fn pick(&mut self, id: Option<SourceId>) {
        self.picked = id;
        self.refused = None;
    }
}

/// The items `record`'s edits keep from being drawn: those removed and
/// those made the world's own, of the layers this build draws.
pub(crate) fn suppressed_by(record: Option<&RoomRecord>) -> HashSet<SourceId> {
    record
        .and_then(|record| record.geo_source.as_ref())
        .map(|source| {
            source
                .removed
                .iter()
                .chain(&source.adopted)
                .filter_map(|id| SourceId::parse(id))
                .collect()
        })
        .unwrap_or_default()
}

/// The item `id` in words - what Berlin records of it, from the walkable
/// ground's `level` - or, where the level does not hold it, what kind of
/// item it is and that it is not here.
pub(crate) fn describe(level: Option<&StreetLevel>, id: &SourceId) -> String {
    let key = &*id.key;
    let found = level.and_then(|level| match id.layer {
        SourceLayer::Building => level.buildings.iter().find(|b| &*b.id == key).map(|b| {
            let storeys = match b.storeys.or(b.peak_storeys) {
                Some(1) => ", of one storey".to_owned(),
                Some(n) => format!(", of {n} storeys"),
                None => String::new(),
            };
            format!("{}{storeys}, {:.0} m\u{b2}", use_words(b.usage), b.area)
        }),
        SourceLayer::Tree => level.trees.iter().find(|t| &*t.id == key).map(|t| {
            let kind = match t.genus.as_deref() {
                Some(genus) => match common_name(genus) {
                    Some(name) => format!("A {name} ({genus})"),
                    None => format!("A tree of the genus {genus}"),
                },
                None => "A tree, its genus not recorded".to_owned(),
            };
            match t.height {
                Some(height) => format!("{kind}, {height:.0} m tall"),
                None => kind,
            }
        }),
        SourceLayer::Furniture => level
            .furniture
            .iter()
            .find(|f| &*f.id == key)
            .map(|f| format!("A {}", kind_word(f.kind))),
        SourceLayer::RingLot => None,
    });
    found.unwrap_or_else(|| {
        format!(
            "{}, not on this world's walkable ground",
            layer_words(id.layer)
        )
    })
}

/// What a kind of building is, as a phrase opening a sentence.
fn use_words(usage: BuildingUse) -> &'static str {
    match usage {
        BuildingUse::Residential => "A residential building",
        BuildingUse::Mixed => "Homes over shops or offices",
        BuildingUse::Commercial => "A commercial building",
        BuildingUse::Industrial => "An industrial building",
        BuildingUse::Parking => "A car park",
        BuildingUse::Underground => "An underground car park",
        BuildingUse::Utility => "A utility building",
        BuildingUse::Public => "A public building",
        BuildingUse::Cultural => "A cultural building",
        BuildingUse::Religious => "A place of worship",
        BuildingUse::Unknown => "A building",
    }
}

/// The common name of a genus Berlin plants.
fn common_name(genus: &str) -> Option<&'static str> {
    const NAMES: &[(&str, &str)] = &[
        ("Tilia", "linden"),
        ("Acer", "maple"),
        ("Carpinus", "hornbeam"),
        ("Fraxinus", "ash"),
        ("Platanus", "plane"),
        ("Aesculus", "horse chestnut"),
        ("Fagus", "beech"),
        ("Ulmus", "elm"),
        ("Quercus", "oak"),
        ("Betula", "birch"),
        ("Gleditsia", "honey locust"),
        ("Robinia", "false acacia"),
        ("Sophora", "pagoda tree"),
        ("Styphnolobium", "pagoda tree"),
        ("Prunus", "cherry"),
        ("Malus", "apple"),
        ("Pyrus", "pear"),
        ("Crataegus", "hawthorn"),
        ("Sorbus", "rowan"),
        ("Corylus", "hazel"),
        ("Salix", "willow"),
        ("Taxus", "yew"),
        ("Pinus", "pine"),
        ("Larix", "larch"),
        ("Picea", "spruce"),
        ("Abies", "fir"),
        ("Pseudotsuga", "Douglas fir"),
        ("Populus", "poplar"),
    ];
    NAMES
        .iter()
        .find(|(latin, _)| latin.eq_ignore_ascii_case(genus))
        .map(|(_, name)| *name)
}

/// One item of a furniture kind, in words.
fn kind_word(kind: FurnitureKind) -> &'static str {
    match kind {
        FurnitureKind::Lamp => "street lamp",
        FurnitureKind::Bench => "bench",
        FurnitureKind::Bin => "litter bin",
        FurnitureKind::Bollard => "bollard",
        FurnitureKind::Shelter => "bus or tram shelter",
        FurnitureKind::Sign => "traffic sign",
        FurnitureKind::Fountain => "fountain",
        FurnitureKind::Column => "advertising column",
        FurnitureKind::BikeRack => "bike rack",
    }
}

/// What a layer's items are, as a phrase opening a sentence.
fn layer_words(layer: SourceLayer) -> &'static str {
    match layer {
        SourceLayer::Building => "A building",
        SourceLayer::Tree => "A tree",
        SourceLayer::Furniture => "An item of street furniture",
        SourceLayer::RingLot => "A building round the walkable ground",
    }
}

/// The source the edits go into, or why there is none.
fn source_of(record: &mut RoomRecord) -> Result<&mut crate::pds::GeoSource, String> {
    record
        .geo_source
        .as_mut()
        .ok_or_else(|| "This world is not built from Berlin.".to_owned())
}

/// Remove the item `id`: it is drawn no more.
pub(crate) fn remove(record: &mut RoomRecord, id: &SourceId) -> Result<(), String> {
    let key = id.to_string();
    let source = source_of(record)?;
    if source.edit_of(&key) != Edit::Drawn {
        return Err("It is already changed: restore it first.".to_owned());
    }
    source.set_edit(&key, Edit::Removed)
}

/// Restore the item `id`, the record's own spelling of it: drawn from
/// Berlin again, and where it was made the world's own, its copy - every
/// placement of the generators named after it, and the generators - gone.
pub(crate) fn restore(record: &mut RoomRecord, id: &str) -> Result<(), String> {
    source_of(record)?.set_edit(id, Edit::Drawn)?;
    let copies: HashSet<String> = record
        .generators
        .keys()
        .filter(|name| adopted_source_of(name) == Some(id))
        .cloned()
        .collect();
    if copies.is_empty() {
        return Ok(());
    }
    record.placements.retain(|placement| match placement {
        Placement::Absolute { generator_ref, .. }
        | Placement::Scatter { generator_ref, .. }
        | Placement::Grid { generator_ref, .. } => !copies.contains(generator_ref),
        Placement::Unknown => true,
    });
    for name in &copies {
        record.generators.remove(name);
        record.traits.remove(name);
    }
    Ok(())
}

/// Make the item `id` this world's own: its copies as `builds` draws them -
/// those drawn, near or far, not those its plan's budgets left out -
/// standing on `ground`, the walkable ground's height at `(x, z)`, become
/// absolute placements of the generators named after it, one generator to
/// each catalogue building and size its copies are, and the original is
/// drawn no more. Answers how many placements the copy is; refused, with
/// the reason, where the item is not drawn here, is not yet grown, or
/// would take the record past its counts or its size budget - and then the
/// record is as it was.
pub(crate) fn adopt(
    record: &mut RoomRecord,
    id: &SourceId,
    builds: &DerivedBuilds,
    ground: &dyn Fn(f32, f32) -> f32,
) -> Result<usize, String> {
    let key = id.to_string();
    if source_of(record)?.edit_of(&key) != Edit::Drawn {
        return Err("It is already changed: restore it first.".to_owned());
    }
    // Each distinct catalogue building and size, in the order the copies
    // first show it, and the copies' placements.
    let mut kinds: Vec<((usize, usize, u32), Generator)> = Vec::new();
    let mut placements = Vec::new();
    for (b, c) in builds.copies_of(id) {
        let build = &builds.builds()[b];
        if build.drawn[c] == super::plan::Drawn::No {
            // Not reached yet, it is still to be drawn; reached, it was
            // left out, and is no part of the copy.
            if !build.reached(c) {
                return Err(STILL_DRAWING.to_owned());
            }
            continue;
        }
        let plan = build.plan();
        let copy = &plan.copies[c];
        let tree = plan.grown(copy.building).ok_or(STILL_DRAWING)?;
        if copy.height.is_some() && plan.bounds(copy.building).is_none() {
            return Err(STILL_DRAWING.to_owned());
        }
        let pose = build.copy_pose(c);
        let scale = pose.scale.x;
        let kind = (b, copy.building, scale.to_bits());
        let n = match kinds.iter().position(|(k, _)| *k == kind) {
            Some(n) => n,
            None => {
                kinds.push((kind, scaled(tree, scale)));
                kinds.len() - 1
            }
        };
        let [x, y, z] = pose.translation.to_array();
        placements.push(Placement::Absolute {
            generator_ref: adopted_generator_name(&key, n + 1),
            transform: TransformData {
                // Snapped, the height is an offset from the ground: a
                // building's sunk foundations stay sunk.
                translation: Fp3([x, y - ground(x, z), z]),
                rotation: Fp4(pose.rotation.to_array()),
                scale: Fp3([1.0; 3]),
            },
            snap_to_terrain: true,
            avoid_water: false,
            avoid_water_clearance: Fp(0.0),
            seed: None,
        });
    }
    if placements.is_empty() {
        return Err("Nothing of it is drawn in this world to copy.".to_owned());
    }
    let mut candidate = record.clone();
    for (n, (_, generator)) in kinds.into_iter().enumerate() {
        candidate
            .generators
            .insert(adopted_generator_name(&key, n + 1), generator);
    }
    let count = placements.len();
    candidate.placements.extend(placements);
    source_of(&mut candidate)?.set_edit(&key, Edit::Adopted)?;
    within_budgets(&candidate)?;
    *record = candidate;
    Ok(count)
}

/// `tree` drawn at `scale`: a placement drops its own scale, so the size a
/// copy was drawn at rides in its generator's root.
fn scaled(tree: &Generator, scale: f32) -> Generator {
    let mut generator = tree.clone();
    if scale != 1.0 {
        generator.transform = TransformData::from(
            Transform::from_scale(Vec3::splat(scale)) * Transform::from(&tree.transform),
        );
    }
    generator
}

/// Whether `record` keeps within the counts its sanitizer holds it to and
/// the size budget a save is held to, or why not.
fn within_budgets(record: &RoomRecord) -> Result<(), String> {
    use crate::pds::record_size::{SOFT_RECORD_BUDGET_BYTES, human_bytes};
    if record.generators.len() > limits::MAX_GENERATORS {
        return Err(format!(
            "Its copy would take this world past the {} items it can hold.",
            limits::MAX_GENERATORS
        ));
    }
    if record.placements.len() > limits::MAX_PLACEMENTS {
        return Err(format!(
            "Its copy would take this world past the {} placements it can hold.",
            limits::MAX_PLACEMENTS
        ));
    }
    let readout = crate::pds::room::measure_publish(record);
    if let Some(bytes) = readout
        .bytes
        .filter(|&bytes| bytes > SOFT_RECORD_BUDGET_BYTES)
    {
        return Err(format!(
            "Its copy would take this world past its size budget: its largest record would be \
             {}, over the {} budget.",
            human_bytes(bytes),
            human_bytes(SOFT_RECORD_BUDGET_BYTES)
        ));
    }
    Ok(())
}
