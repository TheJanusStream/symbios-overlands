//! Guards over the shaders as files (#1679, the visual fidelity epic's
//! sweep): what the build cannot see, since the WESL is loaded at run time
//! and composed there, and what cost the epic time when it went wrong.

use std::path::PathBuf;

/// Every shader the game loads, by file name. A `.wgsl` file in
/// `assets/shaders` would be read by Bevy as plain WGSL - no imports, and
/// its shader defs dropped - so every one is WESL (Bevy 0.20, #1634).
const SHADERS: [&str; 13] = [
    "cloud.wesl",
    "cloud_common.wesl",
    "cloud_shadow.wesl",
    "grass.wesl",
    "ground_common.wesl",
    "precipitation.wesl",
    "sky.wesl",
    "sky_common.wesl",
    "splat.wesl",
    "storm_common.wesl",
    "water.wesl",
    "weathered.wesl",
    "wind.wesl",
];

/// Every `.wesl` file in `assets/shaders`, by name - all of [`SHADERS`] and
/// nothing else, so a guard below can never scan an empty list and pass
/// (as each did once every file was renamed from `.wgsl`).
fn shaders() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/shaders");
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .expect("assets/shaders is tracked")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "wesl"))
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read_to_string(&p).expect("readable"))
        })
        .collect();
    out.sort();
    let names: Vec<&str> = out.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names, SHADERS,
        "assets/shaders holds exactly these WESL files"
    );
    out
}

/// The shaders read every one of [`SHADERS`], and no plain `.wgsl` file is
/// left beside them for Bevy to load without imports or shader defs.
#[test]
fn every_shader_is_read_and_every_one_is_wesl() {
    assert_eq!(shaders().len(), SHADERS.len());
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/shaders");
    let wgsl: Vec<_> = std::fs::read_dir(&dir)
        .expect("assets/shaders is tracked")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "wgsl"))
        .collect();
    assert!(wgsl.is_empty(), "plain WGSL in assets/shaders: {wgsl:?}");
}

fn shader(name: &str) -> String {
    shaders()
        .into_iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("{name} is tracked"))
        .1
}

/// WGSL's reserved words (the spec's "Reserved Words"): none may name
/// anything. naga rejects one only when the shader is composed, at run
/// time - `patch` cost the epic two rounds (#1675, #1683) - so this scan is
/// what fails on the machine doing the editing.
const RESERVED: &[&str] = &[
    "NULL",
    "Self",
    "abstract",
    "active",
    "alignas",
    "alignof",
    "as",
    "asm",
    "asm_fragment",
    "async",
    "attribute",
    "auto",
    "await",
    "become",
    "binding_array",
    "cast",
    "catch",
    "class",
    "co_await",
    "co_return",
    "co_yield",
    "coherent",
    "column_major",
    "common",
    "compile",
    "compile_fragment",
    "concept",
    "const_cast",
    "consteval",
    "constexpr",
    "constinit",
    "crate",
    "debugger",
    "decltype",
    "delete",
    "demote",
    "demote_to_helper",
    "do",
    "dynamic_cast",
    "enum",
    "explicit",
    "export",
    "extends",
    "extern",
    "external",
    "fallthrough",
    "filter",
    "final",
    "finally",
    "friend",
    "from",
    "fxgroup",
    "get",
    "goto",
    "groupshared",
    "highp",
    "impl",
    "implements",
    "import",
    "inline",
    "instanceof",
    "interface",
    "layout",
    "lowp",
    "macro",
    "macro_rules",
    "match",
    "mediump",
    "meta",
    "mod",
    "module",
    "move",
    "mut",
    "mutable",
    "namespace",
    "new",
    "nil",
    "noexcept",
    "noinline",
    "nointerpolation",
    "noperspective",
    "null",
    "nullptr",
    "of",
    "operator",
    "package",
    "packoffset",
    "partition",
    "pass",
    "patch",
    "pixelfragment",
    "precise",
    "precision",
    "premerge",
    "priv",
    "protected",
    "pub",
    "public",
    "readonly",
    "ref",
    "regardless",
    "register",
    "reinterpret_cast",
    "require",
    "resource",
    "restrict",
    "self",
    "set",
    "shared",
    "sizeof",
    "smooth",
    "snorm",
    "static",
    "static_assert",
    "static_cast",
    "std",
    "subroutine",
    "super",
    "target",
    "template",
    "this",
    "thread_local",
    "throw",
    "trait",
    "try",
    "type",
    "typedef",
    "typeid",
    "typename",
    "typeof",
    "union",
    "unless",
    "unorm",
    "unsafe",
    "unsized",
    "use",
    "using",
    "varying",
    "virtual",
    "volatile",
    "wgsl",
    "where",
    "with",
    "writeonly",
    "yield",
];

