//! Shared test harness for shape-grammar catalogue entries.
//!
//! Walks every grammar line through the same `parse_rule` /
//! `add_weighted_rules` path the runtime uses, then derives against
//! the entry's default footprint. Catches rule typos and ensures every
//! `Mat("...")` slot referenced in the grammar has a matching entry in
//! the materials map — otherwise a hand-edit that drops a slot or
//! breaks a rule only surfaces as a runtime warning the first time
//! someone drops the entry in a room.

use std::collections::HashSet;

use symbios_shape::grammar::parse_rule;
use symbios_shape::{Interpreter, Quat as SQuat, Scope, Vec3 as SVec3};

use crate::pds::GeneratorKind;

pub(super) fn assert_grammar_parses_and_derives(kind: GeneratorKind, entry_name: &str) {
    let GeneratorKind::Shape {
        grammar_source,
        root_rule,
        footprint,
        seed,
        materials,
    } = kind
    else {
        panic!("{entry_name}: build_kind must return GeneratorKind::Shape");
    };

    let mut interp = Interpreter::new();
    interp.seed = seed;
    let mut referenced_mats: HashSet<String> = HashSet::new();

    for (i, raw) in grammar_source.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let rule = parse_rule(line)
            .unwrap_or_else(|e| panic!("{entry_name} rule line {} failed to parse: {}", i + 1, e));
        for mat in line
            .split("Mat(\"")
            .skip(1)
            .filter_map(|chunk| chunk.split('"').next())
        {
            referenced_mats.insert(mat.to_string());
        }
        interp
            .add_weighted_rules(&rule.name, rule.variants)
            .unwrap_or_else(|e| panic!("{entry_name} rule {} rejected: {}", rule.name, e));
    }

    assert!(
        interp.has_rule(&root_rule),
        "root rule `{root_rule}` missing from {entry_name} grammar"
    );
    for name in &referenced_mats {
        assert!(
            materials.contains_key(name),
            "{entry_name} grammar references Mat(\"{name}\") but no material slot is defined"
        );
    }

    let scope = Scope::new(
        SVec3::ZERO,
        SQuat::IDENTITY,
        SVec3::new(
            footprint.0[0] as f64,
            footprint.0[1] as f64,
            footprint.0[2] as f64,
        ),
    );
    let model = interp
        .derive(scope, &root_rule)
        .unwrap_or_else(|e| panic!("{entry_name} grammar must derive: {e:?}"));
    assert!(
        !model.terminals.is_empty(),
        "{entry_name} derivation produced zero terminals — footprint is starving the splits"
    );
}
