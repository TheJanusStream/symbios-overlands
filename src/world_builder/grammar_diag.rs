//! Per-generator grammar compile status (#829).
//!
//! L-system and Shape grammars are the flagship creative feature, but
//! their parse/derivation errors used to go to `warn!` only — invisible
//! on native without a terminal and unreachable in the browser. The
//! spawn paths now record every compile outcome here, keyed by the
//! generator name the editors select with, and the grammar forges render
//! the entry under their code editors: a red error with the line number,
//! or a quiet "compiled" tick — so silence is distinguishable from
//! success.
//!
//! Scope: the ROOM compile records under the room generator's key; the
//! LOCAL avatar's visuals record under the avatar editor's fixed root
//! key; REMOTE peers' grammars are deliberately not recorded (a
//! neighbour's broken tree is not the local editor's business). Entries
//! self-heal — every recompile overwrites its key, and a room's arrival
//! compile rewrites all of them — and logout resets the resource.

use std::collections::HashMap;

use bevy::prelude::*;

/// Outcome of the most recent compile of one generator's grammar.
#[derive(Clone, Debug, PartialEq)]
pub enum GrammarStatus {
    /// Parsed, derived and meshed without complaint.
    Ok,
    /// Rejected — `message` is the same text the `warn!` log carries
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
}

impl GrammarDiagnostics {
    pub fn get(&self, generator: &str) -> Option<&GrammarStatus> {
        self.by_generator.get(generator)
    }
}
