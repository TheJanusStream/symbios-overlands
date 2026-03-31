// Fragment shader for the animated water surface.
//
// Extends the base StandardMaterial with sum-of-sines wave perturbation
// applied to the surface normal.  Only the top face of the water cuboid is
// perturbed; side and bottom faces keep their geometric normal so lighting
// at the water volume edges remains correct.
//
// The wave animation is driven entirely by Bevy's `globals.time` uniform —
// no additional bindings or uniforms are required.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    mesh_view_bindings::globals,
    forward_io::{VertexOutput, FragmentOutput},
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    // Get the base material properties (Color, Roughness, etc.)
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // 1. Procedural Waves via Sum-of-Sines
    let t = globals.time;
    let pos = in.world_position.xyz;
    
    // Mix overlapping sine waves to create chaotic peaks and valleys
    let wave_x = sin(pos.x * 1.5 + t * 2.0) * 0.1 
               + cos(pos.z * 0.8 - t * 1.5) * 0.05;
               
    let wave_z = cos(pos.z * 1.5 + t * 1.8) * 0.1 
               + sin(pos.x * 0.9 - t * 1.2) * 0.05;

    // 2. Perturb the normal for the top face; side/bottom faces of the water
    //    cuboid keep their geometric normal so lighting is correct at edges.
    let geo_normal = normalize(in.world_normal);
    let is_top_face = geo_normal.y > 0.5;
    let perturbed_normal = normalize(vec3<f32>(wave_x, 1.0, wave_z));
    pbr_input.N = select(geo_normal, perturbed_normal, is_top_face);

    // 3. Apply standard Bevy lighting
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

    return out;
}