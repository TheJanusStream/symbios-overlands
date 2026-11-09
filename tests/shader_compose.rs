//! Every shader in `assets/shaders/`, composed the way Bevy 0.20's
//! `ShaderCache` composes it and validated the way wgpu validates it, under
//! the shader-def sets the game's pipelines really ask for - on the desktop
//! build and on both web bundles - with no GPU (#1893).
//!
//! Before this, no cargo step compiled a shader: the WESL is composed at run
//! time, once per def set, so a broken shader passed every gate and failed
//! only on screen (a WGSL reserved word, twice; the WESL port of #1634). Each
//! composition here fails on what the game would have failed on:
//!
//! - **the composer** (`failed to process shader` at run time): WESL syntax,
//!   an import that names no module (at run time only a warning, `has an
//!   unresolved import`, after which the material never draws), an `@if`
//!   arm that leaves a name undefined;
//! - **naga** (`failed to create shader module`): the composed WGSL parsed
//!   and validated with the capabilities wgpu would give naga on that target
//!   (a type error, a missing field, a capability the target lacks);
//! - **the pipeline**: exactly one entry point for each stage the pipeline
//!   draws with this shader (Bevy names none, so wgpu needs one), no bind
//!   group past the device's `max_bind_groups`, and a prepass or shadow pass
//!   that reads only the group-0 bindings the prepass view layout has (the
//!   `mesh_view_bindings::globals` trap: binding 11 in a layout without it).
//!
//! ## How it mirrors Bevy, and how to re-derive it after a Bevy bump
//!
//! [`compose`] is `ShaderCache::get` (bevy_shader `src/shader_cache.rs`): the
//! same wesl options, the same asset-path and `crate::path` module mapping,
//! the same generated `constants` module for `Int`/`UInt` defs, and the same
//! def order - the defs of every shader the entry's imports reach, then the
//! pipeline's, then the entry's own. Bevy's own shaders are read from the
//! very sources the build compiled, found with `cargo metadata` ([`graph`]).
//!
//! The def sets are built by [`forward_defs`] and [`prepass_defs`], which
//! follow `MeshPipeline::specialize` (bevy_pbr `src/render/mesh.rs`) and
//! `PrepassPipeline::specialize` (bevy_pbr `src/prepass/mod.rs`) push for
//! push, from the keys this game's cameras, lights, meshes and materials
//! produce ([`uses`]). Every push names its source; device-dependent values
//! come from Bevy's own functions and wgpu's own limits where they are public,
//! and from a cited line where they are not. After a bump, re-read those two
//! `specialize` functions, `PipelineCache::new` (bevy_render
//! `src/render_resource/pipeline_cache.rs`), `MeshRenderPlugin` (the
//! libraries loaded with defs, bevy_pbr `src/render/mesh.rs`) and the light
//! constants (bevy_pbr `src/render/light.rs`), and move what moved.
//!
//! ## What it cannot see
//!
//! The pipeline layout beyond the prepass rule (a main-pass binding the view
//! layout lacks), GPU limits beyond the bind-group count (texture slots are
//! `shader_guards.rs`'s), the GLSL that naga writes for WebGL2 (this crate
//! does not build naga's `glsl-out`), driver and browser behaviour, and
//! whether anything draws right. The browser check of both bundles stays.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use bevy::pbr::{
    MATERIAL_BIND_GROUP_INDEX, MAX_CASCADES_PER_LIGHT, MAX_DIRECTIONAL_LIGHTS, MAX_RECT_LIGHTS,
    MeshUniform, TONEMAPPING_LUT_SAMPLER_BINDING_INDEX, TONEMAPPING_LUT_TEXTURE_BINDING_INDEX,
};
use bevy::render::render_resource::{DownlevelFlags, GpuArrayBuffer, WgpuFeatures, WgpuLimits};
use bevy::render::storage_buffers_are_unsupported;
use bevy::shader::ShaderDefVal;
use naga::ShaderStage;
use wesl::syntax::{ImportContent, ModulePath, PathOrigin, TranslationUnit};

// ---------------------------------------------------------------------------
// Where Bevy's shaders are
// ---------------------------------------------------------------------------

/// What the build resolved, read once per process.
struct Graph {
    /// Each package's `src` directory, by package name. Bevy embeds a crate's
    /// shaders as `embedded://<crate>/<path under src>` and imports them as
    /// `<crate>::<path>` (bevy_shader `src/shader.rs`, `Shader::from_wesl`),
    /// so `bevy_pbr::render::mesh_view_bindings` is
    /// `<bevy_pbr>/src/render/mesh_view_bindings.wesl`.
    src: BTreeMap<String, Vec<PathBuf>>,
    /// The features the build enables on each package, by package name.
    features: BTreeMap<String, BTreeSet<String>>,
}

impl Graph {
    fn src(&self, package: &str) -> Result<&Path, String> {
        match self.src.get(package).map(Vec::as_slice) {
            Some([one]) => Ok(one),
            Some([]) | None => Err(format!("no package `{package}` in the build's graph")),
            Some(many) => Err(format!(
                "the build's graph holds `{package}` {} times: {many:?}",
                many.len()
            )),
        }
    }

    fn features(&self, package: &str) -> &BTreeSet<String> {
        static NONE: BTreeSet<String> = BTreeSet::new();
        self.features.get(package).unwrap_or(&NONE)
    }
}

/// The build's dependency graph, from `cargo metadata`: the source of every
/// Bevy shader this test composes against, at exactly the version the test
/// binary was built with (no registry path is written down anywhere).
///
/// `CARGO` names the cargo running the test (`cargo test` and nextest both
/// set it). `--filter-platform host-tuple` keeps the resolve to what this host
/// builds, which is exactly the set of packages the build itself has already
/// downloaded and locked; with that, `--offline` can never need the network,
/// so the call works on CI (fresh resolve, sources fetched by the build that
/// produced this binary) and on an offline laptop alike.
fn graph() -> &'static Graph {
    static GRAPH: OnceLock<Graph> = OnceLock::new();
    GRAPH.get_or_init(|| {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let output = Command::new(&cargo)
            .args([
                "metadata",
                "--format-version",
                "1",
                "--offline",
                "--filter-platform",
                "host-tuple",
                "--manifest-path",
            ])
            .arg(&manifest)
            .output()
            .unwrap_or_else(|e| panic!("running {cargo:?} metadata: {e}"));
        assert!(
            output.status.success(),
            "`cargo metadata` failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let meta: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("cargo metadata prints JSON");
        let mut names = BTreeMap::new();
        let mut src: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
        for package in meta["packages"]
            .as_array()
            .expect("metadata lists packages")
        {
            let name = package["name"].as_str().expect("a package has a name");
            let id = package["id"].as_str().expect("a package has an id");
            let manifest = package["manifest_path"].as_str().expect("a manifest path");
            names.insert(id.to_owned(), name.to_owned());
            let dir = Path::new(manifest)
                .parent()
                .expect("a manifest is in a directory");
            src.entry(name.to_owned())
                .or_default()
                .push(dir.join("src"));
        }
        let mut features: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for node in meta["resolve"]["nodes"]
            .as_array()
            .expect("metadata resolves")
        {
            let Some(name) = node["id"].as_str().and_then(|id| names.get(id)) else {
                continue;
            };
            let enabled = features.entry(name.clone()).or_default();
            for feature in node["features"].as_array().into_iter().flatten() {
                if let Some(feature) = feature.as_str() {
                    enabled.insert(feature.to_owned());
                }
            }
        }
        Graph { src, features }
    })
}

fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
}

/// Bevy's canonical form of a module path: a package written as a path keeps
/// its last segment (bevy_shader `src/shader_cache.rs`, `canonical_path`).
fn canonical(path: &ModulePath) -> ModulePath {
    match &path.origin {
        PathOrigin::Package(pkg) if pkg.contains('/') => ModulePath {
            origin: PathOrigin::Package(pkg.rsplit('/').next().unwrap().to_string()),
            components: path.components.clone(),
        },
        _ => path.clone(),
    }
}

/// A module path as one string: `bevy_pbr::render::mesh_bindings` for a
/// package's module, `/shaders/sky` for an asset (Bevy's `ShaderImport`
/// spellings).
fn key(path: &ModulePath) -> String {
    match &path.origin {
        PathOrigin::Package(pkg) => std::iter::once(pkg.as_str())
            .chain(path.components.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join("::"),
        PathOrigin::Absolute => format!("/{}", path.components.join("/")),
        PathOrigin::Relative(_) => format!("relative {}", path.components.join("/")),
    }
}

/// The asset shader `assets/shaders/<name>.wesl`, as Bevy imports it.
fn asset_module(name: &str) -> ModulePath {
    ModulePath {
        origin: PathOrigin::Absolute,
        components: vec!["shaders".into(), name.into()],
    }
}

/// The file a module lives in: an asset under `assets/`, a package's module
/// under that package's `src` (see [`Graph::src`]).
fn module_file(path: &ModulePath) -> Result<PathBuf, String> {
    let path = canonical(path);
    let relative = path.components.join("/");
    match &path.origin {
        PathOrigin::Package(pkg) => Ok(graph().src(pkg)?.join(relative).with_extension("wesl")),
        PathOrigin::Absolute => Ok(assets().join(relative).with_extension("wesl")),
        PathOrigin::Relative(_) => Err(format!("a relative path reached the resolver: {relative}")),
    }
}

/// `ShaderResolver` (bevy_shader `src/shader_cache.rs`): `package::constants`
/// is the generated module of `Int`/`UInt` defs, everything else a file.
struct Resolver<'a> {
    constants: &'a str,
}

impl wesl::Resolver for Resolver<'_> {
    fn resolve_source<'a>(
        &'a self,
        path: &ModulePath,
    ) -> Result<Cow<'a, str>, wesl::error::ResolveError> {
        let path = canonical(path);
        if path.origin == PathOrigin::Package("constants".into()) && path.components.is_empty() {
            return Ok(Cow::Borrowed(self.constants));
        }
        let not_found = |why: String| wesl::error::ResolveError::ModuleNotFound(path.clone(), why);
        let file = module_file(&path).map_err(not_found)?;
        std::fs::read_to_string(&file)
            .map(Cow::Owned)
            .map_err(|e| not_found(format!("{}: {e}", file.display())))
    }

    fn display_name(&self, path: &ModulePath) -> Option<String> {
        module_file(path).ok().map(|f| f.display().to_string())
    }

    fn canonical_path(&self, path: &ModulePath) -> ModulePath {
        canonical(path)
    }
}

/// `scan_wesl_imports` (bevy_shader `src/shader.rs`): every path a module's
/// imports name - each module, and each module joined with the imported
/// item - whatever `@if` guards them. Bevy registers a shader's imports this
/// way and walks them for the defs of every shader the entry can reach.
fn scan_wesl_imports(source: &str, self_module_path: &ModulePath) -> Vec<ModulePath> {
    fn leaves(content: &ImportContent, path: ModulePath, out: &mut Vec<ModulePath>) {
        match content {
            ImportContent::Item(item) => {
                let mut full = path.clone();
                full.push(&item.ident.to_string());
                out.push(path);
                out.push(full);
            }
            ImportContent::Collection(collection) => {
                for import in collection {
                    let path = path.clone().join(import.path.iter().cloned());
                    leaves(&import.content, path, out);
                }
            }
        }
    }

    let Ok(translation_unit) = source.parse::<TranslationUnit>() else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for statement in &translation_unit.imports {
        match &statement.path {
            Some(import_path) => {
                let path = self_module_path.join_path(import_path);
                leaves(&statement.content, path, &mut paths);
            }
            None => {
                if let ImportContent::Collection(collection) = &statement.content {
                    for import in collection {
                        let mut components = import.path.iter().cloned();
                        if let Some(package) = components.next() {
                            let path =
                                ModulePath::new(PathOrigin::Package(package), components.collect());
                            leaves(&import.content, path, &mut paths);
                        }
                    }
                }
            }
        }
    }
    paths
        .iter()
        .map(canonical)
        .filter(|path| !matches!(path.origin, PathOrigin::Relative(_)))
        .collect()
}

/// The shaders an entry's imports reach, by [`key`], the entry left out:
/// the walk at the top of `ShaderCache::get` that gathers `closure_defs`.
/// Bevy follows an import only to a registered shader; here, to a file.
fn import_closure(entry: &str) -> BTreeSet<String> {
    let entry = asset_module(entry);
    let mut visited = BTreeSet::new();
    let mut to_visit = vec![entry.clone()];
    while let Some(path) = to_visit.pop() {
        if !visited.insert(key(&path)) {
            continue;
        }
        let Some(source) = module_file(&path)
            .ok()
            .and_then(|f| std::fs::read_to_string(f).ok())
        else {
            continue;
        };
        to_visit.extend(
            scan_wesl_imports(&source, &path)
                .into_iter()
                .filter(|import| module_file(import).is_ok_and(|f| f.is_file())),
        );
    }
    visited.remove(&key(&entry));
    visited
}

// ---------------------------------------------------------------------------
// The three builds
// ---------------------------------------------------------------------------

/// A build the game ships: native, and the two browser bundles (#1670).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Target {
    Desktop,
    WebGpu,
    WebGl2,
}

const EVERY_TARGET: &[Target] = &[Target::Desktop, Target::WebGpu, Target::WebGl2];
/// What the WebGL2 bundle leaves out (`build.rs`, `cfg(webgl2)`): MSAA and
/// the depth prepass (`src/camera.rs`), and Bevy's deferred path.
const NOT_WEBGL2: &[Target] = &[Target::Desktop, Target::WebGpu];

