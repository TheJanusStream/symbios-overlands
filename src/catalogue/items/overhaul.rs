//! The selective overhaul's per-item ledger and checks (#972, #1575).
//!
//! The catalogue is overhauled one item at a time: the owner names an item
//! and it is reworked end to end (#972 holds the rhythm and the lessons).
//! Findings that reach many items are not swept across the catalogue at
//! once - the owner's decision of 2026-10-07, folding Group 6's catalogue
//! issues (#1440 #1439 #1571 #1572) into #972 - they are applied to each
//! item when it is named. This module is where an overhaul finds them:
//!
//! - **Checks every overhauled item passes**, called from its own guard
//!   tests: [`assert_no_z_fighting`] (#1440: 187 of 395 entries drew faces
//!   in one place when it was filed) and [`assert_nothing_floats`] (#1571's
//!   class of defect; [`assert_nothing_floats_but`] for an item meant to
//!   float). The census below says which items fail them today.
//! - **Rows that name an item**: [`KNOWN_DEFECTS`], found in that item and
//!   described well enough to fix; [`SOLID_MESHES_OWED`], the grammar
//!   entries a visitor still walks through (#1572); [`TEXTURE_HINTS`],
//!   items that seemed to fake what a newer generator draws. An item's
//!   overhaul does every row that names it and deletes the row.
//! - **The texture refresh** (the owner's addition): [`TEXTURE_ARRIVALS`]
//!   lists every upstream generator by when it arrived, so an overhaul can
//!   see which arrived since the item's materials were chosen and ask of
//!   each whether it fits better. A generator symbios-texture adds fails a
//!   test here until it is recorded, so the list keeps up with upstream.
//!
//! `catalogue_census` (ignored; run it by name) prints every entry's
//! z-fighting, free parts, owed solids and textures, for choosing what to
//! name next and for regenerating the picture once items have moved:
//!
//! ```text
//! cargo test --profile test-release --lib catalogue_census -- --ignored --nocapture
//! ```

use super::ENTRIES;
use crate::pds::{Generator, GeneratorKind};

/// An overhauled item draws no faces in one place (#1440, folded into #972):
/// the agent's full z-fighting check (#1436) over `generator` - the real
/// mesher, composed transforms, each shape grammar's terminals, faces that
/// face each other or lie buried left out - with no clock. It supersedes
/// [`assert_no_coplanar_faces`](super::util::assert_no_coplanar_faces) in
/// an overhaul's guards: that one reads only axis-aligned whole faces placed
/// by translation, and items that pass it can fail this.
///
/// The check lives in the agent client, which builds on unix only; on any
/// other host this asserts nothing.
pub(in crate::catalogue::items) fn assert_no_z_fighting(generator: &Generator, slug: &str) {
    #[cfg(unix)]
    {
        let pairs = crate::agent::coplanar_overlap_lines(generator);
        assert!(
            pairs.is_empty(),
            "{slug}: {} pair(s) of pieces draw faces in one place where they can be seen \
             (z-fighting, #1440). Sink one into the other or stand it proud:\n  {}",
            pairs.len(),
            pairs.join("\n  ")
        );
    }
    #[cfg(not(unix))]
    let _ = (generator, slug);
}

/// Nothing of an overhauled item floats free of it (#1571's class of defect,
/// folded into #972): the floating report's class a for the item as built,
/// standing on its own ground plane - every part is in a group of touching
/// parts that reaches the ground, or, where no part does, the first part's
/// group. A part resting on an L-system or a sign reads as free, since
/// neither is meshed for the check. A shelf of goods or a strip light hung
/// inside a shell with nothing holding it is a float too (lessons 27 and
/// 33): give it the bracket or the hangers that state what carries it.
pub(in crate::catalogue::items) fn assert_nothing_floats(generator: &Generator, slug: &str) {
    assert_nothing_floats_but(generator, slug, &[]);
}

/// [`assert_nothing_floats`], but the parts under `meant` may float: each a
/// node path as the failure names it (`children[..]`), for an item whose
/// idea is that it floats - a levitating stone, a hologram. Say beside each
/// why. A path that no longer floats fails, so an exception cannot outlive
/// the reason for it.
pub(in crate::catalogue::items) fn assert_nothing_floats_but(
    generator: &Generator,
    slug: &str,
    meant: &[&[usize]],
) {
    let free = crate::render_tool::free_parts(generator);
    let unmeant: Vec<&str> = free
        .iter()
        .filter(|(path, _)| !meant.iter().any(|m| path.starts_with(m)))
        .map(|(_, line)| line.as_str())
        .collect();
    assert!(
        unmeant.is_empty(),
        "{slug}: {} part(s) float free of the body (#1571's class of defect): each touches \
         nothing the item stands by - rest it on a part, or reach it to the ground:\n  {}",
        unmeant.len(),
        unmeant.join("\n  ")
    );
    for m in meant {
        assert!(
            free.iter().any(|(path, _)| path.starts_with(m)),
            "{slug}: children{m:?} is listed as meant to float, but nothing under it floats \
             - drop it from the list"
        );
    }
}

