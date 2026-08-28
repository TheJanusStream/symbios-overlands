//! Shared enum for L-system prop meshes. The hierarchical primitive tree
//! that used to live here (`PrimShape` / `PrimNode`) has been retired in
//! favour of the unified [`super::generator::Generator`] wrapper — every
//! primitive is a first-class generator kind that can live at the top level
//! of a room or as a child of any other generator.

use serde::{Deserialize, Serialize};

/// Prop mesh shapes attached to L-system skeleton nodes. The world
/// compiler's L-system spawner maps a generator's
/// `prop_mappings: HashMap<u16, PropMeshType>` over the
/// [`symbios_turtle_3d::SkeletonProp`] list emitted by the turtle
/// interpreter to decide which billboard or instanced mesh each prop slot
/// renders.
///
/// Open union (#1119). These are the values of an LSystem generator's
/// `prop_mappings`, so a prop shape from a newer engine used to fail that
/// generator's whole child decode — which `list_room_children` then drops,
/// taking every tree in the room with it. A prop is decoration; losing one
/// must not cost the forest.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PropMeshType {
    #[default]
    Leaf,
    Twig,
    Sphere,
    Cone,
    Cylinder,
    Cube,
    /// A prop shape from a newer engine. Renders as [`Leaf`](Self::Leaf) —
    /// the default, and the shape a prop slot with no mapping at all
    /// already falls back to — and refuses to serialize, so this build
    /// cannot save its stand-in over the owner's real prop.
    #[serde(other, skip_serializing)]
    Unknown,
}
