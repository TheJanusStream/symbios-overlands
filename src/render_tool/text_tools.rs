//! Offline no-render text tools: the early-return CLI modes that print
//! and exit before any render app stands up - family-seed survey,
//! road-graph diagnostics, the seeded-room entity census, and the
//! session-log analyzers.

use crate::pds::avatar::livery;
use crate::pds::{Generator, GeneratorKind, Placement, RoomRecord};
use crate::seeded_defaults::hash::fnv1a_64;
use crate::seeded_defaults::{
    ArmouredVariant, AvatarPalette, BoatType, BuggyVariant, ChassisFamily, CraftType, RoadsterBody,
    RoadsterTop, RoadsterWheels, RunaboutVariant, ScowLoad, SkiffType, SloopHull, SloopRig,
    TugVariant, WagonBody,
};

use super::Args;

/// Print the first `count` u64 seeds whose
/// [`ChassisFamily`] matches `fam`
/// (case-insensitive `humanoid` | `boat` | `airship` | `skiff`). A survey aid
/// for the avatar overhaul - seeds map 25 % to each family, so scanning a few
/// thousand always finds enough.
pub(super) fn print_family_seeds(fam: &str, count: usize, craft: Option<&str>) {
    use crate::seeded_defaults::ChassisFamily;
    let want = match fam.to_lowercase().as_str() {
        "humanoid" => ChassisFamily::Humanoid,
        "boat" => ChassisFamily::Boat,
        "airship" => ChassisFamily::Airship,
        "skiff" => ChassisFamily::Skiff,
        other => panic!("unknown family {other:?} (humanoid|boat|airship|skiff)"),
    };
    let craft = craft.map(|c| resolve_craft(c, want));
    let seeds: Vec<u64> = (0u64..1_000_000)
        .filter(|&s| ChassisFamily::for_seed(s) == want)
        .filter(|&s| {
            craft.is_none_or(|c| crate::seeded_defaults::CraftType::for_seed(s) == Some(c))
        })
        .take(count)
        .collect();
    match craft {
        Some(c) => println!("{want:?} / {} seeds: {seeds:?}", c.label()),
        None => println!("{want:?} seeds: {seeds:?}"),
    }
    if craft.is_some() && seeds.len() < count {
        println!(
            "  (only {} below seed 1_000_000 - a type no theme is at home in is rare by design)",
            seeds.len()
        );
    }
    // Humanoid seeds are rigged bodies since #1060 - there is no generator
    // tree to render and no stylization tier to exemplify, so say where the
    // instrument for them lives instead of printing a table about parts
    // that no longer exist.
    if want == ChassisFamily::Humanoid {
        println!(
            "  (humanoid seeds are rigged symbios-avatar bodies - render them \n\
              with the bevy_symbios_avatar viewer, not this tool)"
        );
    }
}

