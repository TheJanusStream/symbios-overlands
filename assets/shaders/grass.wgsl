// Vertex shader for the GPU grass (#1675).
//
// Every tile draws one of a few shared meshes (see `src/grass/tiles.rs`): a
// vertex carries where its blade stands in its 8 m cell and how far up the
// blade it is (POSITION), and which edge it is on, the blade's rank, its
// seed, its cell and its mesh's detail (COLOR).
// This shader does the rest, per vertex:
//
//   1. Turns and mirrors the tile's layout by a hash of where the tile
//      lies, so the 8 m tiles do not read as a grid. A 16 or 32 m tile
//      (#1694) is its 8 m cells side by side, each laid out as an 8 m tile
//      there would be, so the field draws the same in tiles of any size.
//   2. Stands the blade on the drawn ground: the height texture's cell
//      corners, interpolated over the mesher's two triangles a cell, as
//      `ground_cover::mesh_height` does - not bilinearly, which would
//      float a blade or sink it wherever the cell is not flat.
//   3. Grows it as thickly as the ground there grows grass: the splat
//      weights against the room's per-layer density, times the cover (no
//      grass under a building, a road or water), times the thinning with
//      distance. A blade is kept while its rank is below that; one ranked
//      just below it grows short, so a blade crossing the line shrinks
//      away rather than popping.
//   4. Shapes it - height, width, its own droop, the wind's lean (harder in
//      a storm's gust front, `storm_common.wgsl`, #1684), a body's push, its
//      tussock's height, facing and lean (#1696), and a short near blade
//      folded into a V, two for one (#1696) - and colours it: root to tip,
//      a dry share, its tussock's shade, a flower share whose heads
//      take the room's flower colours, and the ground's own tone fields
//      (`ground_common.wgsl`): drier where the ground drifts lighter, a
//      little taller or shorter patch by patch. Before its tile turns to
//      fewer segments a blade, it takes their shape (#1694), so the turn
//      changes nothing drawn; and each blade fades out at the field's edge
//      at a distance of its own, so the edge is no circle.
//   5. Weathers it as the ground under it is weathered (#1683), from the
//      same fields: snow buries most of a blade and whitens what stands
//      out, a puddle drowns it, and wet blades darken.
//   6. Fades it, further out, into the ground under it as the ground's
//      shader draws it (#1695): the ground's colour under the grass it
//      shows, lit as the ground there is - so the field has no edge to see.
//
// The fragment is Bevy's PBR one, which reads the colour written here as
// the base colour (`VERTEX_COLORS`, set because the mesh carries COLOR),
// so the blades are lit and fogged as every other surface is.
//
// WebGL2-safe: vertex-stage texture reads (16 vertex texture units in
// every WebGL2), the height (an integer texture, #1695) and the 32-bit
// float bodies read by `textureLoad` (WebGL2 filters neither), no storage
// buffers. Mirrors of the Rust side, which nothing checks at build time:
// `GrassUniforms` (`src/grass/material.rs`, pinned by
// `grass::tests::wgsl_block_mirrors_the_rust_one`) and the constants below
// that name their Rust source (pinned by the same test, and by
// `grass::tiles`'s).

#import bevy_pbr::{
    forward_io::VertexOutput,
    mesh_functions,
    mesh_view_bindings::{globals, view},
    view_transformations::position_world_to_clip,
}
#import "shaders/ground_common.wgsl"::{hash21, ground_drift, ground_mottle, ground_vary, snow_settled, puddle_at, grass_density, grass_density_seen, carpet_cover, carpet_colour, carpet_normal, carpet_shade, gust_fineness, grass_gust, grass_lean, BLADE_SKYWARD, SNOW_ALBEDO, WET_DARKEN}
#import "shaders/storm_common.wgsl"::storm_gust

