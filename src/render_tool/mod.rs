//! Headless render tool — renders any subject (avatar / catalogue item /
//! primitive / whole seeded room) through the **real** spawn path
//! ([`crate::player::visuals::spawn_avatar_visuals`], which routes every node
//! kind — primitives, Shape grammar, L-system — through the same machinery the
//! game uses) into a multi-angle **contact-sheet** PNG. Lets the agent
//! self-validate geometry/materials without manual in-game screenshots.
//!
//! Lives in the library (not the `render` bin) so it can reach the
//! crate-internal `SpawnCtx`/cache resources; the bin is a one-line shim.
//!
//! ```text
//! cargo run --bin render -- --avatar 1          # seed or DID
//! cargo run --bin render -- --catalogue villa   # any catalogue slug
//! cargo run --bin render -- --prim tube         # a single primitive kind
//! cargo run --bin render -- --room 3            # the seeded settlement
//! cargo run --bin render -- --generator g.json  # a dumped/edited Generator
//! cargo run --bin render -- --catalogue lsys_palm --ages 2,3,4,5
//! #                                             # age-progression grid (#908)
//! # → /tmp/avatar-render/<label>.png  (front / ¾ / side / back tiles;
//! #   with --ages one such row per iteration count)
//! ```
//!
//! The same binary also hosts several no-render modes that short-circuit
//! before any render app stands up: `--family-seeds` (chassis-family seed
//! survey), `--dump` (print a subject's `Generator` JSON), `--road-dump`
//! (road-graph topology stats), and the offline session-log analyzers
//! `--analyze-session` / `--diff-sessions`. See the per-arg docs on `Args`.

use std::time::Duration;

use bevy::app::ScheduleRunnerPlugin;
use bevy::prelude::*;
use bevy::window::ExitCondition;
use clap::Parser;

use crate::pds::avatar::default_visuals::{build_for_did, build_for_seed};
use crate::pds::types::{Fp, Fp2};
use crate::pds::{Generator, GeneratorKind, RoomRecord};

mod headless;
mod text_tools;

use headless::{Capture, Frames, RenderJob, Subject, drive, setup};
use text_tools::{
    analyze_session, diff_sessions, dump_road_graph, find_part, print_family_seeds, print_outfit,
    room_census, scatter_census, scatter_plot,
};

/// Camera yaw per tile (degrees), left→right: front, ¾, side, back. Avatars /
/// vehicles face local -Z, so the camera sits on the -Z side (`cos 180 = -1`)
/// to face the subject.
const ANGLES: [f32; 4] = [180.0, 135.0, 90.0, 0.0];
/// Default perspective FOV (matches Bevy's `PerspectiveProjection` default).
const FOV: f32 = std::f32::consts::FRAC_PI_4;
/// Frames to run (after framing) before capturing, so procedural textures
/// finish baking + patching into their materials.
const WARMUP: u32 = 200;
const OUT_DIR: &str = "/tmp/avatar-render";

