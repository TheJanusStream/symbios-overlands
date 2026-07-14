//! Offline no-render text tools: the early-return CLI modes that print
//! and exit before any render app stands up — family-seed survey,
//! road-graph diagnostics, the seeded-room entity census, and the
//! session-log analyzers.

use crate::pds::{Generator, GeneratorKind, Placement, RoomRecord};

use super::Args;

/// Print the first `count` u64 seeds whose
/// [`ChassisFamily`](crate::seeded_defaults::ChassisFamily) matches `fam`
/// (case-insensitive `humanoid` | `boat` | `airship` | `skiff`). A survey aid
/// for the avatar overhaul — seeds map 25 % to each family, so scanning a few
/// thousand always finds enough.
pub(super) fn print_family_seeds(fam: &str, count: usize) {
    use crate::seeded_defaults::ChassisFamily;
    let want = match fam.to_lowercase().as_str() {
        "humanoid" => ChassisFamily::Humanoid,
        "boat" => ChassisFamily::Boat,
        "airship" => ChassisFamily::Airship,
        "skiff" => ChassisFamily::Skiff,
        other => panic!("unknown family {other:?} (humanoid|boat|airship|skiff)"),
    };
    let seeds: Vec<u64> = (0u64..1_000_000)
        .filter(|&s| ChassisFamily::for_seed(s) == want)
        .take(count)
        .collect();
    println!("{want:?} seeds: {seeds:?}");
    // Humanoids also print their stylization tier + physical height so the
    // overhaul loop can pick one exemplar per tier without trial renders.
    if want == ChassisFamily::Humanoid {
        for &s in &seeds {
            let bp = crate::seeded_defaults::HumanoidBlueprint::for_seed(s);
            println!(
                "  seed {s}: {:?} ({:.2} m, {:.1} heads)",
                bp.tier,
                bp.total_h,
                bp.total_h / bp.head_unit
            );
        }
    }
}

/// Resolve an avatar `subject` (a `u64` seed or a DID string) to its
/// [`AvatarOutfit`](crate::seeded_defaults::AvatarOutfit) and character anchor,
/// matching the derivation the built avatar uses.
fn outfit_for(
    subject: &str,
) -> (
    crate::seeded_defaults::AvatarCharacter,
    crate::seeded_defaults::AvatarOutfit,
) {
    use crate::seeded_defaults::{AvatarCharacter, AvatarOutfit};
    match subject.parse::<u64>() {
        Ok(seed) => (
            AvatarCharacter::for_seed(seed),
            AvatarOutfit::for_seed(seed),
        ),
        Err(_) => (
            AvatarCharacter::for_did(subject),
            AvatarOutfit::for_did(subject),
        ),
    }
}

/// Print the resolved outfit for one avatar `subject` (a `u64` seed or a DID):
/// chassis, style, socio tiers, and each filled slot → part slug. A no-render
/// survey aid for the avatar overhaul — the built [`Generator`] carries only
/// geometry (no slugs), so this is the way to see which optional parts an
/// avatar rolled.
///
/// [`Generator`]: crate::pds::generator::Generator
pub(super) fn print_outfit(subject: &str) {
    let (character, outfit) = outfit_for(subject);
    println!(
        "outfit {subject:?}: {:?} / {:?} / ornateness {:?} / wear {:?}",
        outfit.chassis,
        character.style,
        character.ornateness_tier(),
        character.wear_tier(),
    );
    for part in &outfit.parts {
        println!("  {:?} -> {}", part.slot, part.slug);
    }
}