struct GrassUniforms {
    base: vec4<f32>,
    tip: vec4<f32>,
    dry: vec4<f32>,
    flowers: array<vec4<f32>, 4>,
    layer_density: vec4<f32>,
    ground: array<vec4<f32>, 4>,
    frame: vec4<f32>,
    wind: vec4<f32>,
    reach: vec4<f32>,
    shares: vec4<f32>,
    // The weather on the ground (#1683): how wet, how much snow lies where
    // it has settled, the snow line (world Y) and its edge (m).
    weather: vec4<f32>,
    // The world's grass's shape (#1702): how much it grows in tussocks and
    // clumps (x, 1 unset) and how much of the ground it hides (y, the
    // ground's `carpet_depth`).
    shape: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> grass: GrassUniforms;
// Each heightmap vertex's height (its bits in r) and the terrain mesh's
// normal there (x and z as snorms in g): `ground_cover::height`.
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var height_map: texture_2d<u32>;
// The ground's weight map, two layers: the weights, then how sheltered the
// ground is from what falls in the second's red (#1685).
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var weight_map: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var weight_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var cover_map: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var cover_sampler: sampler;
// The bodies that part the grass, a texel each, nearest the camera first:
// middle on XZ, underside on Y, reach in W; 0 past the last.
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var bender_map: texture_2d<f32>;

// A tile's side (m): `config::grass::TILE_M`. A larger tile's 8 m cells are
// numbered `i + CELLS_A_ROW * j` along X and Z (`tiles::CELLS_A_ROW`), and a
// blade carries its cell's number plus `CELLS` times its mesh's detail
// (`tiles::Detail::index`: 0 near).
const TILE_M: f32 = 8.0;
const CELLS_A_ROW: u32 = 4u;
const CELLS: u32 = 16u;
// How far the wind leans a blade's tip at its default speed (m) in a world
// whose grass sways as it does unset: `config::grass::WIND_LEAN`. The
// world's own lean, `grass.wind.w`, is this times its sway (#1700), which
// the ground's gusts take as a share.
const WIND_LEAN: f32 = 0.12;
// A blade takes the shape of the fewer segments it is about to be drawn with
// from this share of the way to where its tile turns to them (#1694), so a
// tile turning changes no blade's look; a near or a middle blade's upper
// pair stands this far up it (`tiles::Detail::rungs`).
const MORPH_FROM: f32 = 0.85;
const UPPER_RUNG: f32 = 2.0 / 3.0;
// Each blade fades out at the field's edge as though it stood up to this
// share further off than it does (#1694): the edge is a ragged band inside
// the reach, not a circle every blade shrinks at alike.
const EDGE_FEATHER: f32 = 0.12;
// Under settled snow (#1683): the share of a blade's height the snow buries,
// and how white the snow leaves its tip.
const BURIED: f32 = 0.75;
const SNOWY_TIPS: f32 = 0.85;
// The snow on what stands out of it at the blade's foot, under its tip's.
const SNOWY_FOOT: f32 = 0.6;
// Tussocks (#1696, Ghost of Tsushima's clumps): blades grow in tussocks about
// `TUSSOCK_M` across - each blade in the tussock of the nearest of points
// jittered about a grid that far apart - each up to `TUSSOCK_HEIGHT` taller
// or shorter than the next and a shade (`TUSSOCK_TINT`) lighter or darker,
// its blades shorter toward its rim (by `TUSSOCK_RIM` from its middle to
// its rim, the mean kept at `TUSSOCK_RIM_MEAN`, how far out the average
// blade stands from its tussock's middle), facing mostly as their tussock
// faces (`TUSSOCK_FACING` of the way) and leaning out from its middle at
// the rim (`TUSSOCK_SPLAY` of their height). All of it as much as the
// world's grass clumps (`grass.shape.x`, #1702: 0 none, 2 twice - the facing
// at most all the way, past which it would turn the blades back). Past twice
// the field's full distance they fade, by three times it gone - their
// blades too small there to tell - and none is looked for.
// `tiles::BladeBounds::tallest` holds the tallest blade they make.
const TUSSOCK_M: f32 = 1.6;
const TUSSOCK_HEIGHT: f32 = 0.15;
const TUSSOCK_TINT: f32 = 0.1;
const TUSSOCK_RIM: f32 = 0.25;
const TUSSOCK_RIM_MEAN: f32 = 0.43;
const TUSSOCK_FACING: f32 = 0.65;
const TUSSOCK_SPLAY: f32 = 0.3;
// A short near blade folds into a V (#1696, Outerra): its strip of three
// segments drawn as two blades from one foot - the third-way pair down at
// the root, the first segment one arm and the other two the other, each
// arm `FOLD_ARM` of the blade's height and leaning `FOLD_SPLAY` of the
// arm's out to its side - so short grass, which covers the ground the
// least, shows twice the blades for the same vertices. A blade under
// `FOLD_SHORT` of the room's height folds - its height before the field
// thins it never changes, so a blade is folded or not - and unfolds before
// its tile turns to fewer segments, which no folded strip can be drawn
// with: only a near mesh's blades fold.
const FOLD_SHORT: f32 = 0.7;
const FOLD_ARM: f32 = 0.8;
const FOLD_SPLAY: f32 = 0.5;
// The least width (m) of a blade a metre away, and in step further: about
// a pixel and a half on a 1080-pixel-tall screen at Bevy's default 45
// degree field of view. A blade narrower than a pixel shimmers as it sways.
const MIN_WIDTH_PER_M: f32 = 0.0012;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    // The mesh's COLOR attribute: edge, rank, seed, and the blade's 8 m
    // cell in its tile with its mesh's detail (see `CELLS`).
    @location(5) blade: vec4<f32>,
};