#[derive(Parser)]
#[command(about = "Headless contact-sheet renderer for avatars / catalogue / primitives / rooms")]
struct Args {
    /// Avatar subject: a u64 seed or a DID string.
    #[arg(long)]
    avatar: Option<String>,
    /// List the first `--family-count` seeds whose
    /// [`ChassisFamily`](crate::seeded_defaults::ChassisFamily) matches
    /// (`humanoid` | `boat` | `airship` | `skiff`) and exit — a survey aid for
    /// the avatar overhaul: pick seeds from the printed list, then render each
    /// with `--avatar <seed>`. Highest precedence (prints, never renders).
    #[arg(long)]
    family_seeds: Option<String>,
    /// How many seeds `--family-seeds` prints (also the cap for
    /// `--find-part`).
    #[arg(long, default_value_t = 8)]
    family_count: usize,
    /// Print one avatar's resolved outfit (chassis / style / socio tiers /
    /// slot→slug) and exit — a `u64` seed or a DID. A no-render survey aid for
    /// the avatar overhaul: the built geometry carries no slugs, so this is how
    /// to see which optional parts an avatar rolled.
    #[arg(long)]
    outfit: Option<String>,
    /// Scan seeds and print the first `--family-count` whose outfit rolls the
    /// given part slug (e.g. `boat_bow_ram`), with each one's style + tiers,
    /// then exit — finds render-verification seeds for a styled part.
    #[arg(long)]
    find_part: Option<String>,
    /// Catalogue subject: an entry slug (e.g. `villa`, `bench`, `wizard_tower`).
    #[arg(long)]
    catalogue: Option<String>,
    /// With `--catalogue <plant-slug>`: apply that plant's named material
    /// re-skin (#910) before rendering — e.g.
    /// `--catalogue lsys_monopodial_tree --variant larch_gold`. Variants
    /// change bark/foliage materials only, never geometry, so this composes
    /// with `--ages`. An unknown name renders the entry's default materials
    /// (the same fallback the seeded pools get); pass `--variant list` to
    /// print the entry's available variants and exit.
    #[arg(long)]
    variant: Option<String>,
    /// Render a [`Generator`] deserialized from a JSON file. Lets the agent
    /// iterate on an L-system grammar (or any generator) without recompiling
    /// the crate: `--dump` a catalogue entry to seed the JSON, edit the
    /// grammar / scalars, re-render. Highest precedence among the render
    /// subjects (`--generator` > `--room` > `--prim` > `--catalogue` >
    /// `--avatar`); the no-render modes still run first.
    #[arg(long)]
    generator: Option<String>,
    /// With `--catalogue <slug>`, `--prim <tag>` (overrides applied), or
    /// `--avatar <seed|did>`: print that subject's built [`Generator`] as
    /// pretty JSON to stdout and exit (a valid seed file for `--generator`,
    /// enabling a no-recompile geometry-iteration loop).
    #[arg(long, default_value_t = false)]
    dump: bool,
    /// Age-progression sweep for an L-system subject (#908): comma-separated
    /// iteration counts (e.g. `--ages 2,3,4,5`). Renders a grid sheet instead
    /// of the single row — one row per age (top→bottom in argument order),
    /// columns = the four angles — with every row framed at one shared camera
    /// distance so relative plant size across ages stays honest. Each count
    /// overrides `iterations` on every L-system node in the subject's
    /// generator tree; combines with any single-generator subject
    /// (`--generator` > `--prim` > `--catalogue` > `--avatar`), panics on
    /// `--room` or a subject without an L-system node. Values above the
    /// record sanitiser cap (12) are accepted here but blow up derivation
    /// size fast — the `MAX_LSYSTEM_STATE_LEN` guard still applies.
    #[arg(long)]
    ages: Option<String>,
    /// Primitive subject: a kind tag (`cuboid`, `sphere`, `tube`, `bevel`, …).
    #[arg(long)]
    prim: Option<String>,
    /// Room subject: a u64 seed or DID — renders the seeded settlement cluster.
    #[arg(long)]
    room: Option<String>,
    /// Road-graph diagnostics: a u64 seed or DID — reproduces the room's
    /// heightmap, builds the *meshed* road graph, and prints topology +
    /// geometry-risk stats (degree histogram, dead-end spurs, spurious-junction
    /// and spike-risk counts), then exits. A no-render dump to size road-network
    /// data filtering. Runs before any render app stands up.
    #[arg(long)]
    road_dump: Option<String>,
    /// Analytic entity census over seeded rooms (#810): for seeds `0..N`, sum
    /// every placement's instance count × generator-tree node count (the
    /// record-level estimate of what the compile will spawn) and print each
    /// seed's total + top contributors, then the worst seeds. Finds the
    /// seeds/generators that drive a region toward the `MAX_ROOM_ENTITIES`
    /// cap without a browser in the loop. A no-render mode.
    #[arg(long)]
    room_census: Option<u64>,
    /// Placement census over seeded rooms (#912): for seeds `0..N`, replay the
    /// real scatter sampling loop against a heightmap rebuilt from each record
    /// and print what it actually places — yield vs. requested count, what the
    /// slope cutoff costs, the per-instance scale spread, and a Clark–Evans
    /// nearest-neighbour index measuring how clustered the survivors are
    /// against the same scatter with its naturalness zeroed. Where
    /// `--room-census` answers "how many entities", this answers "how are they
    /// arranged". A no-render mode; a few seconds per seed (it rebuilds the
    /// heightmap).
    #[arg(long)]
    scatter_census: Option<u64>,
    /// Plan-view plot of one seeded room's scatters (#912): a u64 seed or DID.
    /// Writes a PNG grid to `--out` — one row per scatter, tuned arrangement
    /// on the left, the same scatter with its naturalness zeroed on the right.
    /// The four-angle contact sheet cannot show this: a stand is hundreds of
    /// metres across, so framed to fit every instance is a speck and the
    /// clustering is invisible. A no-render mode.
    #[arg(long)]
    scatter_plot: Option<String>,
    /// Offline session-log post-mortem: read a captured session log
    /// (`diagnostics/session-latest.jsonl`, or the wasm "Download log" dump —
    /// same NDJSON format) and print an agent-facing report (header, `[Verdict]`,
    /// `[Event Tallies]`, `[Timeline]`, `[Loading Gate]`, `[Metric Trends]`,
    /// `[Invariant Violations]`), then exit. Narrow the analysis with the
    /// `--subsystem` / `--category` / `--severity` / `--since` / `--until`
    /// filters. A no-render analysis alongside `--road-dump`. Native-only.
    #[arg(long)]
    analyze_session: Option<String>,
    /// Offline before/after diff: read two captured session logs (A = baseline,
    /// B = candidate) and print a delta report (verdict / loading-gate timings /
    /// metric peaks / invariant fires) so an agent can confirm a fix in run B
    /// improved on the baseline A, then exit. `--diff-sessions <a> <b>`. A
    /// no-render analysis; runs after `--analyze-session`. Native-only.
    #[arg(long, num_args = 2, value_names = ["A", "B"])]
    diff_sessions: Option<Vec<String>>,
    /// `--analyze-session` filter: restrict the analysis sections to one
    /// subsystem (`loading`|`network`|`offload`|`runtime`|`session`). The header
    /// (session identity) is always shown in full.
    #[arg(long)]
    subsystem: Option<String>,
    /// `--analyze-session` filter: restrict to one event category
    /// (`lifecycle`|`fetch`|`generation`|`audio`|`peer`|… — see docs/diagnostics.md).
    #[arg(long)]
    category: Option<String>,
    /// `--analyze-session` filter: restrict to events at or above this severity
    /// (`trace`|`info`|`warn`|`error`|`critical`).
    #[arg(long)]
    severity: Option<String>,
    /// `--analyze-session` filter: restrict to events at or after this
    /// session-relative time (seconds).
    #[arg(long)]
    since: Option<f64>,
    /// `--analyze-session` filter: restrict to events at or before this
    /// session-relative time (seconds).
    #[arg(long)]
    until: Option<f64>,
    /// Torture/cut overrides for a `--prim` subject (for testing the prim
    /// system). `--shear x,z` · `--twist rad` · `--taper x,z` ·
    /// `--taperbottom x,z` · `--bulge x,z` · `--pathcut a,b` ·
    /// `--profilecut a,b` · `--hollow h`.
    #[arg(long)]
    shear: Option<String>,
    #[arg(long)]
    twist: Option<f32>,
    #[arg(long)]
    taper: Option<String>,
    #[arg(long)]
    taperbottom: Option<String>,
    #[arg(long)]
    bulge: Option<String>,
    #[arg(long)]
    pathcut: Option<String>,
    #[arg(long)]
    profilecut: Option<String>,
    #[arg(long)]
    hollow: Option<f32>,
    /// Per-tile pixel side. Forced to a multiple of 64 (no GPU row padding).
    #[arg(long, default_value_t = 512)]
    size: u32,
    /// Output PNG path (defaults to `/tmp/avatar-render/<label>.png`).
    #[arg(long)]
    out: Option<String>,
}