/// Resolve a `--craft` filter name to the craft type it selects, checked
/// against the family being surveyed so `--family-seeds boat --craft rover`
/// fails loudly instead of quietly printing nothing.
fn resolve_craft(name: &str, fam: ChassisFamily) -> CraftType {
    let name = name.to_lowercase();
    let boat = BoatType::ALL.iter().find(|t| t.slug() == name);
    let skiff = SkiffType::ALL.iter().find(|t| t.slug() == name);
    match (fam, boat, skiff) {
        (ChassisFamily::Boat, Some(&t), _) => CraftType::Boat(t),
        (ChassisFamily::Skiff, _, Some(&t)) => CraftType::Skiff(t),
        (_, None, None) => panic!(
            "unknown craft type {name:?} - boats are {:?}, skiffs are {:?}",
            BoatType::ALL.map(BoatType::slug),
            SkiffType::ALL.map(SkiffType::slug)
        ),
        _ => panic!("craft type {name:?} is not a {fam:?} - it belongs to the other family"),
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

/// The seeded craft type of an avatar `subject`, or `None` for a family
/// without one (airship, humanoid).
fn craft_for(subject: &str) -> Option<CraftType> {
    match subject.parse::<u64>() {
        Ok(seed) => CraftType::for_seed(seed),
        Err(_) => CraftType::for_did(subject),
    }
}

/// Print the resolved outfit for one avatar `subject` (a `u64` seed or a DID):
/// chassis, style, socio tiers, and each filled slot → part slug. A no-render
/// survey aid for the avatar overhaul - the built [`Generator`] carries only
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
    // The seeded craft type (#1362). A property of the SEED, so it answers for
    // every boat and skiff whether or not anything builds that type yet: the
    // slice that builds one opens by finding the seeds that picked it. A craft
    // whose type has no builder is DRAWN as its family's universal floor
    // (#1363 for boats, #1364 for skiffs), and this says which.
    if let Some(craft) = craft_for(subject) {
        let note = match craft {
            CraftType::Boat(t) if t.implemented() => "picked and built".to_string(),
            CraftType::Skiff(t) if t.implemented() => "picked and built".to_string(),
            CraftType::Boat(_) => format!(
                "picked; not built yet, drawn as the {}",
                BoatType::UNIVERSAL.label()
            ),
            CraftType::Skiff(_) => format!(
                "picked; not built yet, drawn as the {}",
                SkiffType::UNIVERSAL.label()
            ),
        };
        println!("  craft type: {} ({note})", craft.label());
        // And the heritage livery it is painted in (#1365) - a second DID-
        // seeded draw on its own stream, so two owners of one craft type still
        // rarely match. `--livery <index>` overrides it for a survey.
        let seed = match subject.parse::<u64>() {
            Ok(seed) => seed,
            Err(_) => fnv1a_64(subject),
        };
        println!(
            "  livery: {}",
            match craft {
                // A junk wears a scheme of her own, hull and sails (#1371).
                CraftType::Boat(BoatType::Junk) => livery::junk_livery(seed, None).name,
                CraftType::Boat(_) => livery::boat_livery(seed, None).name,
                // A wagon wears a scheme of its own (#1377), and the hearse
                // and the ox-cart a forced one.
                CraftType::Skiff(SkiffType::Wagon) => {
                    livery::wagon_livery(seed, WagonBody::for_seed(seed), None).name
                }
                // So does a dune buggy (#1374), and a raider hers.
                CraftType::Skiff(SkiffType::DuneBuggy) => {
                    livery::buggy_livery(seed, BuggyVariant::for_seed(seed), None).name
                }
                // And a cyclecar (#1376).
                CraftType::Skiff(SkiffType::Cyclecar) => livery::cyclecar_livery(seed, None).name,
                // And an armoured car (#1375). Her arm has to come BEFORE the
                // wildcard: forgetting it compiles and prints a heritage
                // roadster scheme she never wears.
                CraftType::Skiff(SkiffType::ArmouredCar) => {
                    livery::armoured_livery(seed, None).name
                }
                CraftType::Skiff(_) => livery::skiff_livery(seed, None).name,
            }
        );
        // And the sloop's own two picks (#1366), for every boat that is drawn
        // as one: a sloop, and a longship pick until #1369 builds her.
        if let CraftType::Boat(t) = craft
            && (t == BoatType::Sloop || !t.implemented())
        {
            println!(
                "  rig: {}, hull: {}",
                SloopRig::for_seed(seed).label(),
                SloopHull::for_seed(seed).label()
            );
        }
        // And the roadster's own three picks (#1367), for every skiff drawn
        // as one - every skiff but a wagon, a dune buggy or a cyclecar, until
        // the other types are built.
        if let CraftType::Skiff(t) = craft
            && (t == SkiffType::Roadster || !t.implemented())
        {
            println!(
                "  body: {}, top: {}, wheels: {}",
                RoadsterBody::for_seed(seed).label(),
                RoadsterTop::for_seed(seed).label(),
                RoadsterWheels::for_seed(seed).label()
            );
        }
        // And the runabout's variant (#1372), which its theme picks.
        if let CraftType::Boat(BoatType::Runabout) = craft {
            println!("  runabout: {}", RunaboutVariant::for_seed(seed).label());
        }
        // And the scow's load (#1373), which its theme picks - and with it
        // her stern gear.
        if let CraftType::Boat(BoatType::Scow) = craft {
            println!("  scow: {}", ScowLoad::for_seed(seed).label());
        }
        // And the tug's variant (#1370), which her theme picks.
        if let CraftType::Boat(BoatType::SteamTug) = craft {
            println!("  tug: {}", TugVariant::for_seed(seed).label());
        }
        // And the wagon's body (#1377), which its theme picks.
        if let CraftType::Skiff(SkiffType::Wagon) = craft {
            println!("  wagon body: {}", WagonBody::for_seed(seed).label());
        }
        // And the dune buggy's variant (#1374), which her theme picks.
        if let CraftType::Skiff(SkiffType::DuneBuggy) = craft {
            println!("  buggy: {}", BuggyVariant::for_seed(seed).label());
        }
        // And the armoured car's (#1375), which her theme picks too.
        if let CraftType::Skiff(SkiffType::ArmouredCar) = craft {
            println!("  armoured: {}", ArmouredVariant::for_seed(seed).label());
        }
    }
    let seed = match subject.parse::<u64>() {
        Ok(seed) => seed,
        Err(_) => fnv1a_64(subject),
    };
    // And its seeded palette's three accents, for every subject: the primary
    // (#1376) is what every identity slot is cleared from, which a dump only
    // carries once a craft has cleared it; the secondary tints a bottom's
    // antifoul and the tertiary lights the windows (#1371) - so a python
    // twin can compute each slot exactly. Three channels each to four
    // places, on one line in this form; the twins' checkers parse it.
    let palette = AvatarPalette::for_seed(seed);
    let three = |c: [f32; 3]| format!("{:.4}, {:.4}, {:.4}", c[0], c[1], c[2]);
    println!(
        "  palette: primary {}; secondary {}; tertiary {}",
        three(palette.primary_accent),
        three(palette.secondary_accent),
        three(palette.tertiary_accent)
    );
    // And its voice (#1383), for every family: which drive a boat speaks
    // with is the DRAWN craft's, and the detune bucket says whether two
    // craft idle at one pitch.
    println!(
        "  voice: {}",
        crate::pds::avatar::default_visuals::voice_label(seed)
    );
    for part in &outfit.parts {
        println!("  {:?} -> {}", part.slot, part.slug);
    }
}

/// Scan seeds and print the first `count` whose outfit fills any slot with the
/// part `slug`, each with its style + socio tiers - answers "which seed rolls
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
        println!("  (none found below seed 2_000_000 - is the slug spelled right?)");
    }
}