/// Defects found in one item, each fixed by that item's overhaul: (slug,
/// where it was found, what is wrong and the fix that was tried). Delete a
/// row when its fix lands; the item's guards should then hold it.
pub(in crate::catalogue::items) const KNOWN_DEFECTS: &[(&str, &str, &str)] = &[(
    "saloon",
    "found by #1607's review (2026-10-10), giving its corner sign a wind sway",
    "The hanging corner sign (board 0.12 x 0.9 x 1.4 m at x -3.6, y slab_h + 4.3, z \
     front_z - 1.0) hangs into the upstairs gallery: the balustrade's top rail (y 4.29-4.41, \
     z front_z - 1.36 to - 1.24, the full width) passes through the board's lower part at \
     rest, and the end baluster (x -3.8) stands 11 cm off its face - inside any swing. So the \
     sign was left without the wind sway the harbour tavern's has. Fix: hang the board clear \
     of the gallery - forward of front_z - 1.4 and above the rail, or at another corner - \
     then give it a pin and `windblown`, as harbour_tavern does.",
)];

/// The grammar entries a visitor walks through above the footing, because
/// their Shape nodes list no `solid_meshes` (#1572, folded into #972; #1506
/// added the field). An overhaul of one lists its walls, roofs, towers and
/// piers - read its `I("...")` ids - leaves real openings off (an arch a
/// visitor could walk through), and deletes its row: the field's own doc
/// says how each terminal collides. Changing it moves the entry's generator
/// bytes, so re-bless tests/prim_wire.rs on purpose. The test below fails
/// for a listed entry that lists them all, and for a grammar entry that
/// does not and is not listed.
pub(in crate::catalogue::items) const SOLID_MESHES_OWED: &[&str] = &[
    "ruined_temple",
    "villa",
    "watchtower",
    "palace_range",
    "rowhouse_terrace",
    "machiya_row",
    "sawtooth_mill",
    "stave_church",
    "craftsman_bungalow",
];

/// Items that seemed, when a generator arrived, to fake by hand or with an
/// older texture what it draws: (slug, generator, why). Hints for the
/// texture refresh, not findings - the overhaul judges by render whether
/// the newer generator fits better, and deletes the row either way.
pub(in crate::catalogue::items) const TEXTURE_HINTS: &[(&str, &str, &str)] = &[
    (
        "villa",
        "RoofTile",
        "its doc: terracotta roofs of terracotta tile",
    ),
    (
        "well_house",
        "DryStone",
        "its doc: a fieldstone kerb over dark water",
    ),
    (
        "farmhouse",
        "DryStone",
        "its doc: a fieldstone foundation and a fieldstone chimney",
    ),
];

/// Every texture generator symbios-texture offers, by the release it
/// arrived in. An overhaul's texture refresh lists the item's materials
/// (`render --catalogue <slug> --dump`, or the census) and asks, for each
/// generator that arrived after the item's materials were chosen, whether
/// it fits better than what the item wears. When symbios-texture adds a
/// generator, `every_upstream_generator_has_an_arrival` fails until it is
/// recorded here under its release.
pub(in crate::catalogue::items) const TEXTURE_ARRIVALS: &[(&str, &[&str])] = &[
    (
        "0.1.0, 2026-06-25 (the first release)",
        &[
            "Ashlar",
            "Asphalt",
            "Bark",
            "Brick",
            "ChainLink",
            "Cobblestone",
            "Concrete",
            "Corrugated",
            "Encaustic",
            "Fabric",
            "Flame",
            "Flower",
            "Ground",
            "Ice",
            "IronGrille",
            "Lava",
            "Leaf",
            "LeafSprite",
            "LogEnd",
            "Marble",
            "Metal",
            "Pavers",
            "Petal",
            "Plank",
            "Puff",
            "Ring",
            "Rock",
            "Sand",
            "Shard",
            "Shingle",
            "Snow",
            "Snowflake",
            "SoftDisc",
            "Spark",
            "StainedGlass",
            "Stucco",
            "Thatch",
            "Twig",
            "Wainscoting",
            "Window",
        ],
    ),
    (
        "0.1.x, 2026-07-21 (vegetation)",
        &[
            "CactusSkin",
            "Frond",
            "GrassTuft",
            "Broadleaf",
            "Lichen",
            "Moss",
            "Needle",
            "Reed",
        ],
    ),
    (
        "0.2.0, 2026-07-28",
        &[
            "CrackedEarth",
            "ForestFloor",
            "Gravel",
            "Enamel",
            "Obsidian",
            "Chitin",
            "SolarPanel",
            "Parquet",
            "Truchet",
        ],
    ),
    (
        "0.8.0, 2026-10-07 (#1574)",
        &["RoofTile", "LogWall", "DryStone", "Fur"],
    ),
];

