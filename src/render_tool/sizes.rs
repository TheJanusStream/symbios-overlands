//! `--catalogue-sizes [WORDS...]` (#1466): the box of every catalogue entry
//! a search finds, measured in one process and printed as one JSON object.
//!
//! A builder arranging catalogue pieces needs each one's size, and `agent
//! catalogue` lists none. The turntable prints one (#1448), but it is a whole
//! render app per entry - a GPU, four tiles and a warm-up to learn three
//! numbers - so sizing a search of twenty hits took twenty runs. Here the
//! entries a search finds, by the matcher `agent catalogue` lists with
//! ([`crate::catalogue::search`]), are grown together through the real spawn
//! path, each under its own root at the origin, and each root's box is
//! folded the turntable's way ([`union_box`]): the bounds of every mesh under
//! it, stretched to its particle emitters' anchors.
//!
//! It is an app rather than arithmetic over the generator tree because of
//! the grammar-grown entries: an L-system tree is as big as its derivation
//! grows it, and only the spawn path grows it. Nothing is drawn, though, so
//! the app has no renderer at all - mesh bounds are computed from the meshes
//! in the main world, where the turntable reads them too - and it runs on a
//! machine with no GPU.
//!
//! Each row also carries the entry's triangles (#1471), counted over the
//! same meshes the box folds ([`super::triangles`]), and the world report
//! `--triangle-report` grows a world's generators through this same app to
//! count one copy of each ([`triangles_of`]).

use bevy::app::PluginsState;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::window::ExitCondition;
use serde_json::{Value, json};

use crate::catalogue::CatalogueEntry;
use crate::pds::Generator;
use crate::player::visuals::{AvatarSpawnDeps, spawn_visual_tree};
use crate::world_builder::particles::ParticleEmitterMarker;

use super::TOOL_DID;
use super::headless::{FRAME_GRACE, SubjectMeshQuery, SubjectQuery, union_box};
use super::triangles::tally;

/// Frames every box must hold still, once every entry has drawn, before the
/// boxes are read. The spawn path meshes in the frame it spawns, so the boxes
/// are whole the frame after; the wait is for anything that lands later.
const SETTLE: u32 = 3;

/// The entries `words` finds - the ones `agent catalogue` lists for the same
/// words, in the same order - or the whole catalogue with no words.
pub(crate) fn select(words: &[String]) -> Vec<&'static dyn CatalogueEntry> {
    crate::catalogue::search(&words.join(" ")).collect()
}

/// `--catalogue-sizes`: size every entry `words` finds and print the report.
pub(super) fn print_catalogue_sizes(words: &[String]) {
    let job = SizeJob::new(
        select(words)
            .into_iter()
            .map(|entry| (entry.slug(), entry.name(), entry.build(TOOL_DID))),
    );
    let mut app = sizing_app();
    add_sizing(&mut app, job);
    println!("{}", run_to_report(&mut app));
}

/// The triangles one copy of each named generator draws, grown together
/// through the same app and the same spawn path as `--catalogue-sizes`, in
/// the order given - what `--triangle-report` multiplies by each
/// placement's copies (#1471). A generator that draws no mesh (a particle
/// emitter alone) counts 0.
pub(super) fn triangles_of(generators: Vec<(String, Generator)>) -> Vec<(String, u64)> {
    let job = SizeJob::new(
        generators
            .into_iter()
            .map(|(name, generator)| (name.clone(), name, generator)),
    );
    let mut app = sizing_app();
    add_sizing(&mut app, job);
    run_to_end(&mut app);
    let job = app.world().resource::<SizeJob>();
    job.entries
        .iter()
        .zip(&job.triangles)
        .map(|(entry, triangles)| (entry.slug.clone(), *triangles))
        .collect()
}

/// The app the sizing runs in: the real spawn path and every default plugin
/// but a renderer, and no window.
fn sizing_app() -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            // No backends: no GPU is opened and no render world is made.
            // Nothing here is drawn, and the bounds are the main world's.
            .set(RenderPlugin {
                render_creation: RenderCreation::Automatic(Box::new(WgpuSettings {
                    backends: None,
                    ..default()
                })),
                ..default()
            })
            // Three plugins complain at start-up that there is no render
            // world - an error and two warnings about nothing being sized.
            .set(LogPlugin {
                filter: [
                    bevy::log::DEFAULT_FILTER.trim_end_matches(','),
                    "bevy_render=off,bevy_gizmos_render=off,bevy_gltf=off",
                ]
                .join(","),
                ..default()
            })
            .disable::<bevy::winit::WinitPlugin>(),
    );
    crate::world_builder::register_headless_spawn(&mut app);
    app
}

