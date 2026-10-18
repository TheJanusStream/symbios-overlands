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
        ("weather", "vec4<f32>"),
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
    assert_eq!(GrassUniforms::min_size().get(), 272);
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
    field_app_on(129, quality, eye)
}

/// [`field_app`] on a flat ground `vertices` a side of 1 m cells.
fn field_app_on(vertices: usize, quality: GrassQuality, eye: Vec3) -> App {
    let hm = HeightMap::new(vertices, vertices, 1.0);
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
        maxima: None,
        cover,
        mask: CoverMask::open(frame.cover_texels as usize, texel, Vec2::splat(-frame.half)),
        revision: 1,
    })
    .insert_resource(FinishedHeightMap(hm, None));
    app.world_mut()
        .spawn((WorldCamera, GlobalTransform::from_translation(eye)));
    app
}

/// The tiles laid, by where they lie, and their details.
fn tiles_of(app: &mut App) -> Vec<(TileKey, Detail)> {
    let mut query = app.world_mut().query::<&GrassTile>();
    let mut tiles: Vec<(TileKey, Detail)> =
        query.iter(app.world()).map(|t| (t.key, t.detail)).collect();
    tiles.sort_by_key(|(k, _)| (k.level, k.coord.y, k.coord.x));
    tiles
}

/// The 8 m cells the laid tiles hold, each with its tile's detail.
fn cells_of(app: &mut App) -> Vec<(IVec2, Detail)> {
    tiles_of(app)
        .into_iter()
        .flat_map(|(key, detail)| key.cells().map(move |cell| (cell, detail)))
        .collect()
}

/// The field lays its tiles around the camera out to its reach, near ones
/// at near detail and far ones at far, and only on the ground; far off in
/// 16 m tiles, each wholly past where that size begins and wholly on the
/// ground (#1694) - and every 8 m cell within the reach in one tile.
#[test]
fn the_field_is_laid_around_the_camera() {
    let mut app = field_app(GrassQuality::Medium, Vec3::new(0.0, 1.7, 0.0));
    app.update();
    let field = app.world().resource::<GrassField>();
    assert!(field.material.is_some());
    let tiles = tiles_of(&mut app);
    let grown = GrassQuality::Medium.field().expect("medium grows");
    let reach = grown.reach;
    let far = cfg::FAR_AT * grown.full;
    let eye = Vec3::new(0.0, 1.7, 0.0);
    assert!(!tiles.is_empty());
    let mut laid: HashMap<IVec2, usize> = HashMap::new();
    for &(key, detail) in &tiles {
        let nearest = tiles::nearest(eye, key, 0.0);
        assert!(nearest < reach, "{key:?} at {nearest} m");
        assert_eq!(detail, Detail::at(nearest, &grown), "{key:?}");
        if key.level > 0 {
            let from = cfg::LARGE_AT[key.level as usize - 1] * far;
            assert!(nearest >= from, "{key:?} at {nearest} m");
            let (lo, hi) = (key.lo(), key.lo() + key.side());
            assert!(
                lo.min_element() >= -64.0 && hi.max_element() <= 64.0,
                "{key:?}"
            );
        }
        for cell in key.cells() {
            *laid.entry(cell).or_default() += 1;
        }
    }
    assert!(laid.values().all(|&n| n == 1), "a cell laid twice");
    // The four tiles meeting under the camera are near, and the far ground
    // is laid in larger tiles.
    for coord in [
        IVec2::new(-1, -1),
        IVec2::new(0, -1),
        IVec2::new(-1, 0),
        IVec2::ZERO,
    ] {
        assert!(
            tiles.contains(&(TileKey::small(coord), Detail::Near)),
            "{coord}"
        );
    }
    assert!(tiles.iter().any(|&(_, d)| d == Detail::Far));
    assert!(tiles.iter().any(|(k, _)| k.level == 1));
    // The field reaches every cell within its reach.
    let within = |c: IVec2| tiles::nearest(eye, TileKey::small(c), 0.0) < reach;
    let all = (-8..8).flat_map(|z| (-8..8).map(move |x| IVec2::new(x, z)));
    for cell in all.filter(|&c| within(c)) {
        assert!(laid.contains_key(&cell), "{cell} within the reach");
    }
}

