//! Shared test harness for shape-grammar catalogue entries.
//!
//! Walks every grammar line through the same `parse_statement` /
//! `add_statement` path the runtime uses, then derives against the entry's
//! default footprint. Catches rule typos and ensures every `Mat("...")`
//! slot referenced in the grammar has a matching entry in the materials map
//! — otherwise a hand-edit that drops a slot or breaks a rule only surfaces
//! as a runtime warning the first time someone drops the entry in a room.
//!
//! Every shape entry's grammar test should be a one-line call to
//! [`assert_grammar_parses_and_derives`]; entries used to carry inline
//! copies of this walk, which drifted from the runtime path.

use std::collections::HashSet;

use symbios_shape::grammar::parse_statement;
use symbios_shape::{Interpreter, Quat as SQuat, Scope, Vec3 as SVec3};

use crate::pds::GeneratorKind;

pub(super) fn assert_grammar_parses_and_derives(kind: GeneratorKind, entry_name: &str) {
    let GeneratorKind::Shape {
        grammar_source,
        root_rule,
        footprint,
        seed,
        materials,
        round_meshes,
    } = kind
    else {
        panic!("{entry_name}: build_kind must return GeneratorKind::Shape");
    };

    let mut interp = Interpreter::new();
    interp.seed = seed;
    let mut referenced_mats: HashSet<String> = HashSet::new();
    let mut emitted_meshes: HashSet<String> = HashSet::new();

    for (i, raw) in grammar_source.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let statement = parse_statement(line).unwrap_or_else(|e| {
            panic!("{entry_name} grammar line {} failed to parse: {}", i + 1, e)
        });
        for mat in line
            .split("Mat(\"")
            .skip(1)
            .filter_map(|chunk| chunk.split('"').next())
        {
            referenced_mats.insert(mat.to_string());
        }
        for id in line
            .split("I(\"")
            .skip(1)
            .filter_map(|chunk| chunk.split('"').next())
        {
            emitted_meshes.insert(id.to_string());
        }
        interp
            .add_statement(statement)
            .unwrap_or_else(|e| panic!("{entry_name} grammar line {} rejected: {}", i + 1, e));
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
    // A `round_meshes` entry that matches no emitted terminal is a silent
    // no-op — exactly the typo that would leave a colonnade square with no
    // error anywhere.
    for id in &round_meshes {
        assert!(
            emitted_meshes.contains(id),
            "{entry_name} marks `{id}` as a turned terminal but the grammar never emits I(\"{id}\")"
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

/// Every `GeneratorKind::Shape` node in the catalogue must sit with its own
/// origin at the entry's grade (world `y == 0`).
///
/// A shape grammar always builds **upward** from its own `y = 0`, so the
/// node's world Y is the building's floor level. [`util::footing`] returns
/// a root whose transform is already pushed *down* by half the plinth
/// (the block is buried, its top left at `FOUNDATION_REVEAL`), so pushing
/// the grammar on with `root.children.push(..)` makes it inherit that
/// offset and sink the whole building — leaving the plinth standing proud
/// around its base. [`util::attach`] rebases out of the root's frame and
/// is the correct way to hang it.
#[cfg(test)]
pub(super) fn assert_shape_nodes_stand_at_grade(built: &crate::pds::Generator, entry_name: &str) {
    fn walk(node: &crate::pds::Generator, world_y: f32, entry_name: &str, found: &mut usize) {
        let y = world_y + node.transform.translation.0[1];
        if matches!(node.kind, GeneratorKind::Shape { .. }) {
            *found += 1;
            assert!(
                y.abs() < 1e-3,
                "{entry_name}: shape grammar sits at world y = {y:.3}, not grade — \
                 the building is sunk into (or floating above) its foundation. \
                 Hang it with util::attach, not root.children.push"
            );
        }
        for child in &node.children {
            walk(child, y, entry_name, found);
        }
    }
    let mut found = 0;
    walk(built, 0.0, entry_name, &mut found);
    assert!(found > 0, "{entry_name}: no Shape node found to check");
}
