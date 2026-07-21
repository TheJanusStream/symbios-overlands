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
/// buckets` — since #812 props are baked into those buckets rather than spawned
/// as one entity each). Shape-grammar nodes also expand at spawn but are left
/// at 1 and flagged via [`tree_has_shape`] — the census evidence shows
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
/// The clustering column is a Clark–Evans nearest-neighbour index — see
/// [`crate::world_builder::compile::scatter_census`]'s module docs for how
/// to read it, and in particular why each row prints the tuned scatter *and*
/// the same scatter with its naturalness zeroed rather than an absolute
/// number.
pub(super) fn scatter_census(seeds: u64) {
    println!(
        "Scatter placement census over seeds 0..{seeds} \
         (`placed` runs the real sampler; `R` is the Clark–Evans index —\n\
         below 1 is clustered, and the `uniform` column is the same scatter \
         with naturalness off).\n\
         `ground` is the steepness actually planted vs. what the scatter was \
         offered — a working cutoff shows up here, not in the placed count, \
         because the sampler simply retries past a rejection."
    );
    // Running totals for the closing summary — the per-seed detail is for
    // spotting outliers, these are the numbers that characterise the change.
    let (mut total_req, mut total_placed) = (0u64, 0u64);
    let (mut r_sum, mut r_uniform_sum, mut r_n) = (0.0f64, 0.0f64, 0u64);
    // Slope evidence. The max is a poor aggregate — it is dominated by
    // whichever scatter set the loosest cutoff (lichen tolerates 62°) — so
    // the headline is the mean p95 across slope-limited scatters, placed vs.
    // offered. That is the number that says vegetation moved onto gentler
    // ground.
    let (mut p95_placed, mut p95_offered, mut limited_n) = (0.0f64, 0.0f64, 0u64);
    let mut cutoff_breaches = 0u64;

    for seed in 0..seeds {
        let record = RoomRecord::default_for_seed(seed, "did:plc:census");
        let census = crate::world_builder::compile::scatter_census(&record);
        println!("\nseed {seed}:");
        if census.rows.is_empty() {
            println!("  (no scatters — a lifeless room)");
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
            // comparison — the boulder field deliberately has none, and
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
            // filters would have kept — worth a second look, since a band
            // that costs nothing is usually mis-set.
            let bands = match (row.above_water_band, row.altitude_band) {
                (None, None) => "        —".to_string(),
                (w, a) => {
                    let fmt = |b: Option<[f32; 2]>| {
                        b.map_or_else(
                            || "—".to_string(),
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
            // Distribution, not count — see the census docs for why.
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
/// Row order is printed to stdout — the plot carries no text, which keeps it
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