impl Target {
    fn name(self) -> &'static str {
        match self {
            Target::Desktop => "desktop",
            Target::WebGpu => "webgpu",
            Target::WebGl2 => "webgl2",
        }
    }

    /// Bevy's `cfg(all(feature = "webgl", target_arch = "wasm32",
    /// not(feature = "webgpu")))`: this crate always enables `webgl2`, and
    /// the WebGPU bundle adds `webgpu` over it (`Cargo.toml`).
    fn webgl2(self) -> bool {
        self == Target::WebGl2
    }

    /// `IRRADIANCE_VOLUMES_ARE_USABLE = cfg!(not(target_arch = "wasm32"))`
    /// (bevy_pbr `src/light_probe/irradiance_volume.rs`).
    fn irradiance_volumes(self) -> bool {
        self == Target::Desktop
    }

    /// wgpu's limits for the target: its WebGPU defaults on the desktop and
    /// the WebGPU bundle (a desktop adapter offers more storage buffers,
    /// which only raises a constant), its WebGL2 defaults there - no storage
    /// buffers, 16 KiB uniform bindings.
    fn limits(self) -> WgpuLimits {
        match self {
            Target::WebGl2 => WgpuLimits::downlevel_webgl2_defaults(),
            Target::Desktop | Target::WebGpu => WgpuLimits::default(),
        }
    }

    /// `binding_arrays_are_usable` (bevy_pbr `src/light_probe/mod.rs`):
    /// texture binding arrays with non-uniform indexing, native-only
    /// features no browser offers.
    fn binding_arrays(self) -> bool {
        self == Target::Desktop
    }

    /// The device features the shaders may lean on: the two that make
    /// binding arrays usable where they are, nothing else. Bevy requests
    /// every feature a desktop adapter has, so this is the floor of what a
    /// desktop GPU gives naga, and the WebGPU baseline on the web.
    fn features(self) -> WgpuFeatures {
        if self.binding_arrays() {
            WgpuFeatures::TEXTURE_BINDING_ARRAY
                | WgpuFeatures::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING
        } else {
            WgpuFeatures::empty()
        }
    }

    /// Downlevel flags: a desktop GPU and WebGPU are compliant; of the three
    /// flags naga reads (cube array textures, multisampled shading, f16 in
    /// f32), WebGL2 has none - so it validates as the downlevel target it is.
    fn downlevel(self) -> DownlevelFlags {
        match self {
            Target::WebGl2 => DownlevelFlags::empty(),
            Target::Desktop | Target::WebGpu => DownlevelFlags::compliant(),
        }
    }

    /// `storage_buffers_are_unsupported` (bevy_render `src/lib.rs`), which
    /// switches metadata and skins to uniform buffers in both pipelines.
    fn storage_buffers(self) -> bool {
        !storage_buffers_are_unsupported(&self.limits())
    }

    /// `PipelineCache::new` (bevy_render
    /// `src/render_resource/pipeline_cache.rs`): spliced into every shader's
    /// own defs by `extract_shaders`.
    fn global_defs(self) -> Vec<ShaderDefVal> {
        let mut defs: Vec<ShaderDefVal> = Vec::new();
        if self.webgl2() {
            defs.push("NO_ARRAY_TEXTURES_SUPPORT".into());
            defs.push("NO_CUBE_ARRAY_TEXTURES_SUPPORT".into());
            defs.push("SIXTEEN_BYTE_ALIGNMENT".into());
        }
        let bindings = self.limits().max_storage_buffers_per_shader_stage;
        defs.push(ShaderDefVal::UInt(
            "AVAILABLE_STORAGE_BUFFER_BINDINGS".into(),
            bindings,
        ));
        defs.push(ShaderDefVal::Bool(
            "AVAILABLE_STORAGE_BUFFER_BINDINGS__GE_3".into(),
            bindings >= 3,
        ));
        defs.push(ShaderDefVal::Bool(
            "AVAILABLE_STORAGE_BUFFER_BINDINGS__GE_6".into(),
            bindings >= 6,
        ));
        defs
    }

    /// `GpuArrayBuffer::<MeshUniform>::batch_size` (bevy_render
    /// `src/render_resource/gpu_array_buffer.rs`): `Some` only without
    /// storage buffers, the meshes then batched in uniform arrays.
    fn per_object_batch(self) -> Option<u32> {
        GpuArrayBuffer::<MeshUniform>::batch_size(&self.limits())
    }

    /// The defs a Bevy library is loaded with, by module ([`key`]):
    /// `MeshRenderPlugin::build` and `::finish` (bevy_pbr
    /// `src/render/mesh.rs`, the two `load_shader_library!` calls with
    /// settings). The light counts are bevy_pbr `src/render/light.rs`'s,
    /// whose WebGL2 values are behind the WebGL2 cfg and so are written here.
    fn library_defs(self, module: &str) -> Vec<ShaderDefVal> {
        match module {
            "bevy_pbr::render::mesh_view_types" => {
                let (lights, cascades) = if self.webgl2() {
                    (1, 1)
                } else {
                    (MAX_DIRECTIONAL_LIGHTS, MAX_CASCADES_PER_LIGHT)
                };
                vec![
                    ShaderDefVal::UInt("MAX_DIRECTIONAL_LIGHTS".into(), lights as u32),
                    ShaderDefVal::UInt("MAX_CASCADES_PER_LIGHT".into(), cascades as u32),
                    ShaderDefVal::UInt("MAX_RECT_LIGHTS".into(), MAX_RECT_LIGHTS as u32),
                ]
            }
            "bevy_pbr::render::mesh_bindings" => self
                .per_object_batch()
                .map(|n| ShaderDefVal::UInt("PER_OBJECT_BUFFER_BATCH_SIZE".into(), n))
                .into_iter()
                .collect(),
            _ => Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// The pipelines the game makes
// ---------------------------------------------------------------------------

/// The vertex attributes of a mesh (positions always).
#[derive(Clone, Copy)]
struct Mesh {
    normals: bool,
    uv0: bool,
    uv1: bool,
    tangents: bool,
    colors: bool,
}

const PLAIN: Mesh = Mesh {
    normals: true,
    uv0: true,
    uv1: false,
    tangents: false,
    colors: false,
};
const TANGENT: Mesh = Mesh {
    tangents: true,
    ..PLAIN
};
const COLOURED: Mesh = Mesh {
    colors: true,
    ..PLAIN
};
const FULL: Mesh = Mesh {
    normals: true,
    uv0: true,
    uv1: true,
    tangents: true,
    colors: true,
};

/// A material's `AlphaMode`, as the pipeline keys see it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Alpha {
    Opaque,
    Mask,
    Blend,
    /// `Premultiplied` and `Add` share a key (bevy_pbr `src/material.rs`,
    /// `alpha_mode_pipeline_key`).
    Premultiplied,
}

/// A camera, as the forward key sees it (bevy_pbr `src/render/mesh.rs`,
/// the view key).
#[derive(Clone, Copy)]
struct View {
    fog: bool,
    depth_prepass: bool,
    msaa: bool,
    env_map: bool,
    tonemap_in_shader: bool,
    oit: bool,
}

/// The game's camera (`src/camera.rs`): distance fog, a depth prepass and
/// 4x MSAA everywhere but WebGL2, an environment map light, HDR tonemapping
/// in the post pass.
const GAME_CAMERA: View = View {
    fog: true,
    depth_prepass: true,
    msaa: true,
    env_map: true,
    tonemap_in_shader: false,
    oit: false,
};

/// A camera with none of that: no fog, no prepass, `Msaa::Off`, tonemapped
/// in the shader - the item preview (`src/item_preview.rs`) and the agent's
/// look camera (`src/agent/daemon/look.rs`). Every forward shader is also
/// composed through it, so the other arm of each view `@if` compiles.
const BARE_CAMERA: View = View {
    fog: false,
    depth_prepass: false,
    msaa: false,
    env_map: false,
    tonemap_in_shader: true,
    oit: false,
};

/// [`GAME_CAMERA`] with order-independent transparency: no camera here has
/// it today, but `weathered.wesl` carries the `MATERIAL_OIT_ENABLED` arm.
const OIT_CAMERA: View = View {
    oit: true,
    ..GAME_CAMERA
};

/// The prepass pipeline's uses (bevy_pbr `src/prepass/mod.rs`).
#[derive(Clone, Copy)]
enum Prepass {
    /// The game camera's `DepthPrepass` (`src/camera.rs`).
    Depth,
    /// Normal and motion-vector prepasses: no camera asks for them, but
    /// `wind.wesl` carries their arms.
    NormalMotion,
    /// The deferred g-buffer pass: this renderer is forward-only, but
    /// `wind.wesl` and `splat.wesl` serve it (`src/wind.rs`, `src/splat.rs`).
    Deferred,
    /// A directional light's shadow map (bevy_pbr `src/render/light.rs`,
    /// `queue_shadows`: orthographic, `UNCLIPPED_DEPTH_ORTHO`), on a device
    /// with or without `DEPTH_CLIP_CONTROL` - without it the depth is clipped
    /// in the shader (`UNCLIPPED_DEPTH_ORTHO_EMULATION`).
    DirectionalShadow { depth_clip_control: bool },
    /// A point light's shadow map (perspective): the item preview's lamp.
    PointShadow,
}

#[derive(Clone, Copy)]
enum Pass {
    Forward(View),
    Prepass(Prepass),
}

/// One pipeline that draws with a shader.
struct Pipeline {
    label: &'static str,
    pass: Pass,
    mesh: Mesh,
    alpha: Alpha,
    /// `VISIBILITY_RANGE_DITHER`: a mesh with a `VisibilityRange`.
    visibility_range: bool,
    /// The defs the material's own `specialize` pushes: `StandardMaterial`'s
    /// key (bevy_pbr `src/pbr_material.rs`), `WeatherExtension`'s
    /// (`src/weathered.rs`).
    material: &'static [&'static str],
    targets: &'static [Target],
}