/// A larger tile is laid only wholly on the ground (#1694): on a ground whose
/// edge cuts through the larger tiles' grid - 120 m across, -60..60 m - no
/// 16 or 32 m tile crosses it, the 8 m cells across it are laid as 8 m
/// tiles, and every cell on the ground within the reach is laid once.
#[test]
fn a_large_tile_is_laid_only_wholly_on_the_ground() {
    let eye = Vec3::new(40.0, 1.7, 0.0);
    let mut app = field_app_on(121, GrassQuality::High, eye);
    app.update();
    let tiles = tiles_of(&mut app);
    assert!(tiles.iter().any(|(k, _)| k.level > 0), "{tiles:?}");
    let mut laid: HashMap<IVec2, usize> = HashMap::new();
    for &(key, _) in &tiles {
        if key.level > 0 {
            let (lo, hi) = (key.lo(), key.lo() + key.side());
            assert!(
                lo.min_element() >= -60.0 && hi.max_element() <= 60.0,
                "{key:?}"
            );
        }
        for cell in key.cells() {
            *laid.entry(cell).or_default() += 1;
        }
    }
    assert!(laid.values().all(|&n| n == 1), "a cell laid twice");
    let mut across = 0;
    for z in -8..8 {
        for x in -8..8 {
            let cell = IVec2::new(x, z);
            let (lo, hi) = (
                cell.as_vec2() * cfg::TILE_M,
                (cell + 1).as_vec2() * cfg::TILE_M,
            );
            let on_ground = lo.max_element() < 60.0 && hi.min_element() > -60.0;
            let within = tiles::nearest(eye, TileKey::small(cell), 0.0) < cfg::HIGH.reach;
            assert_eq!(laid.contains_key(&cell), on_ground && within, "{cell}");
            if on_ground && (lo.min_element() < -60.0 || hi.max_element() > 60.0) {
                assert!(
                    tiles.iter().any(|(k, _)| *k == TileKey::small(cell)),
                    "{cell} across the edge"
                );
                across += 1;
            }
        }
    }
    assert!(across > 0);
}

/// The field counts what it draws (#1692): the tiles a view sees, by
/// detail, and the vertices their meshes hold - a mesh holds the blades
/// ranked below its share, and a blade of `n` segments is `2n + 1`
/// vertices.
#[test]
fn the_field_counts_what_it_draws() {
    let mut app = field_app(GrassQuality::High, Vec3::new(0.0, 1.7, 0.0));
    // What the renderer's plugins give every mesh in the game: its
    // visibility, and so a `ViewVisibility` to count by.
    app.register_required_components::<Mesh3d, Visibility>();
    app.init_resource::<GrassDrawn>()
        .add_systems(Update, count_grass_drawn.after(lay_grass_tiles));
    app.update();
    let blades = tiles::blade_count(cfg::HIGH.density);
    let vertices: HashMap<(Detail, usize, u32), usize> = app
        .world()
        .resource::<GrassField>()
        .meshes
        .iter()
        .map(|(&key, &(_, vertices))| (key, vertices))
        .collect();
    assert!(!vertices.is_empty());
    assert!(vertices.keys().any(|&(_, _, level)| level > 0));
    for (&(detail, share, level), &count) in &vertices {
        let kept = tiles::blades_below(blades, tiles::SHARES[share]);
        let segments = detail.segments() as usize;
        let cells = 1 << (2 * level);
        assert_eq!(
            count,
            cells * kept * (2 * segments + 1),
            "{detail:?} {share} {level}"
        );
    }
    // No view has seen a tile yet.
    assert_eq!(*app.world().resource::<GrassDrawn>(), GrassDrawn::default());

    // A view sees every other tile.
    let mut expected = GrassDrawn::default();
    let mut query = app.world_mut().query::<(&GrassTile, &mut ViewVisibility)>();
    for (i, (tile, mut seen)) in query.iter_mut(app.world_mut()).enumerate() {
        if i % 2 == 0 {
            *seen = ViewVisibility::VISIBLE;
            expected.tiles[tile.detail.index()] += 1;
            expected.vertices += vertices[&(tile.detail, tile.share, tile.key.level)] as u64;
        }
    }
    assert!(expected.tiles.iter().all(|&n| n > 0), "{expected:?}");
    app.update();
    assert_eq!(*app.world().resource::<GrassDrawn>(), expected);
}