/// Reproduce a room's heightmap + road config and print the road-graph
/// diagnostics (see [`crate::urban::road_graph_diagnostics`]) to stdout. The
/// room is the seeded default for a `u64` seed or a DID string - the same
/// derivation `--room` uses - so the heightmap and road network match what the
/// game renders for that room.
pub(super) fn dump_road_graph(room: &str) {
    let record = match room.parse::<u64>() {
        Ok(seed) => RoomRecord::default_for_seed(seed, &format!("did:render:{seed}")),
        Err(_) => RoomRecord::default_for_did(room),
    };
    let Some(config) = crate::pds::find_road_config(&record).cloned() else {
        println!(
            "room {room:?}: no road config - this room grows no roads (try a road-growing theme seed)"
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
/// - the same rule set, replayed over a captured log.
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
/// their before/after diff (see [`crate::diagnostics::analyze::diff_report`]) -
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
/// buckets` - since #812 props are baked into those buckets rather than spawned
/// as one entity each). Shape-grammar nodes also expand at spawn but are left
/// at 1 and flagged via [`tree_has_shape`] - the census evidence shows
/// L-systems dominate seeded-room counts by orders of magnitude.
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
            prop_mappings,
            prop_scale,
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
            prop_mappings,
            *prop_scale,
            generator_ref,
        )
        .map_or(1, |buckets| 1 + buckets.len() as u64),
        _ => 1,
    };
    own + g
        .children
        .iter()
        .map(|c| tree_entities(c, generator_ref))
        .sum::<u64>()
}

/// `true` if any node in the tree is a CGA shape grammar (spawn-time
/// expansion the census does not model - flagged as an underestimate).
fn tree_has_shape(g: &Generator) -> bool {
    matches!(g.kind, GeneratorKind::Shape { .. }) || g.children.iter().any(tree_has_shape)
}

/// Analytic entity census over seeded rooms (#810): for each seed, sum every
/// placement's instance count × generator-tree node count - the record-level
/// estimate of what `compile_room_record` will spawn - and print the total
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

