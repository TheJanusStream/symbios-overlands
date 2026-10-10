//! Drafting a street building (#1598): `--street-fit` builds one at a
//! footprint's fit, `--street-rules` draws it with a rules file read at run
//! time in place of the `.cga` it was built with, and `--street-check`
//! holds it to the street conventions
//! ([`crate::catalogue::items::street`]) and exits - so a theme's grammar
//! is drafted, drawn and checked with no rebuild between drafts.

use crate::catalogue::items::street::check::{
    SEEDS, conventions, part_budget, parts, sample_fits, whole,
};
use crate::catalogue::items::street::{StreetFit, StreetSpec};
use crate::pds::Generator;

/// Parse `--street-fit`: `FRONTAGE,DEPTH,STOREYS`, then optionally `trade`.
pub(super) fn parse_street_fit(s: &str) -> Result<StreetFit, String> {
    let bad = || format!("--street-fit {s:?}: expected FRONTAGE,DEPTH,STOREYS[,trade]");
    let parts: Vec<&str> = s.split(',').map(str::trim).collect();
    let (dims, trade) = match parts.as_slice() {
        [frontage, depth, storeys] => ([*frontage, *depth, *storeys], false),
        [frontage, depth, storeys, "trade"] => ([*frontage, *depth, *storeys], true),
        _ => return Err(bad()),
    };
    Ok(StreetFit::new(
        dims[0].parse().map_err(|_| bad())?,
        dims[1].parse().map_err(|_| bad())?,
        dims[2].parse().map_err(|_| bad())?,
        trade,
    ))
}

/// The street building `slug` names, or why it is no such thing.
fn spec_of(slug: &str, flag: &str) -> &'static StreetSpec {
    let entry = crate::catalogue::by_slug(slug)
        .unwrap_or_else(|| panic!("unknown catalogue slug {slug:?}"));
    entry
        .street()
        .unwrap_or_else(|| panic!("{flag}: {slug:?} is not a street building"))
}

/// `--street-rules`' file, read.
fn read_rules(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("--street-rules {path:?}: {e}"))
}

/// A `--catalogue` subject's generator: the entry as it builds itself, or
/// a street building at `--street-fit`'s fit (its kind's own without one),
/// drawn with `--street-rules`' file (its own rules without one) at
/// `--street-seed`'s grammar seed (its own without one).
pub(super) fn catalogue_generator(
    slug: &str,
    street_fit: Option<&str>,
    street_rules: Option<&str>,
    street_seed: Option<u64>,
) -> Generator {
    if street_fit.is_none() && street_rules.is_none() && street_seed.is_none() {
        return crate::catalogue::by_slug(slug)
            .unwrap_or_else(|| panic!("unknown catalogue slug {slug:?}"))
            .build(super::TOOL_DID);
    }
    let flag = if street_rules.is_some() {
        "--street-rules"
    } else if street_fit.is_some() {
        "--street-fit"
    } else {
        "--street-seed"
    };
    let spec = spec_of(slug, flag);
    let fit = street_fit
        .map(|fit| parse_street_fit(fit).unwrap_or_else(|e| panic!("{e}")))
        .unwrap_or_else(|| spec.kind.default_fit());
    println!("{slug} at {:?}", fit.snapped(spec.kind));
    let built = match street_rules {
        Some(path) => spec.build_with(fit, &read_rules(path)),
        None => spec.build(fit),
    };
    match street_seed {
        Some(seed) => built.with_shape_seed(seed),
        None => built,
    }
}

/// `--street-check`: hold the street building `slug` - drawn with
/// `--street-rules`' file where one is given - to the street conventions at
/// its kind's sampled fits and `--street-fit`'s, each at its own grammar
/// seed and the check's others, and whether its faces fight or its parts
/// float at its smallest and largest fits. Prints each fault under the fit
/// and seed it is found at; `false` when there is one.
pub(super) fn street_check(
    slug: &str,
    street_fit: Option<&str>,
    street_rules: Option<&str>,
) -> bool {
    let spec = spec_of(slug, "--street-check");
    let rules = street_rules.map_or_else(|| spec.rules.to_string(), read_rules);
    let mut fits = sample_fits(spec.kind);
    if let Some(fit) = street_fit {
        // Named as it is drawn: on its kind's steps.
        let fit = parse_street_fit(fit).unwrap_or_else(|e| panic!("{e}"));
        fits.push(fit.snapped(spec.kind));
    }
    let seeds: Vec<Option<u64>> = [None].into_iter().chain(SEEDS.map(Some)).collect();
    let mut clean = true;
    let mut report = |what: &str, fit: StreetFit, seed: Option<u64>, problems: Vec<String>| {
        if problems.is_empty() {
            return;
        }
        clean = false;
        let seed = seed.map_or_else(|| "its own".to_string(), |s| s.to_string());
        println!("{what} at {fit:?}, grammar seed {seed}:");
        for problem in problems {
            println!("  {problem}");
        }
    };
    for &fit in &fits {
        for &seed in &seeds {
            report(
                "conventions",
                fit,
                seed,
                conventions(spec, &rules, fit, seed),
            );
        }
    }
    for fit in [fits[0], fits[1]] {
        for &seed in &seeds {
            report("faces and parts", fit, seed, whole(spec, &rules, fit, seed));
        }
    }
    if clean {
        println!(
            "{slug}: keeps the street conventions at {} fits and {} seeds",
            fits.len(),
            seeds.len()
        );
    }
    for fit in fits {
        if let Some(n) = parts(spec, &rules, fit) {
            println!(
                "  {n} parts at {fit:?} (a {} may draw {})",
                spec.kind.name(),
                part_budget(spec.kind)
            );
        }
    }
    clean
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_street_fit_is_three_numbers_and_optionally_trade() {
        assert_eq!(
            parse_street_fit("16,14,5").unwrap(),
            StreetFit::new(16, 14, 5, false)
        );
        assert_eq!(
            parse_street_fit(" 20, 11, 7, trade ").unwrap(),
            StreetFit::new(20, 11, 7, true)
        );
        for bad in [
            "16,14",
            "16,14,5,shop",
            "16,x,5",
            "16,14,5,trade,1",
            "-1,14,5",
        ] {
            let err = parse_street_fit(bad).unwrap_err();
            assert!(err.contains("--street-fit"), "{bad}: {err}");
        }
    }
}