/// A tile holds the fewest blades that hold every blade the shader can grow
/// in it (#1693): all of them under the camera, fewer the further it is as
/// the field thins, never fewer than the thinning at its nearest point
/// keeps, and never more than its detail's whole share.
#[test]
fn a_tile_holds_the_fewest_blades_that_grow_in_it() {
    let eye = Vec3::new(0.0, 1.7, 0.0);
    let mut app = field_app(GrassQuality::High, eye);
    app.update();
    let grown = cfg::HIGH;
    let mut query = app.world_mut().query::<&GrassTile>();
    let laid: Vec<(TileKey, Detail, usize)> = query
        .iter(app.world())
        .map(|t| (t.key, t.detail, t.share))
        .collect();
    assert!(!laid.is_empty());
    let world = app.world();
    let ground = world.resource::<GroundCover>();
    let profile = world.resource::<GrassField>().profile.as_ref();
    let mut thinned = false;
    for (key, detail, share) in laid {
        let nearest = tiles::nearest(eye, key, 0.0);
        // The ground is open: the most is its layers' and their clumps',
        // and the share follows it by the thinning, held to the detail's.
        let most = tile_density(ground, profile, key);
        assert!(most > 0.0, "{key:?}");
        let need = (most * tiles::thinning(nearest, &grown)).min(detail.share());
        let held = tiles::SHARES[share];
        assert!(held >= need - 1e-6, "{key:?}: {held} under {need}");
        assert!(
            held <= detail.share() + 1e-6,
            "{key:?}: {held} over {detail:?}"
        );
        if share + 1 < tiles::SHARES.len() {
            assert!(
                tiles::SHARES[share + 1] < need,
                "{key:?}: {held} is not the fewest for {need}"
            );
        }
        thinned |= held < detail.share();
    }
    assert!(thinned, "no tile drew fewer than its detail's whole share");
}

/// The tiles behind a ridge are laid hidden and those before it shown
/// (#1693): a ridge 12 m thick and 10 m high stands between the camera and
/// the field's far side.
#[test]
fn a_ridge_hides_the_tiles_behind_it() {
    let eye = Vec3::new(-60.0, 1.7, 0.0);
    let mut app = field_app(GrassQuality::High, eye);
    app.init_resource::<GrassDrawn>()
        .add_systems(Update, count_grass_drawn.after(lay_grass_tiles));
    let mut hm = HeightMap::new(129, 129, 1.0);
    for z in 0..129 {
        for x in 0..129 {
            if (-20.0..=-8.0).contains(&(x as f32 - 64.0)) {
                hm.set(x, z, 10.0);
            }
        }
    }
    app.insert_resource(FinishedHeightMap(hm, None));
    app.update();
    let mut query = app.world_mut().query::<(&GrassTile, &Visibility)>();
    let (mut hidden, mut shown, mut laid_hidden) = (0, 0, 0);
    for (tile, seen) in query.iter(app.world()) {
        let west = tile.key.lo().x;
        if west >= 0.0 {
            assert_eq!(*seen, Visibility::Hidden, "{:?} behind the ridge", tile.key);
            hidden += 1;
        }
        if west + tile.key.side() <= -40.0 {
            assert_eq!(*seen, Visibility::Inherited, "{:?} before it", tile.key);
            shown += 1;
        }
        laid_hidden += u32::from(*seen == Visibility::Hidden);
    }
    assert!(hidden > 0 && shown > 0, "{hidden} hidden, {shown} shown");
    // The count says as much.
    assert_eq!(app.world().resource::<GrassDrawn>().hidden, laid_hidden);
}

