//! The grass field's tests (#1675): the shader's mirror of the uniform
//! block, and the field laid around a camera on a ground.

use bevy::render::render_resource::ShaderType;
use bevy_symbios_ground::HeightMap;

use super::*;
use crate::ground_cover::{CoverMask, GroundFrame};
use crate::pds::RoomRecord;
use avian3d::prelude::ColliderAabb;

/// Ordered `(name, type)` pairs of a `struct <name> { .. }` block in WGSL
/// source, ignoring comments and blank lines.
fn wgsl_fields(src: &str, struct_name: &str) -> Vec<(String, String)> {
    let head = format!("struct {struct_name} {{");
    let start = src.find(&head).expect("struct not found in shader") + head.len();
    let body = &src[start..start + src[start..].find("};").expect("unterminated struct")];
    body.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//") && l.contains(':'))
        .map(|l| {
            let (name, ty) = l.split_once(':').expect("filtered on ':'");
            (
                name.trim().to_string(),
                ty.trim().trim_end_matches(',').to_string(),
            )
        })
        .collect()
}

/// The Rust and WGSL declarations of the uniform block are two hand-written
/// copies of one layout, and nothing in the build compiles the shader: a
/// field added to one and not the other reads at the wrong offsets at
/// runtime. The same holds for the tile size the shader turns tiles by.
#[test]
fn wgsl_block_mirrors_the_rust_one() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/shaders/grass.wgsl"
    ))
    .expect("grass.wgsl is a tracked asset");
    let expected = [
        ("base", "vec4<f32>"),
        ("tip", "vec4<f32>"),
        ("dry", "vec4<f32>"),
        ("flowers", "array<vec4<f32>, 4>"),
        ("layer_density", "vec4<f32>"),
        ("ground", "array<vec4<f32>, 4>"),
        ("frame", "vec4<f32>"),
        ("wind", "vec4<f32>"),
        ("reach", "vec4<f32>"),
        ("shares", "vec4<f32>"),
    ];
    let actual = wgsl_fields(&src, "GrassUniforms");
    let actual: Vec<(&str, &str)> = actual
        .iter()
        .map(|(n, t)| (n.as_str(), t.as_str()))
        .collect();
    assert_eq!(
        actual, expected,
        "grass.wgsl's GrassUniforms drifted from the Rust block"
    );
    assert_eq!(GrassUniforms::min_size().get(), 256);
    assert!(
        src.contains(&format!("const TILE_M: f32 = {:.1};", cfg::TILE_M)),
        "grass.wgsl's TILE_M is config::grass::TILE_M"
    );
}

/// The benders are the bodies nearest the camera, at most eight, nearest
/// first, to the centimetre, and the texels past the last are 0.
#[test]
fn benders_are_the_bodies_nearest_the_camera() {
    let bodies = (0..12)
        .map(|i| (Vec3::new(i as f32 + 0.004, 0.0, 0.0), -1.0, cfg::BEND_REACH))
        .rev();
    let texels = benders(Vec3::ZERO, bodies);
    assert_eq!(texels[0], Vec4::new(0.0, -1.0, 0.0, cfg::BEND_REACH));
    assert_eq!(texels[7].x, 7.0);
    let one = benders(Vec3::ZERO, std::iter::once((Vec3::X, 0.0, 2.0)));
    assert_eq!(one[0], Vec4::new(1.0, 0.0, 0.0, 2.0));
    assert!(one[1..].iter().all(|t| *t == Vec4::ZERO));
}

/// A body with collider bounds parts the grass over its footprint: a car's
/// box, its underside low; one without stands a person's middle above its
/// feet.
#[test]
fn a_body_parts_the_grass_over_its_footprint() {
    let at = GlobalTransform::from_translation(Vec3::new(5.0, 1.0, 5.0));
    let car = ColliderAabb::from_min_max(Vec3::new(3.0, 0.3, 4.0), Vec3::new(7.4, 1.8, 6.0));
    let (middle, underside, reach) = footprint(&at, Some(&car));
    assert_eq!(middle, Vec3::new(5.2, 1.05, 5.0));
    assert_eq!(underside, 0.3);
    assert!((reach - (2.2 + cfg::BEND_MARGIN)).abs() < 1e-5, "{reach}");
    let person = ColliderAabb::from_min_max(Vec3::new(4.7, 0.0, 4.7), Vec3::new(5.3, 1.8, 5.3));
    assert_eq!(footprint(&at, Some(&person)).2, cfg::BEND_REACH);
    let walker = footprint(&at, None);
    assert_eq!(
        walker,
        (
            Vec3::new(5.0, 1.0, 5.0),
            1.0 - cfg::BODY_DROP,
            cfg::BEND_REACH
        )
    );
    assert_eq!(footprint(&at, Some(&ColliderAabb::INVALID)), walker);
}