/// Scan seeds and print the first `count` whose outfit fills any slot with the
/// part `slug`, each with its style + socio tiers — answers "which seed rolls
/// this styled part?" for render-verification. Optional parts are rare (theme +
/// ornateness gated), so the scan runs to a high seed ceiling before giving up.
pub(super) fn find_part(slug: &str, count: usize) {
    use crate::seeded_defaults::{AvatarCharacter, AvatarOutfit};
    let mut hits = 0usize;
    println!("seeds rolling part {slug:?}:");
    for s in 0u64..2_000_000 {
        let outfit = AvatarOutfit::for_seed(s);
        if outfit.parts.iter().any(|p| p.slug == slug) {
            let c = AvatarCharacter::for_seed(s);
            println!(
                "  seed {s}: {:?} / {:?} / ornateness {:?} / wear {:?}",
                outfit.chassis,
                c.style,
                c.ornateness_tier(),
                c.wear_tier()
            );
            hits += 1;
            if hits >= count {
                return;
            }
        }
    }
    if hits == 0 {
        println!("  (none found below seed 2_000_000 — is the slug spelled right?)");
    }
}

/// Reproduce a room's heightmap + road config and print the road-graph
/// diagnostics (see [`crate::urban::road_graph_diagnostics`]) to stdout. The
/// room is the seeded default for a `u64` seed or a DID string — the same
/// derivation `--room` uses — so the heightmap and road network match what the
/// game renders for that room.
pub(super) fn dump_road_graph(room: &str) {
    let record = match room.parse::<u64>() {
        Ok(seed) => RoomRecord::default_for_seed(seed, &format!("did:render:{seed}")),
        Err(_) => RoomRecord::default_for_did(room),
    };
    let Some(config) = crate::pds::find_road_config(&record).cloned() else {
        println!(
            "room {room:?}: no road config — this room grows no roads (try a road-growing theme seed)"
        );
        return;
    };
    if !config.enabled {
        println!("room {room:?}: road config present but disabled");
        return;
    }
    println!(
        "room {room:?}: minor_spacing {:.1} m, major_spacing {:.1} m",
        config.minor_spacing.0, config.major_spacing.0
    );
    let hm = crate::terrain::rebuild_heightmap_for_record(&record);
    match crate::urban::road_graph_diagnostics(&hm, &config) {
        Some(stats) => print!("{}", stats.report(room)),
        None => println!(
            "room {room:?}: road graph produced no network (district window too small or tracer empty)"
        ),
    }
}

/// Read a captured NDJSON session log and print its post-mortem report (see
/// [`crate::diagnostics::analyze`]). An unreadable file is reported to stderr;
/// a torn/truncated log is analyzed best-effort (unparseable lines are counted,
/// not fatal). The report is the offline counterpart to the live anomaly engine
/// — the same rule set, replayed over a captured log.
pub(super) fn analyze_session(args: &Args, path: &str) {
    // Filters (all optional) restrict the analysis sections; an invalid filter
    // name aborts with a clear message rather than silently analyzing everything.
    let filters = match crate::diagnostics::analyze::Filters::parse(
        args.subsystem.as_deref(),
        args.category.as_deref(),
        args.severity.as_deref(),
        args.since,
        args.until,
    ) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("invalid analyzer filter: {e}");
            return;
        }
    };
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cannot read session log {path:?}: {e}");
            return;
        }
    };
    let parsed = crate::diagnostics::analyze::parse_ndjson(&text);
    print!(
        "{}",
        crate::diagnostics::analyze::report_with(path, &parsed, &filters)
    );
}

/// Read two captured NDJSON session logs (A = baseline, B = candidate) and print
/// their before/after diff (see [`crate::diagnostics::analyze::diff_report`]) —
/// the fix-validation counterpart to [`analyze_session`]. An unreadable file is
/// reported to stderr and aborts the diff; torn/truncated logs are diffed
/// best-effort (unparseable lines counted, surfaced in each session's header).
pub(super) fn diff_sessions(path_a: &str, path_b: &str) {
    let read = |path: &str| -> Option<String> {
        match std::fs::read_to_string(path) {
            Ok(t) => Some(t),
            Err(e) => {
                eprintln!("cannot read session log {path:?}: {e}");
                None
            }
        }
    };
    let (Some(text_a), Some(text_b)) = (read(path_a), read(path_b)) else {
        return;
    };
    let parsed_a = crate::diagnostics::analyze::parse_ndjson(&text_a);
    let parsed_b = crate::diagnostics::analyze::parse_ndjson(&text_b);
    print!(
        "{}",
        crate::diagnostics::analyze::diff_report(path_a, &parsed_a, path_b, &parsed_b)
    );
}