/// Ground the weights give no layer lays no tile (#1693): the ground's
/// maxima, read from a weight map that weighs the west and leaves the east
/// bare, keep the field to the west - a tile a texel or two past the edge
/// of the weighted ground still laid, for what the sampling there blends.
#[test]
fn no_tile_is_laid_where_no_layer_lies() {
    let eye = Vec3::new(0.0, 1.7, 0.0);
    let mut app = field_app(GrassQuality::High, eye);
    // 129 x 129 weight texels over the 128 m ground: weighted west of
    // x = 16 m, nothing from it east.
    let texels: Vec<[u8; 4]> = (0..129 * 129)
        .map(|i| {
            let x = (i % 129) as f32 - 64.0;
            if x < 16.0 { [255, 0, 0, 0] } else { [0; 4] }
        })
        .collect();
    let maxima = std::sync::Arc::new(crate::ground_cover::WeightMaxima::of(
        &texels, 129, 129, 128.0,
    ));
    app.world_mut().resource_mut::<GroundCover>().maxima = Some(maxima);
    app.update();
    // Cell 2 (16..24 m) samples bare ground, but its padding reaches the
    // last weighted texel (x = 15 m); cell 3 reads nothing, so no tile
    // starts past cell 2 - a 16 m one holding cells 2 and 3 grows in 2.
    let laid = cells_of(&mut app);
    assert!(laid.iter().any(|(c, _)| c.x == 2), "{laid:?}");
    assert!(laid.iter().any(|(c, _)| c.x < -2));
    let tiles = tiles_of(&mut app);
    assert!(tiles.iter().all(|(k, _)| k.lo().x <= 16.0), "{tiles:?}");
}

/// Ground where nothing can grow lays no tile (#1693): a road's or a
/// building's cover blocks it, and a tile it blocks wholly - a texel clear
/// of open ground - is not laid; the rest of the field is.
#[test]
fn no_tile_is_laid_where_nothing_grows() {
    let eye = Vec3::new(0.0, 1.7, 0.0);
    let mut app = field_app(GrassQuality::High, eye);
    // Block the ground from 16 m east on: tiles 2.. on X.
    let mut ground = app.world_mut().resource_mut::<GroundCover>();
    ground.mask.block_convex(&[
        Vec2::new(16.0, -64.0),
        Vec2::new(64.0, -64.0),
        Vec2::new(64.0, 64.0),
        Vec2::new(16.0, 64.0),
    ]);
    ground.revision += 1;
    app.update();
    let laid = cells_of(&mut app);
    assert!(!laid.is_empty());
    assert!(laid.iter().any(|(c, _)| c.x == 1));
    assert!(laid.iter().any(|(c, _)| c.x < -2));
    // A tile whose square starts a texel past the open ground is gone.
    let tiles = tiles_of(&mut app);
    assert!(tiles.iter().all(|(k, _)| k.lo().x <= 16.0), "{tiles:?}");
}