/// Every word of `src`'s code, with its line number: not a comment, not an
/// import statement (WESL's `import a::b::{c, d};`, which may span lines and
/// names `super`, `package` and `as`, reserved in WGSL but WESL's own
/// keywords there), not the flag expression of an `@if(..)` or `@elif(..)`
/// (shader defs, not WGSL names) and not a `constants::NAME` (a shader def's
/// value). What follows such an attribute on its line is code and is read,
/// as are a qualified name's own parts after a leading `super::` or
/// `package::`.
fn code_words(src: &str) -> Vec<(usize, String)> {
    let mut words = Vec::new();
    let mut in_import = false;
    for (n, line) in src.lines().enumerate() {
        let mut code = line.split("//").next().unwrap_or("").to_string();
        // Conditions first: `@if(FLAG) import ..;` is an import too.
        for attribute in ["@if(", "@elif("] {
            while let Some(at) = code.find(attribute) {
                let open = at + attribute.len();
                let mut depth = 1;
                let close = code[open..]
                    .char_indices()
                    .find(|&(_, c)| {
                        depth += match c {
                            '(' => 1,
                            ')' => -1,
                            _ => 0,
                        };
                        depth == 0
                    })
                    .map_or(code.len(), |(i, _)| open + i + 1);
                code.replace_range(at..close, " ");
            }
        }
        let trimmed = code.trim_start().trim_start_matches("@else").trim_start();
        if !in_import && (trimmed.starts_with("import ") || trimmed == "import") {
            in_import = true;
        }
        if in_import {
            if code.contains(';') {
                in_import = false;
                code = code
                    .split_once(';')
                    .map_or(String::new(), |(_, after)| after.to_string());
            } else {
                continue;
            }
        }
        while let Some(at) = code.find("constants::") {
            let end = code[at + "constants::".len()..]
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .map_or(code.len(), |i| at + "constants::".len() + i);
            code.replace_range(at..end, " ");
        }
        let code = code.replace("super::", " ").replace("package::", " ");
        words.extend(
            code.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .filter(|w| !w.is_empty() && !w.starts_with(|c: char| c.is_ascii_digit()))
                .map(|w| (n + 1, w.to_string())),
        );
    }
    words
}

fn words_of(src: &str) -> Vec<String> {
    code_words(src).into_iter().map(|(_, w)| w).collect()
}

#[test]
fn no_shader_names_a_reserved_word() {
    let mut found = Vec::new();
    for (name, src) in shaders() {
        for (line, word) in code_words(&src) {
            if RESERVED.contains(&word.as_str()) {
                found.push(format!("{name}:{line}: `{word}`"));
            }
        }
    }
    assert!(found.is_empty(), "reserved WGSL words in use: {found:#?}");
    // The scan sees what it must: a line naming `patch` is caught, after a
    // condition on its line too and after an import ends on its line.
    let patch = "patch".to_string();
    assert!(words_of("    let patch = value_noise(xz);").contains(&patch));
    assert!(words_of("    @if(VERTEX_COLORS) let patch = 1.0;").contains(&patch));
    assert!(words_of("import a::{b,\n    c}; const patch = 1.0;").contains(&patch));
    assert!(words_of("const x = super::field::patch(1.0);").contains(&patch));
    // And not what is not code: a comment, an import's keywords and paths
    // over several lines, a flag, a shader def's value.
    assert!(words_of("// a patch of ground").is_empty());
    assert!(words_of("import super::ground_common::{\n    value_noise as noise,\n};").is_empty());
    assert!(words_of("@if(PREPASS_PIPELINE)\n@else import package::render::self;").is_empty());
    assert_eq!(
        words_of("@group(constants::MATERIAL_BIND_GROUP) @binding(1) var t: texture_2d<f32>;"),
        ["group", "binding", "var", "t", "texture_2d", "f32"]
    );
}

