//! Triangle and part counts (#1471, #1479): what a subject, a catalogue
//! entry or a whole world costs to draw.
//!
//! A generator's size was printed and its cost was not, so a scatter could
//! plant thousands of copies of a dense mesh and nothing said so until the
//! world ran slow. Three readers count here, all one way ([`tally`]): the
//! turntable's `subject size` line, each `--catalogue-sizes` row, and
//! `--triangle-report`, which prints a world's cost as one JSON object.
//!
//! # What is counted
//!
//! The meshes the spawn path actually spawned under the subject - an
//! L-system's branches and its prop cards included - each as the indexed
//! triangles of its `Mesh3d`'s mesh asset (vertices / 3 for a mesh with no
//! indices). A mesh shared by many entities - a primitive the dedup cache
//! hands out once, every copy of a scattered tree - counts once for every
//! entity that draws it, because each one is a draw. Live particle quads
//! are left out: they come and go with an emitter's rate, so there is no
//! one number to give.
//!
//! Beside the triangles, the parts (#1479): the same entities, one part
//! each. Most visitors play in a browser, on one thread, where every drawn
//! entity is culled, extracted and batched every frame whatever its
//! triangles, so a world's per-frame CPU cost is its part count. The spawn
//! path draws a primitive on one entity, but a primitive whose faces wear
//! several materials as a transform-only root - no part - with one render
//! child per material, each a part; an L-system as one entity per material
//! bucket, its prop cards baked into them. A mesh shared by many entities
//! is a part for each, and a mesh with no CPU copy to count is still a part.
//!
//! # The world report
//!
//! `--triangle-report` grows one copy of each placed generator through the
//! `--catalogue-sizes` app ([`super::sizes::tallies_of`]) and multiplies
//! it by each placement's copies: 1 for an absolute placement, the cells of
//! a grid, and for a scatter the instances its sampler actually places -
//! the census's replay ([`crate::world_builder::compile::scatter_yields`]),
//! which can be fewer than its `count` when its filters refuse most of its
//! ground. The terrain's ground mesh is listed on its own line, built by the
//! game's own mesher from the rebuilt heightmap; a generator's water is the
//! plane its generator spawns, counted with it. Not counted: particles.
//!
//! What a road network grows is counted as a client draws it (#1554): its
//! streets on a line of their own (`streets`), one row a network with its
//! triangles and parts - one part per surface the mesh emits, as the game
//! spawns them - and the buildings and street props on its lots as the
//! placements they are. A record saved with its district carries them
//! already; a network it carries no content for is grown into the counted
//! copy first, as a visitor's client grows it on load, and the report says
//! how many it grew (`grown`).
//!
//! An absolute placement with a grammar seed of its own (#1505) draws its
//! generator's shape grammars with that seed, which may choose other rules
//! and so other triangles: one copy is grown as each such seed draws the
//! tree ([`Generator::with_shape_seed`]) and counted for the placements it
//! names, whose rows say the seed (`seed`). A generator's row then sums
//! what its copies draw, each as it is drawn, beside `triangles_each` and
//! `parts_each` for one copy as the generator draws itself, and counts the
//! placements that draw it with a seed of their own (`seeded`).
//!
//! Every row carries its parts beside its triangles - `parts_each` for one
//! copy and `parts` for all of them - and the totals a `parts` object beside
//! `triangles`, in which the ground is one part: the game draws it as one
//! mesh on one entity.

use std::collections::{BTreeSet, HashMap};

use bevy::mesh::{Mesh, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use serde_json::{Value, json};

use crate::pds::{Generator, Placement, RoomRecord};
use crate::terrain::FinishedHeightMap;

/// Triangles and parts counted over a set of mesh entities.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Tally {
    pub(super) triangles: u64,
    /// The entities that draw a mesh, one part each (#1479): what a browser
    /// culls, extracts and batches every frame, whatever their triangles. A
    /// mesh that cannot be counted is still drawn, and still a part.
    pub(super) parts: u64,
    /// Meshes that could not be counted: an asset that is gone, or one
    /// whose data went to the GPU with no CPU copy kept. Never one in a
    /// no-renderer app; said aloud on the turntable's line when there is.
    pub(super) unreadable: u32,
}

impl Tally {
    /// A tally with nothing left uncounted.
    #[cfg(test)]
    pub(super) fn counted(triangles: u64, parts: u64) -> Self {
        Self {
            triangles,
            parts,
            unreadable: 0,
        }
    }
}

impl std::fmt::Display for Tally {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} triangles", self.triangles)?;
        if self.unreadable > 0 {
            write!(
                f,
                " (and {} meshes with no CPU copy to count)",
                self.unreadable
            )?;
        }
        let noun = if self.parts == 1 { "part" } else { "parts" };
        write!(f, ", {} {noun}", self.parts)
    }
}