/// A box on the ground at the origin, `size` metres, carrying `child`
/// placed at `at` in its frame - the two guards' fixtures.
fn box_with(size: [f32; 3], child: Option<([f32; 3], [f32; 3])>) -> Generator {
    use super::util::{cuboid_tapered, id_quat, prim};
    use crate::pds::SovereignMaterialSettings;
    let mut root = prim(
        cuboid_tapered(size, 0.0, SovereignMaterialSettings::default()),
        [0.0, size[1] * 0.5, 0.0],
        id_quat(),
    );
    if let Some((child_size, at)) = child {
        root.children.push(prim(
            cuboid_tapered(child_size, 0.0, SovereignMaterialSettings::default()),
            at,
            id_quat(),
        ));
    }
    root
}

/// The z-fighting guard bites: a box sunk into a bigger one with its top
/// flush with the bigger one's top shares a square metre of face with it.
#[cfg(unix)]
#[test]
#[should_panic(expected = "z-fighting")]
fn the_z_fighting_guard_names_a_top_flush_with_its_host() {
    assert_no_z_fighting(
        &box_with([2.0, 1.0, 2.0], Some(([1.0, 1.0, 1.0], [0.0, 0.0, 0.0]))),
        "flush",
    );
}

/// And passes the fix: the same box standing 5 cm proud.
#[test]
fn the_z_fighting_guard_passes_a_top_standing_proud() {
    assert_no_z_fighting(
        &box_with([2.0, 1.0, 2.0], Some(([1.0, 1.0, 1.0], [0.0, 0.05, 0.0]))),
        "proud",
    );
}

/// The floating guard bites: a small box hung over a block, touching
/// nothing, is free of the body that stands on the ground.
#[test]
#[should_panic(expected = "float free")]
fn the_floating_guard_names_a_box_in_the_air() {
    assert_nothing_floats(
        &box_with([1.0, 1.0, 1.0], Some(([0.5, 0.5, 0.5], [0.0, 2.0, 0.0]))),
        "hung",
    );
}

/// A part meant to float may - and an exception for a part that no longer
/// floats is refused, so the list cannot outlive its reason.
#[test]
fn the_floating_guard_lets_a_meant_float_be_and_refuses_a_stale_one() {
    assert_nothing_floats_but(
        &box_with([1.0, 1.0, 1.0], Some(([0.5, 0.5, 0.5], [0.0, 2.0, 0.0]))),
        "levitating",
        &[&[0]],
    );
    let stale = std::panic::catch_unwind(|| {
        assert_nothing_floats_but(
            &box_with([1.0, 1.0, 1.0], Some(([0.5, 0.5, 0.5], [0.0, 0.75, 0.0]))),
            "landed",
            &[&[0]],
        );
    });
    assert!(stale.is_err(), "a stale exception is refused");
}

/// And passes the fix: the same box resting on the block's top.
#[test]
fn the_floating_guard_passes_a_box_resting_on_another() {
    assert_nothing_floats(
        &box_with([1.0, 1.0, 1.0], Some(([0.5, 0.5, 0.5], [0.0, 0.75, 0.0]))),
        "resting",
    );
}

/// How many Shape nodes under `node` list no `solid_meshes`.
fn shapes_without_solids(node: &Generator) -> usize {
    let here = usize::from(matches!(
        &node.kind,
        GeneratorKind::Shape { solid_meshes, .. } if solid_meshes.is_empty()
    ));
    here + node
        .children
        .iter()
        .map(shapes_without_solids)
        .sum::<usize>()
}

/// Every slug the ledger names is a catalogue entry, so a renamed item
/// cannot leave its rows behind unread.
#[test]
fn every_ledger_slug_is_a_catalogue_entry() {
    let slugs = KNOWN_DEFECTS
        .iter()
        .map(|(slug, _, _)| *slug)
        .chain(SOLID_MESHES_OWED.iter().copied())
        .chain(TEXTURE_HINTS.iter().map(|(slug, _, _)| *slug));
    for slug in slugs {
        assert!(
            ENTRIES.iter().any(|e| e.slug() == slug),
            "the overhaul ledger names `{slug}`, which is no catalogue entry"
        );
    }
}

