// Fragment shader for the procedural cloud deck.
//
// What this does:
//   1. Treats every fragment of the cloud-deck plane as a sample point on a
//      flat sheet of clouds at altitude `cloud_height`. The mesh IS the
//      sampling surface, so the shader doesn't have to ray-march anything;
//      `in.world_position.xz` is already the cloud-plane intersection.
//   2. Builds a domain-warped 5-octave FBM noise field over `world_xz / scale`
//      with a `time * speed * wind_dir / scale` scroll term. Domain-warping
//      one FBM by another produces the bulgy, non-grid-aligned silhouettes
//      that distinguish cumulus shapes from a plain blobby noise.
//   3. Threshold-shapes the noise by `cover` and feathers the edge by
//      `softness` to produce the cloud `mass`. Multiplied by `density` for
//      the final alpha.
//   4. Mixes `shadow_color` toward `color` by `clamp(sun_dir.y, 0, 1)` —
//      sun directly overhead lights the underside; sun near horizon leaves
//      the underside shadowed. Cheap directional fake-lighting suitable for
//      a daytime sky without a real lighting pass.
//   5. Fades final RGB toward `fog_color` and final alpha toward zero as
//      horizontal distance from the camera grows past the room's
//      `fog_visibility`. This is what dissolves the cloud-deck plane edge
//      into the existing fog band at the horizon — without it the plane's
//      circular boundary would show as a hard ring.
//
// All tunable values flow through the `CloudUniforms` block bound at slot
// 100. Authored on `pds::Environment` and patched by the world compiler.

#import bevy_pbr::{
    mesh_view_bindings::{globals, view},
    forward_io::{VertexOutput, FragmentOutput},
}

struct CloudUniforms {
    color: vec4<f32>,
    shadow_color: vec4<f32>,
    fog_color: vec4<f32>,
    sun_dir: vec4<f32>,
    wind_dir: vec2<f32>,
    cover: f32,
    density: f32,
    softness: f32,
    speed: f32,
    scale: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> cloud_uniforms: CloudUniforms;

// David Hoskins' sine-free hash. Same construction as the water shader —
// stays numerically well-conditioned at the high integer-coordinate
// magnitudes the cloud sampler reaches when `world_xz / scale` is sampled
// over a 4 km plane (`floor(p)` can run into the hundreds).
fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 = p3 + dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// 5-octave FBM with a small per-octave rotation to break the axis-aligned
// grid that an unrotated 2× lacunarity FBM otherwise leaves visible at low
// cover values. Total weight is normalised so the output stays in `[0, 1]`
// regardless of octave count.
fn fbm(p_in: vec2<f32>) -> f32 {
    var p = p_in;
    var amp = 0.5;
    var sum = 0.0;
    var norm = 0.0;
    // Rotation matrix for ~30° per octave — irrational angle to avoid the
    // octaves locking into a common grid alignment after a few iterations.
    let rc = 0.866025;
    let rs = 0.5;
    let rot = mat2x2<f32>(rc, -rs, rs, rc);
    for (var i = 0; i < 5; i = i + 1) {
        sum = sum + amp * value_noise(p);
        norm = norm + amp;
        p = rot * (p * 2.03) + vec2<f32>(13.7, 7.3);
        amp = amp * 0.5;
    }
    return sum / max(norm, 1.0e-5);
}

@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;

    let scale = max(cloud_uniforms.scale, 1.0e-3);
    let world_xz = in.world_position.xz;
    let cam_xz = view.world_position.xz;

    // Wind drift. Normalise an epsilon-padded copy so a user-zeroed wind
    // direction (sanitiser already guards the record, but defence in depth)
    // can never propagate NaN through the scroll term.
    let wind_in = cloud_uniforms.wind_dir + vec2<f32>(1.0e-4, 0.0);
    let wind = normalize(wind_in);
    let scroll = wind * (cloud_uniforms.speed * globals.time / scale);

