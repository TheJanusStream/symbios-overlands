//! Water surface `MaterialExtension` for the animated water shader.
//!
//! Extends Bevy's `StandardMaterial` with a custom WGSL fragment shader
//! (`assets/shaders/water.wgsl`) that computes Gerstner-wave displacement,
//! Fresnel-driven alpha / reflection, scrolling detail normals, foam, and
//! sun-glitter. Every knob that drives the shader flows through the
//! `WaterUniforms` block on this extension — a mix of per-volume parameters
//! (authored on [`crate::pds::WaterSurface`]) and room-wide parameters
//! (authored on [`crate::pds::Environment`]).

use bevy::{
    math::{Vec2, Vec4},
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

const WATER_SHADER_PATH: &str = "shaders/water.wgsl";

/// GPU uniform block shared with `water.wgsl`. Field ordering is chosen so
/// `Vec4`s lead (16-byte aligned), `Vec2` sits where 8-byte alignment is
/// cheapest, and scalars bring up the rear — the `ShaderType` derive still
/// inserts any padding needed to round the struct up to 16 bytes.
///
/// `scatter_color` is stored as a `Vec4` rather than `Vec3` to avoid the
/// 12-vs-16-byte alignment pitfall that otherwise requires explicit padding
/// members; the alpha channel is unused by the shader.
#[derive(Debug, Clone, Default, ShaderType)]
pub struct WaterUniforms {
    /// Per-volume: sRGBA tint at head-on view (low alpha = transparent).
    pub shallow_color: Vec4,
    /// Per-volume: sRGBA tint at grazing view (high alpha = opaque).
    pub deep_color: Vec4,
    /// Global: subsurface-scatter tint added to wave crests. (rgb used, a=0)
    pub scatter_color: Vec4,
    /// Per-volume: prevailing wave direction in world XZ.
    pub wave_direction: Vec2,
    /// Per-volume: global amplitude multiplier on the Gerstner waves.
    pub wave_scale: f32,
    /// Per-volume: time multiplier. `0` freezes the surface.
    pub wave_speed: f32,
    /// Per-volume: Gerstner steepness in `[0, 1]`.
    pub wave_choppiness: f32,
    /// Per-volume: PBR perceptual roughness override.
    pub roughness: f32,
    /// Per-volume: PBR metallic override.
    pub metallic: f32,
    /// Per-volume: Schlick F0 reflectance at head-on view.
    pub reflectance: f32,
    /// Per-volume: strength of the procedural foam on wave crests.
    pub foam_amount: f32,
    /// Global: close-distance detail normal tiling (1/world-m).
    pub normal_scale_near: f32,
    /// Global: far-distance detail normal tiling (1/world-m).
    pub normal_scale_far: f32,
    /// Global: reserved for screen-space refraction distortion.
    pub refraction_strength: f32,
    /// Global: specular sun-glitter intensity.
    pub sun_glitter: f32,
    /// Global: shoreline foam band width (m). Reserved.
    pub shore_foam_width: f32,
    /// Per-volume: flow-map blend in `[0, 1]`. Mirrors
    /// [`crate::pds::WaterSurface::flow_amount`]. `0` = full Gerstner
    /// standing-wave look; `1` = pure flow-map (scrolling detail normals
    /// along the surface's downhill tangent, suppressed standing waves).
    pub flow_amount: f32,
}

/// [`MaterialExtension`] that drives `water.wgsl`.
///
/// Bind-group slots (group `MATERIAL_BIND_GROUP`, 100 +):
/// - 100 [`WaterUniforms`] uniform
///
/// Prepass is disabled — water is transparent so it must not write depth in
/// the prepass pass, otherwise a shoreline would occlude every fragment the
/// main pass would try to blend underneath it.
#[derive(Asset, TypePath, AsBindGroup, Clone, Default, Debug)]
pub struct WaterExtension {
    #[uniform(100)]
    pub uniforms: WaterUniforms,
}

impl MaterialExtension for WaterExtension {
    fn fragment_shader() -> ShaderRef {
        WATER_SHADER_PATH.into()
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

/// Convenience alias for the full extended-material type used by the water volume.
pub type WaterMaterial = ExtendedMaterial<StandardMaterial, WaterExtension>;

/// Runtime registry of every water plane currently in the scene. Populated
/// by `world_builder::compile_room_record` as it spawns each `WaterVolume`,
/// and consumed by physics (`apply_buoyancy_forces`, humanoid water-state
/// classifier) and the scatter biome filter so multiple water generators in
/// a room all participate in the same lookup.
///
/// Cleared and rebuilt every compile pass; nothing else mutates it.
#[derive(Resource, Default, Debug, Clone)]
pub struct WaterSurfaces {
    pub planes: Vec<WaterPlane>,
}

/// One water plane spawned by the world builder. The `world_from_local`
/// transform is the final spawn transform — the water-level offset is
/// already folded into `translation.y`, the rotation is the cumulative
/// transform-chain rotation, and the scale is the cumulative scale.
///
/// The plane lies at local-Y = 0 in this frame; `local_half_extents` are
/// the half-extents of the underlying `Plane3d` mesh **before** the
/// transform's scale is applied. The world-space rectangle is therefore
/// `local_half_extents * world_from_local.scale.{x, z}`.
///
/// `flow_strength` mirrors [`crate::pds::WaterSurface::flow_strength`] —
/// the force-per-metre-submerged applied to floating bodies along the
/// surface's downhill tangent. Always zero on flat water (the tangent of
/// gravity on a horizontal plane is the zero vector).
#[derive(Debug, Clone)]
pub struct WaterPlane {
    pub world_from_local: Transform,
    pub local_half_extents: Vec2,
    pub flow_strength: f32,
}

/// Result of a 3D water-volume query. Returned by [`WaterSurfaces::query`]
/// when the queried point falls inside a water surface's projection AND is
/// below that surface (along its world-space normal). Buoyancy and flow
/// forces in `apply_buoyancy_forces` read every field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaterQuery {
    /// Index into [`WaterSurfaces::planes`] — the surface the query hit.
    pub surface_idx: usize,
    /// World-space surface normal (`world_from_local.rotation * Y`),
    /// snapped to `Vec3::Y` when the rotation is within [`YAW_ONLY_EPS`]
    /// of horizontal so accidental small tilts don't drive flow physics.
    pub normal: Vec3,
    /// Signed distance from the queried point to the plane along the
    /// surface normal, sign-flipped so positive means submerged. A point
    /// at the surface returns depth = 0; deeper means more positive.
    pub depth: f32,
    /// Unit downhill tangent on the surface (gravity projected onto the
    /// plane, normalised). [`Vec3::ZERO`] when the surface is flat —
    /// gravity has no tangent component on a horizontal plane.
    pub flow_dir: Vec3,
    /// Per-volume `flow_strength` copied from the source `WaterPlane`.
    pub flow_strength: f32,
}

/// Threshold for the yaw-only guard. A surface whose world normal has a
/// Y component above `1 - YAW_ONLY_EPS` is treated as flat — its normal
/// is snapped to `Vec3::Y` and `flow_dir` collapses to zero. cos(5°) ≈
/// 0.9962 so this catches authoring jitter while still letting a 5°+
/// deliberate tilt drive flow physics.
const YAW_ONLY_EPS: f32 = 1.0 - 0.9962;

impl WaterSurfaces {
    /// Returns the highest water surface whose XZ rectangle (in its own
    /// local frame) contains the projection of the given world XZ point,
    /// along with that surface's world-space Y at the projection. Stacked
    /// surfaces win by highest Y so an elevated pond above the home sea
    /// is preferred when the avatar's column intersects both.
    ///
    /// For tilted surfaces the returned Y is the plane's height *at the
    /// queried XZ*, computed by solving the plane equation. Yaw-only
    /// rotation collapses to a constant Y.
    ///
    /// Used by:
    /// * the scatter biome filter (one global threshold per scatter), and
    /// * the humanoid water-state classifier (column-vs-surface comparison).
    pub fn surface_at(&self, world_xz: Vec2) -> Option<(usize, f32)> {
        let mut best: Option<(usize, f32)> = None;
        for (i, plane) in self.planes.iter().enumerate() {
            let normal = (plane.world_from_local.rotation * Vec3::Y).normalize_or(Vec3::Y);
            let t = plane.world_from_local.translation;
            // Solve plane equation `(p - t) · n = 0` for `p.y`. For
            // yaw-only surfaces normal.y ≈ 1 and the formula collapses to
            // `t.y`; for tilted surfaces it produces the correct Y at the
            // queried XZ. The `normal.y` denominator is guarded by the
            // YAW_ONLY_EPS check below — past 90° tilt the plane is
            // ill-defined as a Y-graph and we fall back to translation.y.
            let surface_y = if normal.y.abs() < YAW_ONLY_EPS {
                t.y
            } else {
                let dx = world_xz.x - t.x;
                let dz = world_xz.y - t.z;
                t.y - (dx * normal.x + dz * normal.z) / normal.y
            };
            let world_p = Vec3::new(world_xz.x, surface_y, world_xz.y);
            let local = plane
                .world_from_local
                .compute_affine()
                .inverse()
                .transform_point3(world_p);
            if local.x.abs() <= plane.local_half_extents.x
                && local.z.abs() <= plane.local_half_extents.y
                && best.is_none_or(|(_, by)| surface_y > by)
            {
                best = Some((i, surface_y));
            }
        }
        best
    }

    /// Full 3D water-volume query for buoyancy and flow physics. Returns
    /// the *closest above* surface — the one with the smallest positive
    /// `depth`. That's the surface a submerged object would emerge through
    /// if pushed straight up along the normal: if an avatar is below both
    /// a sea (y=0) and an elevated pond (y=5) at a given XZ, they should
    /// float toward the sea, not the pond.
    ///
    /// `world_p` must be a world-space point. The function:
    /// 1. Inverse-transforms `p` into each plane's local frame and rejects
    ///    the plane if the point's local XZ falls outside the half-extents.
    /// 2. Computes signed distance along the plane normal — points above
    ///    the surface (`depth <= 0`) are skipped.
    /// 3. Among submerged surfaces, picks the one with the smallest depth.
    pub fn query(&self, world_p: Vec3) -> Option<WaterQuery> {
        let mut best: Option<WaterQuery> = None;
        for (i, plane) in self.planes.iter().enumerate() {
            let Some(q) = self.signed_query_against(plane, i, world_p) else {
                continue;
            };
            if q.depth <= 0.0 {
                continue;
            }
            if best.is_none_or(|prev| q.depth < prev.depth) {
                best = Some(q);
            }
        }
        best
    }

    /// Like [`Self::query`], but does not cull points above the visible
    /// surface — `depth` may be negative (point above surface) or
    /// positive (submerged). Picks the surface with the smallest absolute
    /// depth (closest plane along the normal). Used by the HoverBoat
    /// buoyancy computation, which intentionally rests `water_rest_length`
    /// *above* the visible water and needs continuous signed feedback to
    /// keep its rest position stable. Without this, `query` returning
    /// `None` for the hovering position produced a step-function lift
    /// the moment the chassis pierced the surface, slamming the boat
    /// into the water instead of letting it settle.
    pub fn query_signed(&self, world_p: Vec3) -> Option<WaterQuery> {
        let mut best: Option<WaterQuery> = None;
        for (i, plane) in self.planes.iter().enumerate() {
            let Some(q) = self.signed_query_against(plane, i, world_p) else {
                continue;
            };
            if best.is_none_or(|prev: WaterQuery| q.depth.abs() < prev.depth.abs()) {
                best = Some(q);
            }
        }
        best
    }

    /// Shared XZ-bounds + signed-distance computation used by both
    /// [`Self::query`] and [`Self::query_signed`]. Returns `None` when
    /// the point's projection falls outside the plane's local rectangle;
    /// the caller decides whether to honour the sign of `depth`.
    fn signed_query_against(
        &self,
        plane: &WaterPlane,
        idx: usize,
        world_p: Vec3,
    ) -> Option<WaterQuery> {
        let local = plane
            .world_from_local
            .compute_affine()
            .inverse()
            .transform_point3(world_p);
        if local.x.abs() > plane.local_half_extents.x || local.z.abs() > plane.local_half_extents.y
        {
            return None;
        }
        let raw_normal = (plane.world_from_local.rotation * Vec3::Y).normalize_or(Vec3::Y);
        // Yaw-only guard: an effectively-flat surface (whether
        // intentionally or via authoring jitter) gets flat-water
        // physics — vertical lift, no tangent flow — instead of
        // microscopically tilted forces that compound over time.
        let (normal, flow_dir) = if raw_normal.y > 1.0 - YAW_ONLY_EPS {
            (Vec3::Y, Vec3::ZERO)
        } else {
            let g = Vec3::NEG_Y;
            let tangent = g - raw_normal * g.dot(raw_normal);
            (raw_normal, tangent.normalize_or_zero())
        };
        let signed = (world_p - plane.world_from_local.translation).dot(normal);
        Some(WaterQuery {
            surface_idx: idx,
            normal,
            depth: -signed,
            flow_dir,
            flow_strength: plane.flow_strength,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_plane(y: f32, half: Vec2) -> WaterPlane {
        WaterPlane {
            world_from_local: Transform::from_xyz(0.0, y, 0.0),
            local_half_extents: half,
            flow_strength: 0.0,
        }
    }

    #[test]
    fn surface_at_inside_returns_y() {
        let surfaces = WaterSurfaces {
            planes: vec![flat_plane(5.0, Vec2::splat(10.0))],
        };
        let q = surfaces.surface_at(Vec2::new(3.0, -4.0)).unwrap();
        assert_eq!(q.0, 0);
        assert!((q.1 - 5.0).abs() < 1e-5);
    }

    #[test]
    fn surface_at_outside_returns_none() {
        let surfaces = WaterSurfaces {
            planes: vec![flat_plane(0.0, Vec2::splat(10.0))],
        };
        assert!(surfaces.surface_at(Vec2::new(11.0, 0.0)).is_none());
        assert!(surfaces.surface_at(Vec2::new(0.0, -11.0)).is_none());
    }

    #[test]
    fn surface_at_stacked_picks_highest() {
        let surfaces = WaterSurfaces {
            planes: vec![
                flat_plane(0.0, Vec2::splat(100.0)),
                flat_plane(7.5, Vec2::splat(5.0)),
            ],
        };
        // Inside both — should pick the elevated pond.
        let q = surfaces.surface_at(Vec2::new(2.0, 2.0)).unwrap();
        assert_eq!(q.0, 1);
        assert!((q.1 - 7.5).abs() < 1e-5);
        // Inside the sea but outside the pond — should pick the sea.
        let q = surfaces.surface_at(Vec2::new(50.0, 50.0)).unwrap();
        assert_eq!(q.0, 0);
        assert!((q.1 - 0.0).abs() < 1e-5);
    }

    #[test]
    fn surface_at_respects_translated_origin() {
        let mut p = flat_plane(2.0, Vec2::splat(3.0));
        p.world_from_local.translation.x = 100.0;
        p.world_from_local.translation.z = 100.0;
        let surfaces = WaterSurfaces { planes: vec![p] };
        // Centred at (100, 100) with extent 3 → only points near 100 hit.
        assert!(surfaces.surface_at(Vec2::new(100.0, 100.0)).is_some());
        assert!(surfaces.surface_at(Vec2::new(0.0, 0.0)).is_none());
        assert!(surfaces.surface_at(Vec2::new(102.0, 99.0)).is_some());
    }

    #[test]
    fn surface_at_respects_scale() {
        // Mesh half-extent 10, scaled by 0.1 → world half-extent = 1.
        let plane = WaterPlane {
            world_from_local: Transform::from_xyz(0.0, 4.0, 0.0)
                .with_scale(Vec3::new(0.1, 1.0, 0.1)),
            local_half_extents: Vec2::splat(10.0),
            flow_strength: 0.0,
        };
        let surfaces = WaterSurfaces {
            planes: vec![plane],
        };
        assert!(surfaces.surface_at(Vec2::new(0.5, 0.5)).is_some());
        assert!(surfaces.surface_at(Vec2::new(1.5, 0.0)).is_none());
    }

    #[test]
    fn surface_at_respects_yaw() {
        // 45° yaw rotates a (5, 10) rectangle so its corners point along the
        // diagonals of world axes.
        let plane = WaterPlane {
            world_from_local: Transform::from_xyz(0.0, 0.0, 0.0)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_4)),
            local_half_extents: Vec2::new(5.0, 10.0),
            flow_strength: 0.0,
        };
        let surfaces = WaterSurfaces {
            planes: vec![plane],
        };
        // World point (0, 0, 7) — rotates back into local as roughly
        // (-7/√2, 0, 7/√2) ≈ (-4.95, 0, 4.95): x in [-5, 5] ✓, z in [-10, 10] ✓.
        assert!(surfaces.surface_at(Vec2::new(0.0, 7.0)).is_some());
        // World point (7, 0, 0) — rotates back to (4.95, 0, -4.95): inside.
        assert!(surfaces.surface_at(Vec2::new(7.0, 0.0)).is_some());
        // World point (8, 0, 0) — rotates back to (~5.66, 0, -5.66): x exceeds 5.
        assert!(surfaces.surface_at(Vec2::new(8.0, 0.0)).is_none());
    }

    fn tilted_plane(y: f32, tilt_radians: f32, half: Vec2, flow: f32) -> WaterPlane {
        WaterPlane {
            world_from_local: Transform::from_xyz(0.0, y, 0.0)
                .with_rotation(Quat::from_rotation_x(tilt_radians)),
            local_half_extents: half,
            flow_strength: flow,
        }
    }

    #[test]
    fn surface_at_returns_plane_y_on_tilted_surface() {
        // 30° pitch around X axis. The plane equation is `y - 0 = -tan(30°)·z`
        // so at z = +1 the surface should sit ~ -tan(30°) ≈ -0.577 above
        // origin — i.e. lower on the +Z side.
        let plane = tilted_plane(0.0, 30f32.to_radians(), Vec2::splat(20.0), 0.0);
        let surfaces = WaterSurfaces {
            planes: vec![plane],
        };
        let (_, y_at_pos_z) = surfaces.surface_at(Vec2::new(0.0, 1.0)).unwrap();
        let (_, y_at_neg_z) = surfaces.surface_at(Vec2::new(0.0, -1.0)).unwrap();
        // The +Z side dips below origin; the -Z side rises above. The two
        // values must be exactly opposite (flat origin, symmetric tilt).
        assert!((y_at_pos_z + y_at_neg_z).abs() < 1e-4);
        assert!(y_at_pos_z < 0.0 && y_at_neg_z > 0.0);
    }

    #[test]
    fn query_flat_water_yields_world_y_normal_zero_flow() {
        let surfaces = WaterSurfaces {
            planes: vec![flat_plane(0.0, Vec2::splat(50.0))],
        };
        // Submerged at depth 1.5 — feet of an avatar swimming below origin.
        let q = surfaces.query(Vec3::new(0.0, -1.5, 0.0)).unwrap();
        assert_eq!(q.surface_idx, 0);
        assert!((q.normal - Vec3::Y).length() < 1e-5);
        assert!((q.depth - 1.5).abs() < 1e-5);
        assert_eq!(q.flow_dir, Vec3::ZERO);
    }

    #[test]
    fn query_above_surface_returns_none() {
        let surfaces = WaterSurfaces {
            planes: vec![flat_plane(0.0, Vec2::splat(50.0))],
        };
        assert!(surfaces.query(Vec3::new(0.0, 0.5, 0.0)).is_none());
    }

    #[test]
    fn query_outside_extents_returns_none() {
        let surfaces = WaterSurfaces {
            planes: vec![flat_plane(0.0, Vec2::splat(2.0))],
        };
        // Submerged but XZ outside the rectangle.
        assert!(surfaces.query(Vec3::new(10.0, -1.0, 0.0)).is_none());
    }

    #[test]
    fn query_tilted_normal_and_flow_dir_are_consistent() {
        // 30° tilt about X axis: surface normal tips toward -Z, downhill
        // tangent points along +Z (water flows toward +Z down the slope).
        let surfaces = WaterSurfaces {
            planes: vec![tilted_plane(
                0.0,
                30f32.to_radians(),
                Vec2::splat(20.0),
                5.0,
            )],
        };
        // Sample a point that's submerged below the tilted plane. At
        // (0, -1, 0) the plane passes through origin so the point is below
        // the surface along its normal; signed distance is negative on the
        // surface side, depth (= -signed) is positive.
        let q = surfaces.query(Vec3::new(0.0, -1.0, 0.0)).unwrap();
        // Normal: rotation_x(30°) * Y = (0, cos30°, sin30°). cos(30°)≈0.866.
        assert!((q.normal.x).abs() < 1e-5);
        assert!((q.normal.y - 30f32.to_radians().cos()).abs() < 1e-5);
        assert!((q.normal.z - 30f32.to_radians().sin()).abs() < 1e-5);
        // Depth must be positive (submerged).
        assert!(q.depth > 0.0);
        // flow_dir is gravity (0, -1, 0) projected onto the plane and
        // normalised. Gravity·normal = -cos30°. Projected = (0,-1,0) -
        // (-cos30°)·(0, cos30°, sin30°) = (0, -1+cos²30°, cos30°·sin30°).
        // Normalised: should have positive z (downhill = +Z) and negative y
        // (still pointing roughly down).
        assert!(q.flow_dir.z > 0.0);
        assert!(q.flow_dir.y < 0.0);
        assert!((q.flow_dir.length() - 1.0).abs() < 1e-4);
        assert_eq!(q.flow_strength, 5.0);
    }

    #[test]
    fn query_yaw_only_guard_collapses_to_flat() {
        // Pure yaw rotation around Y produces normal = Y; the guard should
        // snap normal exactly to Vec3::Y and flow_dir to ZERO.
        let plane = WaterPlane {
            world_from_local: Transform::from_xyz(0.0, 2.0, 0.0)
                .with_rotation(Quat::from_rotation_y(0.7)),
            local_half_extents: Vec2::splat(10.0),
            flow_strength: 5.0,
        };
        let surfaces = WaterSurfaces {
            planes: vec![plane],
        };
        let q = surfaces.query(Vec3::new(0.0, 0.0, 0.0)).unwrap();
        assert_eq!(q.normal, Vec3::Y);
        assert_eq!(q.flow_dir, Vec3::ZERO);
        assert!((q.depth - 2.0).abs() < 1e-5);
    }

    #[test]
    fn query_signed_returns_negative_depth_above_surface() {
        // A point above the surface returns `Some` with negative depth,
        // unlike `query` which culls above-water hits. This is the
        // hoverboat-rest-position case: the chassis sits above the
        // visible water but still needs continuous lift feedback.
        let surfaces = WaterSurfaces {
            planes: vec![flat_plane(0.0, Vec2::splat(50.0))],
        };
        let q = surfaces.query_signed(Vec3::new(0.0, 0.5, 0.0)).unwrap();
        assert_eq!(q.surface_idx, 0);
        assert!((q.depth + 0.5).abs() < 1e-5);
    }

    #[test]
    fn query_signed_returns_none_outside_extents() {
        let surfaces = WaterSurfaces {
            planes: vec![flat_plane(0.0, Vec2::splat(2.0))],
        };
        // Outside XZ rectangle returns None even with the more
        // permissive signed query.
        assert!(surfaces.query_signed(Vec3::new(10.0, 0.5, 0.0)).is_none());
    }

    #[test]
    fn query_signed_picks_closest_by_absolute_depth() {
        // Stacked sea (y=0) + elevated pond (y=10). A hoverboat corner at
        // y = 0.5 (resting above the sea) is 0.5m above sea, 9.5m below
        // pond — closest by absolute distance is the sea.
        let surfaces = WaterSurfaces {
            planes: vec![
                flat_plane(0.0, Vec2::splat(100.0)),
                WaterPlane {
                    world_from_local: Transform::from_xyz(0.0, 10.0, 0.0),
                    local_half_extents: Vec2::splat(100.0),
                    flow_strength: 0.0,
                },
            ],
        };
        let q = surfaces.query_signed(Vec3::new(0.0, 0.5, 0.0)).unwrap();
        assert_eq!(q.surface_idx, 0);
        assert!((q.depth + 0.5).abs() < 1e-5);
    }

    #[test]
    fn query_picks_smallest_depth_for_stacked_submersion() {
        // Sea at y=0 (large XZ) + elevated pond at y=5 (small XZ). An
        // avatar at (0, 4, 0) is above the sea (depth -4 → skipped) and
        // below the pond (depth 1). Pond wins.
        let surfaces = WaterSurfaces {
            planes: vec![
                flat_plane(0.0, Vec2::splat(100.0)),
                WaterPlane {
                    world_from_local: Transform::from_xyz(0.0, 5.0, 0.0),
                    local_half_extents: Vec2::splat(2.0),
                    flow_strength: 0.0,
                },
            ],
        };
        let q = surfaces.query(Vec3::new(0.0, 4.0, 0.0)).unwrap();
        assert_eq!(q.surface_idx, 1);
        assert!((q.depth - 1.0).abs() < 1e-5);

        // An avatar at (0, -3, 0) is below both surfaces (sea depth 3, pond
        // depth 8). The smaller one (sea) is the correct float target.
        let q = surfaces.query(Vec3::new(0.0, -3.0, 0.0)).unwrap();
        assert_eq!(q.surface_idx, 0);
        assert!((q.depth - 3.0).abs() < 1e-5);
    }
}