/// Placement census over seeded rooms (#912): replay the real scatter
/// sampling loop against a heightmap rebuilt from each record, and print what
/// it actually places.
///
/// The complement to [`room_census`]. That one answers "how many entities
/// will this room cost" from the record alone; this one runs the filters and
/// the naturalness warps, so it can answer the questions the analytic census
/// cannot: how much of the requested count survives the biome filter and the
/// slope cutoff, and whether the survivors read as a grown stand or a
/// sprinkle.
///
/// The clustering column is a Clark–Evans nearest-neighbour index - see
/// [`crate::world_builder::compile::scatter_census`]'s module docs for how
/// to read it, and in particular why each row prints the tuned scatter *and*
/// the same scatter with its naturalness zeroed rather than an absolute
/// number.
pub(super) fn scatter_census(seeds: u64) {
    println!(
        "Scatter placement census over seeds 0..{seeds} \
         (`placed` runs the real sampler; `R` is the Clark–Evans index -\n\
         below 1 is clustered, and the `uniform` column is the same scatter \
         with naturalness off).\n\
         `ground` is the steepness actually planted vs. what the scatter was \
         offered - a working cutoff shows up here, not in the placed count, \
         because the sampler simply retries past a rejection."
    );
    // Running totals for the closing summary - the per-seed detail is for
    // spotting outliers, these are the numbers that characterise the change.
    let (mut total_req, mut total_placed) = (0u64, 0u64);
    let (mut r_sum, mut r_uniform_sum, mut r_n) = (0.0f64, 0.0f64, 0u64);
    // Slope evidence. The max is a poor aggregate - it is dominated by
    // whichever scatter set the loosest cutoff (lichen tolerates 62°) - so
    // the headline is the mean p95 across slope-limited scatters, placed vs.
    // offered. That is the number that says vegetation moved onto gentler
    // ground.
    let (mut p95_placed, mut p95_offered, mut limited_n) = (0.0f64, 0.0f64, 0u64);
    let mut cutoff_breaches = 0u64;

    for seed in 0..seeds {
        let record = RoomRecord::default_for_seed(seed, "did:plc:census");
        let census = crate::world_builder::compile::scatter_census(&record);
        // Name the biome: which rows to expect (aquatic species only roll in
        // Wetland/Coastal pools, lifelessness is Glacial-only, …) is
        // unreadable from a bare seed number.
        let scene = crate::seeded_defaults::SceneCharacter::for_seed(seed);
        println!("\nseed {seed} ({:?}):", scene.biome);
        if census.rows.is_empty() {
            println!("  (no scatters - a lifeless room)");
            continue;
        }
        for row in &census.rows {
            total_req += u64::from(row.requested);
            total_placed += u64::from(row.placed);
            // A stand of one has no nearest neighbour, so it has no index.
            if row.clark_evans.is_finite() && row.clark_evans_uniform.is_finite() {
                r_sum += f64::from(row.clark_evans);
                r_uniform_sum += f64::from(row.clark_evans_uniform);
                r_n += 1;
            }
            // Only scatters that actually set a cutoff belong in this
            // comparison - the boulder field deliberately has none, and
            // folding its 78° faces in would hide the effect entirely.
            if row.max_slope_deg.is_some() {
                p95_placed += f64::from(row.slope_deg.1);
                p95_offered += f64::from(row.slope_deg_offered.1);
                limited_n += 1;
            }
            // The invariant the cutoff exists to enforce. A degree of
            // tolerance covers the bilinear normal lookup landing a hair
            // over the threshold between two samples of the same cell.
            if row
                .max_slope_deg
                .is_some_and(|cutoff| row.slope_deg.2 > cutoff + 1.0)
            {
                cutoff_breaches += 1;
            }

            let yield_pct = if row.requested == 0 {
                0.0
            } else {
                100.0 * f64::from(row.placed) / f64::from(row.requested)
            };
            let cutoff = row
                .max_slope_deg
                .map_or_else(|| "  none".to_string(), |d| format!("{d:>4.0}°"));
            // Microbiome bands and what they cost (#913). `+0` next to a
            // set band means the band never rejected anything the other
            // filters would have kept - worth a second look, since a band
            // that costs nothing is usually mis-set.
            let bands = match (row.above_water_band, row.altitude_band) {
                (None, None) => "        -".to_string(),
                (w, a) => {
                    let fmt = |b: Option<[f32; 2]>| {
                        b.map_or_else(
                            || "-".to_string(),
                            |[lo, hi]| {
                                format!(
                                    "{lo:.0}..{}",
                                    if hi > 9_000.0 {
                                        "∞".into()
                                    } else {
                                        format!("{hi:.0}")
                                    }
                                )
                            },
                        )
                    };
                    format!("w{} a{}", fmt(w), fmt(a))
                }
            };
            // Distribution, not count - see the census docs for why.
            let band_effect = if row.above_water_band.is_some() || row.altitude_band.is_some() {
                format!(
                    "  above-water p50/max {:>4.0}/{:<4.0} (unbanded {:.0}/{:.0})",
                    row.above_water.0,
                    row.above_water.2,
                    row.above_water_unbanded.0,
                    row.above_water_unbanded.2,
                )
            } else {
                String::new()
            };
            println!(
                "  {:<20} {:>4}/{:<4} ({:>5.1}%)  R {:>5.2} vs {:>5.2}  scale {:.2}–{:.2}  \
                 cutoff {cutoff}  ground p95/max {:>3.0}°/{:>3.0}° (offered {:>3.0}°/{:>3.0}°)  \
                 band {bands}{band_effect}",
                row.generator_ref,
                row.placed,
                row.requested,
                yield_pct,
                row.clark_evans,
                row.clark_evans_uniform,
                row.scale_range.0,
                row.scale_range.1,
                row.slope_deg.1,
                row.slope_deg.2,
                row.slope_deg_offered.1,
                row.slope_deg_offered.2,
            );
        }
    }

    println!("\n--- totals over {seeds} seeds ---");
    let yield_pct = if total_req == 0 {
        0.0
    } else {
        100.0 * total_placed as f64 / total_req as f64
    };
    println!("  requested {total_req}, placed {total_placed} ({yield_pct:.1}% yield)");
    if limited_n > 0 {
        println!(
            "  slope-limited scatters ({limited_n}): mean p95 ground planted \
             {:.1}°, vs {:.1}° offered",
            p95_placed / limited_n as f64,
            p95_offered / limited_n as f64,
        );
    }
    println!("  scatters exceeding their own cutoff: {cutoff_breaches} (expected 0)");
    if r_n > 0 {
        println!(
            "  mean Clark–Evans index {:.2} (naturalness off: {:.2}) over {r_n} stands",
            r_sum / r_n as f64,
            r_uniform_sum / r_n as f64,
        );
    }
}