// A blade, as every vertex of it is drawn from (`blade_point`): its root,
// its bend and how high its tip stands, the way across it, its width, and
// whether it is a flower (1 or 0); the eye; how far it has merged into the
// ground it stands on, and the colour and normal that ground shows
// (`ground_common::carpet_normal`); and what colours it -
// whether it is a dry one, the ground's tone drift where it stands, its own
// shade, the density of the grass there, a flower's head colour, and the
// weather: how wet, how much snow settled.
struct Blade {
    root: vec3<f32>,
    bend: vec3<f32>,
    tip_y: f32,
    across: vec3<f32>,
    width: f32,
    flower: f32,
    eye: vec3<f32>,
    merge: f32,
    far_ground: vec3<f32>,
    far_normal: vec3<f32>,
    dry: f32,
    drift: f32,
    shade: f32,
    density: f32,
    head_colour: vec3<f32>,
    wet: f32,
    settled: f32,
};

// A point of a blade: where it stands, its normal and its colour.
struct BladePoint {
    position: vec3<f32>,
    normal: vec3<f32>,
    colour: vec3<f32>,
};

// A heightmap vertex's height and normal from its texel.
fn ground_vertex(texel: vec4<u32>) -> vec4<f32> {
    let xz = unpack2x16snorm(texel.g);
    return vec4<f32>(bitcast<f32>(texel.r), xz.x, sqrt(max(1.0 - dot(xz, xz), 0.0)), xz.y);
}

// The drawn ground at world `xz`: its height (x) and normal (yzw). The
// cell's corners, over the mesher's two triangles, split from the (x + 1,
// z) corner to the (x, z + 1) one - the corners' normals too, as the
// rasterizer interpolates the terrain mesh's across its triangles, so a
// blade reads the light the ground beside it has. Past the edge, the
// edge's.
fn ground_point(xz: vec2<f32>) -> vec4<f32> {
    let last = grass.frame.z - 1.0;
    let g = clamp((xz + vec2<f32>(grass.frame.x)) / grass.frame.y, vec2<f32>(0.0), vec2<f32>(last));
    let c = min(floor(g), vec2<f32>(last - 1.0));
    let f = g - c;
    let i = vec2<i32>(c);
    let tl = ground_vertex(textureLoad(height_map, i, 0));
    let tr = ground_vertex(textureLoad(height_map, i + vec2<i32>(1, 0), 0));
    let bl = ground_vertex(textureLoad(height_map, i + vec2<i32>(0, 1), 0));
    let br = ground_vertex(textureLoad(height_map, i + vec2<i32>(1, 1), 0));
    var point: vec4<f32>;
    if f.x + f.y <= 1.0 {
        point = tl + (tr - tl) * f.x + (bl - tl) * f.y;
    } else {
        point = br + (bl - br) * (1.0 - f.x) + (tr - br) * (1.0 - f.y);
    }
    return vec4<f32>(point.x, normalize(point.yzw));
}

// The share of a near tile's blades kept `distance` metres away: all of
// them out to `reach.x`, the middle share by `reach.y`, the far share by
// `reach.z`, and past that falling as the square of the distance. A tile
// drawn at middle or far detail lies wholly past the distance where this
// reaches its share (`tiles::Detail::at`), so it misses no blade this
// keeps.
fn thinning(distance: f32) -> f32 {
    if distance < grass.reach.y {
        return mix(1.0, grass.shares.x, smoothstep(grass.reach.x, grass.reach.y, distance));
    }
    if distance < grass.reach.z {
        return mix(grass.shares.x, grass.shares.y, smoothstep(grass.reach.y, grass.reach.z, distance));
    }
    // Past it, as the square of the distance (#1694): as many blades a
    // patch of screen however far it lies (`tiles::thinning`).
    let past = grass.reach.z / distance;
    return grass.shares.y * past * past;
}

