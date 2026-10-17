//! Guards over the shaders as files (#1679, the visual fidelity epic's
//! sweep): what the build cannot see, since WGSL is loaded at run time and
//! composed there, and what cost the epic time when it went wrong.

use std::path::PathBuf;

fn shaders() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/shaders");
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .expect("assets/shaders is tracked")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "wgsl"))
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read_to_string(&p).expect("readable"))
        })
        .collect();
    out.sort();
    out
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

/// Every word of `line`'s code: not its comment, not a preprocessor line
/// (`#import`, `#ifdef` - naga_oil's, read before WGSL), not a `#{..}`
/// substitution.
fn code_words(line: &str) -> Vec<String> {
    let line = line.split("//").next().unwrap_or("");
    if line.trim_start().starts_with('#') {
        return Vec::new();
    }
    let mut code = String::new();
    let mut rest = line;
    while let Some(at) = rest.find("#{") {
        code.push_str(&rest[..at]);
        rest = rest[at..].split_once('}').map_or("", |(_, after)| after);
    }
    code.push_str(rest);
    code.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty() && !w.starts_with(|c: char| c.is_ascii_digit()))
        .map(str::to_string)
        .collect()
}

#[test]
fn no_shader_names_a_reserved_word() {
    let mut found = Vec::new();
    for (name, src) in shaders() {
        for (n, line) in src.lines().enumerate() {
            for word in code_words(line) {
                if RESERVED.contains(&word.as_str()) {
                    found.push(format!("{name}:{}: `{word}`", n + 1));
                }
            }
        }
    }
    assert!(found.is_empty(), "reserved WGSL words in use: {found:#?}");
    // The scan sees what it must: a line naming `patch` is caught.
    assert!(code_words("    let patch = value_noise(xz);").contains(&"patch".to_string()));
    assert!(code_words("// a patch of ground").is_empty());
}

/// A material's own texture bindings in `src`, counting those inside an
/// `#ifdef` block of `gate` (if any) only when `gated` is true.
fn material_textures(src: &str, gate: Option<&str>, gated: bool) -> usize {
    let mut inside = false;
    let mut count = 0;
    for line in src.lines() {
        let t = line.trim_start();
        if t.starts_with("#ifdef") && gate.is_some_and(|g| t.contains(g)) {
            inside = true;
        } else if t.starts_with("#endif") && inside {
            inside = false;
        } else if t.contains("MATERIAL_BIND_GROUP")
            && t.contains(": texture_")
            && (!inside || gated)
        {
            count += 1;
        }
    }
    count
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

/// The terrain and the grass keep within the texture budgets the epic
/// measured them against (#1672, #1675, #1680): a 17th texture panics the
/// WebGL2 build at pipeline creation and fails the WebGPU build at its
/// minimum limits, and only a browser would otherwise say so. The WebGPU
/// terrain stands at exactly its 16: another texture there must replace one.
#[test]
fn the_terrain_and_the_grass_keep_within_their_texture_slots() {
    let splat = shader("splat.wgsl");
    let without_stains = material_textures(&splat, Some("STAINS_BINDING"), false);
    let terrain = material_textures(&splat, Some("STAINS_BINDING"), true);
    assert_eq!(
        (without_stains, terrain),
        (3, 4),
        "splat.wgsl's textures moved"
    );
    // The WebGL2 build leaves the stains out (`SplatExtension::specialize`)
    // but has room for them (#1681): both builds are counted with them.
    assert!(WEBGL2_VIEW + STANDARD_MATERIAL + terrain <= GLES_SLOTS);
    assert!(WEBGPU_VIEW + STANDARD_MATERIAL + terrain <= WEBGPU_STAGE_TEXTURES);
    let grass = material_textures(&shader("grass.wgsl"), None, false);
    assert_eq!(grass, 4, "grass.wgsl's textures moved");
    assert!(WEBGL2_VIEW + STANDARD_MATERIAL + grass <= GLES_SLOTS);
    // The epic's other materials bind none of their own.
    for name in [
        "sky.wgsl",
        "cloud.wgsl",
        "water.wgsl",
        "precipitation.wgsl",
        "wind.wgsl",
    ] {
        assert_eq!(material_textures(&shader(name), None, false), 0, "{name}");
    }
}
