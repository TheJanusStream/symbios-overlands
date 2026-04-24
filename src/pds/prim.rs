//! Shared enum for L-system prop meshes. The hierarchical primitive tree
//! that used to live here (`PrimShape` / `PrimNode`) has been retired in
//! favour of the unified `Generator` enum and [`super::generator::ConstructNode`]
//! — every primitive is now a first-class generator that can live at the
//! top level of a room or nested inside a Construct blueprint.

use serde::{Deserialize, Serialize};

/// Prop mesh shapes for `PropMeshType` slots. Mirrors
/// `lsystem-explorer::PropMeshType`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PropMeshType {
    #[default]
    Leaf,
    Twig,
    Sphere,
    Cone,
    Cylinder,
    Cube,
}