/// A material's own texture bindings in `src`.
fn material_textures(src: &str) -> usize {
    src.lines()
        .map(str::trim_start)
        .filter(|t| t.contains("MATERIAL_BIND_GROUP") && t.contains(": texture_"))
        .count()
}

/// What a lit pipeline's layout takes besides the material's own textures,
/// as measured (`splat::SplatExtension`'s and `sky_light`'s docs): on the
/// WebGL2 build every texture in the layout takes one of the GLES backend's
/// 16 slots - the view's five (two shadow maps, the transmission texture
/// and the sky light's two maps, #1672 and #1680) and `StandardMaterial`'s
/// six; on WebGPU a stage counts its sampled textures, 16 at the least - the
/// fragment stage's view six (the depth prepass besides) and the six.
const GLES_SLOTS: usize = 16;
const WEBGPU_STAGE_TEXTURES: usize = 16;
const WEBGL2_VIEW: usize = 5;
const WEBGPU_VIEW: usize = 6;
const STANDARD_MATERIAL: usize = 6;

/// The terrain, the grass and the parts keep within the texture budgets the epic
/// measured them against (#1672, #1675, #1680): a 17th texture panics the
/// WebGL2 build at pipeline creation and fails the WebGPU build at its
/// minimum limits, and only a browser would otherwise say so. The WebGPU
/// terrain stands at exactly its 16: another texture there must replace one.
#[test]
fn the_terrain_the_grass_and_the_parts_keep_within_their_texture_slots() {
    // The weight map, the two layer arrays and the stains overlay, which
    // every build binds since #1681: the WebGL2 terrain at 15 of its 16.
    let terrain = material_textures(&shader("splat.wesl"));
    assert_eq!(terrain, 4, "splat.wesl's textures moved");
    assert!(WEBGL2_VIEW + STANDARD_MATERIAL + terrain <= GLES_SLOTS);
    assert!(WEBGPU_VIEW + STANDARD_MATERIAL + terrain <= WEBGPU_STAGE_TEXTURES);
    let grass = material_textures(&shader("grass.wesl"));
    assert_eq!(grass, 4, "grass.wesl's textures moved");
    assert!(WEBGL2_VIEW + STANDARD_MATERIAL + grass <= GLES_SLOTS);
    // What falls reads the shelter map (#1685): a plain `Material`, so the
    // view's textures and its one.
    let falls = material_textures(&shader("precipitation.wesl"));
    assert_eq!(falls, 1, "precipitation.wesl's textures moved");
    assert!(WEBGL2_VIEW + falls <= GLES_SLOTS);
    // Every part of the world reads the same shelter map (#1686): its one
    // texture beside `StandardMaterial`'s, 12 of WebGL2's 16 and 13 of a
    // WebGPU stage's.
    let parts = material_textures(&shader("weathered.wesl"));
    assert_eq!(parts, 1, "weathered.wesl's textures moved");
    assert!(WEBGL2_VIEW + STANDARD_MATERIAL + parts <= GLES_SLOTS);
    assert!(WEBGPU_VIEW + STANDARD_MATERIAL + parts <= WEBGPU_STAGE_TEXTURES);
    // The epic's other materials bind none of their own.
    for name in ["sky.wesl", "cloud.wesl", "water.wesl", "wind.wesl"] {
        assert_eq!(material_textures(&shader(name)), 0, "{name}");
    }
}