/// Plan-view plot of one seeded room's scatters (#912): a PNG grid, one row
/// per scatter, with the tuned arrangement on the left and the same scatter
/// with its naturalness zeroed on the right.
///
/// The four-angle contact sheet cannot answer this question. A tree stand is
/// 300–460 m across; framed to fit, every instance is a speck, and clustering
/// is a property of the *layout* rather than of any instance. Arrangement is
/// a plan-view question, so this draws the plan view.
///
/// Row order is printed to stdout - the plot carries no text, which keeps it
/// free of a font dependency.
pub(super) fn scatter_plot(room: &str, out: &std::path::Path) {
    /// Side of one panel, px.
    const PANEL: u32 = 300;
    /// Gap between panels and around the grid, px.
    const PAD: u32 = 8;

    let record = match room.parse::<u64>() {
        Ok(seed) => RoomRecord::default_for_seed(seed, &format!("did:render:{seed}")),
        Err(_) => RoomRecord::default_for_did(room),
    };
    let census = crate::world_builder::compile::scatter_census(&record);
    if census.rows.is_empty() {
        println!("room {room:?}: no scatters to plot");
        return;
    }

    let rows = census.rows.len() as u32;
    let width = PAD + 2 * (PANEL + PAD);
    let height = PAD + rows * (PANEL + PAD);
    // RGBA8, matching the contact-sheet writer's buffer format.
    let mut buf = vec![0u8; (width * height * 4) as usize];
    let mut put = |x: i64, y: i64, rgba: [u8; 4]| {
        if x < 0 || y < 0 || x >= i64::from(width) || y >= i64::from(height) {
            return;
        }
        let i = ((y as u32 * width + x as u32) * 4) as usize;
        buf[i..i + 4].copy_from_slice(&rgba);
    };

    const BG: [u8; 4] = [18, 20, 24, 255];
    const PANEL_BG: [u8; 4] = [28, 32, 38, 255];
    const RING: [u8; 4] = [70, 78, 90, 255];
    const TUNED: [u8; 4] = [120, 220, 140, 255];
    const PLAIN: [u8; 4] = [150, 160, 180, 255];

    for y in 0..height as i64 {
        for x in 0..width as i64 {
            put(x, y, BG);
        }
    }

    println!("room {room:?}: plan view, left = tuned, right = naturalness off");
    for (r, row) in census.rows.iter().enumerate() {
        let top = PAD + r as u32 * (PANEL + PAD);
        println!(
            "  row {}: {} ({} placed, R {:.2} vs {:.2})",
            r + 1,
            row.generator_ref,
            row.points.len(),
            row.clark_evans,
            row.clark_evans_uniform,
        );
        for (col, (points, dot)) in [(&row.points, TUNED), (&row.points_uniform, PLAIN)]
            .into_iter()
            .enumerate()
        {
            let left = PAD + col as u32 * (PANEL + PAD);
            for py in 0..PANEL as i64 {
                for px in 0..PANEL as i64 {
                    put(left as i64 + px, top as i64 + py, PANEL_BG);
                }
            }
            // Bounds ring, so the edge-falloff thinning has a reference.
            let half = PANEL as f64 / 2.0;
            for step in 0..1440 {
                let a = f64::from(step) * std::f64::consts::TAU / 1440.0;
                put(
                    left as i64 + (half + (half - 2.0) * a.cos()) as i64,
                    top as i64 + (half + (half - 2.0) * a.sin()) as i64,
                    RING,
                );
            }
            // World → panel: the bounds circle fills the panel.
            let scale = (half - 2.0) as f32 / row.bounds_radius.max(0.001);
            for &(wx, wz) in points {
                let px = half as f32 + (wx - row.bounds_center.0) * scale;
                let py = half as f32 + (wz - row.bounds_center.1) * scale;
                // 2×2 dots: a single pixel vanishes at 300 px for a 400 m
                // stand, and dot overlap is itself the density signal.
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    put(
                        left as i64 + px as i64 + dx,
                        top as i64 + py as i64 + dy,
                        dot,
                    );
                }
            }
        }
    }

    match image::save_buffer(out, &buf, width, height, image::ExtendedColorType::Rgba8) {
        Ok(()) => println!("wrote {}", out.display()),
        Err(e) => eprintln!("cannot write {}: {e}", out.display()),
    }
}