/// A shader the game loads, the stages it serves, and every pipeline that
/// draws with it.
struct Use {
    /// `assets/shaders/<shader>.wesl`.
    shader: &'static str,
    /// The stages its pipelines take from it, each by its single entry point.
    stages: &'static [ShaderStage],
    pipelines: Vec<Pipeline>,
}

const VERTEX: &[ShaderStage] = &[ShaderStage::Vertex];
const FRAGMENT: &[ShaderStage] = &[ShaderStage::Fragment];
const BOTH: &[ShaderStage] = &[ShaderStage::Vertex, ShaderStage::Fragment];

fn pipeline(label: &'static str, pass: Pass, mesh: Mesh, alpha: Alpha) -> Pipeline {
    Pipeline {
        label,
        pass,
        mesh,
        alpha,
        visibility_range: false,
        material: &[],
        targets: EVERY_TARGET,
    }
}

impl Pipeline {
    fn material(mut self, defs: &'static [&'static str]) -> Self {
        self.material = defs;
        self
    }

    fn visibility_range(mut self) -> Self {
        self.visibility_range = true;
        self
    }

    fn on(mut self, targets: &'static [Target]) -> Self {
        self.targets = targets;
        self
    }
}

/// Every shader the game loads and the pipelines that use it, from the
/// `Material`/`MaterialExtension` impls (which stages, which passes) and the
/// meshes and alpha modes they draw.
fn uses() -> Vec<Use> {
    use Alpha::*;
    use Pass::{Forward, Prepass as Pre};
    use Prepass::*;
    let clipped = DirectionalShadow {
        depth_clip_control: true,
    };
    let emulated = DirectionalShadow {
        depth_clip_control: false,
    };
    vec![
        // src/sky.rs: a `Material`, both stages, no prepass, no shadows.
        Use {
            shader: "sky",
            stages: BOTH,
            pipelines: vec![
                pipeline("forward", Forward(GAME_CAMERA), PLAIN, Opaque),
                pipeline("forward-bare", Forward(BARE_CAMERA), PLAIN, Opaque),
            ],
        },
        // src/clouds.rs: the deck's fragment in the main pass (blended).
        Use {
            shader: "cloud",
            stages: FRAGMENT,
            pipelines: vec![
                pipeline("forward", Forward(GAME_CAMERA), PLAIN, Blend),
                pipeline("forward-bare", Forward(BARE_CAMERA), PLAIN, Blend),
            ],
        },
        // src/clouds.rs: the deck's prepass shaders, reached only by the
        // sun's shadow pass (`enable_prepass` false, `enable_shadows` true).
        Use {
            shader: "cloud_shadow",
            stages: BOTH,
            pipelines: vec![
                pipeline("shadow-directional", Pre(clipped), PLAIN, Blend),
                pipeline("shadow-directional-emulated", Pre(emulated), PLAIN, Blend),
            ],
        },
        // src/water.rs: fragment only, main pass only.
        Use {
            shader: "water",
            stages: FRAGMENT,
            pipelines: vec![
                pipeline("forward", Forward(GAME_CAMERA), PLAIN, Blend),
                pipeline("forward-tangents", Forward(GAME_CAMERA), TANGENT, Blend),
                pipeline("forward-bare", Forward(BARE_CAMERA), PLAIN, Blend),
            ],
        },
        // src/splat.rs: the terrain's fragment, forward and deferred.
        Use {
            shader: "splat",
            stages: FRAGMENT,
            pipelines: vec![
                pipeline("forward", Forward(GAME_CAMERA), TANGENT, Opaque),
                pipeline("forward-no-tangents", Forward(GAME_CAMERA), PLAIN, Opaque),
                pipeline("forward-bare", Forward(BARE_CAMERA), TANGENT, Opaque),
                pipeline("deferred", Pre(Deferred), TANGENT, Opaque).on(NOT_WEBGL2),
            ],
        },
        // src/weathered.rs: Bevy's PBR fragment, weathered where the
        // material weathers (`WEATHERED`, `DUSK_TO_DAWN` on the fragment).
        Use {
            shader: "weathered",
            stages: FRAGMENT,
            pipelines: vec![
                pipeline("forward-plain", Forward(GAME_CAMERA), TANGENT, Opaque),
                pipeline("forward-weathered", Forward(GAME_CAMERA), TANGENT, Opaque)
                    .material(&["WEATHERED"]),
                pipeline("forward-dusk", Forward(GAME_CAMERA), TANGENT, Opaque)
                    .material(&["WEATHERED", "DUSK_TO_DAWN"]),
                pipeline("forward-masked-mapped", Forward(GAME_CAMERA), FULL, Mask)
                    .material(&["WEATHERED", "DUSK_TO_DAWN", "STANDARD_MATERIAL_NORMAL_MAP"])
                    .visibility_range(),
                pipeline(
                    "forward-premultiplied",
                    Forward(GAME_CAMERA),
                    PLAIN,
                    Premultiplied,
                )
                .material(&["WEATHERED"]),
                pipeline("forward-bare", Forward(BARE_CAMERA), TANGENT, Opaque)
                    .material(&["WEATHERED"]),
                pipeline("forward-oit", Forward(OIT_CAMERA), PLAIN, Blend)
                    .material(&["WEATHERED"])
                    .on(NOT_WEBGL2),
            ],
        },
        // src/wind.rs: the foliage's vertex in the main pass, the prepass
        // (the camera's depth prepass and every shadow map) and deferred.
        Use {
            shader: "wind",
            stages: VERTEX,
            pipelines: vec![
                pipeline("forward", Forward(GAME_CAMERA), PLAIN, Mask),
                pipeline("forward-full", Forward(GAME_CAMERA), FULL, Mask).visibility_range(),
                pipeline("forward-bare", Forward(BARE_CAMERA), PLAIN, Mask),
                pipeline("depth-prepass", Pre(Depth), PLAIN, Mask).on(NOT_WEBGL2),
                pipeline("shadow-directional", Pre(clipped), PLAIN, Mask),
                pipeline("shadow-directional-emulated", Pre(emulated), FULL, Mask),
                pipeline("shadow-point", Pre(PointShadow), PLAIN, Mask),
                pipeline("prepass-normal-motion", Pre(NormalMotion), FULL, Mask)
                    .visibility_range()
                    .on(NOT_WEBGL2),
                pipeline("deferred", Pre(Deferred), FULL, Opaque).on(NOT_WEBGL2),
            ],
        },
        // src/grass/material.rs: the blades' vertex, main pass only.
        Use {
            shader: "grass",
            stages: VERTEX,
            pipelines: vec![
                pipeline("forward", Forward(GAME_CAMERA), COLOURED, Opaque),
                pipeline(
                    "forward-dither",
                    Forward(GAME_CAMERA),
                    Mesh {
                        tangents: true,
                        ..COLOURED
                    },
                    Opaque,
                )
                .visibility_range(),
                pipeline("forward-bare", Forward(BARE_CAMERA), COLOURED, Opaque),
            ],
        },
        // src/weather/precipitation.rs: a `Material`, both stages, blended,
        // vertex colours.
        Use {
            shader: "precipitation",
            stages: BOTH,
            pipelines: vec![
                pipeline("forward", Forward(GAME_CAMERA), COLOURED, Blend),
                pipeline("forward-bare", Forward(BARE_CAMERA), COLOURED, Blend),
            ],
        },
    ]
}