// The point of `blade` `up` of the way up it, on edge `side` (0 the tip).
fn blade_point(blade: Blade, up: f32, side: f32) -> BladePoint {
    // A flower's head is its top: the last pair of its stalk raised and
    // widened into a small cup, under the tip.
    let head = blade.flower > 0.5 && up > 0.45;
    var along = up;
    var half_width = 0.5 * blade.width * pow(1.0 - up, 0.8);
    if head && side != 0.0 {
        along = mix(up, 1.0, 0.6);
        half_width = 0.65 * blade.width;
    }
    let centre = blade.root + blade.bend * (along * along) + vec3<f32>(0.0, blade.tip_y * along, 0.0);
    let position = centre + blade.across * side * half_width;

    // A normal across the blade, toward the camera, leaning to the sky: a
    // field reads as one surface lit from above, with each blade's own
    // turn showing through.
    let tangent = normalize(2.0 * along * blade.bend + vec3<f32>(0.0, max(blade.tip_y, 1.0e-3), 0.0));
    var normal = normalize(cross(blade.across, tangent) + vec3<f32>(0.0, 1.0e-4, 0.0));
    normal = normal * select(1.0, -1.0, dot(normal, blade.eye - position) < 0.0);
    normal = normalize(mix(normal, vec3<f32>(0.0, 1.0, 0.0), BLADE_SKYWARD) + blade.across * side * 0.2);

    // Root to tip, a dry share, patches of drier grass, a little of each
    // blade's own, shade at the root deepest where the grass is thickest, a
    // flower's head, and the weather.
    var colour = mix(grass.base.rgb, grass.tip.rgb, pow(up, 0.8));
    if blade.dry > 0.5 {
        colour = mix(grass.base.rgb, grass.dry.rgb, smoothstep(0.0, 0.6, up));
    }
    colour = mix(colour, grass.dry.rgb, 0.4 * smoothstep(0.55, 0.9, blade.drift) * up);
    colour = colour * blade.shade * mix(1.0 - 0.6 * blade.density, 1.0, up);
    if head {
        colour = blade.head_colour;
    }
    colour = mix(
        colour * mix(1.0, WET_DARKEN, blade.wet),
        SNOW_ALBEDO,
        blade.settled * mix(SNOWY_FOOT, SNOWY_TIPS, up),
    );

    // Further out the blade takes the colour of the ground it stands on,
    // and its normal (`Blade::merge`).
    return BladePoint(
        position,
        normalize(mix(normal, blade.far_normal, blade.merge)),
        mix(colour, blade.far_ground, blade.merge),
    );
}

// The point `t` of the way from `a` to `b`, as a triangle between them
// interpolates it: normals too, unnormalised.
fn between(a: BladePoint, b: BladePoint, t: f32) -> BladePoint {
    return BladePoint(
        mix(a.position, b.position, t),
        mix(a.normal, b.normal, t),
        mix(a.colour, b.colour, t),
    );
}

// A vertex that draws nothing: on one point outside the view - right of it
// on every backend's clip space (x > w) - with none of a blade's work done.
fn nothing(instance_index: u32, world_from_local: mat4x4<f32>, root: vec3<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.world_position = vec4<f32>(root, 1.0);
    out.position = vec4<f32>(2.0, 2.0, 0.5, 1.0);
    out.world_normal = vec3<f32>(0.0, 1.0, 0.0);
#ifdef VERTEX_COLORS
    out.color = vec4<f32>(0.0, 0.0, 0.0, 1.0);
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        instance_index, world_from_local[3]);
#endif
    return out;
}

// Two numbers in [0, 1) for `p`: David Hoskins' sine-free hash.
fn hash22(p: vec2<f32>) -> vec2<f32> {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * vec3<f32>(0.1031, 0.1030, 0.0973));
    p3 = p3 + dot(p3, p3.yzx + 33.33);
    return fract((p3.xx + p3.yz) * p3.zy);
}

// The tussock a blade at world `xz` grows in, seen from `distance` metres:
// the way from its middle out to the blade (level, unit), how far out
// (`TUSSOCK_M`s, 0 in the middle), its own numbers in [0, 1) - two for its
// height and facing, one for its shade - and how much of it shows: none
// past three times the field's full distance, where nothing is looked for
// and the blade grows plain.
struct Tussock {
    outward: vec2<f32>,
    rim: f32,
    own: vec2<f32>,
    shade: f32,
    shows: f32,
};