/// Print the veil-fit report for every gateway entry (#1006), or just the
/// one whose slug is given. For each gateway: the veil box, then per face
/// whether it is buried in the frame and how far the nearest frame surface
/// ahead of it sits - the numbers the per-theme fit is derived from.
pub(super) fn print_gateway_fit(filter: &str) {
    use crate::catalogue::items::gateway_fit::{Face, fit_faults, measure, probe, recommend};
    use crate::catalogue::{ENTRIES, StructureRole};

    let filter = filter.trim();
    let want_all = filter.is_empty() || filter == "all";
    let mut checked = 0usize;
    let mut clean = 0usize;

    for entry in ENTRIES {
        if entry.role() != StructureRole::Gateway {
            continue;
        }
        if !want_all && entry.slug() != filter {
            continue;
        }
        let built = entry.build("did:plc:gatewayfit");
        let Some(geo) = measure(&built) else {
            println!("{}: no Gateway zone found", entry.slug());
            continue;
        };
        checked += 1;

        let v = geo.veil;
        println!("\n=== {} ({})", entry.slug(), entry.name());
        println!(
            "  veil  size [{:.3}, {:.3}, {:.3}]  x {:.3}..{:.3}  y {:.3}..{:.3}  z {:.3}..{:.3}",
            v.size().x,
            v.size().y,
            v.size().z,
            v.min.x,
            v.max.x,
            v.min.y,
            v.max.y,
            v.min.z,
            v.max.z
        );
        for p in probe(&geo) {
            let cover = match &p.covered_by {
                Some(s) => format!("buried in {} {:?}", s.kind_tag, s.path),
                None => "OPEN AIR".to_string(),
            };
            let ahead = match &p.nearest_ahead {
                Some((d, s)) => {
                    format!("  nearest ahead {:+.3} m ({} {:?})", d, s.kind_tag, s.path)
                }
                None => String::new(),
            };
            println!(
                "  {:<10} at {:+.3}  {cover}{ahead}",
                p.face.label(),
                p.veil_at
            );
        }
        let want = recommend(&geo);
        println!(
            "  FIT -> size [{:.3}, {:.3}, {:.3}]  centre [{:.3}, {:.3}, {:.3}]  \
             (dy {:+.3}, dz-centre {:+.3})",
            want.size().x,
            want.size().y,
            want.size().z,
            want.center().x,
            want.center().y,
            want.center().z,
            want.center().y - v.center().y,
            want.center().z - v.center().z,
        );
        let faults = fit_faults(&geo);
        if faults.is_empty() {
            clean += 1;
            println!("  FIT OK");
        } else {
            for f in &faults {
                println!("  FAULT [{}] {}", f.face.label(), f.detail);
            }
        }
        // Depth reference: how deep the pieces burying the jambs run.
        for face in [Face::Left, Face::Right] {
            let p = face.center_of(&v);
            if let Some(s) = geo.solids.iter().find(|s| s.bounds.contains(p, 1.0e-3)) {
                println!(
                    "  {} piece z {:.3}..{:.3} (veil z {:.3}..{:.3})",
                    face.label(),
                    s.bounds.min.z,
                    s.bounds.max.z,
                    v.min.z,
                    v.max.z
                );
            }
        }
    }
    println!("\n{clean}/{checked} gateways fit");
}

/// Print the foundation-depth audit (#1009): every settlement-placeable
/// entry against the plinth depth its footprint demands, worst shortfall
/// first. `pass` limits the listing to entries that already satisfy it.
pub(super) fn print_foundation_audit(filter: &str) {
    use crate::catalogue::items::foundation::{audit, required_depth};

    let filter = filter.trim();
    let mut rows = audit();
    rows.sort_by(|a, b| {
        b.shortfall()
            .total_cmp(&a.shortfall())
            .then(a.slug.cmp(b.slug))
    });

    let show_pass = filter == "all" || filter == "pass";
    let failing = rows.iter().filter(|r| !r.passes()).count();
    println!(
        "{} of {} settlement structures are too shallow for their footprint\n",
        failing,
        rows.len()
    );
    println!(
        "{:<34} {:<10} {:>6} {:>8} {:>8} {:>8}",
        "slug", "role", "clear", "needs", "has", "short"
    );
    for r in &rows {
        if !show_pass && r.passes() {
            continue;
        }
        println!(
            "{:<34} {:<10} {:>6.1} {:>8.2} {:>8.2} {:>8.2}",
            r.slug,
            format!("{:?}", r.role),
            r.clearance,
            r.required,
            r.actual,
            r.shortfall()
        );
    }
    // The rule itself, so a reader can sanity-check a row by hand.
    println!(
        "\nrequired depth = clamp({:.2} x clearance - 0.35 + 0.4, 1.0, 6.0); \
         e.g. clearance 8 -> {:.2} m",
        crate::catalogue::items::foundation::FOOTPRINT_DROP_RATIO,
        required_depth(8.0)
    );
}

