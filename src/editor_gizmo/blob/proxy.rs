//! Per-element proxy entities: the clickable, gizmo-draggable stand-ins
//! for a BlobGroup's elements while the node is under in-scene edit.
//!
//! Proxies are translucent unlit meshes — green for additive elements,
//! red for carves ([`crate::config::ui::blob_edit`]) — spawned as children
//! of the blob prim entity so the record's element-local coordinates
//! place them correctly through any placement/instance transform. Being
//! real meshes makes them `MeshRayCast`-pickable (scene click-select) and
//! valid `GizmoTarget`s via the same world-space-detach machinery the
//! whole-prim gizmo uses.
//!
//! [`reconcile_blob_proxies`] diffs the live proxy set against the
//! record every frame: GUI edits mutate the record immediately (only the
//! change *tick* is debounced), so proxies track slider drags live without
//! waiting for a rebuild. Asset churn is avoided by sharing one unit mesh
//! per scale-clean shape family (sphere/ellipsoid, box, cylinder, cone —
//! radii ride the `Transform` scale) and regenerating a capsule's / torus'
//! mesh only when its radii change (their curved sections would distort
//! under non-uniform scale).

use bevy::ecs::hierarchy::ChildOf;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use transform_gizmo_bevy::GizmoTarget;

use crate::config::ui::blob_edit as cfg;
use crate::pds::generator::{BlobElement, BlobShape};

use super::super::GizmoDetachedPrim;
use super::{BlobEditContext, write};

/// Marker + last-applied state of one element proxy. The cached fields
/// let the reconcile pass detect exactly which asset (mesh / material)
/// a record edit invalidated instead of respawning everything.
#[derive(Component)]
pub(crate) struct BlobElementProxy {
    pub(crate) index: usize,
    /// The blob prim instance the proxy was built under. A mismatch with
    /// the context's current instance (record rebuild respawned the prim,
    /// or the camera moved and the closest-instance rule re-homed the
    /// edit) marks the proxy stale.
    pub(crate) blob_entity: Entity,
    shape: BlobShape,
    radii: [f32; 3],
    subtract: bool,
    selected: bool,
}

/// Meshes/materials shared by every proxy + the wireframe line material.
/// Built once at plugin init; per-band materials are shared handles so
/// reconciling selection state is a handle swap, not an asset write.
#[derive(Resource)]
pub(crate) struct BlobEditAssets {
    unit_sphere: Handle<Mesh>,
    /// Half-extent-1 cube; box radii ride the transform scale.
    unit_cube: Handle<Mesh>,
    /// Radius-1, half-height-1 shapes; `(r, h, r)` rides the scale.
    unit_cylinder: Handle<Mesh>,
    unit_cone: Handle<Mesh>,
    add_mat: Handle<StandardMaterial>,
    add_mat_selected: Handle<StandardMaterial>,
    carve_mat: Handle<StandardMaterial>,
    carve_mat_selected: Handle<StandardMaterial>,
    /// Unlit line colour for the swapped-in wireframe mesh (used by
    /// [`super::wireframe`]; lives here so all edit-affordance assets are
    /// built in one place).
    pub(crate) line_material: Handle<StandardMaterial>,
}

impl FromWorld for BlobEditAssets {
    fn from_world(world: &mut World) -> Self {
        let (unit_sphere, unit_cube, unit_cylinder, unit_cone) = {
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            (
                meshes.add(
                    Sphere::new(1.0)
                        .mesh()
                        .ico(3)
                        .unwrap_or_else(|_| Sphere::new(1.0).mesh().build()),
                ),
                meshes.add(Mesh::from(Cuboid::new(2.0, 2.0, 2.0))),
                meshes.add(Mesh::from(Cylinder::new(1.0, 2.0))),
                meshes.add(Mesh::from(Cone::new(1.0, 2.0))),
            )
        };
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let proxy_material = |rgba: [f32; 4], alpha_override: Option<f32>| {
            let a = alpha_override.unwrap_or(rgba[3]);
            StandardMaterial {
                base_color: Color::srgba(rgba[0], rgba[1], rgba[2], a),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                // Visible from inside the accumulated surface — the camera
                // regularly ends up inside a blob while sculpting it.
                cull_mode: None,
                double_sided: true,
                ..default()
            }
        };
        let [wr, wg, wb] = cfg::WIREFRAME_COLOR;
        Self {
            unit_sphere,
            unit_cube,
            unit_cylinder,
            unit_cone,
            add_mat: materials.add(proxy_material(cfg::PROXY_ADD_COLOR, None)),
            add_mat_selected: materials.add(proxy_material(
                cfg::PROXY_ADD_COLOR,
                Some(cfg::PROXY_SELECTED_ALPHA),
            )),
            carve_mat: materials.add(proxy_material(cfg::PROXY_CARVE_COLOR, None)),
            carve_mat_selected: materials.add(proxy_material(
                cfg::PROXY_CARVE_COLOR,
                Some(cfg::PROXY_SELECTED_ALPHA),
            )),
            line_material: materials.add(StandardMaterial {
                base_color: Color::srgb(wr, wg, wb),
                unlit: true,
                ..default()
            }),
        }
    }
}