/// CLI entry point (called by the `render` bin).
pub fn run() {
    let args = Args::parse();

    // `--family-seeds <fam>`: print the first N seeds mapping to a chassis
    // family and exit — a survey aid, never renders.
    if let Some(fam) = &args.family_seeds {
        print_family_seeds(fam, args.family_count);
        return;
    }

    // `--outfit <seed|did>`: print one avatar's resolved outfit and exit.
    if let Some(subject) = &args.outfit {
        print_outfit(subject);
        return;
    }

    // `--find-part <slug>`: scan for seeds that roll a styled part and exit.
    if let Some(slug) = &args.find_part {
        find_part(slug, args.family_count);
        return;
    }

    // `--road-dump <seed|did>`: print the room's road-graph diagnostics and
    // exit — a no-render topology/geometry-risk dump for the road-filtering work.
    if let Some(room) = &args.road_dump {
        dump_road_graph(room);
        return;
    }

    // `--room-census <n>`: print seeded rooms' analytic entity estimates and
    // exit — the #810 density survey, never renders.
    if let Some(n) = args.room_census {
        room_census(n);
        return;
    }

    // `--scatter-census <n>`: replay the real sampling loop over seeded rooms
    // and print placement yield + arrangement — the #912 naturalness survey.
    if let Some(n) = args.scatter_census {
        scatter_census(n);
        return;
    }

    // `--scatter-plot <seed>`: write the plan-view PNG that shows what the
    // census's clustering number means.
    if let Some(room) = &args.scatter_plot {
        scatter_plot(
            room,
            std::path::Path::new(args.out.as_deref().unwrap_or("scatter-plot.png")),
        );
        return;
    }

    // `--analyze-session <path>`: read a captured NDJSON session log, replay the
    // anomaly rules over it, and print an agent-facing post-mortem — a no-render
    // analysis, the offline counterpart to the live diagnostic engine.
    if let Some(path) = &args.analyze_session {
        analyze_session(&args, path);
        return;
    }

    // `--diff-sessions <a> <b>`: read two captured logs and print a before/after
    // delta report — the fix-validation counterpart to `--analyze-session`.
    if let Some(pair) = &args.diff_sessions {
        diff_sessions(&pair[0], &pair[1]);
        return;
    }

    // `--dump`: serialize the subject's generator to stdout (a valid
    // `--generator` seed) and exit before standing up the render app. Supports
    // a catalogue slug, a primitive tag (with the `--cut`/`--hollow`/…
    // overrides applied, #663), or an avatar seed/DID so any of them can
    // drive the fast no-recompile geometry loop.
    if args.dump {
        let g = if let Some(slug) = args.catalogue.as_deref() {
            crate::catalogue::by_slug(slug)
                .unwrap_or_else(|| panic!("unknown catalogue slug {slug:?}"))
                .build("did:render:tool")
        } else if let Some(tag) = args.prim.as_deref() {
            // Same construction as resolve_subject's --prim arm, so the
            // dumped JSON is exactly what a render of the same flags spawns.
            let mut kind =
                primitive_for_tag(tag).unwrap_or_else(|| panic!("unknown primitive tag {tag:?}"));
            apply_prim_overrides(&mut kind, &args);
            Generator::from_kind(kind)
        } else if let Some(avatar) = args.avatar.as_deref() {
            match avatar.parse::<u64>() {
                Ok(seed) => build_for_seed(seed).0,
                Err(_) => build_for_did(avatar).0,
            }
        } else {
            panic!(
                "--dump requires --catalogue <slug>, --prim <tag>, or --avatar <seed|did> \
                 (--room/--generator subjects are file/derived records — dump not supported)"
            );
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&g).expect("generator serialize")
        );
        return;
    }

    let (subject, label) = resolve_subject(&args);
    let (subject, label) = match &args.ages {
        Some(ages) => age_sweep(subject, &label, ages),
        None => (subject, label),
    };
    let out = args
        .out
        .clone()
        .unwrap_or_else(|| format!("{OUT_DIR}/{label}.png"));
    let size = (args.size / 64).max(1) * 64;

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .disable::<bevy::winit::WinitPlugin>(),
    )
    .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::ZERO));
    // Resources + texture/material plugins the real spawn path reads.
    crate::world_builder::register_headless_spawn(&mut app);
    app.insert_resource(ClearColor(Color::srgb(0.52, 0.55, 0.70)))
        .insert_resource(RenderJob { subject, out, size })
        .init_resource::<Frames>()
        .init_resource::<Capture>()
        .add_systems(Startup, setup)
        .add_systems(Update, drive)
        .run();
}

