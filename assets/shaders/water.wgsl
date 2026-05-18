// Fragment shader for the animated water surface.
//
// Step-by-step what this does vs the old sum-of-two-sines implementation:
//   1. A surface basis is derived from `in.world_normal` so wave UVs are
//      measured along the *surface tangent plane*, not world XZ. On flat
//      water (normal ≈ Y) this collapses exactly to the previous
//      world-XZ parameterisation; on a tilted plane it tracks the slope,
//      which keeps wave fronts horizontal-along-surface instead of
//      stamping vertical-world bands across a sloped sheet.
//   2. Wave displacement is a sum of six Gerstner waves rotated around
//      the user-controlled prevailing wind direction, at golden-ratio-ish
//      wavelengths / amplitudes / speeds, all evaluated in surface-local
//      UVs. Normals are computed analytically in the local frame and
//      rotated to world via the surface basis.
//   3. A two-scale scrolling detail noise (near/far tiles blended by
//      camera distance) overlays fine ripples onto the Gerstner normal
//      to mask the wave-frequency grain at distance. UVs scroll along the
//      prevailing wind direction in still mode and along the surface's
//      downhill tangent in flow mode (`flow_amount = 1`), with linear
//      blending in between.
//   4. `flow_amount` (mirrors `WaterSurface::flow_amount`) blends from
//      classic standing-wave Gerstner (0.0) toward a river-style
//      flow-map look (1.0): Gerstner amplitude is suppressed and the
//      detail-normal scroll speed scales with the surface's tilt
//      magnitude (`sin(tilt_angle)` of the gravity tangent).
//   5. Fresnel (Schlick) drives both the reflection strength and the
//      final alpha, mixing a shallow/transparent tint at head-on view
//      with a deep/opaque tint at grazing angles.
//   6. Subsurface scatter, wave-crest foam, and a sharp sun-glitter
//      specular highlight all ride on top of the PBR lighting pass.
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

// Must match `WAKE_SAMPLES_MAX` in `src/water.rs` exactly. The shader
// reads `wake_active_count` for the live extent; capacity is fixed at
// 32 so the WGSL array length is a constant the std140 layout needs.
const WAKE_SAMPLES_MAX: u32 = 32u;

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
    flow_amount: f32,
    // Avatar-wake perturbation channel (Phase 1, revised). Each live
    // perturbation occupies one slot across two parallel arrays:
    //   wake_samples_a[i] = (pos.x, pos.z, dir.x, dir.z)
    //   wake_samples_b[i] = (age_norm, amplitude, kind, speed)
    // where age_norm ∈ [0,1] drives the lifetime envelope and kind is
    // 0 RadialRipple / 1 DirectionalWake / 2 SplashRing.
    // `wake_active_count` is the number of valid slots;
    // `wake_strength = 0` (default) makes the shader skip the loop
    // entirely so existing scenes render pixel-identical.
    wake_samples_a: array<vec4<f32>, WAKE_SAMPLES_MAX>,
    wake_samples_b: array<vec4<f32>, WAKE_SAMPLES_MAX>,
    wake_active_count: u32,
    wake_strength: f32,
    wake_ripple_wavelength: f32,
    wake_decay_radius: f32,
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
// surface-local offset plus the partial derivatives of that offset with
// respect to the undisturbed local U and V positions — enough to build an
// exact surface normal in local space without finite differences.
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
// Surface basis
// ---------------------------------------------------------------------------