    let uv = world_xz / scale + scroll;

    // Domain-warp the sampling field by a low-frequency FBM so the cloud
    // outlines bulge instead of forming the smooth puffballs a plain FBM
    // produces. The warp amplitude is in UV units; ~0.7 gives noticeable
    // distortion without losing the original FBM's large-scale coherence.
    let warp = vec2<f32>(
        fbm(uv * 0.6),
        fbm(uv * 0.6 + vec2<f32>(31.0, 17.0)),
    );
    let n = fbm(uv + warp * 0.7);

    // Threshold by cover. cover = 0 → empty sky (high threshold passes
    // nothing); cover = 1 → solid overcast (low threshold passes everything).
    // softness widens the smoothstep band so cloud edges feather instead of
    // popping in/out as the noise field crosses the threshold.
    let thresh = mix(1.0, 0.0, clamp(cloud_uniforms.cover, 0.0, 1.0));
    let soft = max(cloud_uniforms.softness, 1.0e-3);
    let mass = smoothstep(thresh - soft, thresh + soft, n);
    let alpha_raw = clamp(mass * cloud_uniforms.density, 0.0, 1.0);

    // Early out for fragments that wouldn't contribute anything visible —
    // the alpha-blend pipeline still rasterises and blends them, but
    // returning a fully transparent black saves any further math and keeps
    // the resulting framebuffer composition clean.
    if alpha_raw < 1.0e-3 {
        out.color = vec4<f32>(0.0, 0.0, 0.0, 0.0);
        return out;
    }

    // Cheap directional shading: when the sun is high (sun_dir.y near 1)
    // the underside of the deck reads bright; when low (near horizon) the
    // underside falls toward `shadow_color`, suggesting a low-angle sunset
    // mood without a real lighting pass.
    let sun_lit = clamp(cloud_uniforms.sun_dir.y, 0.0, 1.0);
    let lit_tint = mix(
        cloud_uniforms.shadow_color.rgb,
        cloud_uniforms.color.rgb,
        sun_lit,
    );

    // Horizon fade — angle-based, not distance-based.
    //
    // Earlier draft tied the fade band to `fog_visibility`, which produced
    // two bugs: (a) any user who raised the visibility slider past the
    // camera's far plane saw the cloud-deck plane clipped at the slant
    // distance corresponding to far-clip, leaving a sharp ring; (b) the
    // band size was decoupled from the cloud altitude, so high decks fed
    // a too-tight band and low decks a too-wide one.
    //
    // Switching to `tan(angle_from_zenith) = horiz / vertical` makes the
    // fade band a function of the deck's altitude relative to the
    // camera. Same band is "30°–80° from zenith" for every altitude:
    // clouds directly overhead are crisp, clouds near the horizon dissolve
    // smoothly into `fog_color`. Independent of `fog_visibility`, so the
    // distance-fog slider can vary without ever introducing a ring.
    let horiz = distance(world_xz, cam_xz);
    let vertical = max(in.world_position.y - view.world_position.y, 1.0);
    let zenith_tan = horiz / vertical;
    // tan(30°) ≈ 0.577, tan(80°) ≈ 5.671. The band width is asymmetric on
    // purpose — the eye reads "near-horizontal" cloud as the entire fade
    // region, so most of the smoothstep budget lives in the upper portion.
    let fog_blend = clamp(
        (zenith_tan - 0.577) / (5.671 - 0.577),
        0.0,
        1.0,
    );
    let final_rgb = mix(lit_tint, cloud_uniforms.fog_color.rgb, fog_blend);
    let final_alpha = alpha_raw * (1.0 - fog_blend);

    // Straight (non-premultiplied) alpha — Bevy's `AlphaMode::Blend` uses
    // `src.rgb * src.a + dst.rgb * (1 - src.a)` which expects un-premultiplied
    // colours.
    out.color = vec4<f32>(final_rgb, final_alpha);
    return out;
}