/// Build the subject + a filename label from the CLI args.
/// Precedence: `--generator` → `--room` → `--prim` → `--catalogue` → `--avatar` → seed 7.
fn resolve_subject(args: &Args) -> (Subject, String) {
    if let Some(path) = &args.generator {
        let json = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read generator {path:?}: {e}"));
        let generator: Generator =
            serde_json::from_str(&json).unwrap_or_else(|e| panic!("parse generator {path:?}: {e}"));
        let label = std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("generator")
            .to_string();
        return (Subject::Single(Box::new(generator)), format!("gen-{label}"));
    }
    if let Some(room) = &args.room {
        let record = match room.parse::<u64>() {
            Ok(seed) => RoomRecord::default_for_seed(seed, &format!("did:render:{seed}")),
            Err(_) => RoomRecord::default_for_did(room),
        };
        let label = format!("room-{}", room.replace([':', '/'], "_"));
        return (Subject::Room(Box::new(record)), label);
    }
    if let Some(tag) = &args.prim {
        let mut kind =
            primitive_for_tag(tag).unwrap_or_else(|| panic!("unknown primitive tag {tag:?}"));
        apply_prim_overrides(&mut kind, args);
        return (
            Subject::Single(Box::new(Generator::from_kind(kind))),
            format!("prim-{}", tag.to_lowercase()),
        );
    }
    if let Some(slug) = &args.catalogue {
        let entry = crate::catalogue::by_slug(slug)
            .unwrap_or_else(|| panic!("unknown catalogue slug {slug:?}"));
        let mut generator = entry.build("did:render:tool");
        let mut label = format!("cat-{slug}");
        if let Some(variant) = &args.variant {
            if variant == "list" {
                println!("{slug} variants:");
                for v in entry.variants() {
                    println!("  {:<16} {}", v.name, v.label);
                }
                if entry.variants().is_empty() {
                    println!("  (none — this entry has no material re-skins)");
                }
                std::process::exit(0);
            }
            if let GeneratorKind::LSystem { materials, .. } = &mut generator.kind {
                crate::catalogue::items::plants::variant::apply_named(
                    entry.variants(),
                    variant,
                    materials,
                );
            }
            label.push_str(&format!("-{variant}"));
        }
        return (Subject::Single(Box::new(generator)), label);
    }
    let avatar = args.avatar.clone().unwrap_or_else(|| "7".to_string());
    let (generator, label) = match avatar.parse::<u64>() {
        Ok(seed) => (build_for_seed(seed).0, format!("seed-{seed}")),
        Err(_) => (build_for_did(&avatar).0, avatar.replace([':', '/'], "_")),
    };
    (Subject::Single(Box::new(generator)), label)
}