// Build a (tangent, normal, bitangent) right-handed frame for the surface
// at this fragment. Tangent is derived from the projection of world X onto
// the surface plane, falling back to projected world Z when the surface is
// near-vertical (so the projection of X collapses). For flat water (normal
// ≈ Y) this picks tangent = X and bitangent = Z, exactly matching the
// previous world-XZ parameterisation — flat-water visuals stay
// pixel-identical to the pre-rework shader.
//
// Returned matrix has tangent in column 0, normal in column 1, bitangent
// in column 2: `world_v = basis * local_v`, where local_v is in the
// `(t, n, b)` frame.
fn build_surface_basis(normal: vec3<f32>) -> mat3x3<f32> {
    // Tangent ≈ projection of world X onto the surface, normalised.
    let proj_x = vec3<f32>(1.0, 0.0, 0.0) - normal * normal.x;
    let proj_x_len = length(proj_x);
    var tangent: vec3<f32>;
    if proj_x_len > 1e-3 {
        tangent = proj_x / proj_x_len;
    } else {
        // Surface normal is too close to world X — fall back to projecting
        // world Z. This branch only fires for surfaces tilted past ~88°.
        let proj_z = vec3<f32>(0.0, 0.0, 1.0) - normal * normal.z;
        tangent = normalize(proj_z);
    }
    // `cross(tangent, normal)` (not `cross(normal, tangent)`) so flat water
    // produces bitangent = +Z to match the legacy `pos.z` convention.
    let bitangent = normalize(cross(tangent, normal));
    return mat3x3<f32>(tangent, normal, bitangent);
}

// ---------------------------------------------------------------------------
// Gradient noise for detail normals + foam breakup
// ---------------------------------------------------------------------------

// David Hoskins' sine-free scalar hash ("Hash without Sine", Shadertoy
// XlGcRh). We need this specifically because the water's integer-coordinate
// lattice can reach magnitudes of several hundred — `xz * normal_scale_near`
// over a ~256 m world extent puts `floor(p)` well past 200 on each axis.
//
// Two earlier iterations failed here:
//   1. `fract(p * 123.34, 456.21) + dot(...)` — 123.34 = 6167/50, so
//      `fract(i.x * 123.34)` had period 50, planting a rigid 50×100 grid
//      across the water. Denser near-tile scaling tiled that grid thicker
//      into view, producing the diagonal bands.
//   2. `fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453)` — with
//      integer `p` above ~100, the argument to sin() exceeds ~10⁴ and
//      f32 argument-reduction quantises runs of adjacent cells onto
//      identical hash values, reading as hard-edged square splotches.
//
// Hoskins' construction stays in the numerically well-conditioned range
// `[0, 1)` throughout (multiplier `0.1031`), uses a 3-component spread so
// the returned scalar mixes all three carriers, and has no transcendental
// dependency — so it's immune to both period and precision failure modes.
fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 = p3 + dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// Hash a 2D integer lattice point to a unit-length 2D gradient. Polar
// form (random angle → unit vector) gives uniform distribution on the
// circle, which is what we want — the naive `(h*2-1, h2*2-1)` form is
// uniform on a square and biases gradients toward the four corner
// directions, which would reintroduce axis-aligned cell artifacts.
fn hash_grad(p: vec2<f32>) -> vec2<f32> {
    let h = hash21(p);
    let angle = h * 6.2831853;
    return vec2<f32>(cos(angle), sin(angle));
}

// Perlin-style gradient noise with quintic interpolation. Two reasons
// we chose this over the previous bilinear value noise for the detail-
// normal path:
//
//   1. Value noise's smoothstep interpolant (`f²(3−2f)`) has zero
//      derivative at f=0 and f=1, so the noise gradient *vanishes on
//      every cell edge* and peaks in cell centres. Visualised through
//      a finite-difference normal map this reads as bright/dark
//      diamond shapes inside axis-aligned cells — the "grid look"
//      that became obvious whenever `normal_scale_near` was small
//      enough that one cell covered many pixels.
//   2. Gradient noise has value zero at every corner (gradient · 0
//      offset = 0), so cell edges don't form coherent
//      bright/dark axis-aligned ridges. Combined with quintic fade
//      (C², zero 1st and 2nd derivative at boundaries) the cell
//      structure is no longer visible at any practical scale.
//
// Output is remapped to roughly [0, 1] to match the call-site
// semantics of the previous `noise2d`. The exact range of raw 2D
// gradient noise is ±√2/2 ≈ ±0.707, so we scale by 1/√2 then bias.
fn gradient_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let ga = hash_grad(i);
    let gb = hash_grad(i + vec2<f32>(1.0, 0.0));
    let gc = hash_grad(i + vec2<f32>(0.0, 1.0));
    let gd = hash_grad(i + vec2<f32>(1.0, 1.0));
    let va = dot(ga, f);
    let vb = dot(gb, f - vec2<f32>(1.0, 0.0));
    let vc = dot(gc, f - vec2<f32>(0.0, 1.0));
    let vd = dot(gd, f - vec2<f32>(1.0, 1.0));
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let raw = mix(mix(va, vb, u.x), mix(vc, vd, u.x), u.y);
    return raw * 0.7071 + 0.5;
}

