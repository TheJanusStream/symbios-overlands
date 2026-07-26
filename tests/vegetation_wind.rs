//! Vegetation wind sway (#916): the material-attachment lifecycle.
//!
//! The shader itself is exercised by the headless render tool, which creates
//! the real foliage pipeline (`register_headless_spawn` registers
//! `VegetationWindPlugin` for exactly that reason). What these tests cover is
//! the plumbing around it, all of which is invisible to a still render:
//! that marked entities actually swap material type, that a scatter collapses
//! onto one shared material rather than one per instance, that the async
//! texture bake still reaches foliage through the extended material, and that
//! the links map does not pin assets for the life of the session.

use bevy::prelude::*;
use symbios_overlands::wind::{
    VegetationWind, VegetationWindMaterial, WindMaterialLinks, WindSway, apply_wind_state,
    attach_wind_materials, mirror_wind_material_bases,
};

/// A minimal app carrying the wind systems and the two asset stores they
/// move materials between. `MaterialPlugin` is deliberately not used: it
/// needs the render app, and none of the behaviour under test does.
fn wind_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()));
    app.init_asset::<StandardMaterial>();
    app.init_asset::<VegetationWindMaterial>();
    app.init_resource::<VegetationWind>();
    app.init_resource::<WindMaterialLinks>();
    app.add_systems(
        Update,
        (
            attach_wind_materials,
            mirror_wind_material_bases,
            apply_wind_state,
        ),
    );
    app
}

/// Spawn `count` entities sharing one `StandardMaterial`, as a scatter does.
fn spawn_marked(
    app: &mut App,
    profile: WindSway,
    count: usize,
) -> (Handle<StandardMaterial>, Vec<Entity>) {
    let source = app
        .world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.6, 0.2),
            ..default()
        });
    let entities = (0..count)
        .map(|_| {
            app.world_mut()
                .spawn((MeshMaterial3d(source.clone()), profile))
                .id()
        })
        .collect();
    (source, entities)
}

/// The core swap: a marked entity loses its `StandardMaterial` component and
/// gains the extended one, whose base is a copy of the source.
#[test]
fn a_marked_entity_swaps_to_the_wind_material() {
    let mut app = wind_app();
    let (source, entities) = spawn_marked(&mut app, WindSway::Card, 1);
    let entity = entities[0];

    app.update();

    let world = app.world();
    assert!(
        world
            .get::<MeshMaterial3d<StandardMaterial>>(entity)
            .is_none(),
        "the plain material component must be removed, or the entity would \
         be drawn twice"
    );
    let wind = world
        .get::<MeshMaterial3d<VegetationWindMaterial>>(entity)
        .expect("the wind material must have been attached");
    let materials = world.resource::<Assets<VegetationWindMaterial>>();
    let mat = materials.get(&wind.0).expect("material asset exists");
    assert_eq!(
        mat.extension.source.id(),
        source.id(),
        "the extension must hold the source it was cloned from"
    );
    assert_eq!(
        mat.base.base_color,
        Color::srgb(0.3, 0.6, 0.2),
        "the base must be a copy of the source material"
    );
}

/// The batching contract the whole scatter tier rests on. Every instance of a
/// ground-cover prop shares one `StandardMaterial` handle (the prim cache is
/// content-addressed), and they must therefore share one wind material too —
/// a per-instance material would fork the handle per instance and undo the
/// batching that makes hundreds of cards affordable.
#[test]
fn a_scatter_shares_one_wind_material() {
    let mut app = wind_app();
    let (_source, entities) = spawn_marked(&mut app, WindSway::Card, 64);

    app.update();

    let world = app.world();
    let ids: Vec<_> = entities
        .iter()
        .map(|&e| {
            world
                .get::<MeshMaterial3d<VegetationWindMaterial>>(e)
                .expect("every instance is converted")
                .0
                .id()
        })
        .collect();
    assert!(
        ids.windows(2).all(|w| w[0] == w[1]),
        "64 instances of one source material must collapse onto one wind material"
    );
    assert_eq!(
        world.resource::<Assets<VegetationWindMaterial>>().len(),
        1,
        "one material asset, not one per instance"
    );
    assert_eq!(world.resource::<WindMaterialLinks>().len(), 1);
}

/// The two profiles must not share a material even off one source: their
/// uniform blocks differ, and silently serving a card the branch profile
/// would make ground cover sway as though it were a tree canopy.
#[test]
fn the_two_profiles_do_not_share_a_material() {
    let mut app = wind_app();
    let (source, card) = spawn_marked(&mut app, WindSway::Card, 1);
    let branch = app
        .world_mut()
        .spawn((MeshMaterial3d(source.clone()), WindSway::Branch))
        .id();

    app.update();

    let world = app.world();
    let card_id = world
        .get::<MeshMaterial3d<VegetationWindMaterial>>(card[0])
        .expect("card converted")
        .0
        .id();
    let branch_id = world
        .get::<MeshMaterial3d<VegetationWindMaterial>>(branch)
        .expect("branch converted")
        .0
        .id();
    assert_ne!(card_id, branch_id, "one material per (source, profile)");

    let materials = world.resource::<Assets<VegetationWindMaterial>>();
    let card_u = &materials.get(card_id).unwrap().extension.uniforms;
    let branch_u = &materials.get(branch_id).unwrap().extension.uniforms;
    assert_eq!(
        card_u.height_bias, 0.5,
        "a card's origin is its centre, so its weight straddles zero"
    );
    assert_eq!(
        branch_u.height_bias, 0.0,
        "a bucket's origin is the plant base, so its weight starts there"
    );
}