/// The vertex-attribute defs, in the order both `specialize`s push them.
fn push_attributes(defs: &mut Vec<ShaderDefVal>, mesh: Mesh) {
    defs.push("VERTEX_POSITIONS".into());
    if mesh.normals {
        defs.push("VERTEX_NORMALS".into());
    }
    if mesh.uv0 {
        defs.push("VERTEX_UVS".into());
        defs.push("VERTEX_UVS_A".into());
    }
    if mesh.uv1 {
        defs.push("VERTEX_UVS".into());
        defs.push("VERTEX_UVS_B".into());
    }
    if mesh.tangents {
        defs.push("VERTEX_TANGENTS".into());
    }
    if mesh.colors {
        defs.push("VERTEX_COLORS".into());
    }
}

/// bevy_pbr's cargo features and the defs `MeshPipeline::specialize` pushes
/// for them, read off the build's graph so a feature switched on in
/// `Cargo.toml` reaches the matrix by itself.
const PBR_FEATURE_DEFS: &[(&str, &str)] = &[
    (
        "pbr_transmission_textures",
        "PBR_TRANSMISSION_TEXTURES_SUPPORTED",
    ),
    (
        "pbr_multi_layer_material_textures",
        "PBR_MULTI_LAYER_MATERIAL_TEXTURES_SUPPORTED",
    ),
    ("pbr_anisotropy_texture", "PBR_ANISOTROPY_TEXTURE_SUPPORTED"),
    ("pbr_specular_textures", "PBR_SPECULAR_TEXTURES_SUPPORTED"),
    ("bluenoise_texture", "BLUE_NOISE_TEXTURE"),
    ("dfg_lut", "DFG_LUT"),
    ("area_light_luts", "AREA_LIGHT_LUTS"),
];

