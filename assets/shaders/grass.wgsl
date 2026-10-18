// Vertex shader for the GPU grass (#1675).
//
// Every tile is the same mesh (see `src/grass/tiles.rs`): a vertex carries
// where its blade stands in the tile and how far up the blade it is
// (POSITION), which edge it is on, the blade's rank and its seed (COLOR).
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
//      a storm's gust front, `storm_common.wgsl`, #1684), a body's push -
//      and colours it: root to tip, a dry share, a flower share whose heads
//      take the room's flower colours, and the ground's own tone fields
//      (`ground_common.wgsl`): drier where the ground drifts lighter, a
//      little taller or shorter patch by patch. Before its tile turns to
//      fewer segments a blade, it takes their shape (#1694), so the turn
//      changes nothing drawn; and each blade fades out at the field's edge
//      at a distance of its own, so the edge is no circle.
//   5. Weathers it as the ground under it is weathered (#1683), from the
//      same fields: snow buries most of a blade and whitens what stands
//      out, a puddle drowns it, and wet blades darken.
//
// The fragment is Bevy's PBR one, which reads the colour written here as
// the base colour (`VERTEX_COLORS`, set because the mesh carries COLOR),
// so the blades are lit and fogged as every other surface is.
//
// WebGL2-safe: vertex-stage texture reads (16 vertex texture units in
// every WebGL2), the 32-bit float height and bodies read by `textureLoad`
// (they cannot be filtered there), no storage buffers. Mirrors of the Rust
// side, which nothing checks at build time: `GrassUniforms`
// (`src/grass/material.rs`, pinned by
// `grass::tests::wgsl_block_mirrors_the_rust_one`) and the constants below
// that name their Rust source (pinned by the same test, and by
// `grass::tiles`'s).

#import bevy_pbr::{
    forward_io::VertexOutput,
    mesh_functions,
    mesh_view_bindings::{globals, view},
    view_transformations::position_world_to_clip,
}
#import "shaders/ground_common.wgsl"::{hash21, value_noise, ground_drift, ground_mottle, ground_vary, snow_settled, puddle_at, SNOW_ALBEDO, WET_DARKEN}
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
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> grass: GrassUniforms;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var height_map: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var weight_map: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var weight_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var cover_map: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var cover_sampler: sampler;
// The bodies that part the grass, a texel each, nearest the camera first:
// middle on XZ, underside on Y, reach in W; 0 past the last.
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var bender_map: texture_2d<f32>;

// A tile's side (m): `config::grass::TILE_M`. A larger tile's 8 m cells are
// numbered `i + CELLS_A_ROW * j` along X and Z (`tiles::CELLS_A_ROW`).
const TILE_M: f32 = 8.0;
const CELLS_A_ROW: u32 = 4u;
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
// In a storm's gust front (#1684): how much harder the blades lean.
const STORM_LEAN: f32 = 1.5;
// Under settled snow (#1683): the share of a blade's height the snow buries,
// and how white the snow leaves its tip.
const BURIED: f32 = 0.75;
const SNOWY_TIPS: f32 = 0.85;
// The snow on what stands out of it at the blade's foot, under its tip's.
const SNOWY_FOOT: f32 = 0.6;
// The least width (m) of a blade a metre away, and in step further: about
// a pixel and a half on a 1080-pixel-tall screen at Bevy's default 45
// degree field of view. A blade narrower than a pixel shimmers as it sways.
const MIN_WIDTH_PER_M: f32 = 0.0012;
// The wind speed (m/s) at which a tip leans `grass.wind.w`: the room
// default.
const REFERENCE_WIND: f32 = 4.0;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    // The mesh's COLOR attribute: edge, rank, seed, and the blade's 8 m
    // cell in its tile.
    @location(5) blade: vec4<f32>,
};

// A blade, as every vertex of it is drawn from (`blade_point`): its root,
// its bend and how high its tip stands, the way across it, its width, and
// whether it is a flower (1 or 0); the eye; how far it has merged into the
// ground it stands on, and that ground's colour; and what colours it -
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