/// Near the ground's edge the field stops at it; turned off it is gone;
/// a camera high above the ground draws none.
#[test]
fn the_field_stays_on_the_ground_and_goes_when_off() {
    let mut app = field_app(GrassQuality::High, Vec3::new(60.0, 1.7, 0.0));
    app.update();
    let cells = cells_of(&mut app);
    assert!(!cells.is_empty());
    assert!(cells.iter().all(|(c, _)| c.x < 8), "{cells:?}");

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

/// The grass varies with the ground under it (#1676) and weathers with it
/// (#1683), and the ground shows it (#1695): both shaders take the drift,
/// the mottle, the variation, the settled snow, the puddles, the grass's
/// clumps, what the ground shows of it and its gusts from
/// `ground_common.wgsl`, call them in their own code - an import alone
/// compiles to nothing - and keep no copy of their own that could drift
/// from the other's: the drier grass would no longer stand on the lighter
/// ground, a puddle would keep its blades, and the far blades would fade
/// into a ground of another tone, snow or grass.
#[test]
fn the_grass_and_the_ground_read_one_field() {
    let read = |name: &str| {
        std::fs::read_to_string(format!(
            "{}/assets/shaders/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap_or_else(|_| panic!("{name} is a tracked asset"))
    };
    let common = read("ground_common.wgsl");
    let fields = [
        "ground_drift",
        "ground_mottle",
        "ground_vary",
        "snow_settled",
        "puddle_at",
        // The grass on the ground (#1695): what the ground shows of it is
        // what the far blades fade into.
        "grass_density_seen",
        "carpet_cover",
        "carpet_colour",
        "carpet_normal",
        "carpet_shade",
        "gust_fineness",
        "grass_gust",
    ];
    for field in fields.iter().chain(&["ground_tint"]) {
        let definition = format!("fn {field}(");
        assert_eq!(
            common.matches(&definition).count(),
            1,
            "ground_common.wgsl defines {field}"
        );
    }
    for shader in ["grass.wgsl", "splat.wgsl"] {
        let src = read(shader);
        let import = src
            .lines()
            .find(|l| l.starts_with("#import \"shaders/ground_common.wgsl\""))
            .unwrap_or_else(|| panic!("{shader} imports ground_common.wgsl"));
        for field in fields {
            assert!(import.contains(field), "{shader} takes {field} from it");
            let call = format!("{field}(");
            assert!(
                src.lines()
                    .filter(|l| !l.starts_with("#import") && !l.trim_start().starts_with("//"))
                    .any(|l| l.contains(&call)),
                "{shader} calls {field}"
            );
            assert!(
                !src.contains(&format!("fn {field}(")),
                "{shader} keeps no copy of {field}"
            );
        }
    }
}

/// The ground shows its grass as the blades are (#1695): the terrain's
/// shader holds the blades' roughness, dry and soaked, and the water margin
/// the cover grows no grass within - which a test can only pin here, the
/// shader being read at runtime. And the ground's gust light leans no
/// further than a blade can: its tip nine tenths of its height over.
#[test]
fn the_ground_shows_the_grass_as_its_blades_are() {
    let read = |name: &str| {
        std::fs::read_to_string(format!(
            "{}/assets/shaders/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap_or_else(|_| panic!("{name} is a tracked asset"))
    };
    let splat = read("splat.wgsl");
    for line in [
        format!(
            "const GRASS_ROUGHNESS: f32 = {:?};",
            material::grass_base().perceptual_roughness
        ),
        format!("const GRASS_WET_ROUGHNESS: f32 = {:?};", cfg::WET_ROUGHNESS),
        format!(
            "const GRASS_WATER_MARGIN: f32 = {:?};",
            crate::config::terrain::ground_cover::WATER_MARGIN
        ),
    ] {
        assert!(splat.contains(&line), "splat.wgsl: {line}");
    }
    let common = read("ground_common.wgsl");
    let lean_most: f32 = common
        .lines()
        .find_map(|l| l.strip_prefix("const LEAN_MOST: f32 = "))
        .and_then(|v| v.trim_end_matches(';').parse().ok())
        .expect("ground_common.wgsl's LEAN_MOST");
    assert!(
        lean_most <= 0.9 * cfg::BLADE_HEIGHT / cfg::WIND_LEAN,
        "{lean_most}"
    );
    // A steady wind at its strongest leans a swell no further.
    assert!(lean_most >= 2.5, "{lean_most}");
}

/// The room's grass is laid on the ground (#1695) - its colours, where it
/// grows and the wind it leans in, as the blades read them - and taken off
/// it when the grass is turned off.
#[test]
fn the_grass_is_laid_on_the_ground_while_it_grows() {
    let mut app = field_app(GrassQuality::Low, Vec3::new(0.0, 1.7, 0.0));
    app.init_resource::<crate::terrain::GroundGrass>()
        .add_systems(Update, show_grass_on_the_ground.after(sync_grass_material));
    app.update();
    let profile = app
        .world()
        .resource::<GrassField>()
        .profile
        .clone()
        .expect("read");
    let wind = *app.world().resource::<VegetationWind>();
    let laid = *app.world().resource::<crate::terrain::GroundGrass>();
    assert_eq!(laid, ground_grass(&profile, &wind));
    assert_eq!(laid.carpet, 1.0);
    assert_eq!(laid.tip, Vec3::from_array(profile.tip));
    assert_eq!(laid.layer_density, Vec4::from_array(profile.layer_density));
    app.world_mut()
        .resource_mut::<LocalSettings>()
        .grass_quality = Some(GrassQuality::Off);
    app.update();
    assert_eq!(
        *app.world().resource::<crate::terrain::GroundGrass>(),
        crate::terrain::GroundGrass::default()
    );
}