/// `MeshPipeline::specialize` (bevy_pbr `src/render/mesh.rs`) and then
/// `MaterialPipelineSpecializer::specialize` (bevy_pbr `src/material.rs`),
/// push for push, for one forward pipeline.
fn forward_defs(target: Target, view: View, p: &Pipeline) -> Vec<ShaderDefVal> {
    let pbr = graph().features("bevy_pbr");
    let mut defs: Vec<ShaderDefVal> = vec![
        "MESH_PIPELINE".into(),
        "VERTEX_OUTPUT_INSTANCE_INDEX".into(),
    ];
    push_attributes(&mut defs, p.mesh);
    for (feature, def) in PBR_FEATURE_DEFS {
        if pbr.contains(*feature) {
            defs.push((*def).into());
        }
    }
    // `msaa_samples() > 1`: `EdgeSmoothing::BUILD` is FXAA on WebGL2.
    if view.msaa && !target.webgl2() {
        defs.push("MULTISAMPLED".into());
    }
    if !target.storage_buffers() {
        defs.push("METADATA_USE_UNIFORM_BUFFERS".into());
        // `setup_morph_and_skinning_defs`.
        defs.push("SKINS_USE_UNIFORM_BUFFERS".into());
    }
    if view.oit {
        defs.push("OIT_ENABLED".into());
    }
    if p.alpha == Alpha::Premultiplied {
        defs.push("PREMULTIPLY_ALPHA".into());
        defs.push("BLEND_PREMULTIPLIED_ALPHA".into());
    }
    // `#[cfg(not(webgl2))] DepthPrepass` on the game camera.
    if view.depth_prepass && !target.webgl2() {
        defs.push("DEPTH_PREPASS".into());
    }
    defs.push("VIEW_PROJECTION_PERSPECTIVE".into());
    if target.webgl2() {
        defs.push("WEBGL2".into());
    }
    if pbr.contains("experimental_pbr_pcss") {
        defs.push("PCSS_SAMPLERS_AVAILABLE".into());
    }
    if view.tonemap_in_shader {
        defs.push("TONEMAP_IN_SHADER".into());
        defs.push(ShaderDefVal::UInt(
            "TONEMAPPING_LUT_TEXTURE_BINDING_INDEX".into(),
            TONEMAPPING_LUT_TEXTURE_BINDING_INDEX,
        ));
        defs.push(ShaderDefVal::UInt(
            "TONEMAPPING_LUT_SAMPLER_BINDING_INDEX".into(),
            TONEMAPPING_LUT_SAMPLER_BINDING_INDEX,
        ));
        // `camera::TONEMAPPING` (`src/camera.rs`).
        defs.push("TONEMAP_METHOD_SOMEWHAT_BORING_DISPLAY_TRANSFORM".into());
        defs.push("DEBAND_DITHER".into());
    }
    if p.alpha == Alpha::Mask {
        defs.push("MAY_DISCARD".into());
    }
    if view.env_map {
        defs.push("ENVIRONMENT_MAP".into());
    }
    // `ShadowFilteringMethod::default()`.
    defs.push("SHADOW_FILTER_METHOD_GAUSSIAN".into());
    // No transmission: the key's two quality bits are zero, `_LOW`, 4 taps.
    defs.push(ShaderDefVal::Int(
        "SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS".into(),
        4,
    ));
    if p.visibility_range {
        defs.push("VISIBILITY_RANGE_DITHER".into());
    }
    if view.fog {
        defs.push("DISTANCE_FOG".into());
    }
    if target.binding_arrays() {
        defs.push("MULTIPLE_LIGHT_PROBES_IN_ARRAY".into());
        defs.push("MULTIPLE_LIGHTMAPS_IN_ARRAY".into());
    }
    if target.irradiance_volumes() {
        defs.push("IRRADIANCE_VOLUMES_ARE_USABLE".into());
    }
    // `clustered_decals_are_usable` (bevy_pbr `src/decal/clustered.rs`).
    if target.binding_arrays() && pbr.contains("pbr_clustered_decals") {
        defs.push("CLUSTERED_DECALS_ARE_USABLE".into());
        if pbr.contains("pbr_light_textures") {
            defs.push("LIGHT_TEXTURES".into());
        }
    }
    if let Some(n) = target.per_object_batch() {
        defs.push(ShaderDefVal::UInt("PER_OBJECT_BUFFER_BATCH_SIZE".into(), n));
    }
    // bevy_pbr `src/material.rs`.
    defs.push(ShaderDefVal::UInt(
        "MATERIAL_BIND_GROUP".into(),
        MATERIAL_BIND_GROUP_INDEX as u32,
    ));
    if view.oit && matches!(p.alpha, Alpha::Blend | Alpha::Premultiplied) {
        defs.push("MATERIAL_OIT_ENABLED".into());
    }
    defs.extend(p.material.iter().map(|d| ShaderDefVal::from(*d)));
    defs
}

/// `PrepassPipeline::specialize` (bevy_pbr `src/prepass/mod.rs`), push for
/// push, for one prepass or shadow pipeline.
fn prepass_defs(target: Target, pass: Prepass, p: &Pipeline) -> Vec<ShaderDefVal> {
    let (depth, normal, motion, deferred) = match pass {
        Prepass::Depth => (true, false, false, false),
        Prepass::NormalMotion => (true, true, true, false),
        Prepass::Deferred => (true, false, false, true),
        // `queue_shadows`: `DEPTH_PREPASS` is the light's whole key.
        Prepass::DirectionalShadow { .. } | Prepass::PointShadow => (true, false, false, false),
    };
    let shadow = matches!(
        pass,
        Prepass::DirectionalShadow { .. } | Prepass::PointShadow
    );
    // A shadow caster that is not opaque may discard (`queue_shadows`); the
    // camera's prepasses draw opaque and masked materials only.
    let may_discard = if shadow {
        p.alpha != Alpha::Opaque
    } else {
        p.alpha == Alpha::Mask
    };
    let emulate_unclipped_depth = matches!(
        pass,
        Prepass::DirectionalShadow {
            depth_clip_control: false
        }
    );

    let mut defs: Vec<ShaderDefVal> = vec![
        "PREPASS_PIPELINE".into(),
        ShaderDefVal::UInt(
            "MATERIAL_BIND_GROUP".into(),
            MATERIAL_BIND_GROUP_INDEX as u32,
        ),
    ];
    if target.webgl2() {
        defs.push("WEBGL2".into());
    }
    defs.push("VERTEX_OUTPUT_INSTANCE_INDEX".into());
    defs.push(if matches!(pass, Prepass::DirectionalShadow { .. }) {
        "VIEW_PROJECTION_ORTHOGRAPHIC".into()
    } else {
        "VIEW_PROJECTION_PERSPECTIVE".into()
    });
    if depth {
        defs.push("DEPTH_PREPASS".into());
    }
    if may_discard {
        defs.push("MAY_DISCARD".into());
    }
    defs.push("VERTEX_POSITIONS".into());
    if emulate_unclipped_depth {
        defs.push("UNCLIPPED_DEPTH_ORTHO_EMULATION".into());
        defs.push("PREPASS_FRAGMENT".into());
    }
    if p.mesh.uv0 {
        defs.push("VERTEX_UVS".into());
        defs.push("VERTEX_UVS_A".into());
    }
    if p.mesh.uv1 {
        defs.push("VERTEX_UVS".into());
        defs.push("VERTEX_UVS_B".into());
    }
    if normal {
        defs.push("NORMAL_PREPASS".into());
    }
    if normal || deferred {
        defs.push("NORMAL_PREPASS_OR_DEFERRED_PREPASS".into());
        if p.mesh.normals {
            defs.push("VERTEX_NORMALS".into());
        }
        if p.mesh.tangents {
            defs.push("VERTEX_TANGENTS".into());
        }
    }
    if motion || deferred {
        defs.push("MOTION_VECTOR_PREPASS_OR_DEFERRED_PREPASS".into());
    }
    if deferred {
        defs.push("DEFERRED_PREPASS".into());
    }
    if p.mesh.colors {
        defs.push("VERTEX_COLORS".into());
    }
    if motion {
        defs.push("MOTION_VECTOR_PREPASS".into());
    }
    if target.binding_arrays() {
        defs.push("MULTIPLE_LIGHTMAPS_IN_ARRAY".into());
    }
    if p.visibility_range {
        defs.push("VISIBILITY_RANGE_DITHER".into());
    }
    if normal || motion || deferred {
        defs.push("PREPASS_FRAGMENT".into());
    }
    if !target.storage_buffers() {
        defs.push("METADATA_USE_UNIFORM_BUFFERS".into());
        // `setup_morph_and_skinning_defs`.
        defs.push("SKINS_USE_UNIFORM_BUFFERS".into());
    }
    defs.extend(p.material.iter().map(|d| ShaderDefVal::from(*d)));
    defs
}