/// The reason the material is attached by a system instead of at spawn time.
/// A procedural material's textures are still baking when the entity is
/// spawned; the bake lands in `Assets<StandardMaterial>` frames later. An
/// `ExtendedMaterial` embeds its base *by value*, so without the mirror the
/// foliage would keep the untextured copy forever — alpha-masked cards with
/// no alpha, which render as opaque squares.
#[test]
fn a_later_texture_bake_reaches_the_wind_material() {
    let mut app = wind_app();
    let (source, entities) = spawn_marked(&mut app, WindSway::Card, 1);

    app.update();

    // Stand in for the bake completing: the upstream patch system writes the
    // image handles into the source material by handle, exactly like this.
    let baked: Handle<Image> = Handle::default();
    {
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let mat = materials.get_mut(&source).expect("source still live");
        mat.base_color_texture = Some(baked.clone());
        mat.alpha_mode = AlphaMode::Mask(0.5);
    }

    // Two frames, not one: `AssetEvent`s are emitted by bevy_asset in `Last`,
    // so a modification made during frame N is only readable by a system in
    // frame N+1's `Update`. That one-frame lag is why the mirror is a system
    // reacting to events rather than something the bake could call directly —
    // and it is invisible in practice, the frame in question being one where
    // the texture had not finished baking anyway.
    app.update();
    app.update();

    let world = app.world();
    let wind = world
        .get::<MeshMaterial3d<VegetationWindMaterial>>(entities[0])
        .expect("converted");
    let mat = world
        .resource::<Assets<VegetationWindMaterial>>()
        .get(&wind.0)
        .expect("material asset exists");
    assert_eq!(
        mat.base.base_color_texture.as_ref().map(Handle::id),
        Some(baked.id()),
        "the baked texture must reach the foliage through the extension"
    );
    assert_eq!(
        mat.base.alpha_mode,
        AlphaMode::Mask(0.5),
        "and so must the alpha mode it arrives with"
    );
}

/// The room's wind reaches materials that already exist. A wind-direction
/// drag in the editor writes the resource; every live foliage material must
/// pick it up, or the clouds would turn while the trees kept leaning the old
/// way.
#[test]
fn a_wind_change_reaches_live_materials() {
    let mut app = wind_app();
    let (_source, entities) = spawn_marked(&mut app, WindSway::Branch, 1);
    app.update();

    let turned = Vec2::new(-0.4, 0.9);
    app.world_mut().resource_mut::<VegetationWind>().dir = turned;
    app.world_mut().resource_mut::<VegetationWind>().speed = 11.0;

    app.update();

    let world = app.world();
    let wind = world
        .get::<MeshMaterial3d<VegetationWindMaterial>>(entities[0])
        .expect("converted");
    let mat = world
        .resource::<Assets<VegetationWindMaterial>>()
        .get(&wind.0)
        .expect("material asset exists");
    assert_eq!(mat.extension.uniforms.wind_dir, turned);
    assert_eq!(mat.extension.uniforms.speed, 11.0);
    // The per-profile half must survive the patch — only the two global
    // fields are the environment's to write.
    assert_eq!(mat.extension.uniforms.height_bias, 0.0);
    assert!(mat.extension.uniforms.strength > 0.0);
}

/// A material created *after* the last wind change must still be built with
/// the current wind, not the default. Nothing re-patches it, so getting this
/// wrong leaves foliage spawned mid-session blowing the wrong way.
#[test]
fn a_material_built_after_a_wind_change_uses_it() {
    let mut app = wind_app();
    let turned = Vec2::new(0.0, -1.0);
    app.world_mut().resource_mut::<VegetationWind>().dir = turned;
    app.update();

    let (_source, entities) = spawn_marked(&mut app, WindSway::Card, 1);
    app.update();

    let world = app.world();
    let wind = world
        .get::<MeshMaterial3d<VegetationWindMaterial>>(entities[0])
        .expect("converted");
    let mat = world
        .resource::<Assets<VegetationWindMaterial>>()
        .get(&wind.0)
        .expect("material asset exists");
    assert_eq!(
        mat.extension.uniforms.wind_dir, turned,
        "a freshly built material must start on the room's current wind"
    );
}

/// Retention (#919's shape): once the foliage using a wind material is gone,
/// nothing may keep the material — and through it a `StandardMaterial` and
/// its images — alive. The links map holds an `AssetId`, not a `Handle`,
/// precisely so a re-roll does not accumulate a session's worth of dead
/// foliage.
#[test]
fn despawned_foliage_releases_its_wind_material() {
    let mut app = wind_app();
    let (source, entities) = spawn_marked(&mut app, WindSway::Card, 4);
    app.update();
    assert_eq!(
        app.world()
            .resource::<Assets<VegetationWindMaterial>>()
            .len(),
        1
    );

    for entity in entities {
        app.world_mut().despawn(entity);
    }
    // Drop the test's own handle too, leaving the links map as the only
    // thing that could still be holding the pair.
    drop(source);
    app.update();
    app.update();

    let world = app.world();
    assert_eq!(
        world.resource::<Assets<VegetationWindMaterial>>().len(),
        0,
        "the links map must not pin the wind material"
    );
    assert_eq!(
        world.resource::<Assets<StandardMaterial>>().len(),
        0,
        "nor, through the extension's source handle, the material it wrapped"
    );
}
