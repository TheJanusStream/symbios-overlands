//! Shared enum for L-system prop meshes. The hierarchical primitive tree
//! that used to live here (`PrimShape` / `PrimNode`) has been retired in
//! favour of the unified `Generator` enum and [`super::generator::ConstructNode`]
//! — every primitive is now a first-class generator that can live at the
//! top level of a room or nested inside a Construct blueprint.

use serde::{Deserialize, Serialize};

/// Prop mesh shapes attached to L-system skeleton nodes. The world
/// compiler's L-system spawner maps a generator's
/// `prop_mappings: HashMap<u16, PropMeshType>` over the
/// [`symbios_turtle_3d::SkeletonProp`] list emitted by the turtle
/// interpreter to decide which billboard or instanced mesh each prop slot
/// renders.
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