/// Measure the drop real seeded settlements actually span (#1009): for
/// each of `seeds` rooms, rebuild the terrain and, for every seeded
/// structure placement, report `max - min` terrain height across its
/// footprint disc. That drop is exactly what a plinth has to cover, so
/// this is what the depth rule should be sized from rather than a
/// worst-case slope assumption.
pub(super) fn print_settlement_drop(seeds: u64) {
    use crate::pds::{Placement, RoomRecord};

    let mut all: Vec<(f32, f32)> = Vec::new(); // (clearance, drop)
    let mut covered = 0usize;
    let mut short: Vec<(f32, f32)> = Vec::new(); // (drop, daylight gap)
    for seed in 0..seeds.max(1) {
        let did = format!("did:plc:drop{seed}");
        let record = RoomRecord::default_for_seed(seed, &did);
        let hm = crate::terrain::rebuild_heightmap_for_record(&record);
        let extent = (hm.width() - 1) as f32 * hm.scale();
        let half = extent * 0.5;
        let sample = |x: f32, z: f32| {
            hm.get_height_at((x + half).clamp(0.0, extent), (z + half).clamp(0.0, extent))
        };

        let mut room = Vec::new();
        for p in &record.placements {
            let Placement::Absolute {
                transform,
                avoid_water,
                avoid_water_clearance,
                snap_to_terrain,
                ..
            } = p
            else {
                continue;
            };
            // The seeded-structure marker, same gate the compiler uses.
            if !*avoid_water || !*snap_to_terrain {
                continue;
            }
            let r = avoid_water_clearance.0 * transform.scale.0[0].max(0.0);
            if r <= 0.0 {
                continue;
            }
            let (x, z) = (transform.translation.0[0], transform.translation.0[2]);
            let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
            for i in 0..48 {
                for j in 0..8 {
                    let a = i as f32 * std::f32::consts::TAU / 48.0;
                    let rr = r * (j as f32 / 7.0);
                    let h = sample(x + a.sin() * rr, z + a.cos() * rr);
                    lo = lo.min(h);
                    hi = hi.max(h);
                }
            }
            // End-to-end check of #1008 + #1009 together. The building's
            // origin lands at the footprint's high point less the 0.35 m
            // placement sink, and its plinth reaches `depth` below that, so
            // daylight shows exactly when the plinth stops above the lowest
            // ground under the footprint.
            let scale = transform.scale.0[0].max(1e-3);
            let depth_world =
                crate::catalogue::items::foundation::required_depth(r / scale) * scale;
            let plinth_bottom = hi - 0.35 - depth_world;
            if plinth_bottom <= lo {
                covered += 1;
            } else {
                short.push((hi - lo, plinth_bottom - lo));
            }
            room.push((r, hi - lo));
        }
        println!("seed {seed}: {} seeded structures", room.len());
        all.extend(room);
    }

    if all.is_empty() {
        println!("no seeded structures found");
        return;
    }
    println!(
        "\nPLINTH COVERAGE: {covered}/{} footprints show no daylight ({:.1} %)",
        all.len(),
        100.0 * covered as f32 / all.len() as f32
    );
    if !short.is_empty() {
        short.sort_by(|a, b| b.1.total_cmp(&a.1));
        println!("  worst uncovered (drop -> daylight gap):");
        for (d, g) in short.iter().take(8) {
            println!("    drop {d:6.2} m -> {g:5.2} m of daylight");
        }
    }

    let mut drops: Vec<f32> = all.iter().map(|(_, d)| *d).collect();
    drops.sort_by(f32::total_cmp);
    let pct = |p: f32| drops[((drops.len() - 1) as f32 * p) as usize];
    println!(
        "\n{} footprints | drop median {:.2} p75 {:.2} p90 {:.2} p99 {:.2} max {:.2} m",
        drops.len(),
        pct(0.5),
        pct(0.75),
        pct(0.9),
        pct(0.99),
        drops[drops.len() - 1]
    );

    // Drop against footprint radius - the rule is depth = k x clearance,
    // so print the ratio the data actually supports.
    let mut ratios: Vec<f32> = all
        .iter()
        .filter(|(r, _)| *r > 0.0)
        .map(|(r, d)| d / r)
        .collect();
    ratios.sort_by(f32::total_cmp);
    let rp = |p: f32| ratios[((ratios.len() - 1) as f32 * p) as usize];
    println!(
        "drop/radius   median {:.3} p75 {:.3} p90 {:.3} p99 {:.3} max {:.3}",
        rp(0.5),
        rp(0.75),
        rp(0.9),
        rp(0.99),
        ratios[ratios.len() - 1]
    );
    for (lo, hi) in [(0.0, 6.0), (6.0, 9.0), (9.0, 14.0), (14.0, 100.0)] {
        let mut b: Vec<f32> = all
            .iter()
            .filter(|(r, _)| *r >= lo && *r < hi)
            .map(|(_, d)| *d)
            .collect();
        if b.is_empty() {
            continue;
        }
        b.sort_by(f32::total_cmp);
        println!(
            "  clearance {lo:>5.1}..{hi:<5.1} n={:<4} drop median {:.2} p90 {:.2} max {:.2}",
            b.len(),
            b[b.len() / 2],
            b[((b.len() - 1) as f32 * 0.9) as usize],
            b[b.len() - 1]
        );
    }
}