impl BlobEditAssets {
    fn material_for(&self, subtract: bool, selected: bool) -> Handle<StandardMaterial> {
        match (subtract, selected) {
            (false, false) => self.add_mat.clone(),
            (false, true) => self.add_mat_selected.clone(),
            (true, false) => self.carve_mat.clone(),
            (true, true) => self.carve_mat_selected.clone(),
        }
    }
}

/// Proxy mesh for one element. Scale-clean shapes share a unit mesh (radii
/// live on the transform); a capsule / torus bakes its radii into the mesh
/// because non-uniform scale would distort its curved sections.
fn element_mesh(
    assets: &BlobEditAssets,
    meshes: &mut Assets<Mesh>,
    e: &BlobElement,
) -> Handle<Mesh> {
    match e.shape {
        BlobShape::Capsule => meshes.add(Mesh::from(Capsule3d::new(
            e.radii.0[0].max(0.005),
            (e.radii.0[1] * 2.0).max(0.001),
        ))),
        BlobShape::Torus => meshes.add(Mesh::from(Torus {
            minor_radius: e.radii.0[1].max(0.005),
            major_radius: e.radii.0[0].max(0.005),
        })),
        BlobShape::Box => assets.unit_cube.clone(),
        BlobShape::Cylinder => assets.unit_cylinder.clone(),
        BlobShape::Cone => assets.unit_cone.clone(),
        _ => assets.unit_sphere.clone(),
    }
}

/// `true` for the shapes whose proxy mesh bakes the element radii (and so
/// must be regenerated when they change) instead of riding the transform
/// scale.
fn is_baked_mesh(shape: BlobShape) -> bool {
    matches!(shape, BlobShape::Capsule | BlobShape::Torus)
}