/// Per-instance entity count of a generator tree, with L-systems **expanded**
/// (the spawn path turns one L-system node into `1 root + material mesh
/// buckets + one entity per prop` — leaves and fruit are where a "1-node"
/// tree becomes thousands of entities, #810). Shape-grammar nodes also expand
/// at spawn but are left at 1 and flagged via [`tree_has_shape`] — the census
/// evidence shows L-systems dominate seeded-room counts by orders of
/// magnitude.
fn tree_entities(g: &Generator, generator_ref: &str) -> u64 {
    let own = match &g.kind {
        GeneratorKind::LSystem {
            source_code,
            finalization_code,
            iterations,
            seed,
            angle,
            step,
            width,
            elasticity,
            tropism,
            mesh_resolution,
            ..
        } => crate::world_builder::lsystem::build_lsystem_geometry(
            source_code,
            finalization_code,
            *iterations,
            *seed,
            *angle,
            *step,
            *width,
            *elasticity,
            *tropism,
            *mesh_resolution,
            generator_ref,
        )
        .map_or(1, |(buckets, props)| {
            1 + buckets.len() as u64 + props.len() as u64
        }),
        _ => 1,
    };
    own + g
        .children
        .iter()
        .map(|c| tree_entities(c, generator_ref))
        .sum::<u64>()
}

/// `true` if any node in the tree is a CGA shape grammar (spawn-time
/// expansion the census does not model — flagged as an underestimate).
fn tree_has_shape(g: &Generator) -> bool {
    matches!(g.kind, GeneratorKind::Shape { .. }) || g.children.iter().any(tree_has_shape)
}

/// Analytic entity census over seeded rooms (#810): for each seed, sum every
/// placement's instance count × generator-tree node count — the record-level
/// estimate of what `compile_room_record` will spawn — and print the total
/// plus the top contributors. Finds the seeds/generators that drive a region
/// toward the `MAX_ROOM_ENTITIES` cap (500 k, unplayable on wasm) without a
/// browser in the loop.
pub(super) fn room_census(seeds: u64) {
    let mut totals: Vec<(u64, u64)> = Vec::new();
    for seed in 0..seeds {
        let record = RoomRecord::default_for_seed(seed, "did:plc:census");
        // (estimate, description) per placement, for the per-seed top list.
        let mut rows: Vec<(u64, String)> = Vec::new();
        for p in &record.placements {
            let (generator_ref, instances) = match p {
                Placement::Absolute { generator_ref, .. } => (generator_ref, 1u64),
                Placement::Scatter {
                    generator_ref,
                    count,
                    ..
                } => (generator_ref, u64::from(*count)),
                Placement::Grid {
                    generator_ref,
                    counts,
                    ..
                } => (
                    generator_ref,
                    counts.iter().map(|&c| u64::from(c)).product::<u64>(),
                ),
                Placement::Unknown => continue,
            };
            let Some(g) = record.generators.get(generator_ref) else {
                continue;
            };
            let nodes = tree_entities(g, generator_ref);
            let est = instances * nodes;
            let shape = if tree_has_shape(g) {
                "  [+shape-grammar expansion]"
            } else {
                ""
            };
            rows.push((
                est,
                format!("{generator_ref} ×{instances} × {nodes} entities = {est}{shape}"),
            ));
        }
        let total: u64 = rows.iter().map(|(e, _)| e).sum();
        totals.push((total, seed));
        rows.sort_by_key(|b| std::cmp::Reverse(b.0));
        println!("seed {seed}: ~{total} entities ({} placements)", rows.len());
        for (_, desc) in rows.iter().take(3) {
            println!("    {desc}");
        }
    }
    totals.sort_by_key(|b| std::cmp::Reverse(b.0));
    println!("\nworst seeds:");
    for (total, seed) in totals.iter().take(10) {
        println!("  seed {seed}: ~{total}");
    }
}