fn grass_tussock(xz: vec2<f32>, distance: f32) -> Tussock {
    // As much as the world's grass clumps (#1702): none in an even lawn,
    // which looks for no tussock at all.
    let shows = (1.0 - smoothstep(2.0 * grass.reach.x, 3.0 * grass.reach.x, distance)) * grass.shape.x;
    if shows <= 0.0 {
        return Tussock(vec2<f32>(1.0, 0.0), TUSSOCK_RIM_MEAN, vec2<f32>(0.5), 0.5, 0.0);
    }
    let cell = floor(xz / TUSSOCK_M);
    var nearest = 1.0e9;
    var middle = xz;
    for (var j = -1; j <= 1; j = j + 1) {
        for (var i = -1; i <= 1; i = i + 1) {
            let c = cell + vec2<f32>(f32(i), f32(j));
            let point = (c + hash22(c)) * TUSSOCK_M;
            let d = dot(xz - point, xz - point);
            if d < nearest {
                nearest = d;
                middle = point;
            }
        }
    }
    let far = sqrt(nearest);
    return Tussock(
        (xz - middle) / max(far, 1.0e-3),
        far / TUSSOCK_M,
        hash22(middle * 0.731 + vec2<f32>(17.0, 3.0)),
        hash21(middle * 1.37 + vec2<f32>(5.1, 11.7)),
        shows,
    );
}