/// The triangles one mesh draws: its indices / 3 on a triangle list, or its
/// vertices / 3 with no indices; a strip's corners less two. `None` when its
/// data is not in the main world to read.
pub(super) fn mesh_triangles(mesh: &Mesh) -> Option<u64> {
    let corners = match mesh.try_indices_option().ok()? {
        Some(indices) => indices.len(),
        None => mesh
            .try_attribute_option(Mesh::ATTRIBUTE_POSITION)
            .ok()?
            .map_or(0, VertexAttributeValues::len),
    } as u64;
    Some(match mesh.primitive_topology() {
        PrimitiveTopology::TriangleList => corners / 3,
        PrimitiveTopology::TriangleStrip => corners.saturating_sub(2),
        // Points and lines draw no triangles.
        _ => 0,
    })
}

/// The triangles `drawn` draw between them, and the parts they are: every
/// entity's mesh counted once for that entity, however many others share
/// it, and every entity one part, whether its mesh can be counted or not.
pub(super) fn tally<'a>(
    drawn: impl IntoIterator<Item = &'a Mesh3d>,
    assets: &Assets<Mesh>,
) -> Tally {
    let mut out = Tally::default();
    for mesh in drawn {
        out.parts += 1;
        match assets.get(&mesh.0).and_then(mesh_triangles) {
            Some(triangles) => out.triangles += triangles,
            None => out.unreadable += 1,
        }
    }
    out
}

/// `--triangle-report`: what `world`'s placements cost to draw, printed as
/// one JSON object, one row a line.
pub(super) fn print_triangle_report(world: &str, record: &RoomRecord) {
    // The ground first: the road layer grows its districts on it (#1554),
    // and the copies to grow are the grown record's.
    let heightmap = crate::terrain::rebuild_terrain_for_record(record);
    let mut grown = record.clone();
    let grown_count = crate::terrain::grow_missing_districts(&mut grown, &heightmap, world);
    let record = &grown;
    let copies = copies_to_grow(record);
    let labels: Vec<(String, Generator)> = copies
        .iter()
        .map(|((name, seed), tree)| match seed {
            Some(seed) => (format!("{name} (seed {seed})"), tree.clone()),
            None => (name.clone(), tree.clone()),
        })
        .collect();
    let tallies = super::sizes::tallies_of(labels);
    // The tallies come back in the order the copies were given.
    let (mut each, mut seeded) = (HashMap::new(), HashMap::new());
    for (((name, seed), _), (_, tally)) in copies.into_iter().zip(tallies) {
        match seed {
            Some(seed) => seeded.insert((name, seed), tally),
            None => each.insert(name, tally),
        };
    }
    let streets = street_costs(record, &heightmap);
    let mut report = world_report(world, record, &heightmap, &each, &seeded, &streets);
    report.insert(4, ("grown", json!(grown_count)));
    println!("{}", one_row_a_line(&report));
}

/// The copies `--triangle-report` grows: each placed generator as it draws
/// itself, `(name, None)`, and as each grammar seed an absolute placement of
/// it names draws it (#1505), `(name, Some(seed))` - the tree with that seed
/// written into its Shape nodes, as the compile draws that placement. Each
/// once, in name order.
fn copies_to_grow(record: &RoomRecord) -> Vec<((String, Option<u64>), Generator)> {
    let mut grown: BTreeSet<(&str, Option<u64>)> = BTreeSet::new();
    for placement in &record.placements {
        let Some(name) = generator_ref(placement) else {
            continue;
        };
        if !record.generators.contains_key(name) {
            continue;
        }
        grown.insert((name, None));
        if let Some(seed) = placement.shape_seed() {
            grown.insert((name, Some(seed)));
        }
    }
    grown
        .into_iter()
        .map(|(name, seed)| {
            let generator = &record.generators[name];
            let tree = match seed {
                Some(seed) => generator.with_shape_seed(seed),
                None => generator.clone(),
            };
            ((name.to_string(), seed), tree)
        })
        .collect()
}

/// The generator a placement plants, if it plants one.
fn generator_ref(placement: &Placement) -> Option<&str> {
    match placement {
        Placement::Absolute { generator_ref, .. }
        | Placement::Scatter { generator_ref, .. }
        | Placement::Grid { generator_ref, .. } => Some(generator_ref),
        Placement::Unknown => None,
    }
}

/// What the world report leaves out, in its own words.
const NOT_COUNTED: &str = "live particle quads, which come and go with an emitter's rate";