/// One entry to size: its names in the report, and its built tree until the
/// spawn takes it.
struct Pending {
    slug: String,
    name: String,
    generator: Option<Generator>,
}

/// The sizing's state, and at the end its report.
#[derive(Resource)]
struct SizeJob {
    entries: Vec<Pending>,
    /// Frames measured so far.
    frames: u32,
    /// Every entry's box the last frame, by slot.
    boxes: Vec<Option<(Vec3, Vec3)>>,
    /// Every entry's triangles the last frame, by slot (#1471).
    triangles: Vec<u64>,
    /// Frames the boxes and the counts have held still.
    quiet: u32,
    /// The boxes and counts are final.
    done: bool,
}

impl SizeJob {
    /// A job over `(slug, name, built tree)` triples, in report order.
    fn new(
        entries: impl IntoIterator<Item = (impl Into<String>, impl Into<String>, Generator)>,
    ) -> Self {
        let entries: Vec<Pending> = entries
            .into_iter()
            .map(|(slug, name, generator)| Pending {
                slug: slug.into(),
                name: name.into(),
                generator: Some(generator),
            })
            .collect();
        Self {
            boxes: vec![None; entries.len()],
            triangles: vec![0; entries.len()],
            entries,
            frames: 0,
            quiet: 0,
            done: false,
        }
    }
}

/// The root one entry is spawned under, by its slot in the job.
#[derive(Component)]
struct SizeRoot(usize);

/// Spawn the job's entries on startup and measure them every frame after.
fn add_sizing(app: &mut App, job: SizeJob) {
    app.insert_resource(job)
        .add_systems(Startup, spawn_entries)
        .add_systems(Update, measure);
}

/// Stand the app up as `App::run` would, then run frames until the job's
/// boxes and counts are final.
fn run_to_end(app: &mut App) {
    while app.plugins_state() == PluginsState::Adding {
        bevy::tasks::tick_global_task_pools_on_main_thread();
    }
    app.finish();
    app.cleanup();
    while !app.world().resource::<SizeJob>().done {
        app.update();
    }
}

/// [`run_to_end`], then the job's report.
fn run_to_report(app: &mut App) -> Value {
    run_to_end(app);
    report(app.world().resource::<SizeJob>())
}

/// Every entry under its own root at the origin, as the turntable stands its
/// one subject, through the same spawn path.
fn spawn_entries(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut deps: AvatarSpawnDeps,
    mut job: ResMut<SizeJob>,
) {
    for (slot, entry) in job.entries.iter_mut().enumerate() {
        let Some(generator) = entry.generator.take() else {
            continue;
        };
        let root = commands.spawn((Transform::default(), SizeRoot(slot))).id();
        spawn_visual_tree(
            &mut commands,
            root,
            &generator,
            &mut meshes,
            &mut materials,
            &mut images,
            &mut deps,
            false,
        );
    }
}

/// Each root's box and triangles, from the meshes and emitters under it.
/// Once every entry has a box and neither boxes nor counts have changed for
/// [`SETTLE`] frames - or the turntable's own grace for a subject that
/// never draws has run out - they are final.
#[allow(clippy::too_many_arguments)]
fn measure(
    mut job: ResMut<SizeJob>,
    roots: Query<(Entity, &SizeRoot)>,
    children: Query<&Children>,
    meshes: SubjectQuery,
    drawn: SubjectMeshQuery,
    assets: Res<Assets<Mesh>>,
    emitters: Query<&GlobalTransform, With<ParticleEmitterMarker>>,
) {
    if job.done {
        return;
    }
    job.frames += 1;
    let mut boxes = vec![None; job.entries.len()];
    let mut triangles = vec![0; job.entries.len()];
    for (root, slot) in &roots {
        let under: Vec<Entity> = children.iter_descendants(root).collect();
        boxes[slot.0] = union_box(
            under.iter().filter_map(|&e| meshes.get(e).ok()),
            under.iter().filter_map(|&e| emitters.get(e).ok()),
        );
        triangles[slot.0] =
            tally(under.iter().filter_map(|&e| drawn.get(e).ok()), &assets).triangles;
    }
    if boxes == job.boxes && triangles == job.triangles {
        job.quiet += 1;
    } else {
        job.boxes = boxes;
        job.triangles = triangles;
        job.quiet = 0;
    }
    let all_drawn = job.boxes.iter().all(Option::is_some);
    if (all_drawn && job.quiet >= SETTLE) || job.frames >= FRAME_GRACE {
        job.done = true;
    }
}

