//! Offline no-render text tools: the early-return CLI modes that print
//! and exit before any render app stands up — family-seed survey,
//! road-graph diagnostics, and the session-log analyzers.

use crate::pds::RoomRecord;

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
