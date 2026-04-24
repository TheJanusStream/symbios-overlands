// Fragment shader for the animated water surface.
//
// Step-by-step what this does vs the old sum-of-two-sines implementation:
//   1. Discards everything but the top face of the water cuboid. The old
//      shader discarded only the bottom, leaving the side faces to draw
//      against the top with AlphaMode::Blend — whichever sort order the
//      current view produced, you'd get the characteristic "parallel hard
//      edges" artifact. One surface only, no sort race.
//   2. Wave displacement is now a sum of six Gerstner waves rotated around
//      the user-controlled prevailing wind direction, at golden-ratio-ish
//      wavelengths / amplitudes / speeds. Normals are computed analytically
//      from the Gerstner partial derivatives — not a faked vec3(x, 1, z).
//      Rotated direction vectors kill the axis-aligned grid repetition the
//      old implementation showed on long sightlines.
//   3. A two-scale scrolling detail noise (near/far tiles blended by camera
//      distance) overlays fine ripples onto the Gerstner normal to mask the
//      wave-frequency grain at distance.
//   4. Fresnel (Schlick) drives both the reflection strength and the final
//      alpha, mixing a shallow/transparent tint at head-on view with a
//      deep/opaque tint at grazing angles. This is what fixes the "sometimes
//      very translucent" behaviour — there was no view-angle term before.
//   5. Subsurface scatter, wave-crest foam, and a sharp sun-glitter specular
//      highlight all ride on top of the PBR lighting pass.
//
// All tunable values flow through the `WaterUniforms` block bound at slot
// 100 — authored on `pds::WaterSurface` (per water body) and
// `pds::Environment` (room-wide).

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    mesh_view_bindings::{globals, view},
    forward_io::{VertexOutput, FragmentOutput},
}

struct WaterUniforms {
    shallow_color: vec4<f32>,
    deep_color: vec4<f32>,
    scatter_color: vec4<f32>,  // rgb used, a unused
    wave_direction: vec2<f32>,
    wave_scale: f32,
    wave_speed: f32,
    wave_choppiness: f32,
    roughness: f32,
    metallic: f32,
    reflectance: f32,
    foam_amount: f32,
    normal_scale_near: f32,
    normal_scale_far: f32,
    refraction_strength: f32,
    sun_glitter: f32,
    shore_foam_width: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> water_uniforms: WaterUniforms;

// ---------------------------------------------------------------------------
// Gerstner wave helpers
// ---------------------------------------------------------------------------

struct GerstnerOut {
    offset: vec3<f32>,
    d_dx: vec3<f32>,
    d_dz: vec3<f32>,
};

// Single Gerstner wave contribution. `Q` is the steepness (0 = plain sine,
// 1 = sharpest crests that still keep the surface function-valued). Returns
// world-space offset plus the partial derivatives of that offset with
// respect to the undisturbed X and Z positions — enough to build an exact
// surface normal without finite differences.
//
// `phase_offset` breaks the coherence of multiple waves passing through the
// origin at t=0. Without it all six waves peak at p=(0,0,t=0) and form a
// stationary interference pattern of bright bands; with irrational offsets
// the peaks scatter across the domain and the bands disappear.
fn gerstner_wave(
    p: vec2<f32>,
    t: f32,
    dir: vec2<f32>,
    amplitude: f32,
    wavelength: f32,
    speed: f32,
    steepness: f32,
    phase_offset: f32,
) -> GerstnerOut {
    let k = 6.2831853 / max(wavelength, 0.0001);
    let c = speed * k;
    let d = normalize(dir);
    let phase = k * dot(d, p) - c * t + phase_offset;
    let cos_f = cos(phase);
    let sin_f = sin(phase);
    // Distribute steepness across all summed waves so the crests never
    // self-intersect — matches GPU Gems Ch. 1 convention.
    let Q = steepness;
    let wa = amplitude * k;

    var o: GerstnerOut;
    o.offset = vec3<f32>(
        Q * amplitude * d.x * cos_f,
        amplitude * sin_f,
        Q * amplitude * d.y * cos_f,
    );
    o.d_dx = vec3<f32>(
        -Q * d.x * d.x * wa * sin_f,
         d.x * wa * cos_f,
        -Q * d.x * d.y * wa * sin_f,
    );
    o.d_dz = vec3<f32>(
        -Q * d.x * d.y * wa * sin_f,
         d.y * wa * cos_f,
        -Q * d.y * d.y * wa * sin_f,
    );
    return o;
}

fn rot2(v: vec2<f32>, a: f32) -> vec2<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec2<f32>(c * v.x - s * v.y, s * v.x + c * v.y);
}