// Normal perturbation from the gradient of a two-octave gradient-noise
// field. The vertical Y coefficient biases the result toward +Y so the
// combined water normal never flips horizontal (which would invert
// lighting on a flat pond). Returned in surface-local frame `(t, n, b)`
// — caller rotates to world.
//
// Each octave is sampled in a UV frame rotated by a different non-axis
// angle. Without these rotations the noise lattice aligns with world
// XZ, and even with gradient noise (no per-cell edge brightness) the
// finite-difference gradient inherits a faint axis bias visible as
// world-aligned bands at grazing angles. The two angles are chosen to
// be ~40° apart from each other and roughly 40° / 97° from world X so
// the two octaves' cell directions don't reinforce.
//
// The octave scale ratio (`OCT2_SCALE`) is a deliberately non-harmonic
// 1.937 rather than the previous 2.17. 2.17 sat very close to 13/6 ≈
// 2.1667 — a low-order rational that synchronised the two octaves
// every six base cells along the world axes, planting a soft
// equidistant-band beat pattern across the surface. 1.937 has no
// nearby simple ratio under 30, so the octaves drift across each other
// indefinitely.
//
// `footprint` is the screen-space size of one UV unit at this pixel.
// When the footprint approaches or exceeds one noise cell the
// finite-difference gradient aliases hard. We fade the gradient
// amplitude inversely to the footprint so under-sampled regions
// collapse to a flat normal rather than producing garbage derivatives.
// The coefficient is chosen so a pixel covering ~40% of a noise cell
// is already half-faded — erring on the side of too much smoothing,
// which is cheaper visually than leaving spiky residue on the
// specular lobe.
fn detail_normal(uv: vec2<f32>, footprint: f32) -> vec3<f32> {
    let fade = clamp(1.0 - footprint * 2.5, 0.0, 1.0);
    if fade < 0.01 {
        return vec3<f32>(0.0, 1.0, 0.0);
    }
    let eps = 0.05;

    // mat2x2 is column-major: columns (c, s) and (-s, c) form the
    // CCW rotation by the angle whose (cos, sin) is (c, s).
    let r1 = mat2x2<f32>(0.7648, 0.6442, -0.6442, 0.7648); // ≈ 40° CCW
    let r2 = mat2x2<f32>(-0.1288, 0.9917, -0.9917, -0.1288); // ≈ 97° CCW
    let oct2_scale = 1.937;

    let p1 = r1 * uv;
    let p2 = r2 * uv * oct2_scale;
    let dp1_x = r1 * vec2<f32>(eps, 0.0);
    let dp1_z = r1 * vec2<f32>(0.0, eps);
    let dp2_x = r2 * vec2<f32>(eps * oct2_scale, 0.0);
    let dp2_z = r2 * vec2<f32>(0.0, eps * oct2_scale);

    let v   = gradient_noise(p1)         + 0.5 * gradient_noise(p2);
    let vx  = gradient_noise(p1 + dp1_x) + 0.5 * gradient_noise(p2 + dp2_x);
    let vz  = gradient_noise(p1 + dp1_z) + 0.5 * gradient_noise(p2 + dp2_z);
    let dx = vx - v;
    let dz = vz - v;
    return normalize(vec3<f32>(-dx / eps * fade, 3.0, -dz / eps * fade));
}

// ---------------------------------------------------------------------------
// Avatar-wake perturbation field (Phase 1, revised — see
// `crate::interaction::perturbation`)
// ---------------------------------------------------------------------------