/// Keep the proxy set matching the record: despawn stale proxies, spawn
/// missing ones, and patch pose/mesh/material in place on the rest.
///
/// A proxy currently held by the gizmo (detached, world-space transform)
/// keeps its pose — the drag owns it; writing record-local values onto it
/// would teleport it mid-gesture.
#[allow(clippy::type_complexity)]
pub(in crate::editor_gizmo) fn reconcile_blob_proxies(
    mut commands: Commands,
    ctx: Res<BlobEditContext>,
    assets: Res<BlobEditAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut proxies: Query<(
        Entity,
        &mut BlobElementProxy,
        &mut Transform,
        &mut Mesh3d,
        &mut MeshMaterial3d<StandardMaterial>,
        Has<GizmoTarget>,
        Has<GizmoDetachedPrim>,
    )>,
) {
    let Some(active) = &ctx.active else {
        for (entity, ..) in proxies.iter() {
            commands.entity(entity).despawn();
        }
        return;
    };
    let elements = active.elements();

    let mut seen = vec![false; elements.len()];
    for (entity, mut proxy, mut tf, mut mesh, mut mat, has_gizmo, is_detached) in proxies.iter_mut()
    {
        let stale = proxy.blob_entity != active.blob_entity
            || proxy.index >= elements.len()
            || seen[proxy.index];
        if stale {
            commands.entity(entity).despawn();
            continue;
        }
        seen[proxy.index] = true;
        let e = &elements[proxy.index];
        let selected = ctx.selected_element == Some(proxy.index);

        let baked_radii_changed = is_baked_mesh(e.shape)
            && (proxy.radii[0] != e.radii.0[0] || proxy.radii[1] != e.radii.0[1]);
        if proxy.shape != e.shape || baked_radii_changed {
            mesh.0 = element_mesh(&assets, &mut meshes, e);
        }
        if proxy.subtract != e.subtract || proxy.selected != selected {
            mat.0 = assets.material_for(e.subtract, selected);
        }
        if !(has_gizmo || is_detached) {
            let desired = write::proxy_local_transform(e);
            if *tf != desired {
                *tf = desired;
            }
        }
        proxy.shape = e.shape;
        proxy.radii = e.radii.0;
        proxy.subtract = e.subtract;
        proxy.selected = selected;
    }

    for (i, e) in elements.iter().enumerate() {
        if seen[i] {
            continue;
        }
        let selected = ctx.selected_element == Some(i);
        commands.spawn((
            BlobElementProxy {
                index: i,
                blob_entity: active.blob_entity,
                shape: e.shape,
                radii: e.radii.0,
                subtract: e.subtract,
                selected,
            },
            Mesh3d(element_mesh(&assets, &mut meshes, e)),
            MeshMaterial3d(assets.material_for(e.subtract, selected)),
            write::proxy_local_transform(e),
            ChildOf(active.blob_entity),
            NotShadowCaster,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor_gizmo::ActiveTarget;
    use crate::editor_gizmo::blob::{ActiveBlobEdit, BlobEditKey};
    use crate::pds::generator::GeneratorKind;
    use crate::pds::types::{Fp, Fp3, Fp4};
    use bevy::MinimalPlugins;
    use bevy::asset::AssetPlugin;

    fn sphere(pos: [f32; 3], r: f32, subtract: bool) -> BlobElement {
        BlobElement {
            shape: BlobShape::Sphere,
            position: Fp3(pos),
            rotation: Fp4([0.0, 0.0, 0.0, 1.0]),
            radii: Fp3([r, r, r]),
            subtract,
            blend: Fp(0.1),
        }
    }

    fn blob_kind(elements: Vec<BlobElement>) -> GeneratorKind {
        let mut kind = GeneratorKind::default_primitive_for_tag("BlobGroup").unwrap();
        if let GeneratorKind::BlobGroup { elements: e, .. } = &mut kind {
            *e = elements;
        }
        kind
    }

    /// Minimal headless app that can run `reconcile_blob_proxies`: the two
    /// asset stores its `FromWorld` assets need, plus the resource + system.
    fn harness() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AssetPlugin::default())
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_resource::<BlobEditContext>()
            .init_resource::<BlobEditAssets>()
            .add_systems(Update, reconcile_blob_proxies);
        app
    }

    fn activate(app: &mut App, elements: Vec<BlobElement>) -> Entity {
        let blob_entity = app.world_mut().spawn(Transform::default()).id();
        let mut ctx = app.world_mut().resource_mut::<BlobEditContext>();
        ctx.active = Some(ActiveBlobEdit {
            key: BlobEditKey {
                target: ActiveTarget::Room,
                generator_ref: Some("g".into()),
                path: vec![],
            },
            kind: blob_kind(elements),
            blob_entity,
        });
        blob_entity
    }

    fn proxy_count(app: &mut App) -> usize {
        app.world_mut()
            .query::<&BlobElementProxy>()
            .iter(app.world())
            .count()
    }

    #[test]
    fn spawns_one_proxy_per_element() {
        let mut app = harness();
        activate(
            &mut app,
            vec![
                sphere([0.0; 3], 0.3, false),
                sphere([1.0, 0.0, 0.0], 0.2, true),
            ],
        );
        app.update();
        assert_eq!(proxy_count(&mut app), 2);
    }

    #[test]
    fn carve_and_add_get_distinct_materials() {
        let mut app = harness();
        activate(
            &mut app,
            vec![
                sphere([0.0; 3], 0.3, false),
                sphere([1.0, 0.0, 0.0], 0.2, true),
            ],
        );
        app.update();
        let mats: Vec<_> = app
            .world_mut()
            .query::<(&BlobElementProxy, &MeshMaterial3d<StandardMaterial>)>()
            .iter(app.world())
            .map(|(p, m)| (p.subtract, m.0.id()))
            .collect();
        let add = mats.iter().find(|(s, _)| !s).unwrap().1;
        let carve = mats.iter().find(|(s, _)| *s).unwrap().1;
        assert_ne!(
            add, carve,
            "carve and add proxies must not share a material"
        );
    }

    #[test]
    fn removing_an_element_despawns_its_proxy() {
        let mut app = harness();
        activate(
            &mut app,
            vec![
                sphere([0.0; 3], 0.3, false),
                sphere([1.0, 0.0, 0.0], 0.2, false),
            ],
        );
        app.update();
        assert_eq!(proxy_count(&mut app), 2);
        // Shrink the element list — reconcile must drop the orphan.
        if let GeneratorKind::BlobGroup { elements, .. } = &mut app
            .world_mut()
            .resource_mut::<BlobEditContext>()
            .active
            .as_mut()
            .unwrap()
            .kind
        {
            elements.pop();
        }
        app.update();
        assert_eq!(proxy_count(&mut app), 1);
    }

    #[test]
    fn deactivating_despawns_all_proxies() {
        let mut app = harness();
        activate(&mut app, vec![sphere([0.0; 3], 0.3, false)]);
        app.update();
        assert_eq!(proxy_count(&mut app), 1);
        app.world_mut().resource_mut::<BlobEditContext>().active = None;
        app.update();
        assert_eq!(proxy_count(&mut app), 0);
    }

    #[test]
    fn proxy_pose_follows_element_edit() {
        let mut app = harness();
        activate(&mut app, vec![sphere([0.0; 3], 0.3, false)]);
        app.update();
        // Move the element; reconcile should re-pose the existing proxy
        // (not respawn it).
        if let GeneratorKind::BlobGroup { elements, .. } = &mut app
            .world_mut()
            .resource_mut::<BlobEditContext>()
            .active
            .as_mut()
            .unwrap()
            .kind
        {
            elements[0].position = Fp3([5.0, 6.0, 7.0]);
        }
        app.update();
        let tf = *app
            .world_mut()
            .query_filtered::<&Transform, With<BlobElementProxy>>()
            .single(app.world())
            .unwrap();
        assert_eq!(tf.translation, Vec3::new(5.0, 6.0, 7.0));
        assert_eq!(proxy_count(&mut app), 1);
    }
}