/// `{"entries": [..], "unsized": [..]}`: a row per entry that drew, in job
/// order, and each one that did not, with why.
fn report(job: &SizeJob) -> Value {
    let mut entries = Vec::new();
    let mut undrawn = Vec::new();
    for ((entry, drawn), triangles) in job.entries.iter().zip(&job.boxes).zip(&job.triangles) {
        match drawn {
            Some((min, max)) => entries.push(row(&entry.slug, &entry.name, *min, *max, *triangles)),
            None => undrawn.push(json!({
                "slug": entry.slug,
                "why": format!(
                    "it drew no mesh in {} frames, so it has no box (a turntable of it \
                     frames an empty placeholder)",
                    job.frames
                ),
            })),
        }
    }
    json!({ "entries": entries, "unsized": undrawn })
}

/// One entry's box - its size and its corners, from its origin - and the
/// triangles its meshes draw.
fn row(slug: &str, name: &str, min: Vec3, max: Vec3, triangles: u64) -> Value {
    json!({
        "slug": slug,
        "name": name,
        "size": centimetres3(max - min),
        "from": centimetres3(min),
        "to": centimetres3(max),
        "triangles": triangles,
    })
}

/// A length as the turntable's `subject size` line prints it - to the
/// centimetre, by the same `{:.2}` - as the number JSON carries. Rounding the
/// f32 by hand can disagree with that line: 0.125 prints as 0.12, and rounds
/// to 0.13. Adding zero turns the `-0.00` a hair left of the axis prints as
/// into `0.0`.
fn centimetres(value: f32) -> f64 {
    format!("{value:.2}")
        .parse::<f64>()
        .expect("a formatted float parses")
        + 0.0
}