/// A grammar entry lists its solid meshes or owes them in
/// [`SOLID_MESHES_OWED`] - and a listed entry that lists them all is told
/// to leave the ledger, so the list only shrinks as items are overhauled.
#[test]
fn grammar_entries_list_their_solid_meshes_or_owe_them_here() {
    let mut walk_through = Vec::new();
    let mut paid = Vec::new();
    for entry in ENTRIES {
        let owes = shapes_without_solids(&entry.build("did:test:overhaul")) > 0;
        let listed = SOLID_MESHES_OWED.contains(&entry.slug());
        if owes && !listed {
            walk_through.push(entry.slug());
        }
        if listed && !owes {
            paid.push(entry.slug());
        }
    }
    assert!(
        walk_through.is_empty(),
        "grammar entries with a Shape node listing no solid_meshes (a visitor walks through \
         them above the footing, #1572): list its walls and roofs, or owe them in \
         SOLID_MESHES_OWED: {walk_through:?}"
    );
    assert!(
        paid.is_empty(),
        "these list their solid meshes now - delete their SOLID_MESHES_OWED rows: {paid:?}"
    );
}

/// Every generator symbios-texture offers has its arrival recorded, once,
/// and nothing is recorded that upstream does not offer.
#[test]
fn every_upstream_generator_has_an_arrival() {
    macro_rules! variants {
        ($( ($variant:ident, $module:ident, $config:ty, $generator:ty, $kind:ident) ),+ $(,)?) => {
            [$(stringify!($variant)),+]
        };
    }
    let upstream: Vec<&str> = symbios_texture::for_each_generator!(variants).to_vec();
    let recorded: Vec<&str> = TEXTURE_ARRIVALS
        .iter()
        .flat_map(|(_, names)| names.iter().copied())
        .collect();
    let unrecorded: Vec<&str> = upstream
        .iter()
        .filter(|v| !recorded.contains(v))
        .copied()
        .collect();
    assert!(
        unrecorded.is_empty(),
        "symbios-texture offers generators the overhaul ledger has not recorded - add them to \
         TEXTURE_ARRIVALS under the release that brought them, so the next overhaul's texture \
         refresh considers them: {unrecorded:?}"
    );
    let unknown: Vec<&str> = recorded
        .iter()
        .filter(|r| !upstream.contains(r))
        .copied()
        .collect();
    assert!(
        unknown.is_empty(),
        "TEXTURE_ARRIVALS records generators upstream does not offer: {unknown:?}"
    );
    let mut once = recorded.clone();
    once.sort_unstable();
    once.dedup();
    assert_eq!(once.len(), recorded.len(), "a generator recorded twice");
}

/// Every entry's z-fighting pairs, free parts, owed solids and textures, one
/// Markdown row each: for choosing the next item to name and for redrawing
/// the picture once items have moved. Run it by name (see the module doc);
/// the z-fighting column needs the agent's check, so it is unix-only. With
/// `CENSUS_SLUGS=a,b` it also prints, for each entry named, every pair and
/// every free part as the guards would name them - an overhaul's first look
/// at what its item owes.
#[cfg(unix)]
#[test]
#[ignore = "a census of every catalogue entry, run on demand"]
fn catalogue_census() {
    println!("| slug | z-fighting pairs (m2) | free parts | owes solids | textures |");
    println!("|---|---|---|---|---|");
    let (mut fighting, mut floating, mut owing) = (0, 0, 0);
    for entry in ENTRIES {
        let built = entry.build("did:test:overhaul");
        let pairs = crate::agent::coplanar_overlap_lines(&built);
        let area: f32 = pairs
            .iter()
            .filter_map(|line| line.split(' ').next()?.parse::<f32>().ok())
            .sum();
        let free = crate::render_tool::free_parts(&built).len();
        let owes = shapes_without_solids(&built) > 0;
        let mut textures = std::collections::BTreeSet::new();
        let mut tree = built.clone();
        let mut stack = vec![&mut tree];
        while let Some(node) = stack.pop() {
            for material in crate::pds::material_finish::node_materials_mut(&mut node.kind) {
                textures.insert(material.texture.label());
            }
            stack.extend(node.children.iter_mut());
        }
        fighting += usize::from(!pairs.is_empty());
        floating += usize::from(free > 0);
        owing += usize::from(owes);
        println!(
            "| {} | {} ({area:.1}) | {free} | {} | {} |",
            entry.slug(),
            pairs.len(),
            if owes { "yes" } else { "" },
            textures.into_iter().collect::<Vec<_>>().join(", ")
        );
    }
    println!(
        "\n{} entries: {fighting} draw faces in one place, {floating} have parts that float \
         free, {owing} owe solid meshes",
        ENTRIES.len()
    );
    for slug in std::env::var("CENSUS_SLUGS").unwrap_or_default().split(',') {
        let Some(entry) = ENTRIES.iter().find(|e| e.slug() == slug.trim()) else {
            continue;
        };
        let built = entry.build("did:test:overhaul");
        println!("\n## {}", entry.slug());
        for line in crate::agent::coplanar_overlap_lines(&built) {
            println!("z-fighting: {line}");
        }
        for (_, line) in crate::render_tool::free_parts(&built) {
            println!("free: {line}");
        }
    }
}