/// A flat 129 x 129 ground of 1 m cells (128 m across, -64..64 m), its
/// cover drawn, a camera standing at `eye`, and the grass at `quality`.
fn field_app(quality: GrassQuality, eye: Vec3) -> App {
    let hm = HeightMap::new(129, 129, 1.0);
    let frame = GroundFrame::of(&hm);
    let mut app = App::new();
    app.init_resource::<Assets<Image>>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<GrassMaterial>>()
        .init_resource::<GrassField>()
        .insert_resource(LocalSettings {
            grass_quality: Some(quality),
            ..default()
        })
        .insert_resource(VegetationWind::default())
        .insert_resource(LiveRoomRecord(RoomRecord::default()))
        .add_systems(Update, (sync_grass_material, lay_grass_tiles).chain());
    let mut images = app.world_mut().resource_mut::<Assets<Image>>();
    let (height, weights, cover) = (
        images.add(Image::default()),
        images.add(Image::default()),
        images.add(Image::default()),
    );
    let texel = frame.cover_texel();
    app.insert_resource(GroundCover {
        frame,
        height,
        weights: Some(weights),
        cover,
        mask: CoverMask::open(frame.cover_texels as usize, texel, Vec2::splat(-frame.half)),
        revision: 1,
    })
    .insert_resource(FinishedHeightMap(hm, None));
    app.world_mut()
        .spawn((WorldCamera, GlobalTransform::from_translation(eye)));
    app
}

fn tiles_of(app: &mut App) -> Vec<(IVec2, Detail)> {
    let mut query = app.world_mut().query::<&GrassTile>();
    let mut tiles: Vec<(IVec2, Detail)> = query
        .iter(app.world())
        .map(|t| (t.coord, t.detail))
        .collect();
    tiles.sort_by_key(|(c, _)| (c.y, c.x));
    tiles
}

/// The field lays its tiles around the camera out to its reach, near ones
/// at near detail and far ones at far, and only on the ground.
#[test]
fn the_field_is_laid_around_the_camera() {
    let mut app = field_app(GrassQuality::Medium, Vec3::new(0.0, 1.7, 0.0));
    app.update();
    let field = app.world().resource::<GrassField>();
    assert!(field.material.is_some());
    let tiles = tiles_of(&mut app);
    let grown = GrassQuality::Medium.field().expect("medium grows");
    let reach = grown.reach;
    let eye = Vec3::new(0.0, 1.7, 0.0);
    assert!(!tiles.is_empty());
    for &(coord, detail) in &tiles {
        let nearest = tiles::nearest(eye, coord, 0.0);
        assert!(nearest < reach, "{coord} at {nearest} m");
        assert_eq!(detail, Detail::at(nearest, &grown), "{coord}");
    }
    // The four tiles meeting under the camera are near, and the field
    // reaches every tile within its reach.
    for coord in [
        IVec2::new(-1, -1),
        IVec2::new(0, -1),
        IVec2::new(-1, 0),
        IVec2::ZERO,
    ] {
        assert!(tiles.contains(&(coord, Detail::Near)), "{coord}");
    }
    assert!(tiles.iter().any(|&(_, d)| d == Detail::Far));
    let within = |c: IVec2| tiles::nearest(eye, c, 0.0) < reach;
    let all = (-8..8).flat_map(|z| (-8..8).map(move |x| IVec2::new(x, z)));
    assert_eq!(all.filter(|&c| within(c)).count(), tiles.len());
}

/// Near the ground's edge the field stops at it; turned off it is gone;
/// a camera high above the ground draws none.
#[test]
fn the_field_stays_on_the_ground_and_goes_when_off() {
    let mut app = field_app(GrassQuality::High, Vec3::new(60.0, 1.7, 0.0));
    app.update();
    let tiles = tiles_of(&mut app);
    assert!(!tiles.is_empty());
    assert!(tiles.iter().all(|(c, _)| c.x < 8), "{tiles:?}");

    app.world_mut()
        .resource_mut::<LocalSettings>()
        .grass_quality = Some(GrassQuality::Off);
    app.update();
    app.update();
    assert!(tiles_of(&mut app).is_empty());
    assert!(app.world().resource::<GrassField>().material.is_none());

    let mut app = field_app(GrassQuality::High, Vec3::new(0.0, 200.0, 0.0));
    app.update();
    assert!(tiles_of(&mut app).is_empty());
}