/// The parts the terrain's ground is: the game draws it as one mesh on one
/// entity (`terrain::heightmap`'s spawn).
const GROUND_PARTS: u64 = 1;

/// Each enabled road network's streets as the game meshes them on this
/// ground (#1554): `(network index, streets, junctions, triangles, parts)`.
/// A network whose tracer finds nothing draws nothing and is left out.
fn street_costs(record: &RoomRecord, heightmap: &FinishedHeightMap) -> Vec<(usize, Value, Tally)> {
    let water = crate::world_builder::compile::drawn_water_level(record, Some(heightmap));
    crate::pds::find_road_configs(record)
        .into_iter()
        .enumerate()
        .filter(|(_, c)| c.enabled)
        .filter_map(|(i, c)| {
            let parts = crate::urban::build_road_geometry(&heightmap.0, c, water)?;
            let (triangles, drawn) = parts.draw_cost();
            let tally = Tally {
                triangles: triangles as u64,
                parts: drawn as u64,
                unreadable: 0,
            };
            let row = json!({
                "network": i,
                "streets": parts.chains,
                "junctions": parts.junctions,
                "triangles": tally.triangles,
                "parts": tally.parts,
            });
            Some((i, row, tally))
        })
        .collect()
}

/// One generator's cost, summed over its placements (#1505): one copy as
/// it draws itself, and what all its copies draw, each as it is drawn.
#[derive(Default)]
struct Planted {
    own: Tally,
    copies: u64,
    placements: u32,
    triangles: u64,
    parts: u64,
    /// Its placements that draw it with a grammar seed of their own.
    seeded: u32,
}

/// The report's fields in print order: the world, the totals in triangles
/// and in parts, what is not counted, each placed generator's cost and each
/// placement's, both dearest first by triangles. `each` is one copy's
/// triangles and parts by generator name, as the generator draws itself;
/// `seeded`, one copy's as an absolute placement's grammar seed draws it,
/// by name and seed (#1505).
fn world_report(
    world: &str,
    record: &RoomRecord,
    heightmap: &FinishedHeightMap,
    each: &HashMap<String, Tally>,
    seeded: &HashMap<(String, u64), Tally>,
    streets: &[(usize, Value, Tally)],
) -> Vec<(&'static str, Value)> {
    let yields = crate::world_builder::compile::scatter_yields(record, heightmap);
    // (triangles, parts, index, row)
    let mut placements = Vec::new();
    let mut generators: HashMap<&str, Planted> = HashMap::new();
    for (index, (placement, placed)) in record.placements.iter().zip(&yields).enumerate() {
        let (name, kind, copies) = match placement {
            Placement::Absolute { generator_ref, .. } => (generator_ref, "absolute", 1),
            Placement::Grid {
                generator_ref,
                counts,
                ..
            } => (
                generator_ref,
                "grid",
                counts.iter().map(|&n| u64::from(n)).product(),
            ),
            Placement::Scatter { generator_ref, .. } => {
                (generator_ref, "scatter", u64::from(placed.unwrap_or(0)))
            }
            Placement::Unknown => continue,
        };
        let own = each.get(name.as_str()).copied();
        // A grammar seed draws its own copy (#1505), when one was grown.
        let seed = placement.shape_seed();
        let one = seed
            .and_then(|seed| seeded.get(&(name.clone(), seed)).copied())
            .or(own);
        let copy = one.unwrap_or_default();
        let triangles = copy.triangles * copies;
        let parts = copy.parts * copies;
        let mut row = json!({
            "index": index,
            "generator": name,
            "kind": kind,
            "copies": copies,
            "triangles_each": copy.triangles,
            "triangles": triangles,
            "parts_each": copy.parts,
            "parts": parts,
        });
        if let Placement::Scatter { count, .. } = placement {
            row["requested"] = json!(count);
        }
        if let Some(seed) = seed {
            row["seed"] = json!(seed.to_string());
        }
        match own {
            Some(own) => {
                let planted = generators.entry(name).or_insert_with(|| Planted {
                    own,
                    ..Planted::default()
                });
                planted.copies += copies;
                planted.placements += 1;
                planted.triangles += triangles;
                planted.parts += parts;
                planted.seeded += u32::from(seed.is_some());
            }
            None => {
                row["why"] = json!("no generator by this name: the compile skips it");
            }
        }
        placements.push((triangles, parts, index, row));
    }
    placements.sort_by(|a, b| b.0.cmp(&a.0).then(a.2.cmp(&b.2)));
    let mut generators: Vec<(u64, &str, Value)> = generators
        .into_iter()
        .map(|(name, planted)| {
            let mut row = json!({
                "generator": name,
                "triangles_each": planted.own.triangles,
                "copies": planted.copies,
                "placements": planted.placements,
                "triangles": planted.triangles,
                "parts_each": planted.own.parts,
                "parts": planted.parts,
            });
            if planted.seeded > 0 {
                row["seeded"] = json!(planted.seeded);
            }
            (planted.triangles, name, row)
        })
        .collect();
    generators.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));

    let planted: u64 = placements.iter().map(|p| p.0).sum();
    let planted_parts: u64 = placements.iter().map(|p| p.1).sum();
    let ground = ground_triangles(heightmap);
    let street_triangles: u64 = streets.iter().map(|s| s.2.triangles).sum();
    let street_parts: u64 = streets.iter().map(|s| s.2.parts).sum();
    vec![
        ("world", json!(world)),
        (
            "triangles",
            json!({
                "total": planted + ground + street_triangles,
                "placements": planted,
                "ground": ground,
                "streets": street_triangles,
            }),
        ),
        (
            "parts",
            json!({
                "total": planted_parts + GROUND_PARTS + street_parts,
                "placements": planted_parts,
                "ground": GROUND_PARTS,
                "streets": street_parts,
            }),
        ),
        ("not_counted", json!(NOT_COUNTED)),
        (
            "streets",
            Value::Array(streets.iter().map(|s| s.1.clone()).collect()),
        ),
        (
            "generators",
            Value::Array(generators.into_iter().map(|g| g.2).collect()),
        ),
        (
            "placements",
            Value::Array(placements.into_iter().map(|p| p.3).collect()),
        ),
    ]
}