// ---------------------------------------------------------------------------
// Value noise for detail normals + foam breakup
// ---------------------------------------------------------------------------

fn hash21(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let r = q + dot(q, q + 45.32);
    return fract(r.x * r.y);
}

fn noise2d(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    let u = f * f * (3.0 - 2.0 * f);
    return mix(a, b, u.x) + (c - a) * u.y * (1.0 - u.x) + (d - b) * u.x * u.y;
}

// Normal perturbation from the gradient of a value-noise field, sampled at
// the given UV and ~one octave above. The vertical Y coefficient biases the
// result toward +Y so the combined water normal never flips horizontal
// (which would invert lighting on a flat pond).
//
// `footprint` is the screen-space size of one UV unit at this pixel. When
// the footprint approaches or exceeds one noise cell the finite-difference
// gradient aliases hard: the characteristic symptom is a diagonal hash-cell
// grid becoming visible at grazing angles, especially when amplified by a
// low-roughness specular lobe. We fade the gradient amplitude inversely to
// the footprint so under-sampled regions collapse to a flat normal rather
// than producing garbage derivatives.
//
// The coefficient below is chosen so a pixel covering ~40% of a noise cell
// is already half-faded — erring on the side of too much smoothing, which
// is cheaper visually than leaving spiky residue on the specular lobe.
fn detail_normal(uv: vec2<f32>, footprint: f32) -> vec3<f32> {
    let fade = clamp(1.0 - footprint * 2.5, 0.0, 1.0);
    if fade < 0.01 {
        return vec3<f32>(0.0, 1.0, 0.0);
    }
    let eps = 0.05;
    let v = noise2d(uv) + 0.5 * noise2d(uv * 2.17);
    let dx = (noise2d(uv + vec2<f32>(eps, 0.0)) + 0.5 * noise2d((uv + vec2<f32>(eps, 0.0)) * 2.17)) - v;
    let dz = (noise2d(uv + vec2<f32>(0.0, eps)) + 0.5 * noise2d((uv + vec2<f32>(0.0, eps)) * 2.17)) - v;
    return normalize(vec3<f32>(-dx / eps * fade, 3.0, -dz / eps * fade));
}