/// `--describe`: what a seeded room *is*, from its record and its scene
/// roll, before a render says so the slow way. One line per seed for a
/// range, a labelled block for a single seed or DID.
pub(super) fn describe_rooms(what: &str) {
    if let Some((a, b)) = what.split_once("..") {
        let a: u64 = a
            .trim()
            .parse()
            .unwrap_or_else(|e| panic!("--describe {what:?}: {e}"));
        let b: u64 = b
            .trim()
            .parse()
            .unwrap_or_else(|e| panic!("--describe {what:?}: {e}"));
        println!(
            "{:>5}  {:<10} {:<10} {:<14} {:>5} {:>5}  {:>6} {:>5} {:>5}  {:>5} {:>5}",
            "seed",
            "landform",
            "biome",
            "theme",
            "prosp",
            "escal",
            "fog_m",
            "sun_y",
            "cloud",
            "abs",
            "scat"
        );
        for seed in a..b {
            let d = RoomDescription::for_seed(seed);
            println!(
                "{:>5}  {:<10} {:<10} {:<14} {:>5.2} {:>5.2}  {:>6.0} {:>5.2} {:>5.2}  {:>5} {:>5}",
                seed,
                d.landform,
                d.biome,
                d.theme,
                d.prosperity,
                d.escalation,
                d.fog_visibility,
                d.sun_height,
                d.cloud_cover,
                d.absolute,
                d.scattered,
            );
        }
        return;
    }
    let d = match what.parse::<u64>() {
        Ok(seed) => RoomDescription::for_seed(seed),
        Err(_) => RoomDescription::for_did(what),
    };
    println!("{what}:");
    println!("  scene      {} / {} / {}", d.landform, d.biome, d.theme);
    println!(
        "  dials      prosperity {:.2}  escalation {:.2}  hue {:.0}°  temperature {:+.2}  daylight bias {:+.2}",
        d.prosperity, d.escalation, d.hue_deg, d.temperature, d.time_of_day_bias
    );
    println!(
        "  air        fog visibility {:.0} m  sun height {:.2} (of unit)  cloud cover {:.2}  sky {}",
        d.fog_visibility, d.sun_height, d.cloud_cover, d.sky
    );
    println!("  water      level {}", d.water);
    println!("  landing    {}", d.landing);
    println!(
        "  placements {} absolute, {} scatter ({} instances), {} grid; {} generators",
        d.absolute, d.scatters, d.scattered, d.grids, d.generators
    );
    println!("  structures (x, z):");
    for st in &d.structures {
        println!("    {st}");
    }
}

/// The facts `--describe` prints, gathered once so the table and the block
/// cannot disagree.
struct RoomDescription {
    landform: &'static str,
    biome: &'static str,
    theme: &'static str,
    prosperity: f32,
    escalation: f32,
    hue_deg: f32,
    temperature: f32,
    time_of_day_bias: f32,
    fog_visibility: f32,
    /// The sun direction's unit-vector Y: 1 is noon, 0 the horizon.
    sun_height: f32,
    cloud_cover: f32,
    sky: String,
    water: String,
    landing: String,
    absolute: usize,
    scatters: usize,
    scattered: u32,
    grids: usize,
    generators: usize,
    /// Every `Absolute` placement's generator name and ground position
    /// `(x, z)`, in record order - where to aim a camera.
    structures: Vec<String>,
}

impl RoomDescription {
    fn for_seed(seed: u64) -> Self {
        let did = format!("did:render:{seed}");
        Self::new(
            crate::seeded_defaults::scene::SceneCharacter::for_seed(seed),
            &RoomRecord::default_for_seed(seed, &did),
        )
    }

    fn for_did(did: &str) -> Self {
        Self::new(
            crate::seeded_defaults::scene::SceneCharacter::for_did(did),
            &RoomRecord::default_for_did(did),
        )
    }

    fn new(scene: crate::seeded_defaults::scene::SceneCharacter, record: &RoomRecord) -> Self {
        let env = &record.environment;
        let sun = bevy::math::Vec3::from_array(env.sun_position.0).normalize_or_zero();
        let sky = env.sky_color.0;
        let (mut absolute, mut scatters, mut scattered, mut grids) = (0usize, 0usize, 0u32, 0usize);
        let mut structures = Vec::new();
        for p in &record.placements {
            match p {
                Placement::Absolute {
                    generator_ref,
                    transform,
                    ..
                } => {
                    absolute += 1;
                    let [x, _, z] = transform.translation.0;
                    structures.push(format!("{generator_ref} ({x:.0}, {z:.0})"));
                }
                Placement::Scatter { count, .. } => {
                    scatters += 1;
                    scattered += *count;
                }
                Placement::Grid { .. } => grids += 1,
                _ => {}
            }
        }
        let water = crate::world_builder::compile::room_water_level(record)
            .map_or_else(|| "none".to_string(), |y| format!("{y:.1} m"));
        let landing = record.default_landing.as_ref().map_or_else(
            || "none".to_string(),
            |l| {
                format!(
                    "({:.1}, {:.1}) facing {:.0}°",
                    l.pos.0[0], l.pos.0[1], l.yaw_deg.0
                )
            },
        );
        Self {
            landform: scene.landform.label(),
            biome: scene.biome.label(),
            theme: scene.theme.label(),
            prosperity: scene.prosperity,
            escalation: scene.escalation,
            hue_deg: scene.base_hue_deg,
            temperature: scene.temperature,
            time_of_day_bias: scene.time_of_day_bias,
            fog_visibility: env.fog_visibility.0,
            sun_height: sun.y,
            cloud_cover: env.cloud_cover.0,
            sky: format!("({:.2}, {:.2}, {:.2})", sky[0], sky[1], sky[2]),
            water,
            landing,
            absolute,
            scatters,
            scattered,
            grids,
            generators: record.generators.len(),
            structures,
        }
    }
}
