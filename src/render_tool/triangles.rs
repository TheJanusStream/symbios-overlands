//! Triangle counts (#1471): what a subject, a catalogue entry or a whole
//! world costs to draw.
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
//! # The world report
//!
//! `--triangle-report` grows one copy of each placed generator through the
//! `--catalogue-sizes` app ([`super::sizes::triangles_of`]) and multiplies
//! it by each placement's copies: 1 for an absolute placement, the cells of
//! a grid, and for a scatter the instances its sampler actually places -
//! the census's replay ([`crate::world_builder::compile::scatter_yields`]),
//! which can be fewer than its `count` when its filters refuse most of its
//! ground. The terrain's ground mesh is listed on its own line, built by the
//! game's own mesher from the rebuilt heightmap; a generator's water is the
//! plane its generator spawns, counted with it. Not counted: particles, and
//! what a road network grows (its roads and the buildings on its lots).

use std::collections::{BTreeSet, HashMap};

use bevy::mesh::{Mesh, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use serde_json::{Value, json};

use crate::pds::{Generator, Placement, RoomRecord};
use crate::terrain::FinishedHeightMap;

/// Triangles counted over a set of mesh entities.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Tally {
    pub(super) triangles: u64,
    /// Meshes that could not be counted: an asset that is gone, or one
    /// whose data went to the GPU with no CPU copy kept. Never one in a
    /// no-renderer app; said aloud on the turntable's line when there is.
    pub(super) unreadable: u32,
}

impl Tally {
    /// A tally with nothing left uncounted.
    #[cfg(test)]
    pub(super) fn counted(triangles: u64) -> Self {
        Self {
            triangles,
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
        Ok(())
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

/// The triangles `drawn` draw between them: every entity's mesh counted
/// once for that entity, however many others share it.
pub(super) fn tally<'a>(
    drawn: impl IntoIterator<Item = &'a Mesh3d>,
    assets: &Assets<Mesh>,
) -> Tally {
    let mut out = Tally::default();
    for mesh in drawn {
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
    let placed: BTreeSet<&str> = record
        .placements
        .iter()
        .filter_map(generator_ref)
        .filter(|name| record.generators.contains_key(*name))
        .collect();
    let generators: Vec<(String, Generator)> = placed
        .into_iter()
        .map(|name| (name.to_string(), record.generators[name].clone()))
        .collect();
    // The heightmap job and the spawn app are the two slow halves, and
    // neither reads the other: the map is rebuilt beside the app.
    let (heightmap, each) = std::thread::scope(|scope| {
        let heightmap =
            scope.spawn(|| FinishedHeightMap(crate::terrain::rebuild_heightmap_for_record(record)));
        let each = super::sizes::triangles_of(generators);
        (
            heightmap.join().expect("the heightmap rebuild panicked"),
            each,
        )
    });
    let report = world_report(world, record, &heightmap, &each.into_iter().collect());
    println!("{}", one_row_a_line(&report));
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
const NOT_COUNTED: &str = "live particle quads, which come and go with an emitter's rate; \
     what a road network grows, its roads and the buildings on its lots";

/// The report's fields in print order: the world, the totals, what is not
/// counted, each placed generator's cost and each placement's, both
/// dearest first. `each` is one copy's triangles by generator name.
fn world_report(
    world: &str,
    record: &RoomRecord,
    heightmap: &FinishedHeightMap,
    each: &HashMap<String, u64>,
) -> Vec<(&'static str, Value)> {
    let yields = crate::world_builder::compile::scatter_yields(record, heightmap);
    let mut placements = Vec::new();
    // name -> (one copy, copies, placements)
    let mut generators: HashMap<&str, (u64, u64, u32)> = HashMap::new();
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
        let one = each.get(name.as_str()).copied();
        let triangles = one.unwrap_or(0) * copies;
        let mut row = json!({
            "index": index,
            "generator": name,
            "kind": kind,
            "copies": copies,
            "triangles_each": one.unwrap_or(0),
            "triangles": triangles,
        });
        if let Placement::Scatter { count, .. } = placement {
            row["requested"] = json!(count);
        }
        match one {
            Some(one) => {
                let entry = generators.entry(name).or_insert((one, 0, 0));
                entry.1 += copies;
                entry.2 += 1;
            }
            None => {
                row["why"] = json!("no generator by this name: the compile skips it");
            }
        }
        placements.push((triangles, index, row));
    }
    placements.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut generators: Vec<(u64, &str, Value)> = generators
        .into_iter()
        .map(|(name, (one, copies, used))| {
            let triangles = one * copies;
            let row = json!({
                "generator": name,
                "triangles_each": one,
                "copies": copies,
                "placements": used,
                "triangles": triangles,
            });
            (triangles, name, row)
        })
        .collect();
    generators.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));

    let planted: u64 = placements.iter().map(|p| p.0).sum();
    let ground = ground_triangles(heightmap);
    vec![
        ("world", json!(world)),
        (
            "triangles",
            json!({
                "total": planted + ground,
                "placements": planted,
                "ground": ground,
            }),
        ),
        ("not_counted", json!(NOT_COUNTED)),
        (
            "generators",
            Value::Array(generators.into_iter().map(|g| g.2).collect()),
        ),
        (
            "placements",
            Value::Array(placements.into_iter().map(|p| p.2).collect()),
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
        FinishedHeightMap(map)
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
            opaque_refs: Default::default(),
        }
    }

    fn each() -> HashMap<String, u64> {
        [("rock".to_string(), 12), ("lump".to_string(), 80)]
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
        let report = world_report("did:test", &record, &map, &each());

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

        let open = placement(&report, 3);
        assert_eq!(open["copies"], 10, "{open}");
        assert_eq!(open["triangles"], 800, "{open}");
        // An absolute is one copy, a grid its cells.
        assert_eq!(placement(&report, 0)["triangles"], 12);
        assert_eq!(placement(&report, 2)["copies"], 6);
        assert_eq!(placement(&report, 2)["triangles"], 72);
    }

    /// The printed report is one JSON object: the world, the totals - which
    /// add up - what is not counted, and two lists sorted dearest first,
    /// each row on a line of its own with the fields it promises. A
    /// generator the record lacks costs 0 and says why.
    #[test]
    fn the_triangle_report_is_one_object_with_sorted_rows_a_line_each() {
        let (record, map) = (fixture(), ramp());
        let printed = one_row_a_line(&world_report("did:test", &record, &map, &each()));
        let report: Value = serde_json::from_str(&printed).expect("the report is JSON");

        let keys: Vec<&String> = report.as_object().expect("an object").keys().collect();
        assert_eq!(
            keys,
            [
                "generators",
                "not_counted",
                "placements",
                "triangles",
                "world"
            ],
            "{printed}"
        );
        assert_eq!(report["world"], "did:test");

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
                "triangles",
                "triangles_each"
            ]
        );
        let gone = placements.iter().find(|r| r["index"] == 4).expect("row 4");
        assert_eq!(gone["triangles"], 0);
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
