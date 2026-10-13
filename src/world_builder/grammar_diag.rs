//! Per-generator grammar compile status (#829).
//!
//! L-system and Shape grammars are the flagship creative feature, but
//! their parse/derivation errors used to go to `warn!` only - invisible
//! on native without a terminal and unreachable in the browser. The
//! spawn paths now record every compile outcome here, keyed by the
//! generator name the editors select with, and the grammar forges render
//! the entry under their code editors: a red error with the line number,
//! or a quiet "compiled" tick - so silence is distinguishable from
//! success.
//!
//! Scope: the ROOM compile records under the room generator's key; the
//! LOCAL avatar's visuals record under the avatar editor's fixed root
//! key; REMOTE peers' grammars are deliberately not recorded (a
//! neighbour's broken tree is not the local editor's business). Entries
//! self-heal - every recompile overwrites its key, and a room's arrival
//! compile rewrites all of them - a generator no placement places any
//! more loses its entries at the plan that finds it so, one a placement
//! was pointed away from has the entries of its nodes that draw nothing
//! with some seed rewritten at that plan (#1505), and logout resets the
//! resource.

use std::collections::HashMap;

use bevy::prelude::*;

/// Outcome of the most recent compile of one generator's grammar.
#[derive(Clone, Debug, PartialEq)]
pub enum GrammarStatus {
    /// Parsed, derived and meshed without complaint.
    Ok,
    /// Rejected - `message` is the same text the `warn!` log carries
    /// (line-numbered where the parser knows one), minus the generator
    /// name the UI already shows.
    Error { message: String },
}

/// Latest grammar compile status per generator key. Written by the
/// world-builder spawn paths via queued commands
/// ([`crate::world_builder::compile::SpawnCtx::record_grammar_status`]),
/// read by the L-system / Shape forges in the editors.
#[derive(Resource, Default)]
pub struct GrammarDiagnostics {
    pub by_generator: HashMap<String, GrammarStatus>,
    /// Why a variant of an instance set draws nothing, by `(placement,
    /// variant)` (#1644): its templates are grown as no record generator's
    /// node, so the forges cannot show it; the set's own section does.
    by_variant: HashMap<(usize, usize), String>,
}

impl GrammarDiagnostics {
    pub fn get(&self, generator: &str) -> Option<&GrammarStatus> {
        self.by_generator.get(generator)
    }

    /// File `error` - or Ok, where there is none - as the status of the
    /// node keyed `key`, in place of any before it.
    pub(crate) fn record(&mut self, key: String, error: Option<String>) {
        let status = match error {
            None => GrammarStatus::Ok,
            Some(message) => GrammarStatus::Error { message },
        };
        self.by_generator.insert(key, status);
    }

    /// File why variant `variant` of the instance set `placement` draws
    /// nothing - or that it draws, where `failure` is `None` (#1644).
    pub(crate) fn record_variant(
        &mut self,
        (placement, variant): (usize, usize),
        failure: Option<String>,
    ) {
        match failure {
            Some(failure) => {
                self.by_variant.insert((placement, variant), failure);
            }
            None => {
                self.by_variant.remove(&(placement, variant));
            }
        }
    }

    /// Why variant `variant` of the instance set `placement` draws
    /// nothing, where it does not draw.
    pub(crate) fn variant_failure(&self, placement: usize, variant: usize) -> Option<&str> {
        self.by_variant
            .get(&(placement, variant))
            .map(String::as_str)
    }

    /// Forget what is filed of the instance set `placement`'s variants: it
    /// is being built anew.
    pub(crate) fn forget_variants_of(&mut self, placement: usize) {
        self.by_variant.retain(|(p, _), _| *p != placement);
    }

    /// Forget every status filed under the generators `roots` - each
    /// root's own and every node's below it (#1505): nothing of them is
    /// drawn, so there is no compile of them to report, and a status of
    /// one that names a placement's seed would name what no longer draws
    /// it.
    pub(crate) fn forget_generators(&mut self, roots: &[String]) {
        self.by_generator.retain(|key, _| {
            !roots
                .iter()
                .any(|root| crate::world_builder::compile::is_node_of(key, root))
        });
    }
}