// Per-fragment height contribution from every live perturbation.
//
// A perturbation is a typed, aging disturbance shed by an avatar
// contact event — it is NOT the avatar's live position. `age_norm`
// (∈[0,1]) drives a birth→death amplitude envelope, so a wake persists
// and fades in place after the avatar has moved on.
//
// Three kinds (encoded in `wake_samples_b[i].z`):
//   0 RadialRipple   — isotropic concentric ring; slow-Dwell footfall.
//   1 DirectionalWake — a faded teardrop trailing BEHIND the spawn
//                       point along the frozen heading: the vehicle
//                       sits at the front tip (apex), the half-width
//                       swells mid-trail and closes at the far end,
//                       and the lateral edges + far end fade out. A
//                       single smooth lobe (not a repeating sinusoid)
//                       so consecutive fast-Dwell stamps blend into a
//                       continuous wake instead of beating into a
//                       stacked pile. Fast-Dwell trail.
//   2 SplashRing      — a single crest whose radius grows with age;
//                       water Enter (splash) and Exit (settle).
//
// Bails out cheaply when the channel is disabled (`wake_strength` 0 or
// `wake_active_count` 0) so the fast path on un-waked water stays
// pixel-identical to the pre-wake shader. Per-sample early-out skips
// fragments far past the contribution's spatial support.
fn wake_height_at(xz: vec2<f32>, t: f32) -> f32 {
    let strength = water_uniforms.wake_strength;
    let count = water_uniforms.wake_active_count;
    if strength <= 0.0 || count == 0u {
        return 0.0;
    }
    let R = max(water_uniforms.wake_decay_radius, 0.05);
    let lambda = max(water_uniforms.wake_ripple_wavelength, 0.05);
    let k = 6.2831853 / lambda;
    // Ripple phase velocity — brisk enough to read as propagating
    // without competing with the Gerstner animation.
    let omega = 4.0;
    // How far (in decay radii) a SplashRing crest travels over its
    // life, and the crest's gaussian half-width.
    let splash_expand = 5.0;
    let splash_w = lambda * 0.5;

    var h = 0.0;
    for (var i = 0u; i < count; i = i + 1u) {
        let a = water_uniforms.wake_samples_a[i];
        let b = water_uniforms.wake_samples_b[i];
        let sp = vec2<f32>(a.x, a.y);
        let o = xz - sp;
        let r2 = dot(o, o);
        let age_norm = clamp(b.x, 0.0, 1.0);
        let amp_p = b.y;
        let kind = b.z;
        let spd = b.w;

        // Per-sample early-out. The spatial support differs by kind:
        // a SplashRing crest reaches ~5R, the radial ripple is
        // visually zero past ~7R (exp(-7) ≈ 9e-4), and a fast
        // DirectionalWake's teardrop runs back R·(0.8+0.3·spd). Take
        // the max so a fast vehicle's long trail isn't clipped while
        // slow / non-directional samples still cut off tight.
        let reach = max(
            7.0 * R,
            max(splash_expand * R + 3.0 * splash_w, R * (0.8 + 0.3 * spd)),
        );
        if r2 > reach * reach {
            continue;
        }

        // Birth→death envelope: linear fade-out, plus a ~3% fade-in so
        // a freshly spawned perturbation doesn't pop on for one frame.
        let env = clamp(1.0 - age_norm, 0.0, 1.0)
            * smoothstep(0.0, 0.03, age_norm);
        if env <= 0.0 {
            continue;
        }

        var contribution = 0.0;
        if kind < 0.5 {
            // RadialRipple — isotropic.
            let r = sqrt(r2);
            contribution = exp(-r / R) * sin(k * r - omega * t);
        } else if kind < 1.5 {
            // DirectionalWake — a teardrop trailing strictly behind the
            // spawn point (the avatar apex). Build a vehicle-relative
            // frame: `along` is distance forward of the apex (>0 ahead,
            // <0 behind), `across` is the lateral offset.
            let sdir = vec2<f32>(a.z, a.w);
            let dir = select(
                vec2<f32>(1.0, 0.0),
                normalize(sdir),
                dot(sdir, sdir) > 1e-6,
            );
            let perp = vec2<f32>(-dir.y, dir.x);
            let along = dot(o, dir);
            let across = dot(o, perp);
            // Trail length / max half-width scale with spawn speed:
            // faster movers throw a longer, slightly broader wake.
            let len = R * (0.8 + 0.3 * spd);
            let half_max = R * 0.4 * (1.0 + 0.1 * spd);
            // Normalised distance behind the apex: 0 at the vehicle,
            // 1 at the far end. Everything ahead (along > 0) or past
            // the tail contributes nothing.
            let u = -along / max(len, 1e-3);
            if along <= 0.0 && u <= 1.0 {
                // Leaf half-width: 0 at the apex (u=0), peak near
                // u≈0.5, back to 0 at the far end (u=1) — a closed
                // teardrop entirely behind the vehicle.
                let halfw = half_max * sqrt(max(0.0, 4.0 * u * (1.0 - u)));
                let vn = select(
                    2.0,
                    across / halfw,
                    halfw > 1e-4,
                );
                if abs(vn) < 1.0 {
                    // Lateral edges fade to 0; a single longitudinal
                    // lobe fades in at the apex and out at the far end
                    // (the "fade-out at the ends"). A faint travelling
                    // ripple adds texture but is kept low so adjacent
                    // stamps blend instead of beating into a stack.
                    let lateral = 1.0 - vn * vn;
                    let lon = sin(3.1415927 * u);
                    let ripple = 1.0 + 0.25 * sin(k * (-along) - omega * t);
                    contribution = lateral * lon * ripple;
                }
            }
        } else {
            // SplashRing — a single crest expanding with age.
            let r = sqrt(r2);
            let ring_r = age_norm * R * splash_expand;
            let d = r - ring_r;
            contribution = exp(-(d * d) / (2.0 * splash_w * splash_w));
        }

        h = h + env * amp_p * contribution;
    }
    return h * strength;
}