/// The terrain's ground mesh: the heightfield the game's own mesher builds
/// from `heightmap`, counted like any other mesh.
fn ground_triangles(heightmap: &FinishedHeightMap) -> u64 {
    let mesh = bevy_symbios_ground::HeightMapMeshBuilder::new().build(&heightmap.0);
    mesh_triangles(&mesh).unwrap_or(0)
}

/// `fields` as one JSON object with each field on a line of its own and
/// each row of a list on a line of its own, so `head` shows the dearest
/// rows whole.
fn one_row_a_line(fields: &[(&str, Value)]) -> String {
    let mut out = String::from("{\n");
    for (i, (key, value)) in fields.iter().enumerate() {
        let comma = if i + 1 < fields.len() { "," } else { "" };
        let key = Value::from(*key);
        match value {
            Value::Array(rows) if !rows.is_empty() => {
                out += &format!("  {key}: [\n");
                for (j, row) in rows.iter().enumerate() {
                    let sep = if j + 1 < rows.len() { "," } else { "" };
                    out += &format!("    {row}{sep}\n");
                }
                out += &format!("  ]{comma}\n");
            }
            _ => out += &format!("  {key}: {value}{comma}\n"),
        }
    }
    out.push('}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{
        BiomeFilter, Environment, Fp, Fp2, Fp3, ScatterBounds, ScatterNaturalness, TransformData,
    };
    use bevy_symbios_ground::HeightMap;

    /// A 100 m map rising 20 m west to east: a height band picks a strip.
    fn ramp() -> FinishedHeightMap {
        let mut map = HeightMap::new(3, 3, 50.0);
        for z in 0..3 {
            for x in 0..3 {
                map.data_mut()[z * 3 + x] = x as f32 * 10.0;
            }
        }
        FinishedHeightMap(map, None)
    }

    fn scatter(generator: &str, count: u32, altitude_band: Option<[f32; 2]>) -> Placement {
        Placement::Scatter {
            generator_ref: generator.into(),
            bounds: ScatterBounds::Circle {
                center: Fp2([0.0, 0.0]),
                radius: Fp(40.0),
            },
            count,
            local_seed: 7,
            biome_filter: BiomeFilter::default(),
            snap_to_terrain: true,
            random_yaw: true,
            avoid_urban: false,
            float_on_water: false,
            naturalness: ScatterNaturalness {
                altitude_band: altitude_band.map(Fp2),
                ..Default::default()
            },
        }
    }

    /// A rock, a lump scattered 100 times onto a 1 m strip of the ramp -
    /// which the sampler's `count * 10` tries cannot fill - and 10 times
    /// anywhere, a grid of rocks 2 x 1 x 3, and a placement of a generator
    /// the record does not have.
    fn fixture() -> RoomRecord {
        let absolute = |generator: &str| Placement::Absolute {
            generator_ref: generator.into(),
            transform: TransformData::default(),
            snap_to_terrain: true,
            avoid_water: false,
            avoid_water_clearance: Fp(0.0),
            seed: None,
        };
        RoomRecord {
            lex_type: "network.symbios.room".to_string(),
            environment: Environment::default(),
            generators: [
                ("rock".to_string(), Generator::default_cuboid()),
                ("lump".to_string(), Generator::default_cuboid()),
            ]
            .into_iter()
            .collect(),
            placements: vec![
                absolute("rock"),
                scatter("lump", 100, Some([9.5, 10.5])),
                Placement::Grid {
                    generator_ref: "rock".into(),
                    transform: TransformData::default(),
                    counts: [2, 1, 3],
                    gaps: Fp3([1.0, 1.0, 1.0]),
                    snap_to_terrain: true,
                    random_yaw: false,
                },
                scatter("lump", 10, None),
                absolute("gone"),
            ],
            traits: HashMap::new(),
            contact_effects: Default::default(),
            default_landing: None,
            geo_source: None,
            opaque_refs: Default::default(),
        }
    }

    /// One copy of each: a rock of 12 triangles on one part, a lump of 80
    /// on three.
    fn each() -> HashMap<String, Tally> {
        [
            ("rock".to_string(), Tally::counted(12, 1)),
            ("lump".to_string(), Tally::counted(80, 3)),
        ]
        .into_iter()
        .collect()
    }

    fn field<'a>(report: &'a [(&str, Value)], key: &str) -> &'a Value {
        &report
            .iter()
            .find(|(k, _)| *k == key)
            .unwrap_or_else(|| panic!("no {key:?} in the report"))
            .1
    }

    fn placement(report: &[(&str, Value)], index: u64) -> Value {
        field(report, "placements")
            .as_array()
            .expect("a list")
            .iter()
            .find(|row| row["index"] == index)
            .unwrap_or_else(|| panic!("no placement {index}"))
            .clone()
    }

    /// THE CASE (#1471): a scatter costs the instances its sampler places,
    /// not the ones it asks for. The strip scatter asks for 100 and places
    /// fewer - the census's own count of them - and its cost is that many
    /// lumps; the unfiltered one places all it asks for.
    #[test]
    fn a_scatter_costs_what_its_sampler_places_not_what_it_asks_for() {
        let (record, map) = (fixture(), ramp());
        let report = world_report("did:test", &record, &map, &each(), &HashMap::new(), &[]);

        let placed = crate::world_builder::compile::scatter_yields(&record, &map)[1]
            .expect("placement 1 is a scatter");
        assert!(
            0 < placed && placed < 100,
            "the strip must place some but not all, or this case checks nothing: {placed}"
        );
        let strip = placement(&report, 1);
        assert_eq!(strip["requested"], 100, "{strip}");
        assert_eq!(strip["copies"], placed, "{strip}");
        assert_eq!(strip["triangles"], u64::from(placed) * 80, "{strip}");
        assert_eq!(strip["parts"], u64::from(placed) * 3, "{strip}");

        let open = placement(&report, 3);
        assert_eq!(open["copies"], 10, "{open}");
        assert_eq!(open["triangles"], 800, "{open}");
        assert_eq!(open["parts"], 30, "{open}");
        // An absolute is one copy, a grid its cells.
        assert_eq!(placement(&report, 0)["triangles"], 12);
        assert_eq!(placement(&report, 0)["parts"], 1);
        assert_eq!(placement(&report, 2)["copies"], 6);
        assert_eq!(placement(&report, 2)["triangles"], 72);
        assert_eq!(placement(&report, 2)["parts"], 6);
    }

    /// THE CASE (#1479): a world's parts are each generator's parts for one
    /// copy - grown through the spawn path, as the report grows them - times
    /// its copies, and the ground one more. A generator of two primitives
    /// scattered k times is 2 parts a copy and 2k in all, a one-primitive
    /// rock standing once and in a grid of 6 is 7, and the totals add up.
    #[test]
    fn a_world_counts_the_parts_its_copies_draw_and_the_ground_as_one() {
        let mut pair = Generator::default_cuboid();
        pair.children = vec![Generator::default_cuboid()];
        let each: HashMap<String, Tally> = super::super::sizes::tallies_staged(vec![
            ("pair".to_string(), pair),
            ("rock".to_string(), Generator::default_cuboid()),
        ])
        .into_iter()
        .collect();
        assert_eq!(each["pair"], Tally::counted(24, 2), "{each:?}");
        assert_eq!(each["rock"], Tally::counted(12, 1), "{each:?}");

        let mut record = fixture();
        record
            .generators
            .insert("pair".to_string(), Generator::default_cuboid());
        record.placements[1] = scatter("pair", 10, None);
        record.placements[3] = Placement::Unknown;
        let map = ramp();
        let report = world_report("did:test", &record, &map, &each, &HashMap::new(), &[]);

        let k = crate::world_builder::compile::scatter_yields(&record, &map)[1]
            .expect("placement 1 is a scatter");
        assert!(
            k > 1,
            "the scatter must place copies, or this checks nothing"
        );
        let scattered = placement(&report, 1);
        assert_eq!(scattered["copies"], k, "{scattered}");
        assert_eq!(scattered["parts_each"], 2, "{scattered}");
        assert_eq!(scattered["parts"], 2 * u64::from(k), "{scattered}");

        let generators = field(&report, "generators").as_array().expect("a list");
        let row = |name: &str| {
            generators
                .iter()
                .find(|row| row["generator"] == name)
                .unwrap_or_else(|| panic!("no {name} row"))
        };
        assert_eq!(row("pair")["parts_each"], 2);
        assert_eq!(row("pair")["parts"], 2 * u64::from(k));
        assert_eq!(row("rock")["copies"], 7);
        assert_eq!(row("rock")["parts_each"], 1);
        assert_eq!(row("rock")["parts"], 7);
        assert_eq!(placement(&report, 4)["parts"], 0, "the missing generator");

        let planted = 2 * u64::from(k) + 7;
        assert_eq!(
            *field(&report, "parts"),
            json!({"total": planted + 1, "placements": planted, "ground": 1, "streets": 0})
        );
    }

    /// Every entity that draws a mesh is one part (#1479): a mesh two
    /// entities share is two parts, and a mesh with no CPU copy left to
    /// count is still drawn, so still a part. The line says the parts after
    /// the triangles, and one part in the singular.
    #[test]
    fn every_drawing_entity_is_a_part_whether_or_not_its_mesh_can_be_read() {
        let mut assets = Assets::<Mesh>::default();
        let cube = assets.add(Cuboid::default());
        let gone = assets.add(Cuboid::default());
        assets.remove(&gone);
        let drawn = [Mesh3d(cube.clone()), Mesh3d(cube), Mesh3d(gone)];

        let got = tally(&drawn, &assets);
        assert_eq!(
            got,
            Tally {
                triangles: 24,
                parts: 3,
                unreadable: 1
            }
        );
        assert_eq!(
            got.to_string(),
            "24 triangles (and 1 meshes with no CPU copy to count), 3 parts"
        );
        assert_eq!(
            tally(&drawn[..1], &assets).to_string(),
            "12 triangles, 1 part"
        );
    }

    /// The printed report is one JSON object: the world, the totals in
    /// triangles and beside them in parts - which add up - what is not
    /// counted, and two lists sorted dearest first, each row on a line of
    /// its own with the fields it promises. A generator the record lacks
    /// costs 0 and says why.
    #[test]
    fn the_triangle_report_is_one_object_with_sorted_rows_a_line_each() {
        let (record, map) = (fixture(), ramp());
        let printed = one_row_a_line(&world_report(
            "did:test",
            &record,
            &map,
            &each(),
            &HashMap::new(),
            &[],
        ));
        let report: Value = serde_json::from_str(&printed).expect("the report is JSON");

        let keys: Vec<&String> = report.as_object().expect("an object").keys().collect();
        assert_eq!(
            keys,
            [
                "generators",
                "not_counted",
                "parts",
                "placements",
                "streets",
                "triangles",
                "world"
            ],
            "{printed}"
        );
        assert_eq!(report["world"], "did:test");
        // The parts are printed on the line after the triangles.
        let starts: Vec<&str> = printed
            .lines()
            .filter_map(|line| line.strip_prefix("  \""))
            .filter_map(|line| line.split('"').next())
            .collect();
        assert_eq!(
            starts,
            [
                "world",
                "triangles",
                "parts",
                "not_counted",
                "streets",
                "generators",
                "placements"
            ],
            "{printed}"
        );

        let rows = |key: &str| report[key].as_array().expect("a list").clone();
        let (placements, generators) = (rows("placements"), rows("generators"));
        let cost = |row: &Value| row["triangles"].as_u64().expect("a count");
        assert_eq!(placements.len(), 5, "{printed}");
        for list in [&placements, &generators] {
            let costs: Vec<u64> = list.iter().map(cost).collect();
            assert!(
                costs.windows(2).all(|w| w[0] >= w[1]),
                "not dearest first: {costs:?}"
            );
        }
        // Each row is one line of the print.
        for row in placements.iter().chain(&generators) {
            let whole = row.to_string();
            assert!(
                printed
                    .lines()
                    .any(|line| line.trim().trim_end_matches(',') == whole),
                "{row} is not a line of its own:\n{printed}"
            );
        }

        let fields = |row: &Value| -> Vec<String> {
            row.as_object().expect("a row").keys().cloned().collect()
        };
        let strip = placements.iter().find(|r| r["index"] == 1).expect("row 1");
        assert_eq!(
            fields(strip),
            [
                "copies",
                "generator",
                "index",
                "kind",
                "parts",
                "parts_each",
                "requested",
                "triangles",
                "triangles_each"
            ]
        );
        let rock = placements.iter().find(|r| r["index"] == 0).expect("row 0");
        assert_eq!(
            fields(rock),
            [
                "copies",
                "generator",
                "index",
                "kind",
                "parts",
                "parts_each",
                "triangles",
                "triangles_each"
            ]
        );
        let gone = placements.iter().find(|r| r["index"] == 4).expect("row 4");
        assert_eq!(gone["triangles"], 0);
        assert_eq!(gone["parts"], 0);
        assert!(
            gone["why"]
                .as_str()
                .is_some_and(|w| w.contains("no generator"))
        );
        assert_eq!(
            fields(&generators[0]),
            [
                "copies",
                "generator",
                "parts",
                "parts_each",
                "placements",
                "triangles",
                "triangles_each"
            ]
        );
        assert_eq!(generators.len(), 2, "the missing generator has no row");

        let planted: u64 = placements.iter().map(cost).sum();
        assert_eq!(generators.iter().map(cost).sum::<u64>(), planted);
        let totals = &report["triangles"];
        assert_eq!(totals["placements"], planted);
        // A 3 x 3 map is 2 x 2 quads of two triangles each.
        assert_eq!(totals["ground"], 8);
        assert_eq!(totals["total"], planted + 8);

        let parts = |row: &Value| row["parts"].as_u64().expect("a count");
        let drawn: u64 = placements.iter().map(parts).sum();
        assert_eq!(generators.iter().map(parts).sum::<u64>(), drawn);
        let totals = &report["parts"];
        assert_eq!(totals["placements"], drawn);
        assert_eq!(totals["ground"], 1, "the ground is one mesh on one entity");
        assert_eq!(totals["total"], drawn + 1);
    }

    /// #1505: an absolute placement with a grammar seed of its own costs
    /// what its seed draws, and its row says the seed; its generator's row
    /// sums what each copy draws beside one copy as the generator draws
    /// itself, and counts the seeded placement.
    #[test]
    fn a_seeded_placement_costs_what_its_seed_draws() {
        let (mut record, map) = (fixture(), ramp());
        let Placement::Absolute { seed, .. } = &mut record.placements[0] else {
            panic!("placement 0 is absolute");
        };
        *seed = Some(7);
        let seeded = HashMap::from([(("rock".to_string(), 7), Tally::counted(36, 3))]);
        let report = world_report("did:test", &record, &map, &each(), &seeded, &[]);

        let rock = placement(&report, 0);
        assert_eq!(rock["seed"], "7", "{rock}");
        assert_eq!(rock["triangles"], 36, "{rock}");
        assert_eq!(rock["parts"], 3, "{rock}");
        assert!(
            placement(&report, 2).get("seed").is_none(),
            "a grid has no grammar seed"
        );
        let generators = field(&report, "generators").as_array().expect("a list");
        let row = generators
            .iter()
            .find(|row| row["generator"] == "rock")
            .expect("a rock row");
        assert_eq!(row["triangles_each"], 12, "{row}");
        assert_eq!(row["copies"], 7, "{row}");
        assert_eq!(row["triangles"], 36 + 6 * 12, "{row}");
        assert_eq!(row["parts"], 3 + 6, "{row}");
        assert_eq!(row["seeded"], 1, "{row}");
    }

    /// #1505: the copy a seeded placement is counted by is grown with its
    /// seed written into the tree's grammar, as the compile draws it - here
    /// a grammar whose seed draws one block or two - and each copy is grown
    /// once, however many placements draw it.
    #[test]
    fn a_seeded_copy_is_grown_as_its_seed_draws_it() {
        let grammar = [
            "Lot --> Extrude(1) Split(X) { ~1: Block | ~1: Right }",
            "Right --> 50% Block | 50% Gap",
            "Block --> I(\"Block\")",
            "Gap --> NIL",
        ]
        .join("\n");
        let hut = |seed: u64| -> Generator {
            serde_json::from_value(json!({
                "$type": "network.symbios.gen.shape",
                "grammar_source": grammar,
                "root_rule": "Lot",
                "footprint": [20_000, 0, 10_000],
                "seed": seed.to_string(),
            }))
            .expect("a Shape node")
        };
        let grown = super::super::sizes::tallies_staged(
            (1..16u64)
                .map(|seed| (seed.to_string(), hut(seed)))
                .collect(),
        );
        let parts_of = |parts: u64| {
            grown
                .iter()
                .find(|(_, tally)| tally.parts == parts)
                .map(|(seed, _)| seed.parse::<u64>().expect("a seed"))
                .unwrap_or_else(|| panic!("no seed draws {parts} blocks: {grown:?}"))
        };
        let (one, two) = (parts_of(1), parts_of(2));

        let mut record = fixture();
        record.generators.insert("hut".to_string(), hut(one));
        let at = |seed| Placement::Absolute {
            generator_ref: "hut".into(),
            transform: TransformData::default(),
            snap_to_terrain: true,
            avoid_water: false,
            avoid_water_clearance: Fp(0.0),
            seed,
        };
        record.placements = vec![at(None), at(Some(two)), at(Some(two))];
        let copies = copies_to_grow(&record);
        assert_eq!(
            copies
                .iter()
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>(),
            [("hut".to_string(), None), ("hut".to_string(), Some(two))],
            "each copy once"
        );
        let tallies = super::super::sizes::tallies_staged(
            copies
                .iter()
                .map(|((name, _), tree)| (name.clone(), tree.clone()))
                .collect(),
        );
        let (mut each, mut seeded) = (HashMap::new(), HashMap::new());
        for (((name, seed), _), (_, tally)) in copies.into_iter().zip(tallies) {
            match seed {
                Some(seed) => seeded.insert((name, seed), tally),
                None => each.insert(name, tally),
            };
        }
        let report = world_report("did:test", &record, &ramp(), &each, &seeded, &[]);
        assert_eq!(placement(&report, 0)["parts"], 1);
        assert_eq!(placement(&report, 1)["parts"], 2);
        assert_eq!(placement(&report, 2)["parts"], 2);
    }

    /// #1554: a road network's streets are counted as the game meshes them -
    /// a row a network, its triangles and one part per surface it emits - and
    /// summed into the world's totals beside the ground and the placements.
    #[test]
    fn a_road_network_is_counted_as_a_client_draws_it() {
        let mut record = RoomRecord::default_for_seed(3, "did:test:streets");
        let terrain = record
            .generators
            .values_mut()
            .find(|g| matches!(g.kind, crate::pds::GeneratorKind::Terrain(_)))
            .expect("a seeded room has terrain");
        terrain.children.push(Generator::from_kind(
            crate::pds::GeneratorKind::RoadNetwork(crate::pds::generator::RoadConfig {
                seed: crate::urban::test_support::PILOT_ROAD_SEED,
                ..Default::default()
            }),
        ));
        let map = FinishedHeightMap(crate::urban::test_support::pilot_heightmap(), None);
        let streets = street_costs(&record, &map);
        assert_eq!(streets.len(), 1, "one network, one row");
        let (_, row, tally) = &streets[0];
        assert!(tally.triangles > 1000, "a district's streets: {row}");
        assert!((1..=3).contains(&tally.parts), "a part per surface: {row}");
        assert!(row["streets"].as_u64().is_some_and(|n| n > 0), "{row}");

        let report = world_report(
            "did:test",
            &record,
            &map,
            &HashMap::new(),
            &HashMap::new(),
            &streets,
        );
        let triangles = field(&report, "triangles");
        assert_eq!(triangles["streets"], json!(tally.triangles));
        assert_eq!(
            triangles["total"].as_u64(),
            Some(
                triangles["ground"].as_u64().unwrap()
                    + triangles["placements"].as_u64().unwrap()
                    + tally.triangles
            )
        );
        assert_eq!(field(&report, "parts")["streets"], json!(tally.parts));
        assert_eq!(field(&report, "streets").as_array().map(Vec::len), Some(1));
    }

    /// A mesh with indices draws its indices / 3; one without, its vertices
    /// / 3; points and lines draw none.
    #[test]
    fn a_mesh_draws_its_indices_or_else_its_vertices_in_threes() {
        use bevy::asset::RenderAssetUsages;
        use bevy::mesh::Indices;
        let soup = |topology| {
            Mesh::new(topology, RenderAssetUsages::default())
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 9])
        };
        assert_eq!(
            mesh_triangles(&soup(PrimitiveTopology::TriangleList)),
            Some(3)
        );
        assert_eq!(
            mesh_triangles(&soup(PrimitiveTopology::TriangleStrip)),
            Some(7)
        );
        assert_eq!(mesh_triangles(&soup(PrimitiveTopology::LineList)), Some(0));
        let indexed = soup(PrimitiveTopology::TriangleList)
            .with_inserted_indices(Indices::U32((0..9).cycle().take(36).collect()));
        assert_eq!(mesh_triangles(&indexed), Some(12));
    }
}