/// How many frames the grass material's assets were written in.
#[derive(Resource, Default)]
struct Writes(usize);

fn count_writes(materials: Res<Assets<GrassMaterial>>, mut writes: ResMut<Writes>) {
    if materials.is_changed() {
        writes.0 += 1;
    }
}

/// The material is written once, and again only when something in it
/// changes - a write rebuilds it - and not for a body walking through the
/// grass, which goes to the bodies' texture.
#[test]
fn the_material_is_written_only_on_a_change() {
    let mut app = field_app(GrassQuality::Low, Vec3::new(0.0, 1.7, 0.0));
    app.init_resource::<Writes>()
        .add_systems(Update, count_writes.after(sync_grass_material));
    let walker = app
        .world_mut()
        .spawn((
            LocalPlayer,
            GlobalTransform::from_translation(Vec3::new(1.0, 0.0, 1.0)),
        ))
        .id();
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(
        app.world().resource::<Writes>().0,
        1,
        "made once, untouched after"
    );
    for step in 1..4 {
        *app.world_mut().get_mut::<GlobalTransform>(walker).unwrap() =
            GlobalTransform::from_translation(Vec3::new(1.0 + step as f32 * 0.3, 0.0, 1.0));
        app.update();
        let bodies = app.world().resource::<GrassField>().bodies;
        assert_eq!(
            bodies[0].x,
            ((1.0 + step as f32 * 0.3) * 100.0f32).round() / 100.0
        );
    }
    assert_eq!(app.world().resource::<Writes>().0, 1, "not for a walker");
    app.world_mut().resource_mut::<VegetationWind>().speed = 9.0;
    app.update();
    app.update();
    assert_eq!(
        app.world().resource::<Writes>().0,
        2,
        "written for the wind"
    );
    let material = app
        .world()
        .resource::<GrassField>()
        .material
        .clone()
        .expect("growing");
    let wind = app
        .world()
        .resource::<Assets<GrassMaterial>>()
        .get(&material)
        .map(|m| m.extension.uniforms.wind.z);
    assert_eq!(wind, Some(9.0));
}

/// A record that changes while its ground is not ready - every room after
/// the login backdrop's lands before its heightmap - is the one the grass
/// grows from once the ground is.
#[test]
fn the_grass_follows_a_record_that_lands_before_its_ground() {
    use crate::pds::{
        Fp3, GeneratorKind, Placement, SovereignGrassTuftConfig, SovereignTextureConfig,
    };
    let mut app = field_app(GrassQuality::Low, Vec3::new(0.0, 1.7, 0.0));
    app.update();
    let before = app
        .world()
        .resource::<GrassField>()
        .profile
        .clone()
        .expect("read");
    app.world_mut().resource_mut::<GroundCover>().revision = 0;
    app.update();
    // The next room: a scatter of red-tipped tufts.
    let mut record = RoomRecord::default();
    let mut kind = GeneratorKind::default_cuboid();
    if let Some(material) = kind.material_mut() {
        material.texture = SovereignTextureConfig::GrassTuft(SovereignGrassTuftConfig {
            color_tip: Fp3([0.9, 0.1, 0.1]),
            ..Default::default()
        });
    }
    record
        .generators
        .insert("red".into(), crate::pds::Generator::from_kind(kind));
    record.placements.push(Placement::Scatter {
        generator_ref: "red".into(),
        bounds: Default::default(),
        count: 100_000,
        local_seed: 0,
        biome_filter: Default::default(),
        snap_to_terrain: true,
        random_yaw: true,
        avoid_urban: false,
        float_on_water: false,
        naturalness: Default::default(),
    });
    app.insert_resource(LiveRoomRecord(record));
    app.update();
    assert!(app.world().resource::<GrassField>().material.is_none());
    app.world_mut().resource_mut::<GroundCover>().revision = 1;
    app.update();
    let field = app.world().resource::<GrassField>();
    assert!(field.material.is_some());
    let after = field.profile.clone().expect("read");
    assert_ne!(after, before);
    assert_eq!(after.tip, [0.9, 0.1, 0.1]);
}