// A tile's layout turned and mirrored one of eight ways.
fn tile_layout(at: vec2<f32>, which: u32) -> vec2<f32> {
    var p = at;
    if (which & 1u) != 0u {
        p.x = TILE_M - p.x;
    }
    if (which & 2u) != 0u {
        p.y = TILE_M - p.y;
    }
    if (which & 4u) != 0u {
        p = p.yx;
    }
    return p;
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    // The blade's 8 m cell, whose corner turns its layout as an 8 m tile's
    // would: a larger tile is its cells side by side (#1694).
    let packed = u32(vertex.blade.w + 0.5);
    let in_tile = packed % CELLS;
    let near_mesh = packed / CELLS == 0u;
    let corner = world_from_local[3].xz
        + vec2<f32>(f32(in_tile % CELLS_A_ROW), f32(in_tile / CELLS_A_ROW)) * TILE_M;
    let which = u32(hash21(floor(corner / TILE_M + 0.5) + 0.37) * 8.0) & 7u;
    let at = corner + tile_layout(vertex.position.xz, which);
    let up = vertex.position.y;
    let side = vertex.blade.x;
    let rank = vertex.blade.y;
    let seed = vertex.blade.z;
    let eye = view.world_position;

    // Past the field's reach across the ground alone - a large tile's far
    // cells - a blade draws nothing, before a texture is read for it.
    if length(eye.xz - at) >= grass.reach.w {
        return nothing(vertex.instance_index, world_from_local, vec3<f32>(at.x, eye.y, at.y));
    }

    let drawn = ground_point(at);
    let ground = drawn.x;
    let ground_normal = drawn.yzw;
    let root = vec3<f32>(at.x, ground, at.y);
    let distance = length(eye - root);

    // How thickly the ground here grows grass: the layers' densities by
    // their weights, sampled where the ground's shader samples them.
    let uv = (at + vec2<f32>(grass.frame.x)) / (2.0 * grass.frame.x);
    let inside = select(0.0, 1.0, all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0)));
    let weights = textureSampleLevel(weight_map, weight_sampler, uv, 0, 0.0);
    // Under a roof what falls does not reach it (#1685), as on the ground.
    let open = 1.0 - textureSampleLevel(weight_map, weight_sampler, uv, 1, 0.0).r;
    let cover = textureSampleLevel(cover_map, cover_sampler, uv, 0.0).r;
    let weight_sum = max(dot(weights, vec4<f32>(1.0)), 1.0e-3);
    let ground_density = dot(weights, grass.layer_density) / weight_sum * cover * inside;
    // Thin ground grows its grass in clumps (`ground_common::grass_density`).
    let density = grass_density(ground_density, at, grass.shape.x);

    let keep = density * thinning(distance);
    let grow = saturate((keep - rank) / max(0.15 * keep, 1.0e-4));
    // Each blade fades out at the edge a little nearer than the next one
    // over (`EDGE_FEATHER`).
    let feather = 1.0 + EDGE_FEATHER * hash21(at * 0.61 + vec2<f32>(seed * 23.0, 11.0));
    let fade = 1.0 - smoothstep(grass.shares.z, grass.reach.w, distance * feather);

    // A blade the field does not grow here - thinned out by distance or by
    // thin ground, or past the field's reach - draws nothing, and none of
    // the work below is done for it (#1693). A tile's mesh holds its blades
    // in rank order, so on thin ground whole runs of them leave here
    // together.
    if grow <= 0.0 || fade <= 0.0 {
        return nothing(vertex.instance_index, world_from_local, root);
    }

    let ground_colour = (grass.ground[0].rgb * weights.x + grass.ground[1].rgb * weights.y
        + grass.ground[2].rgb * weights.z + grass.ground[3].rgb * weights.w) / weight_sum;

    // The blade's own numbers: from where it stands, so the repeats of a
    // tile differ, and its seed, so two blades of a tile differ.
    let r1 = hash21(at * 1.37 + vec2<f32>(seed * 17.0, 0.0));
    let r2 = hash21(at.yx * 2.11 + vec2<f32>(0.0, seed * 31.0));
    let r3 = hash21(at * 0.73 + vec2<f32>(seed * 53.0, 7.0));
    let r4 = hash21(at.yx * 3.17 + vec2<f32>(5.0, seed * 71.0));

    let flower_count = dot(vec4<f32>(
        grass.flowers[0].w, grass.flowers[1].w, grass.flowers[2].w, grass.flowers[3].w
    ), vec4<f32>(1.0));
    let flower = r3 < grass.frame.w && flower_count > 0.0;
    // What the flowers show from afar (#1700): their colours' mean, at the
    // share that flower - none without a colour to flower in. The ground
    // takes the same (`grass::flower_mean`).
    let flower_mean = (grass.flowers[0].rgb * grass.flowers[0].w
        + grass.flowers[1].rgb * grass.flowers[1].w
        + grass.flowers[2].rgb * grass.flowers[2].w
        + grass.flowers[3].rgb * grass.flowers[3].w) / max(flower_count, 1.0);
    let flowering = select(0.0, grass.frame.w, flower_count > 0.0);

    // The ground's tone fields (`ground_common.wgsl`, #1676): the grass
    // stands taller where the ground is darker - mostly patch by patch (the
    // mottle), a little stretch by stretch (the drift) - and is drier where
    // it drifts lighter (below).
    let drift = ground_drift(at);
    let mottle = ground_mottle(at);
    let stand = (1.0 - drift) * 0.4 + (1.0 - mottle) * 0.6;

    // The weather on the ground (#1683), as the ground's shader reads it:
    // snow settled here buries the blades by as much and whitens what
    // stands out of it; a puddle drowns them; wet blades darken (and
    // shine: the material).
    let wet = grass.weather.x * open;
    let snow = grass.weather.y * open;
    var settled = 0.0;
    var puddle = 0.0;
    if snow > 0.0 || wet > 0.0 {
        // On the ground's own normal, as its shader reads it.
        settled = snow_settled(at, ground, ground_normal.y, snow, grass.weather.z, grass.weather.w);
        puddle = puddle_at(at, ground_normal.y, wet);
    }

    // Its tussock (#1696): a little taller or shorter than the next, its
    // blades shorter toward its rim.
    let tussock = grass_tussock(at, distance);
    let tussock_height = 1.0 + tussock.shows
        * ((1.0 + TUSSOCK_HEIGHT * (2.0 * tussock.own.x - 1.0))
            * (1.0 + TUSSOCK_RIM * (TUSSOCK_RIM_MEAN - tussock.rim)) - 1.0);
    // How tall it grows before the field thins it or the weather buries it.
    let natural = (0.55 + 0.9 * r1) * mix(0.7, 1.25, stand) * mix(0.55, 1.0, density) * tussock_height;
    var height = grass.base.w * natural * grow * fade * (1.0 - BURIED * settled) * (1.0 - puddle);
    if flower {
        height = height * 1.15;
    }
    // Wider where the field is thinned, so a far tile's few blades still
    // cover; and never under about a pixel and a half, or a blade
    // shimmers.
    var width = grass.tip.w * (0.7 + 0.6 * r2) * inverseSqrt(max(thinning(distance), 1.0e-3));
    width = max(width, distance * MIN_WIDTH_PER_M) * select(0.0, 1.0, height > 1.0e-3);

    // Which way the blade faces: its own way close by, turned toward the
    // camera further out, so no distant blade is seen edge-on as a line.
    let angle = r1 * 6.2831853 + r4 * 2.0;
    var across = vec3<f32>(cos(angle), 0.0, sin(angle));
    // Mostly as its tussock faces (a blade faces either way alike).
    let tussock_angle = tussock.own.y * 6.2831853;
    var tussock_across = vec3<f32>(cos(tussock_angle), 0.0, sin(tussock_angle));
    tussock_across = tussock_across * select(1.0, -1.0, dot(tussock_across, across) < 0.0);
    across = normalize(
        mix(across, tussock_across, min(TUSSOCK_FACING * tussock.shows, 1.0)) + vec3<f32>(1.0e-4, 0.0, 0.0),
    );
    let to_eye = normalize(vec3<f32>(eye.x - root.x, 0.0, eye.z - root.z) + vec3<f32>(1.0e-4, 0.0, 0.0));
    let facing_eye = vec3<f32>(-to_eye.z, 0.0, to_eye.x);
    across = across * select(1.0, -1.0, dot(across, facing_eye) < 0.0);
    across = normalize(mix(across, facing_eye, 0.85 * smoothstep(4.0, 16.0, distance)));
    let facing = vec3<f32>(-across.z, 0.0, across.x);

    // The bend: the blade's own droop, the wind's lean, and the push of any
    // body standing in the grass, as a horizontal offset of the tip.
    var bend = facing * (0.15 + 0.4 * r3) * height * select(1.0, -1.0, r2 > 0.5);
    let wind_dir = normalize(grass.wind.xy + vec2<f32>(1.0e-5, 0.0));
    let t = globals.time;
    let gust = grass_gust(at, wind_dir, grass.wind.z, t);
    let flutter = 0.25 * sin(t * (2.3 + 2.0 * r1) + r2 * 6.2831853);
    // A storm's gust front (#1684) leans the blades harder as it passes,
    // the foliage and the rain with them (`storm_common.wgsl`).
    let front = grass.shares.w * storm_gust(at, wind_dir, t);
    let lean = grass.wind.w * grass_lean(gust + flutter, front, grass.wind.z);
    bend = bend + vec3<f32>(wind_dir.x, 0.0, wind_dir.y) * lean * (height / grass.base.w);
    // Out from its tussock's middle, the more the nearer its rim.
    bend = bend + vec3<f32>(tussock.outward.x, 0.0, tussock.outward.y)
        * (TUSSOCK_SPLAY * tussock.shows * smoothstep(0.0, 0.7, tussock.rim) * height);
    var flatten = 0.0;
    for (var i = 0; i < i32(textureDimensions(bender_map).x); i = i + 1) {
        let body = textureLoad(bender_map, vec2<i32>(i, 0), 0);
        if body.w <= 0.0 {
            break;
        }
        let away = at - body.xz;
        let gap = length(away);
        // Flat under most of the body, leaning less toward its reach; and
        // none under a body whose underside is clear of the blades (a
        // plane, a jump).
        let low = 1.0 - smoothstep(0.3, 1.0, body.y - ground);
        let push = (1.0 - smoothstep(0.7 * body.w, body.w, gap)) * low;
        bend = bend + vec3<f32>(away.x, 0.0, away.y) / max(gap, 1.0e-3) * push * height;
        flatten = max(flatten, push);
    }
    let reach = 0.9 * height;
    let sway = length(bend);
    bend = bend * min(1.0, reach / max(sway, 1.0e-5));
    let tip_y = sqrt(max(height * height - dot(bend, bend), 0.0)) * (1.0 - 0.5 * flatten);

    // Further out the blades take the colour of the ground they stand on -
    // as the ground's shader draws it: its layers' colour under the grass it
    // shows this view (#1695), lit by the gusts as it is, its drift and
    // mottle and weather included - and its normal, all of both by the
    // field's reach: a far blade is shaded as the ground beside it is, and
    // the field has no edge to see.
    let view_facing = abs(dot(normalize(eye - root), ground_normal));
    let density_seen = grass_density_seen(ground_density, at, distance, view_facing, grass.shape.x);
    let shown = carpet_colour(
        grass.base.rgb,
        grass.tip.rgb,
        grass.dry.rgb,
        grass.dry.w,
        flower_mean,
        flowering,
        drift,
        density_seen,
        view_facing,
    ) * carpet_shade(gust, front, grass.wind.z, grass.wind.w / WIND_LEAN, gust_fineness(distance, view_facing));
    let carpet = carpet_cover(density_seen, view_facing, grass.shape.y) * (1.0 - puddle);
    let far_ground = mix(
        mix(ground_vary(ground_colour, drift, mottle), ground_vary(shown, drift, mottle), carpet)
            * mix(1.0, WET_DARKEN, wet),
        SNOW_ALBEDO,
        settled,
    );
    let blade = Blade(
        root,
        bend,
        tip_y,
        across,
        width,
        select(0.0, 1.0, flower),
        eye,
        smoothstep(grass.reach.x, grass.reach.w, distance),
        far_ground,
        normalize(mix(ground_normal, carpet_normal(to_eye, ground_normal, wet), carpet * (1.0 - settled))),
        select(0.0, 1.0, r4 < grass.dry.w),
        drift,
        (0.82 + 0.36 * r1) * (1.0 + TUSSOCK_TINT * tussock.shows * (2.0 * tussock.shade - 1.0)),
        density,
        grass.flowers[min(u32(r2 * flower_count), 3u)].rgb,
        wet,
        settled,
    );
    var point = blade_point(blade, up, side);
    // The pairs between the root and the tip take the place, the normal and
    // the colour of the fewer segments the blade is about to be drawn with
    // (#1694), from `MORPH_FROM` of the way to where its tile turns to them
    // (`tiles::Detail::at`): the upper pair onto the line from the root to
    // the tip, which a far blade draws, and a near blade's third-way pair
    // onto the line from the root to the upper pair, which a middle blade
    // draws. Every blade of a tile that turns lies past where it turns, so
    // the turn changes nothing drawn.
    if up > 0.0 && up < 1.0 {
        let to_far = smoothstep(MORPH_FROM * grass.reach.z, grass.reach.z, distance);
        let to_mid = select(0.0, smoothstep(MORPH_FROM * grass.reach.y, grass.reach.y, distance), up < 0.5);
        if to_far > 0.0 || to_mid > 0.0 {
            let foot = blade_point(blade, 0.0, side);
            var upper = blade_point(blade, UPPER_RUNG, side);
            if to_far > 0.0 {
                upper = between(upper, between(foot, blade_point(blade, 1.0, 0.0), UPPER_RUNG), to_far);
            }
            if up < 0.5 {
                point = between(point, between(foot, upper, 0.5), to_mid);
            } else {
                point = upper;
            }
        }
    }

    // A short near blade folds into a V (`FOLD_SHORT`): two arms from its
    // root, each a blade of its own - the first segment one arm (its root
    // pair the arm's tip), the rest the other - and unfolds over the way its
    // tile turns to fewer segments. It folds in two halves, so a rung always
    // stands on the ground: first the third-way pair comes down to the root
    // as the rest takes the second arm's shape, the first segment shrinking
    // to nothing at the root; then the root pair rises into the first arm's
    // tip. No flower folds.
    let fold = select(0.0, 1.0, near_mesh && natural < FOLD_SHORT && !flower)
        * (1.0 - smoothstep(MORPH_FROM * grass.reach.y, grass.reach.y, distance));
    if fold > 0.0 {
        let second = up > 0.3;
        let along = select(1.0 - 3.0 * up, (up - 1.0 / 3.0) * 1.5, second);
        var arm = blade;
        let arm_height = height * FOLD_ARM;
        let splay = blade.across * select(-1.0, 1.0, second) * FOLD_SPLAY * arm_height;
        let arm_bend = blade.bend * FOLD_ARM + splay;
        arm.bend = arm_bend * min(1.0, 0.9 * arm_height / max(length(arm_bend), 1.0e-5));
        arm.tip_y = sqrt(max(arm_height * arm_height - dot(arm.bend, arm.bend), 0.0)) * (1.0 - 0.5 * flatten);
        let lowered = saturate(2.0 * fold);
        let raised = saturate(2.0 * fold - 1.0);
        point = between(point, blade_point(arm, along, side), select(lowered, raised, up < 0.1));
    }

    out.world_position = vec4<f32>(point.position, 1.0);
    out.position = position_world_to_clip(point.position);
    out.world_normal = point.normal;
#ifdef VERTEX_COLORS
    out.color = vec4<f32>(point.colour, 1.0);
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex.instance_index, world_from_local[3]);
#endif
    return out;
}