/// The group-0 bindings the prepass view layouts hold, read from the
/// `bevy_pbr` the build compiled (`init_prepass_pipeline`, bevy_pbr
/// `src/prepass/mod.rs`): `(without motion vectors, with)`. A prepass
/// pipeline takes the second only when its key has `MOTION_VECTOR_PREPASS`.
fn prepass_view_layouts() -> (BTreeSet<u32>, BTreeSet<u32>) {
    let file = graph()
        .src("bevy_pbr")
        .expect("bevy_pbr is in the graph")
        .join("prepass/mod.rs");
    let text = std::fs::read_to_string(&file).expect("bevy_pbr's prepass module is readable");
    let layout = |label: &str| -> BTreeSet<u32> {
        let quoted = format!("\"{label}\"");
        let start = text
            .find(&quoted)
            .unwrap_or_else(|| panic!("{} names no layout {quoted}", file.display()));
        let rest = &text[start..];
        let body = &rest[..rest.find("\n    );").expect("the layout's call ends")];
        // Each entry is a tuple `(index, layout)`, on one line or two.
        let mut indices = BTreeSet::new();
        let mut lines = body.lines().map(str::trim).peekable();
        while let Some(line) = lines.next() {
            let Some(after) = line.strip_prefix('(') else {
                continue;
            };
            let entry = if after.is_empty() {
                lines.peek().copied().unwrap_or("")
            } else {
                after
            };
            let digits = entry.chars().take_while(char::is_ascii_digit).count();
            if digits > 0 && entry[digits..].starts_with(',') {
                indices.insert(entry[..digits].parse().expect("digits"));
            }
        }
        assert!(
            indices.contains(&0),
            "read no view binding from {quoted} in {}: re-read the layout there",
            file.display()
        );
        indices
    };
    (
        layout("prepass_view_layout_no_motion_vectors"),
        layout("prepass_view_layout_motion_vectors"),
    )
}

// ---------------------------------------------------------------------------
// Composing and validating
// ---------------------------------------------------------------------------

/// One composition to run.
struct Job {
    shader: &'static str,
    stages: &'static [ShaderStage],
    label: &'static str,
    target: Target,
    defs: Vec<ShaderDefVal>,
    /// For a prepass pipeline, the group-0 bindings its view layout holds.
    prepass_layout: Option<BTreeSet<u32>>,
}

impl Job {
    fn name(&self) -> String {
        format!(
            "{}.wesl {} ({})",
            self.shader,
            self.label,
            self.target.name()
        )
    }
}

fn jobs() -> Vec<Job> {
    let (no_motion, motion) = prepass_view_layouts();
    let mut jobs = Vec::new();
    for u in uses() {
        assert!(!u.pipelines.is_empty(), "{}.wesl has no pipeline", u.shader);
        for p in &u.pipelines {
            assert!(
                !p.targets.is_empty(),
                "{} {} has no target",
                u.shader,
                p.label
            );
            for &target in p.targets {
                let (defs, prepass_layout) = match p.pass {
                    Pass::Forward(view) => (forward_defs(target, view, p), None),
                    Pass::Prepass(pass) => {
                        let layout = if matches!(pass, Prepass::NormalMotion) {
                            motion.clone()
                        } else {
                            no_motion.clone()
                        };
                        (prepass_defs(target, pass, p), Some(layout))
                    }
                };
                jobs.push(Job {
                    shader: u.shader,
                    stages: u.stages,
                    label: p.label,
                    target,
                    defs,
                    prepass_layout,
                });
            }
        }
    }
    jobs
}

/// `ShaderCache::get`'s composition (bevy_shader `src/shader_cache.rs`): the
/// defs of every shader the entry's imports reach (each carries the global
/// defs, and Bevy's libraries their own), then the pipeline's, then the
/// entry's own (the globals); flags for all, a `constants` module for the
/// numbers. Returns the WGSL and the modules it was built from.
fn compose(job: &Job, closure: &BTreeSet<String>) -> Result<(String, Vec<String>), String> {
    let globals = job.target.global_defs();
    let mut all: Vec<ShaderDefVal> = Vec::new();
    for module in closure {
        all.extend(globals.iter().cloned());
        all.extend(job.target.library_defs(module));
    }
    all.extend(job.defs.iter().cloned());
    all.extend(globals);

    let mut options = wesl::CompileOptions {
        imports: true,
        condcomp: true,
        visibility: false,
        ..Default::default()
    };
    let mut constants = BTreeMap::new();
    for def in &all {
        match def {
            ShaderDefVal::Bool(name, value) => {
                options
                    .features
                    .flags
                    .insert(name.to_string(), (*value).into());
            }
            ShaderDefVal::Int(name, value) => {
                options.features.flags.insert(name.to_string(), true.into());
                constants.insert(name.to_string(), value.to_string());
            }
            ShaderDefVal::UInt(name, value) => {
                options.features.flags.insert(name.to_string(), true.into());
                constants.insert(name.to_string(), value.to_string());
            }
        }
    }
    let constants: String = constants
        .iter()
        .map(|(name, value)| format!("const {name} = {value};\n"))
        .collect();
    let resolver = Resolver {
        constants: &constants,
    };
    let compiled = wesl::compile(&asset_module(job.shader), &options, &resolver)
        .map_err(|e| format!("the composer: {}", e.diagnostic().render_plain()))?;
    let modules = compiled
        .modules
        .iter()
        .map(|m| key(&canonical(&m.path)))
        .collect();
    Ok((compiled.to_string(), modules))
}