// ---------------------------------------------------------------------------
// Fragment entry
// ---------------------------------------------------------------------------

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    // Step 1 / artifact fix: discard every fragment whose geometric normal
    // is not pointing up. Only the top face of the water cuboid contributes.
    let geo_normal = normalize(in.world_normal);
    if geo_normal.y < 0.5 {
        discard;
    }

    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let t = globals.time * water_uniforms.wave_speed;
    let pos = in.world_position.xyz;
    let xz = pos.xz;

    // Normalise the prevailing direction; guard against a zero input vector
    // (which the editor can produce if a user drags both components to 0)
    // by nudging with a tiny epsilon so normalize() cannot return NaN.
    let prevailing_in = water_uniforms.wave_direction + vec2<f32>(0.0001, 0.0);
    let prevailing = normalize(prevailing_in);

    let scale = water_uniforms.wave_scale;
    // Per-wave steepness — total steepness must stay ≤ 1 / (k * amplitude)
    // summed across every component to keep the surface from looping back on
    // itself. Divide the uniform by the wave count so the user-facing slider
    // can sit at `1.0` without producing self-intersecting crests.
    let chop = water_uniforms.wave_choppiness / 6.0;

    // Six Gerstner waves. Design rules picked to kill the diagonal-banding
    // interference pattern we saw on the first pass:
    //
    //   * Angles span ~260° of the circle in irrational steps — clustered
    //     angles on one hemisphere let the crest lines overlap into visible
    //     grid diagonals at grazing view.
    //   * Wavelengths are a non-harmonic 1.53× geometric progression from
    //     1.4m to 14.0m so no two waves beat into a low-frequency envelope.
    //   * Amplitudes fall off faster than linearly so the largest wave
    //     dominates the silhouette; smaller waves just add surface texture.
    //   * Speeds scale roughly with sqrt(wavelength) — the physical deep-
    //     water dispersion relation — so the six waves don't synchronise
    //     back onto a common period.
    //   * Phase offsets are irrational constants so the six waves don't
    //     all peak at (p=0, t=0), which would plant a bright static cross
    //     right in front of the camera on every scene reload.
    var total_off = vec3<f32>(0.0);
    var total_dx = vec3<f32>(0.0);
    var total_dz = vec3<f32>(0.0);

    let angles  = array<f32, 6>(0.00, 0.89, -0.56, 1.93, -1.41, 2.77);
    let lambdas = array<f32, 6>(14.0, 9.20, 6.00, 3.90, 2.40, 1.50);
    let amps    = array<f32, 6>(0.55, 0.28, 0.16, 0.09, 0.04, 0.018);
    let speeds  = array<f32, 6>(1.30, 1.15, 1.00, 0.87, 0.75, 0.64);
    let phases  = array<f32, 6>(0.000, 2.137, 4.712, 1.853, 3.141, 0.713);

    for (var i = 0; i < 6; i = i + 1) {
        let dir = rot2(prevailing, angles[i]);
        let w = gerstner_wave(xz, t, dir, amps[i] * scale, lambdas[i], speeds[i], chop, phases[i]);
        total_off = total_off + w.offset;
        total_dx = total_dx + w.d_dx;
        total_dz = total_dz + w.d_dz;
    }

    // Analytic normal from the accumulated partial derivatives. Tangent and
    // bitangent include the identity world axes plus the Gerstner partials;
    // the surface normal is their cross product.
    let tangent = vec3<f32>(1.0, 0.0, 0.0) + total_dx;
    let bitangent = vec3<f32>(0.0, 0.0, 1.0) + total_dz;
    var n_gerstner = normalize(cross(bitangent, tangent));
    if n_gerstner.y < 0.0 {
        n_gerstner = -n_gerstner;
    }

    // Scrolling detail normals. Two UV tiling scales are blended by camera
    // distance so the high-frequency sparkle that reads well up close fades
    // into the low-frequency ripple that reads well at distance — kills the
    // repetition artefact the old shader showed on long sightlines.
    let cam_pos = view.world_position;
    let dist = length(cam_pos - pos);
    let far_weight = clamp(smoothstep(30.0, 180.0, dist), 0.0, 1.0);
    let near_weight = 1.0 - far_weight;

    let near_uv = xz * water_uniforms.normal_scale_near + prevailing * t * 0.35;
    let far_uv = xz * water_uniforms.normal_scale_far + prevailing * t * 0.15;

    // Pixel footprint in each UV space — drives the anti-alias fade inside
    // detail_normal. `fwidth(xz)` is a world-space per-pixel span; multiply
    // by the tile scale to get the UV-space equivalent.
    let world_footprint = length(fwidth(xz));
    let near_footprint = world_footprint * water_uniforms.normal_scale_near;
    let far_footprint = world_footprint * water_uniforms.normal_scale_far;

    let near_n = detail_normal(near_uv, near_footprint);
    let far_n = detail_normal(far_uv, far_footprint);
    let detail = normalize(near_weight * near_n + far_weight * far_n);

    // Blend the Gerstner analytic normal with the detail ripple. Reduce the
    // detail contribution with distance so the fine-grain ripple can't
    // dominate the lit result past the scale where its cells are tiny on
    // screen — this is the secondary cushion against aliasing, on top of
    // the footprint fade inside detail_normal itself.
    let detail_mix = 0.35 * (1.0 - far_weight * 0.75);
    let n = normalize(n_gerstner + detail_mix * vec3<f32>(detail.x, 0.0, detail.z));
    pbr_input.N = n;
    pbr_input.world_normal = n;

    // ------------------------------------------------------------------
    // Fresnel — the fix for "sometimes very translucent"
    // ------------------------------------------------------------------
    let v = normalize(cam_pos - pos);
    let n_dot_v = clamp(dot(n, v), 0.0, 1.0);
    let f0 = clamp(water_uniforms.reflectance, 0.0, 1.0);
    let fresnel = f0 + (1.0 - f0) * pow(1.0 - n_dot_v, 5.0);

    // Mix shallow (head-on) and deep (grazing) tints. `deep_color` alpha is
    // typically high (opaque) and `shallow_color` alpha low (transparent);
    // Fresnel then pushes the final alpha further toward opaque at grazing.
    let grazing = 1.0 - n_dot_v;
    let tint = mix(water_uniforms.shallow_color.rgb, water_uniforms.deep_color.rgb, grazing);
    let base_alpha = mix(water_uniforms.shallow_color.a, water_uniforms.deep_color.a, grazing);
    let final_alpha = clamp(base_alpha + fresnel * (1.0 - base_alpha), 0.0, 1.0);

    // Cheap subsurface scatter: bright the crests with the scatter tint.
    let crest_strength = clamp(total_off.y * 0.6, 0.0, 1.0);
    let scatter = crest_strength * water_uniforms.scatter_color.rgb;

    // Procedural foam where the wave slope is steep, gated by noise so the
    // foam breaks into clumps rather than a continuous halo.
    let slope = clamp(1.0 - n_gerstner.y, 0.0, 1.0);
    let foam_noise = noise2d(xz * 0.6 + prevailing * t * 0.5);
    let foam = clamp(
        smoothstep(0.28, 0.8, slope * 1.3 + foam_noise * 0.4) * water_uniforms.foam_amount,
        0.0,
        1.0,
    );

    // Push the per-volume overrides into the PBR input before lighting runs.
    // Distance-based roughness boost (Toksvig-lite): widen the specular lobe
    // as the water recedes, so under-sampled Gerstner / detail-normal
    // variations don't produce the spiky BRDF response that would otherwise
    // alias into visible grid patterns.
    let base_roughness = clamp(water_uniforms.roughness, 0.02, 1.0);
    let distance_rough = mix(base_roughness, 0.45, clamp(smoothstep(40.0, 250.0, dist), 0.0, 1.0));
    pbr_input.material.base_color = vec4<f32>(tint + scatter, final_alpha);
    pbr_input.material.perceptual_roughness = distance_rough;
    pbr_input.material.metallic = clamp(water_uniforms.metallic, 0.0, 1.0);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);

    // Sun-glitter specular — a sharp highlight layered on top of the PBR
    // result. The directional light uniform's direction is available via
    // `lights.directional_lights[0]` but reading that across the #ifdef
    // matrix for this example adds complexity; instead, approximate the
    // sun as "where the sun would roughly be" via a fixed up-biased vector
    // and let the Environment sun-glitter slider tune intensity.
    //
    // The exponent is kept moderate (~160) and the contribution fades with
    // distance — a sharper lobe alongside aliased normals was the single
    // biggest contributor to the diagonal-grid artifact on the previous
    // iteration, since tiny normal errors became order-of-magnitude BRDF
    // spikes.
    let sun_approx = normalize(vec3<f32>(0.4, 1.0, 0.3));
    let half_vec = normalize(sun_approx + v);
    let n_dot_h = max(dot(n, half_vec), 0.0);
    let glitter_fade = clamp(1.0 - smoothstep(60.0, 260.0, dist), 0.0, 1.0);
    let glitter = pow(n_dot_h, 160.0) * water_uniforms.sun_glitter * fresnel * glitter_fade;
    out.color = vec4<f32>(out.color.rgb + vec3<f32>(glitter), out.color.a);

    // Foam overlay — mix near-white on top of the lit colour. Keep the
    // alpha driven by Fresnel so foam is still visible at grazing view but
    // doesn't make the centre of the pond look like cream soup.
    let foam_color = vec3<f32>(0.94, 0.97, 1.0);
    out.color = vec4<f32>(mix(out.color.rgb, foam_color, foam), out.color.a);

    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