// ---------------------------------------------------------------------------
// Fragment entry
// ---------------------------------------------------------------------------

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let t = globals.time * water_uniforms.wave_speed;
    let pos = in.world_position.xyz;

    // Surface basis. Collapses to identity on flat water so a pre-rework
    // record renders pixel-identical. On a tilted surface this gives us
    // the (tangent, normal, bitangent) frame; we keep basis rotation
    // *only* for the wave normal — wave / noise sampling stays in
    // world XZ to keep the visible pattern invariant under tilt. A
    // previous iteration sampled in surface UV, which produced visible
    // band artifacts on any tilt: the in-plane noise grid stretched
    // anisotropically, and the resulting detail-normal pattern aliased
    // with the per-fragment UV step under perspective.
    let n_surface = normalize(in.world_normal);
    let basis = build_surface_basis(n_surface);

    // World-XZ wave coordinate. Identical to the pre-rework shader so
    // flat-water visuals are unchanged; on a tilted plane the pattern
    // stays world-aligned (the tilt manifests through the normal
    // rotation below, not through stretched UVs).
    let xz = pos.xz;

    // Normalise the prevailing direction; guard against a zero input vector
    // (which the editor can produce if a user drags both components to 0)
    // by nudging with a tiny epsilon so normalize() cannot return NaN.
    let prevailing_in = water_uniforms.wave_direction + vec2<f32>(0.0001, 0.0);
    let prevailing = normalize(prevailing_in);

    // Flow-map plumbing. Gravity projected onto the surface gives a
    // downhill direction in world space; we keep the *world-XZ* horizontal
    // component for UV scrolling so the noise frame is consistent with
    // the wave-sampling frame. `flow_speed` (length of the full
    // surface-tangent gravity vector — sin(tilt_angle)) is preserved so
    // scroll-speed kicks at high tilt still fire.
    let flow_amount = clamp(water_uniforms.flow_amount, 0.0, 1.0);
    let g_world = vec3<f32>(0.0, -1.0, 0.0);
    let tangent_g = g_world - n_surface * dot(g_world, n_surface);
    let flow_speed = length(tangent_g);
    var flow_dir_xz = vec2<f32>(0.0, 0.0);
    let flow_horiz_len = length(vec2<f32>(tangent_g.x, tangent_g.z));
    if flow_horiz_len > 1e-4 {
        flow_dir_xz = vec2<f32>(tangent_g.x, tangent_g.z) / flow_horiz_len;
    }

    // Standing-wave amplitude is suppressed as flow_amount climbs — a
    // streaming river is not a sum-of-Gerstner-waves visually. At
    // `flow_amount = 1` the Gerstner term contributes only ~20% of its
    // still-water amplitude so the surface gets most of its texture from
    // scrolling detail normals instead.
    let still_factor = mix(1.0, 0.2, flow_amount);
    let scale = water_uniforms.wave_scale * still_factor;
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

    // Analytic surface normal in *local* (t, n, b) frame, then rotated to
    // world via the surface basis. The augmented tangent `(1, 0, 0) + dx`
    // and bitangent `(0, 0, 1) + dz` live in the local frame because the
    // Gerstner partials are local; their cross product is a local normal.
    let local_tangent = vec3<f32>(1.0, 0.0, 0.0) + total_dx;
    let local_bitangent = vec3<f32>(0.0, 0.0, 1.0) + total_dz;
    var n_gerstner_local = normalize(cross(local_bitangent, local_tangent));
    if n_gerstner_local.y < 0.0 {
        n_gerstner_local = -n_gerstner_local;
    }

    // Avatar-wake perturbation. Sample the wake height field at this
    // fragment and at two small offsets, finite-difference the gradient,
    // and tilt the local-frame normal by that slope. Wake math lives in
    // world XZ (matching the Gerstner sampling frame), so on a tilted
    // water plane the wake follows the surface plane the same way
    // Gerstner waves do — the basis rotation downstream takes care of
    // orienting the perturbed normal back to world space.
    //
    // The `wake_height_at` function bails out cheaply when the channel
    // is disabled, so this whole block costs ~one branch + three trivial
    // function calls on un-waked water.
    let wake_eps = 0.05;
    let h0 = wake_height_at(xz, t);
    let h_dx = wake_height_at(xz + vec2<f32>(wake_eps, 0.0), t) - h0;
    let h_dz = wake_height_at(xz + vec2<f32>(0.0, wake_eps), t) - h0;
    // Tilt the local normal away from the slope direction. Adding
    // `(-grad_x, 0, -grad_z)` to the (0, 1, 0)-aligned local normal
    // rotates it toward the height field's downhill direction.
    n_gerstner_local = normalize(
        n_gerstner_local + vec3<f32>(-h_dx / wake_eps, 0.0, -h_dz / wake_eps),
    );
    if n_gerstner_local.y < 0.0 {
        n_gerstner_local = -n_gerstner_local;
    }

    let n_gerstner = normalize(basis * n_gerstner_local);

    // Scrolling detail normals. Two UV tiling scales are blended by camera
    // distance so the high-frequency sparkle that reads well up close fades
    // into the low-frequency ripple that reads well at distance — kills the
    // repetition artefact the old shader showed on long sightlines.
    let cam_pos = view.world_position;
    let dist = length(cam_pos - pos);
    let far_weight = clamp(smoothstep(30.0, 180.0, dist), 0.0, 1.0);
    let near_weight = 1.0 - far_weight;

    // Scroll direction blends from prevailing-wind drift (still water) to
    // surface-downhill flow (river mode), and the scroll *speed* gains a
    // tilt-proportional kick at high `flow_amount` so a steep slope reads
    // as visibly faster water than a gentle one. Direction is in world
    // XZ, matching the wave-sampling frame.
    let scroll_dir = mix(prevailing, flow_dir_xz, flow_amount);
    let scroll_speed_near = mix(0.35, 0.35 + 1.5 * flow_speed, flow_amount);
    let scroll_speed_far = mix(0.15, 0.15 + 0.7 * flow_speed, flow_amount);
    let near_uv = xz * water_uniforms.normal_scale_near + scroll_dir * t * scroll_speed_near;
    let far_uv = xz * water_uniforms.normal_scale_far + scroll_dir * t * scroll_speed_far;

    // Pixel footprint in each UV space — drives the anti-alias fade inside
    // detail_normal. `fwidth(xz)` is a per-pixel span in *local* (t/b)
    // units; multiply by the tile scale to get the UV-space equivalent.
    let world_footprint = length(fwidth(xz));
    let near_footprint = world_footprint * water_uniforms.normal_scale_near;
    let far_footprint = world_footprint * water_uniforms.normal_scale_far;

    let near_n_local = detail_normal(near_uv, near_footprint);
    let far_n_local = detail_normal(far_uv, far_footprint);
    let detail_local = normalize(near_weight * near_n_local + far_weight * far_n_local);
    let detail = normalize(basis * detail_local);

    // Blend the Gerstner analytic normal with the detail ripple. Reduce the
    // detail contribution with distance so the fine-grain ripple can't
    // dominate the lit result past the scale where its cells are tiny on
    // screen — this is the secondary cushion against aliasing, on top of
    // the footprint fade inside detail_normal itself. Boost the ripple
    // contribution slightly at high `flow_amount` so a river shows visible
    // surface texture even when its standing waves are damped.
    let detail_mix_base = 0.35 * (1.0 - far_weight * 0.75);
    let detail_mix = mix(detail_mix_base, detail_mix_base * 1.6, flow_amount);
    // Project the detail normal onto the tangent plane (subtract its
    // component along `n_surface`) before adding so detail_mix only
    // perturbs *within the surface tangent plane* — without this the
    // accumulated normal can drift away from the surface basis on tilted
    // water, producing a subtle but visible "sheen drift" at grazing.
    let detail_planar = detail - n_surface * dot(detail, n_surface);
    let n = normalize(n_gerstner + detail_mix * detail_planar);
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
    // `total_off.y` is along the local up axis = surface normal, so this
    // stays correct on tilted water without rotation.
    let crest_strength = clamp(total_off.y * 0.6, 0.0, 1.0);
    let scatter = crest_strength * water_uniforms.scatter_color.rgb;

    // Procedural foam where the wave slope is steep, gated by noise so the
    // foam breaks into clumps rather than a continuous halo. `n_gerstner_local.y`
    // (the surface-normal component of the Gerstner local normal) gives the
    // tilt magnitude relative to the rest surface — same semantics as the
    // legacy `n_gerstner.y` on flat water but invariant to the surface
    // basis on tilted water.
    let slope = clamp(1.0 - n_gerstner_local.y, 0.0, 1.0);
    let foam_noise = gradient_noise(xz * 0.6 + scroll_dir * t * 0.5);
    var foam = clamp(
        smoothstep(0.28, 0.8, slope * 1.3 + foam_noise * 0.4) * water_uniforms.foam_amount,
        0.0,
        1.0,
    );

    // Streamline foam: at high `flow_amount` on a tilted surface, add a
    // moving stripe pattern aligned across the flow direction so a river
    // reads as flowing rather than just drifting. Built from a 1D noise
    // sampled along the flow tangent in world XZ (matching the wave
    // sampling frame); gated by `flow_amount * flow_speed` so flat water
    // and still ponds get nothing.
    if flow_amount > 0.001 && flow_horiz_len > 1e-3 {
        let perp = vec2<f32>(-flow_dir_xz.y, flow_dir_xz.x);
        let stripe_uv = vec2<f32>(
            dot(xz, flow_dir_xz) - t * (1.5 + 2.5 * flow_speed),
            dot(xz, perp) * 0.6,
        );
        let stripe = gradient_noise(stripe_uv * 0.7);
        let streamline = smoothstep(0.55, 0.85, stripe) * flow_amount * flow_speed;
        foam = clamp(foam + streamline * water_uniforms.foam_amount, 0.0, 1.0);
    }

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