/// What wgpu does with the WGSL (wgpu-core `Device::create_shader_module`:
/// parse, the bind-group bound, `create_validator` with every validation flag
/// and the device's capabilities), then what pipeline creation would refuse.
fn check(job: &Job, wgsl: &str) -> Result<(), String> {
    let module = naga::front::wgsl::parse_str(wgsl)
        .map_err(|e| format!("naga's parser: {}", e.emit_to_string(wgsl)))?;
    let limits = job.target.limits();
    for (_, var) in module.global_variables.iter() {
        if let Some(binding) = &var.binding
            && binding.group >= limits.max_bind_groups
        {
            return Err(format!(
                "`{}` is in bind group {}, past the device's {}",
                var.name.as_deref().unwrap_or("?"),
                binding.group,
                limits.max_bind_groups
            ));
        }
    }
    let info = wgpu_naga_bridge::create_validator(
        job.target.features(),
        job.target.downlevel(),
        naga::valid::ValidationFlags::all(),
    )
    .validate(&module)
    .map_err(|e| format!("naga's validator: {}", e.emit_to_string(wgsl)))?;

    for stage in job.stages {
        let count = module
            .entry_points
            .iter()
            .filter(|ep| ep.stage == *stage)
            .count();
        if count != 1 {
            return Err(format!(
                "{count} {stage:?} entry points: the pipeline names none, so wgpu needs exactly one"
            ));
        }
    }

    if let Some(layout) = &job.prepass_layout {
        let mut stray = BTreeSet::new();
        for (index, ep) in module.entry_points.iter().enumerate() {
            if !job.stages.contains(&ep.stage) {
                continue;
            }
            let uses = info.get_entry_point(index);
            for (handle, var) in module.global_variables.iter() {
                if let Some(binding) = &var.binding
                    && binding.group == 0
                    && !uses[handle].is_empty()
                    && !layout.contains(&binding.binding)
                {
                    stray.insert(format!(
                        "`{}` at @group(0) @binding({})",
                        var.name.as_deref().unwrap_or("?"),
                        binding.binding
                    ));
                }
            }
        }
        if !stray.is_empty() {
            return Err(format!(
                "the prepass view layout holds group-0 bindings {layout:?} only, and this \
                 pipeline reads {stray:?} - pipeline creation would fail with `Shader global \
                 ResourceBinding {{ .. }} is not available in the pipeline layout`"
            ));
        }
    }
    Ok(())
}

/// What one job found.
struct Outcome {
    modules: Vec<String>,
    error: Option<String>,
}

fn run(job: &Job, closure: &BTreeSet<String>) -> Outcome {
    match compose(job, closure) {
        Err(error) => Outcome {
            modules: Vec::new(),
            error: Some(error),
        },
        Ok((wgsl, modules)) => {
            let error = check(job, &wgsl).err().map(|error| {
                let dump = std::env::temp_dir().join(format!(
                    "shader_compose-{}-{}-{}.wgsl",
                    job.shader,
                    job.label,
                    job.target.name()
                ));
                let saved = std::fs::write(&dump, &wgsl)
                    .map(|()| format!("\n(composed WGSL: {})", dump.display()))
                    .unwrap_or_default();
                format!("{error}{saved}")
            });
            Outcome { modules, error }
        }
    }
}

/// The `.wesl` files in `assets/shaders`, by stem.
fn shader_files() -> BTreeSet<String> {
    std::fs::read_dir(assets().join("shaders"))
        .expect("assets/shaders is tracked")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "wesl"))
        .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
        .collect()
}

/// Every pipeline of every shader in [`uses`], composed and validated on
/// every target it draws on - and proof that all of it ran: each planned
/// composition reported back, each entry shader composed on all three
/// builds, every `.wesl` file reached (the entries directly, the shared
/// modules through an import), the prepass rule applied.
#[test]
fn every_shader_composes_and_validates_under_every_def_set() {
    let uses = uses();
    let closures: BTreeMap<&str, BTreeSet<String>> = uses
        .iter()
        .map(|u| (u.shader, import_closure(u.shader)))
        .collect();
    let jobs = jobs();

    let names: BTreeSet<String> = jobs.iter().map(Job::name).collect();
    assert_eq!(names.len(), jobs.len(), "two compositions share a name");

    // Up to four workers, each taking the next job until none is left.
    let next = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(4);
    let mut outcomes: Vec<Option<Outcome>> = (0..jobs.len()).map(|_| None).collect();
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(job) = jobs.get(index) else {
                            break done;
                        };
                        done.push((index, run(job, &closures[job.shader])));
                    }
                })
            })
            .collect();
        for worker in workers {
            for (index, outcome) in worker.join().expect("a composition panicked") {
                outcomes[index] = Some(outcome);
            }
        }
    });

    let failures: Vec<String> = jobs
        .iter()
        .zip(&outcomes)
        .filter_map(|(job, outcome)| {
            let error = outcome
                .as_ref()
                .map_or(Some("never ran"), |o| o.error.as_deref())?;
            Some(format!("{}:\n{error}", job.name()))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} compositions failed:\n\n{}",
        failures.len(),
        jobs.len(),
        failures.join("\n\n")
    );

    // Every entry composed on every build.
    let composed: BTreeSet<(&str, Target)> = jobs.iter().map(|j| (j.shader, j.target)).collect();
    for u in &uses {
        for &target in EVERY_TARGET {
            assert!(
                composed.contains(&(u.shader, target)),
                "{}.wesl was never composed for {}",
                u.shader,
                target.name()
            );
        }
    }

    // Every file reached: the entries as entries, the rest through imports.
    let entries: BTreeSet<String> = uses.iter().map(|u| u.shader.to_owned()).collect();
    let imported: BTreeSet<String> = outcomes
        .iter()
        .flatten()
        .flat_map(|o| o.modules.iter())
        .filter_map(|m| m.strip_prefix("/shaders/"))
        .map(str::to_owned)
        .collect();
    for file in shader_files() {
        if entries.contains(&file) {
            continue;
        }
        assert!(
            imported.contains(&file),
            "assets/shaders/{file}.wesl is neither composed as an entry nor imported by one"
        );
    }
    assert!(
        imported.len() > entries.len(),
        "the shared modules were not reached through imports: {imported:?}"
    );

    // The prepass rule ran, on every shader that serves a prepass.
    let prepass: BTreeSet<&str> = jobs
        .iter()
        .filter(|j| j.prepass_layout.is_some())
        .map(|j| j.shader)
        .collect();
    assert_eq!(
        prepass,
        BTreeSet::from(["cloud_shadow", "splat", "wind"]),
        "the shaders a prepass or shadow pass draws with"
    );
}

/// Every shader the game loads by path (`"shaders/<name>.wesl"` in `src/`)
/// has its row in [`uses`], and every row is a file the game loads - so a
/// new material cannot ship a shader this test never composes.
#[test]
fn every_shader_the_game_loads_has_a_row_in_the_matrix() {
    fn walk(dir: &Path, found: &mut BTreeSet<String>) {
        for entry in std::fs::read_dir(dir).expect("src is readable").flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, found);
            } else if path.extension().is_some_and(|x| x == "rs") {
                let text = std::fs::read_to_string(&path).expect("a source file is UTF-8");
                for (at, _) in text.match_indices("\"shaders/") {
                    let rest = &text[at + "\"shaders/".len()..];
                    let name = &rest[..rest.find('"').expect("a closed literal")];
                    if let Some(stem) = name.strip_suffix(".wesl") {
                        found.insert(stem.to_owned());
                    }
                }
            }
        }
    }
    let mut loaded = BTreeSet::new();
    walk(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut loaded,
    );
    let rows: BTreeSet<String> = uses().iter().map(|u| u.shader.to_owned()).collect();
    assert_eq!(rows.len(), 9, "the nine entry shaders: {rows:?}");
    assert_eq!(
        loaded, rows,
        "the shaders src/ loads, and the matrix's rows"
    );
    let files = shader_files();
    assert!(
        rows.is_subset(&files),
        "every row is a file in assets/shaders: {rows:?} against {files:?}"
    );
}
