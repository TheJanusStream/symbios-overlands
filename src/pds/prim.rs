//! Hierarchical primitive nodes used by the `Construct` generator plus the
//! L-system prop-mesh enum. A [`PrimNode`] is a single parametric mesh with a
//! transform, material, and optional children; nested nodes inherit their
//! parent's transform so rotated assemblies stay rigid.

use super::texture::SovereignMaterialSettings;
use super::types::{Fp, Fp2, Fp3, TransformData};
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

/// Parametric mesh shape for a `Construct` node.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "$type")]
pub enum PrimShape {
    #[serde(rename = "network.symbios.shape.cuboid")]
    Cuboid { size: Fp3 },
    #[serde(rename = "network.symbios.shape.sphere")]
    Sphere { radius: Fp, resolution: u32 },
    #[serde(rename = "network.symbios.shape.cylinder")]
    Cylinder {
        radius: Fp,
        height: Fp,
        resolution: u32,
    },
    #[serde(rename = "network.symbios.shape.capsule")]
    Capsule {
        radius: Fp,
        length: Fp,
        latitudes: u32,
        longitudes: u32,
    },
    #[serde(rename = "network.symbios.shape.cone")]
    Cone {
        radius: Fp,
        height: Fp,
        resolution: u32,
    },
    #[serde(rename = "network.symbios.shape.torus")]
    Torus {
        minor_radius: Fp,
        major_radius: Fp,
        minor_resolution: u32,
        major_resolution: u32,
    },
    #[serde(rename = "network.symbios.shape.plane")]
    Plane { size: Fp2, subdivisions: u32 },
    #[serde(rename = "network.symbios.shape.tetrahedron")]
    Tetrahedron { size: Fp },
}

impl Default for PrimShape {
    fn default() -> Self {
        Self::Cuboid {
            size: Fp3([1.0, 1.0, 1.0]),
        }
    }
}

impl PrimShape {
    pub fn kind_tag(&self) -> &'static str {
        match self {
            Self::Cuboid { .. } => "Cuboid",
            Self::Sphere { .. } => "Sphere",
            Self::Cylinder { .. } => "Cylinder",
            Self::Capsule { .. } => "Capsule",
            Self::Cone { .. } => "Cone",
            Self::Torus { .. } => "Torus",
            Self::Plane { .. } => "Plane",
            Self::Tetrahedron { .. } => "Tetrahedron",
        }
    }

    pub fn default_for_tag(tag: &str) -> Self {
        match tag {
            "Cuboid" => Self::Cuboid {
                size: Fp3([1.0, 1.0, 1.0]),
            },
            "Sphere" => Self::Sphere {
                radius: Fp(0.5),
                resolution: 3,
            },
            "Cylinder" => Self::Cylinder {
                radius: Fp(0.5),
                height: Fp(1.0),
                resolution: 16,
            },
            "Capsule" => Self::Capsule {
                radius: Fp(0.5),
                length: Fp(1.0),
                latitudes: 8,
                longitudes: 16,
            },
            "Cone" => Self::Cone {
                radius: Fp(0.5),
                height: Fp(1.0),
                resolution: 16,
            },
            "Torus" => Self::Torus {
                minor_radius: Fp(0.1),
                major_radius: Fp(0.5),
                minor_resolution: 12,
                major_resolution: 24,
            },
            "Plane" => Self::Plane {
                size: Fp2([1.0, 1.0]),
                subdivisions: 0,
            },
            "Tetrahedron" => Self::Tetrahedron { size: Fp(1.0) },
            _ => Self::default(),
        }
    }

    /// Enforce strict bounds to prevent GPU OOM or physics panics when a
    /// malicious record pushes absurd resolutions or dimensions through the
    /// Bevy mesh builders and Avian colliders.
    pub fn sanitize(&mut self) {
        let c_dim = |v: f32| {
            if v.is_finite() {
                v.clamp(0.01, 100.0)
            } else {
                1.0
            }
        };
        match self {
            Self::Cuboid { size } => {
                size.0 = [c_dim(size.0[0]), c_dim(size.0[1]), c_dim(size.0[2])];
            }
            Self::Sphere { radius, resolution } => {
                *radius = Fp(c_dim(radius.0));
                *resolution = (*resolution).clamp(0, 10);
            }
            Self::Cylinder {
                radius,
                height,
                resolution,
            } => {
                *radius = Fp(c_dim(radius.0));
                *height = Fp(c_dim(height.0));
                *resolution = (*resolution).clamp(3, 128);
            }
            Self::Capsule {
                radius,
                length,
                latitudes,
                longitudes,
            } => {
                *radius = Fp(c_dim(radius.0));
                *length = Fp(c_dim(length.0));
                *latitudes = (*latitudes).clamp(2, 64);
                *longitudes = (*longitudes).clamp(4, 128);
            }
            Self::Cone {
                radius,
                height,
                resolution,
            } => {
                *radius = Fp(c_dim(radius.0));
                *height = Fp(c_dim(height.0));
                *resolution = (*resolution).clamp(3, 128);
            }
            Self::Torus {
                minor_radius,
                major_radius,
                minor_resolution,
                major_resolution,
            } => {
                *minor_radius = Fp(c_dim(minor_radius.0));
                *major_radius = Fp(c_dim(major_radius.0));
                *minor_resolution = (*minor_resolution).clamp(3, 64);
                *major_resolution = (*major_resolution).clamp(3, 128);
            }
            Self::Plane { size, subdivisions } => {
                size.0 = [c_dim(size.0[0]), c_dim(size.0[1])];
                *subdivisions = (*subdivisions).clamp(0, 32);
            }
            Self::Tetrahedron { size } => {
                *size = Fp(c_dim(size.0));
            }
        }
    }
}

/// A single node in a `Construct` hierarchy. Each node carries its own
/// shape, transform, material, and optional children. Child transforms are
/// interpreted relative to the parent so a rotated assembly stays rigid.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PrimNode {
    pub shape: PrimShape,
    pub transform: TransformData,
    pub solid: bool,
    pub material: SovereignMaterialSettings,
    #[serde(default)]
    pub children: Vec<PrimNode>,
}

impl Default for PrimNode {
    fn default() -> Self {
        Self {
            shape: PrimShape::default(),
            transform: TransformData::default(),
            solid: true,
            material: SovereignMaterialSettings::default(),
            children: Vec::new(),
        }
    }
}