/// Expand a single-generator subject into the `--ages` lineup: one clone per
/// iteration count, ready for the grid contact sheet (rows top→bottom follow
/// the argument order). Panics on `--room` subjects and on generator trees
/// without an L-system node — an age sweep of those is meaningless.
fn age_sweep(subject: Subject, label: &str, ages: &str) -> (Subject, String) {
    let Subject::Single(base) = subject else {
        panic!("--ages needs a single-generator subject (--generator/--prim/--catalogue/--avatar)");
    };
    let ages: Vec<u32> = ages
        .split(',')
        .map(|a| {
            a.trim()
                .parse::<u32>()
                .unwrap_or_else(|e| panic!("bad --ages entry {a:?}: {e}"))
        })
        .collect();
    assert!(
        !ages.is_empty(),
        "--ages needs at least one iteration count"
    );
    let variants: Vec<Generator> = ages
        .iter()
        .map(|&n| {
            let mut g = (*base).clone();
            assert!(
                override_lsystem_iterations(&mut g, n),
                "--ages: subject has no L-system node to sweep"
            );
            g
        })
        .collect();
    println!("age sweep rows, top→bottom: {ages:?} iterations");
    (Subject::Lineup(variants), format!("{label}-ages"))
}

/// Set `iterations` on every L-system node in the tree; returns whether any
/// node was hit.
fn override_lsystem_iterations(g: &mut Generator, iterations: u32) -> bool {
    let mut hit = false;
    if let GeneratorKind::LSystem { iterations: it, .. } = &mut g.kind {
        *it = iterations;
        hit = true;
    }
    for child in &mut g.children {
        hit |= override_lsystem_iterations(child, iterations);
    }
    hit
}

/// Parse a `"a,b"` pair into `[f32; 2]` (missing components default to 0).
fn parse2(s: &str) -> [f32; 2] {
    let mut it = s.split(',').map(|x| x.trim().parse::<f32>().unwrap_or(0.0));
    [it.next().unwrap_or(0.0), it.next().unwrap_or(0.0)]
}

/// Apply the CLI torture/cut overrides to a `--prim` subject for testing.
fn apply_prim_overrides(kind: &mut GeneratorKind, args: &Args) {
    let Some(t) = kind.torture_mut() else {
        return;
    };
    if let Some(s) = &args.shear {
        t.shear = Fp2(parse2(s));
    }
    if let Some(v) = args.twist {
        t.twist = Fp(v);
    }
    if let Some(s) = &args.taper {
        t.taper = Fp2(parse2(s));
    }
    if let Some(s) = &args.taperbottom {
        t.taper_bottom = Fp2(parse2(s));
    }
    if let Some(s) = &args.bulge {
        t.bulge = Fp2(parse2(s));
    }
    if let Some(s) = &args.pathcut {
        t.path_cut = Fp2(parse2(s));
    }
    if let Some(s) = &args.profilecut {
        t.profile_cut = Fp2(parse2(s));
    }
    if let Some(h) = args.hollow {
        t.hollow = Fp(h);
    }
}

/// Resolve a primitive tag (case-insensitive) to a default kind. Wraps
/// [`GeneratorKind::default_primitive_for_tag`], which is title-cased.
fn primitive_for_tag(tag: &str) -> Option<GeneratorKind> {
    let mut chars = tag.chars();
    let titled: String = {
        let first = chars.next()?;
        first.to_uppercase().collect::<String>() + chars.as_str()
    };
    GeneratorKind::default_primitive_for_tag(&titled)
}