// The drawn ground's height at world `xz`: the cell's corners, over the
// mesher's two triangles, split from the (x + 1, z) corner to the
// (x, z + 1) one. Past the edge, the edge's.
fn ground_height(xz: vec2<f32>) -> f32 {
    let last = grass.frame.z - 1.0;
    let g = clamp((xz + vec2<f32>(grass.frame.x)) / grass.frame.y, vec2<f32>(0.0), vec2<f32>(last));
    let c = min(floor(g), vec2<f32>(last - 1.0));
    let f = g - c;
    let i = vec2<i32>(c);
    let tl = textureLoad(height_map, i, 0).r;
    let tr = textureLoad(height_map, i + vec2<i32>(1, 0), 0).r;
    let bl = textureLoad(height_map, i + vec2<i32>(0, 1), 0).r;
    let br = textureLoad(height_map, i + vec2<i32>(1, 1), 0).r;
    if f.x + f.y <= 1.0 {
        return tl + (tr - tl) * f.x + (bl - tl) * f.y;
    }
    return br + (bl - br) * (1.0 - f.x) + (tr - br) * (1.0 - f.y);
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
    normal = normalize(mix(normal, vec3<f32>(0.0, 1.0, 0.0), 0.6) + blade.across * side * 0.2);

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
        normalize(mix(normal, vec3<f32>(0.0, 1.0, 0.0), blade.merge)),
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
    let in_tile = u32(vertex.blade.w + 0.5);
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

    let ground = ground_height(at);
    let root = vec3<f32>(at.x, ground, at.y);
    let distance = length(eye - root);

    // How thickly the ground here grows grass: the layers' densities by
    // their weights, sampled where the ground's shader samples them.
    let uv = (at + vec2<f32>(grass.frame.x)) / (2.0 * grass.frame.x);
    let inside = select(0.0, 1.0, all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0)));
    let weights = textureSampleLevel(weight_map, weight_sampler, uv, 0.0);
    let cover = textureSampleLevel(cover_map, cover_sampler, uv, 0.0).r;
    let weight_sum = max(dot(weights, vec4<f32>(1.0)), 1.0e-3);
    let ground_density = dot(weights, grass.layer_density) / weight_sum * cover * inside;
    // Thin ground grows its grass in clumps a metre or two across, bare
    // between, not as stalks spaced evenly over it.
    let clump = 2.2 * smoothstep(0.25, 0.75, value_noise(at * 0.6 + vec2<f32>(3.0, 29.0)));
    let density = ground_density * mix(1.0, clump, 0.85 * (1.0 - ground_density));

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
    let wet = grass.weather.x;
    var settled = 0.0;
    var puddle = 0.0;
    if grass.weather.y > 0.0 || wet > 0.0 {
        // The ground's slope a cell either side, as smooth across the
        // mesh's triangles as the ground's own normals, which its shader
        // reads: one triangle's slope alone would drown or keep the blades
        // in triangles.
        let cell = grass.frame.y;
        let dx = vec2<f32>(cell, 0.0);
        let dz = vec2<f32>(0.0, cell);
        let rise_x = (ground_height(at + dx) - ground_height(at - dx)) / (2.0 * cell);
        let rise_z = (ground_height(at + dz) - ground_height(at - dz)) / (2.0 * cell);
        let ground_up = inverseSqrt(1.0 + rise_x * rise_x + rise_z * rise_z);
        settled = snow_settled(at, ground, ground_up, grass.weather.y, grass.weather.z, grass.weather.w);
        puddle = puddle_at(at, ground_up, wet);
    }

    var height = grass.base.w * (0.55 + 0.9 * r1) * mix(0.7, 1.25, stand) * mix(0.55, 1.0, density)
        * grow * fade * (1.0 - BURIED * settled) * (1.0 - puddle);
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
    let wave = dot(at, wind_dir) * 0.21 - t * (0.8 + 0.12 * min(grass.wind.z, 20.0));
    let gust = 0.55 + 0.45 * sin(wave) * sin(wave * 0.43 + 1.3);
    let flutter = 0.25 * sin(t * (2.3 + 2.0 * r1) + r2 * 6.2831853);
    // A storm's gust front (#1684) leans the blades harder as it passes,
    // the foliage and the rain with them (`storm_common.wgsl`).
    let front = grass.shares.w * storm_gust(at, wind_dir, t);
    let lean = grass.wind.w * clamp(grass.wind.z / REFERENCE_WIND, 0.0, 2.5) * (gust + flutter)
        * (1.0 + STORM_LEAN * front);
    bend = bend + vec3<f32>(wind_dir.x, 0.0, wind_dir.y) * lean * (height / grass.base.w);
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
    // as the ground draws it, its drift and mottle included - and its
    // normal, all of both by the field's reach: a far blade is shaded as the
    // ground beside it is, and the field has no edge to see.
    let far_ground = mix(
        ground_vary(ground_colour, drift, mottle) * mix(1.0, WET_DARKEN, wet),
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
        select(0.0, 1.0, r4 < grass.dry.w),
        drift,
        0.82 + 0.36 * r1,
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