/// [`centimetres`] of each of a vector's coordinates.
fn centimetres3(v: Vec3) -> [f64; 3] {
    [centimetres(v.x), centimetres(v.y), centimetres(v.z)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::GeneratorKind;
    use bevy::ecs::system::RunSystemOnce;

    /// The spawn path, transform propagation and mesh bounds: what the
    /// sizing reads in the tool's app, without the rest of `DefaultPlugins`.
    fn stage(job: SizeJob) -> App {
        let mut app = crate::player::visuals::spawn_path_app();
        app.add_plugins(bevy::transform::TransformPlugin);
        app.add_systems(PostUpdate, bevy::camera::visibility::calculate_bounds);
        add_sizing(&mut app, job);
        app
    }

    fn entry(slug: &'static str) -> (&'static str, &'static str, Generator) {
        let found = crate::catalogue::by_slug(slug)
            .unwrap_or_else(|| panic!("the catalogue has no {slug:?} to size"));
        (slug, found.name(), found.build(TOOL_DID))
    }

    /// `signal_fire` with its flame lifted 5 m clear of its meshes, so the
    /// box has an emitter anchor to reach for that no mesh covers.
    fn lifted_fire() -> (&'static str, &'static str, Generator) {
        fn lift(node: &mut Generator) -> usize {
            let mut lifted = 0;
            if matches!(node.kind, GeneratorKind::ParticleSystem(_)) {
                node.transform.translation.0[1] += 5.0;
                lifted += 1;
            }
            lifted + node.children.iter_mut().map(lift).sum::<usize>()
        }
        let (slug, name, mut fire) = entry("signal_fire");
        assert!(lift(&mut fire) > 0, "signal_fire has no flame to lift");
        (slug, name, fire)
    }

    fn rows(report: &Value, key: &str) -> Vec<Value> {
        report[key].as_array().expect("an array").clone()
    }

    /// The box the turntable would print for `alone`, the only entry in its
    /// world, the box of its meshes without its emitters, and the triangles
    /// the turntable would print beside the box.
    fn turntable_box(
        alone: (&'static str, &'static str, Generator),
    ) -> ((Vec3, Vec3), (Vec3, Vec3), u64) {
        let mut app = stage(SizeJob::new([alone]));
        run_to_report(&mut app);
        app.world_mut()
            .run_system_once(
                |meshes: SubjectQuery,
                 drawn: SubjectMeshQuery,
                 assets: Res<Assets<Mesh>>,
                 emitters: Query<&GlobalTransform, With<ParticleEmitterMarker>>| {
                    (
                        super::super::headless::subject_box(&meshes, &emitters)
                            .expect("it drew alone"),
                        union_box(meshes.iter(), []).expect("it drew alone"),
                        tally(drawn.iter(), &assets).triangles,
                    )
                },
            )
            .expect("the turntable's fold ran")
    }

    /// THE CASE (#1466): entries grown together, each under its own root at
    /// the origin, get the box the turntable gives each one alone - a tree
    /// grown from its grammar, a ground-cover card, and a fire whose flame
    /// sits clear of its meshes, whose box must reach the flame - and the
    /// triangles it counts for each alone (#1471).
    #[test]
    fn entries_sized_together_get_the_box_each_gets_alone() {
        let cases = || [entry("lsys_palm"), entry("gc_grass_tuft"), lifted_fire()];
        let together = run_to_report(&mut stage(SizeJob::new(cases())));
        let sized = rows(&together, "entries");

        assert!(rows(&together, "unsized").is_empty(), "{together}");
        assert_eq!(sized.len(), 3, "{together}");
        for (case, got) in cases().into_iter().zip(&sized) {
            let (slug, name) = (case.0, case.1);
            let ((min, max), meshes_only, triangles) = turntable_box(case);
            assert!(triangles > 0, "{slug} drew no triangles alone");
            assert_eq!(*got, row(slug, name, min, max, triangles), "{slug}");
            if slug == "signal_fire" {
                assert!(
                    max.y > meshes_only.1.y + 1.0,
                    "the lifted flame must stand clear of the meshes, or this case \
                     checks nothing: {max} vs {}",
                    meshes_only.1
                );
            }
        }
        let palm = &sized[0];
        assert!(
            palm["size"][1].as_f64().expect("a height") > 3.0,
            "the palm is only as tall as its grammar grows it: {palm}"
        );
    }

    /// The boxes are read only once every entry has drawn and none has
    /// moved since: an entry whose first mesh lands frames after the rest
    /// hold still is waited for, and so is a second mesh that grows its box
    /// a frame after that.
    #[test]
    fn an_entry_that_draws_late_or_grows_is_waited_for() {
        let late = Generator::from_kind(GeneratorKind::Unknown);
        let mut app = stage(SizeJob::new([
            ("late", "Late", late),
            entry("gc_grass_tuft"),
        ]));
        app.add_systems(
            Update,
            (|mut commands: Commands,
              roots: Query<(Entity, &SizeRoot)>,
              mut meshes: ResMut<Assets<Mesh>>,
              mut frame: Local<u32>| {
                *frame += 1;
                let (size, y) = match *frame {
                    10 => (Vec3::new(1.0, 2.0, 3.0), 1.0),
                    12 => (Vec3::ONE, 5.0),
                    _ => return,
                };
                let (root, _) = roots
                    .iter()
                    .find(|(_, slot)| slot.0 == 0)
                    .expect("the late entry's root");
                commands.spawn((
                    Mesh3d(meshes.add(Cuboid::from_size(size))),
                    Transform::from_xyz(0.0, y, 0.0),
                    ChildOf(root),
                ));
            })
            .before(measure),
        );

        let report = run_to_report(&mut app);

        assert!(rows(&report, "unsized").is_empty(), "{report}");
        let late = &rows(&report, "entries")[0];
        assert_eq!(late["slug"], "late");
        assert_eq!(late["size"], json!([1.0, 5.5, 3.0]), "{report}");
        assert_eq!(late["triangles"], 24, "both late cuboids: {report}");
    }

    /// An icosphere of resolution `n`, through the primitive spawn path.
    fn icosphere(n: u32) -> Generator {
        let mut kind = GeneratorKind::default_primitive_for_tag("Sphere").expect("a sphere");
        let GeneratorKind::Sphere { resolution, .. } = &mut kind else {
            unreachable!("the Sphere tag builds a sphere")
        };
        *resolution = n;
        Generator::from_kind(kind)
    }

    /// The count one entry's row carries, by slug.
    fn triangles_in(report: &Value, slug: &str) -> u64 {
        rows(report, "entries")
            .iter()
            .find(|row| row["slug"] == slug)
            .unwrap_or_else(|| panic!("no row for {slug}: {report}"))["triangles"]
            .as_u64()
            .expect("a count")
    }

    /// THE CASE (#1471): a row counts the triangles its meshes hold. A
    /// cuboid is 12. An icosphere of resolution `n` is Bevy's `ico(n)`,
    /// which cuts every edge of the icosahedron's 20 faces into `n + 1`,
    /// so `20 * (n + 1)^2` - 720 at resolution 5, and not the `20 * 4^n`
    /// (20,480) of halving each edge `n` times, which a builder was told.
    #[test]
    fn a_cuboid_and_an_icosphere_count_the_triangles_their_meshes_hold() {
        let spheres = [1, 2, 5];
        let report = run_to_report(&mut stage(SizeJob::new(
            std::iter::once(("cuboid".to_string(), Generator::default_cuboid()))
                .chain(spheres.map(|n| (format!("ico{n}"), icosphere(n))))
                .map(|(slug, generator)| (slug.clone(), slug, generator)),
        )));

        assert_eq!(triangles_in(&report, "cuboid"), 12, "{report}");
        for n in spheres {
            let want = 20 * u64::from(n + 1).pow(2);
            assert_eq!(triangles_in(&report, &format!("ico{n}")), want, "{report}");
        }
        assert_eq!(triangles_in(&report, "ico5"), 720);
        assert_ne!(720, 20 * 4u64.pow(5), "the two rules part at resolution 2");
    }

    /// A mesh many entities draw counts once for each of them: three equal
    /// spheres under a cuboid share one mesh from the primitive cache, and
    /// cost three spheres.
    #[test]
    fn a_mesh_shared_by_many_entities_counts_once_for_each() {
        let mut lumps = Generator::default_cuboid();
        lumps.children = vec![icosphere(2); 3];
        let mut app = stage(SizeJob::new([("lumps", "Lumps", lumps)]));
        let report = run_to_report(&mut app);

        let handles: Vec<AssetId<Mesh>> = app
            .world_mut()
            .run_system_once(|drawn: SubjectMeshQuery| {
                drawn.iter().map(|m| m.id()).collect::<Vec<_>>()
            })
            .expect("the meshes are read");
        let shared = handles
            .iter()
            .filter(|id| handles.iter().filter(|other| other == id).count() == 3)
            .count();
        assert_eq!(
            shared, 3,
            "the three spheres must share one mesh, or this case checks nothing: {handles:?}"
        );
        assert_eq!(triangles_in(&report, "lumps"), 12 + 3 * 180, "{report}");
    }

    /// A live particle quad is not counted, where any other mesh that
    /// lands under an entry later is: the same cuboid hung under two
    /// entries a few frames in counts under one and not under the other,
    /// the one it lands on as a particle.
    #[test]
    fn a_live_particle_is_not_counted() {
        let mut app = stage(SizeJob::new([
            ("sparks", "Sparks", Generator::default_cuboid()),
            ("stone", "Stone", Generator::default_cuboid()),
        ]));
        app.add_systems(
            Update,
            (|mut commands: Commands,
              roots: Query<(Entity, &SizeRoot)>,
              mut meshes: ResMut<Assets<Mesh>>,
              mut frame: Local<u32>| {
                *frame += 1;
                if *frame != 3 {
                    return;
                }
                let quad = meshes.add(Cuboid::from_size(Vec3::splat(0.5)));
                for (root, slot) in &roots {
                    let mut child = commands.spawn((Mesh3d(quad.clone()), ChildOf(root)));
                    if slot.0 == 0 {
                        child.insert(crate::world_builder::particles::Particle {
                            age: 0.0,
                            lifetime: 1.0,
                            velocity: Vec3::ZERO,
                            emitter: root,
                            atlas_dim: None,
                            frame_index: 0,
                            frame_mode: Default::default(),
                            ramp_index: 0,
                        });
                    }
                }
            })
            .before(measure),
        );

        let report = run_to_report(&mut app);

        assert_eq!(triangles_in(&report, "stone"), 24, "{report}");
        assert_eq!(triangles_in(&report, "sparks"), 12, "{report}");
    }

    /// An entry that draws nothing does not fail the run: it is listed
    /// under `unsized` with why, and the rest are still sized. The report
    /// holds those two lists and nothing else, and a row its six fields.
    #[test]
    fn an_entry_that_draws_nothing_is_listed_unsized_and_the_rest_are_sized() {
        let nothing = Generator::from_kind(GeneratorKind::Unknown);
        let report = run_to_report(&mut stage(SizeJob::new([
            ("nothing", "Nothing", nothing),
            entry("gc_grass_tuft"),
        ])));

        let keys: Vec<&String> = report.as_object().expect("an object").keys().collect();
        assert_eq!(keys, ["entries", "unsized"], "{report}");
        let undrawn = rows(&report, "unsized");
        assert_eq!(undrawn.len(), 1, "{report}");
        assert_eq!(undrawn[0]["slug"], "nothing");
        assert!(
            undrawn[0]["why"]
                .as_str()
                .is_some_and(|why| why.contains("no mesh")),
            "{report}"
        );
        let sized = rows(&report, "entries");
        assert_eq!(sized.len(), 1, "{report}");
        let fields: Vec<&String> = sized[0].as_object().expect("a row").keys().collect();
        assert_eq!(
            fields,
            ["from", "name", "size", "slug", "to", "triangles"],
            "{report}"
        );
        assert_eq!(sized[0]["slug"], "gc_grass_tuft");
    }

    /// `--catalogue-sizes` alone sizes the whole catalogue, and its words may
    /// come one to an argument or quoted together.
    #[test]
    fn the_size_flag_takes_no_words_or_any_number_of_them() {
        use clap::Parser;
        let words = |argv: &[&str]| {
            super::super::Args::parse_from(std::iter::once("render").chain(argv.iter().copied()))
                .catalogue_sizes
                .expect("the flag was given")
        };
        let slugs = |words: &[String]| -> Vec<&str> {
            select(words).iter().map(|entry| entry.slug()).collect()
        };

        let none = words(&["--catalogue-sizes"]);
        assert!(none.is_empty());
        assert_eq!(slugs(&none).len(), crate::catalogue::ENTRIES.len());
        let split = slugs(&words(&["--catalogue-sizes", "rust", "scrap"]));
        let quoted = slugs(&words(&["--catalogue-sizes", "rust scrap"]));
        assert!(!split.is_empty());
        assert_eq!(split, quoted);
    }

    /// A row carries the numbers the turntable's `subject size` line prints
    /// for the same box and count - 0.125 included, which the line prints
    /// as 0.12 and rounding by hand makes 0.13 - and never a `-0.0`.
    #[test]
    fn a_size_row_says_what_the_turntable_line_says() {
        let (min, max) = (Vec3::new(-1.5, -0.001, -0.125), Vec3::new(1.5, 4.6, 0.75));
        let triangles = super::super::triangles::Tally::counted(1_452);
        let line = super::super::headless::describe_box(min, max, triangles);
        let printed: Vec<f64> = line
            .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
            .filter_map(|word| word.parse().ok())
            .collect();

        let got = row("x", "X", min, max, triangles.triangles);
        let carried: Vec<f64> = ["size", "from", "to"]
            .iter()
            .flat_map(|key| (0..3).map(move |i| (*key, i)))
            .map(|(key, i)| got[key][i].as_f64().expect("a number"))
            .chain(got["triangles"].as_f64())
            .collect();

        assert_eq!(carried, printed, "{line}");
        assert_eq!(got["from"][2], -0.12, "{got}");
        assert!(!got.to_string().contains("-0.0"), "{got}");
    }
}
