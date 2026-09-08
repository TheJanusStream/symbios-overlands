//! Byte-level wire guard for the fifty-seven procedural texture mirrors
//! (#1313, ahead of #1304 slice 3).
//!
//! Every `Sovereign*Config` is a hand-declared mirror of a
//! `bevy_symbios_texture` config with `f32`/`f64` swapped for fixed-point,
//! written by [`define_sovereign_mirror!`](symbios_overlands::pds) with a
//! default-eliding serializer (#695): a field equal to the mirror's own
//! default is left off the wire entirely. Two things follow, and both are
//! invisible to the compiler.
//!
//! * **The field order is content-address-visible.** `serde_json` writes
//!   struct fields in declaration order and [`child_rkey`] is a hash of
//!   those bytes, so moving one field rewrites every child record carrying
//!   that texture on its next publish.
//! * **A mirror default that drifts from upstream changes what an *absent*
//!   key means**, silently re-interpreting records already stored.
//!
//! So this pins the bytes, not the shape. Each variant contributes a
//! default-valued mirror — which elides to almost nothing, and is where a
//! moved default shows up — and one with every field driven off its
//! default through the upstream config's own serde, which is where a moved
//! field order shows up. Neither line is sanitised: this is the mirror's
//! wire form, not what the sanitiser makes of it.
//!
//! The last test guards the other direction: what the sanitiser makes of
//! bytes it did not write. It shares the roster, which is why it lives here.
//!
//! Regenerate only when the wire is *meant* to move, with
//! `TEXTURE_WIRE_BLESS=1`, and say so in the commit.

use std::path::PathBuf;

use serde_json::Value;
use symbios_overlands::pds::room::child_rkey;
use symbios_overlands::pds::{Generator, GeneratorKind, SovereignTextureConfig};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/texture_wire.jsonl")
}

/// The variants of each enum-valued field, keyed by its path. Keyed by path
/// rather than by value because two configs both call theirs `layout` and
/// mean different enums, and `Basket` is a variant of two of them. An
/// unlisted path fails the run, so a new enum field cannot quietly stay at
/// its default here.
fn variants_at(path: &str) -> Option<&'static [&'static str]> {
    Some(match path {
        "Metal.style" => &[
            "Brushed",
            "StandingSeam",
            "Hammered",
            "DiamondPlate",
            "Riveted",
            "Perforated",
        ],
        "Pavers.layout" => &["Square", "Hexagonal"],
        "Encaustic.pattern" => &["Checkerboard", "Octagon", "Diamond"],
        "Gravel.metric" => &["Euclidean", "Manhattan", "Chebyshev"],
        "Parquet.layout" => &["Herringbone", "Basket", "Brick"],
        "Fabric.weave" => &["Plain", "Twill", "Satin", "Basket"],
        _ => return None,
    })
}

/// Drive every leaf of an upstream config's JSON off its default, keeping
/// each one's JSON type so the config still decodes.
fn perturb(value: Value, path: &str) -> Value {
    match value {
        // A float is a fixed-point field on the mirror; keep it well inside
        // what `Fp`'s i32 grid can carry.
        Value::Number(n) if n.is_f64() => {
            let v = n.as_f64().expect("f64");
            Value::from(v * 1.5 + 0.125)
        }
        Value::Number(n) => Value::from(n.as_u64().expect("count or seed") + 3),
        Value::Bool(b) => Value::Bool(!b),
        // The first variant that is not the one already there, so the line
        // does not depend on which variant happens to be the default.
        Value::String(s) => {
            let variants = variants_at(path)
                .unwrap_or_else(|| panic!("{path}: no variant list recorded for {s:?}"));
            let next = variants
                .iter()
                .find(|v| **v != s)
                .unwrap_or_else(|| panic!("{path}: {s:?} is the only variant listed"));
            Value::String((*next).to_string())
        }
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .enumerate()
                .map(|(i, v)| perturb(v, &format!("{path}[{i}]")))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    let child = format!("{path}.{k}");
                    (k, perturb(v, &child))
                })
                .collect(),
        ),
        Value::Null => Value::Null,
    }
}

/// One corpus line: a label, the content address the texture mints when it
/// rides on a room child, and the mirror's own bytes.
fn line(label: &str, texture: &SovereignTextureConfig) -> String {
    let bytes = serde_json::to_string(texture).expect("texture serialises");
    let mut kind =
        GeneratorKind::default_primitive_for_tag("Cuboid").expect("the roster has a cuboid");
    kind.material_mut()
        .expect("a primitive has a material")
        .texture = texture.clone();
    let rkey = child_rkey("corpus", &Generator::from_kind(kind));
    format!("{label}\t{rkey}\t{bytes}")
}

/// `(variant, mirror type, upstream config)` for all fifty-seven procedural
/// variants, in `to_texture_config`'s order. `None`, `Unknown` and
/// `Referenced` carry no mirror and are pinned separately below.
macro_rules! corpus_rows {
    ( $( $Variant:ident, $Sov:ty, $Native:ty );+ $(;)? ) => {{
        let mut out: Vec<String> = Vec::with_capacity(120);
        $({
            let base = <$Sov>::default();
            out.push(line(
                concat!(stringify!($Variant), "/default"),
                &SovereignTextureConfig::$Variant(base.clone()),
            ));

            let native = base.to_native();
            let doc = serde_json::to_value(&native).expect("upstream config serialises");
            let varied: $Native = serde_json::from_value(perturb(doc, stringify!($Variant)))
                .expect("the perturbed config decodes");
            out.push(line(
                concat!(stringify!($Variant), "/varied"),
                &SovereignTextureConfig::$Variant(<$Sov>::from_native(&varied)),
            ));
        })+
        out
    }};
}

fn corpus() -> Vec<String> {
    use symbios_overlands::pds::*;
    let mut out = corpus_rows!(
        Leaf, SovereignLeafConfig, bevy_symbios_texture::leaf::LeafConfig;
        Twig, SovereignTwigConfig, bevy_symbios_texture::twig::TwigConfig;
        Bark, SovereignBarkConfig, bevy_symbios_texture::bark::BarkConfig;
        Window, SovereignWindowConfig, bevy_symbios_texture::window::WindowConfig;
        StainedGlass, SovereignStainedGlassConfig, bevy_symbios_texture::stained_glass::StainedGlassConfig;
        IronGrille, SovereignIronGrilleConfig, bevy_symbios_texture::iron_grille::IronGrilleConfig;
        Ground, SovereignGroundConfig, bevy_symbios_texture::ground::GroundConfig;
        Rock, SovereignRockConfig, bevy_symbios_texture::rock::RockConfig;
        Brick, SovereignBrickConfig, bevy_symbios_texture::brick::BrickConfig;
        Plank, SovereignPlankConfig, bevy_symbios_texture::plank::PlankConfig;
        Shingle, SovereignShingleConfig, bevy_symbios_texture::shingle::ShingleConfig;
        Stucco, SovereignStuccoConfig, bevy_symbios_texture::stucco::StuccoConfig;
        Concrete, SovereignConcreteConfig, bevy_symbios_texture::concrete::ConcreteConfig;
        Metal, SovereignMetalConfig, bevy_symbios_texture::metal::MetalConfig;
        Pavers, SovereignPaversConfig, bevy_symbios_texture::pavers::PaversConfig;
        Ashlar, SovereignAshlarConfig, bevy_symbios_texture::ashlar::AshlarConfig;
        Cobblestone, SovereignCobblestoneConfig, bevy_symbios_texture::cobblestone::CobblestoneConfig;
        Thatch, SovereignThatchConfig, bevy_symbios_texture::thatch::ThatchConfig;
        Marble, SovereignMarbleConfig, bevy_symbios_texture::marble::MarbleConfig;
        Corrugated, SovereignCorrugatedConfig, bevy_symbios_texture::corrugated::CorrugatedConfig;
        Asphalt, SovereignAsphaltConfig, bevy_symbios_texture::asphalt::AsphaltConfig;
        Wainscoting, SovereignWainscotingConfig, bevy_symbios_texture::wainscoting::WainscotingConfig;
        Encaustic, SovereignEncausticConfig, bevy_symbios_texture::encaustic::EncausticConfig;
        SoftDisc, SovereignSoftDiscConfig, bevy_symbios_texture::soft_disc::SoftDiscConfig;
        Spark, SovereignSparkConfig, bevy_symbios_texture::spark::SparkConfig;
        Snowflake, SovereignSnowflakeConfig, bevy_symbios_texture::snowflake::SnowflakeConfig;
        Puff, SovereignPuffConfig, bevy_symbios_texture::puff::PuffConfig;
        Ring, SovereignRingConfig, bevy_symbios_texture::ring::RingConfig;
        Petal, SovereignPetalConfig, bevy_symbios_texture::petal::PetalConfig;
        Shard, SovereignShardConfig, bevy_symbios_texture::shard::ShardConfig;
        LeafSprite, SovereignLeafSpriteConfig, bevy_symbios_texture::leaf_sprite::LeafSpriteConfig;
        Flame, SovereignFlameConfig, bevy_symbios_texture::flame::FlameConfig;
        Flower, SovereignFlowerConfig, bevy_symbios_texture::flower::FlowerConfig;
        GrassTuft, SovereignGrassTuftConfig, bevy_symbios_texture::grass::GrassTuftConfig;
        Frond, SovereignFrondConfig, bevy_symbios_texture::frond::FrondConfig;
        Reed, SovereignReedConfig, bevy_symbios_texture::reed::ReedConfig;
        Needle, SovereignNeedleConfig, bevy_symbios_texture::needle::NeedleConfig;
        Broadleaf, SovereignBroadleafConfig, bevy_symbios_texture::broadleaf::BroadleafConfig;
        Moss, SovereignMossConfig, bevy_symbios_texture::moss::MossConfig;
        Lichen, SovereignLichenConfig, bevy_symbios_texture::lichen::LichenConfig;
        Fabric, SovereignFabricConfig, bevy_symbios_texture::fabric::FabricConfig;
        Sand, SovereignSandConfig, bevy_symbios_texture::sand::SandConfig;
        Snow, SovereignSnowConfig, bevy_symbios_texture::snow::SnowConfig;
        Ice, SovereignIceConfig, bevy_symbios_texture::ice::IceConfig;
        Lava, SovereignLavaConfig, bevy_symbios_texture::lava::LavaConfig;
        CactusSkin, SovereignCactusSkinConfig, bevy_symbios_texture::cactus::CactusSkinConfig;
        CrackedEarth, SovereignCrackedEarthConfig, bevy_symbios_texture::cracked_earth::CrackedEarthConfig;
        Gravel, SovereignGravelConfig, bevy_symbios_texture::gravel::GravelConfig;
        ForestFloor, SovereignForestFloorConfig, bevy_symbios_texture::forest_floor::ForestFloorConfig;
        Enamel, SovereignEnamelConfig, bevy_symbios_texture::enamel::EnamelConfig;
        Obsidian, SovereignObsidianConfig, bevy_symbios_texture::obsidian::ObsidianConfig;
        Chitin, SovereignChitinConfig, bevy_symbios_texture::chitin::ChitinConfig;
        SolarPanel, SovereignSolarPanelConfig, bevy_symbios_texture::solar_panel::SolarPanelConfig;
        Parquet, SovereignParquetConfig, bevy_symbios_texture::parquet::ParquetConfig;
        Truchet, SovereignTruchetConfig, bevy_symbios_texture::truchet::TruchetConfig;
        ChainLink, SovereignChainLinkConfig, bevy_symbios_texture::chain_link::ChainLinkConfig;
        LogEnd, SovereignLogEndConfig, bevy_symbios_texture::log_end::LogEndConfig;
    );

    // The two serialisable variants with no generator config of their own.
    // `Referenced` is the only texture whose payload is not a mirror at all.
    // `Unknown` is deliberately absent: it is the `#[serde(other)]` arm, so
    // serde can decode into it but never out of it, and a room holding one
    // cannot be re-published at all (see `pds::room::wire`).
    out.push(line("None", &SovereignTextureConfig::None));
    out.push(line(
        "Referenced",
        &SovereignTextureConfig::Referenced {
            source: Default::default(),
        },
    ));
    out
}

/// The corpus bytes match the blessed fixture line for line.
#[test]
fn texture_wire_bytes_are_pinned() {
    let got = corpus();
    let path = fixture_path();
    if std::env::var_os("TEXTURE_WIRE_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().expect("fixtures dir")).expect("create fixtures dir");
        std::fs::write(&path, got.join("\n") + "\n").expect("write fixture");
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} — bless with TEXTURE_WIRE_BLESS=1", path.display()));
    let want: Vec<&str> = want.lines().collect();
    let mut diffs = Vec::new();
    for (i, g) in got.iter().enumerate() {
        match want.get(i) {
            Some(w) if *w == g => {}
            Some(w) => diffs.push(format!("line {}:\n  want {w}\n  got  {g}", i + 1)),
            None => diffs.push(format!("line {}: not in fixture\n  got  {g}", i + 1)),
        }
    }
    if want.len() > got.len() {
        diffs.push(format!(
            "fixture has {} lines, corpus {}",
            want.len(),
            got.len()
        ));
    }
    assert!(
        diffs.is_empty(),
        "the texture wire form moved ({} difference(s)); child rkeys would rewrite on the \
         next publish:\n{}",
        diffs.len(),
        diffs.join("\n")
    );
}

/// Every pinned line decodes back to the value that produced it and
/// re-encodes to the same bytes — the fixture is a fixed point, not just a
/// snapshot. This is what catches an elided field whose default moved: the
/// bytes still parse, but they no longer mean what they meant.
#[test]
fn texture_wire_fixture_is_a_fixed_point() {
    for l in corpus() {
        let (label, rest) = l.split_once('\t').expect("label");
        let (_, bytes) = rest.split_once('\t').expect("rkey");
        let texture: SovereignTextureConfig =
            serde_json::from_str(bytes).expect("fixture line decodes");
        assert_eq!(
            serde_json::to_string(&texture).expect("re-encodes"),
            bytes,
            "{label}: decode→encode is not the identity"
        );
    }
}

/// Every mirror's declared default must equal the upstream default carried
/// onto the fixed-point grid.
///
/// This is what decides *elision*: the serializer omits a field equal to the
/// mirror's own default, so a mirror default that has drifted from upstream
/// changes what an absent key means on every record already stored. It is
/// also the precondition for deriving those defaults from
/// `Native::default()` instead of hand-typing them — if this passes for all
/// fifty-seven, deriving them cannot move a single byte.
///
/// Compared through serde because the upstream configs do not implement
/// `PartialEq`, and on the fixed-point grid because a mirror can only ever
/// carry the quantised value.
#[test]
fn every_mirror_default_matches_upstream() {
    use symbios_overlands::pds::*;

    macro_rules! check_defaults {
        ( $( $Variant:ident, $Sov:ty, $Native:ty );+ $(;)? ) => {{
            let mut checked = 0usize;
            $({
                let declared = serde_json::to_value(<$Sov>::default().to_native())
                    .expect("the declared default serialises");
                let derived = serde_json::to_value(
                    <$Sov>::from_native(&<$Native>::default()).to_native(),
                )
                .expect("the derived default serialises");
                assert_eq!(
                    declared,
                    derived,
                    "{}: the mirror's declared default drifted from upstream's",
                    stringify!($Variant)
                );
                checked += 1;
            })+
            checked
        }};
    }

    let checked = check_defaults!(
        Leaf, SovereignLeafConfig, bevy_symbios_texture::leaf::LeafConfig;
        Twig, SovereignTwigConfig, bevy_symbios_texture::twig::TwigConfig;
        Bark, SovereignBarkConfig, bevy_symbios_texture::bark::BarkConfig;
        Window, SovereignWindowConfig, bevy_symbios_texture::window::WindowConfig;
        StainedGlass, SovereignStainedGlassConfig, bevy_symbios_texture::stained_glass::StainedGlassConfig;
        IronGrille, SovereignIronGrilleConfig, bevy_symbios_texture::iron_grille::IronGrilleConfig;
        Ground, SovereignGroundConfig, bevy_symbios_texture::ground::GroundConfig;
        Rock, SovereignRockConfig, bevy_symbios_texture::rock::RockConfig;
        Brick, SovereignBrickConfig, bevy_symbios_texture::brick::BrickConfig;
        Plank, SovereignPlankConfig, bevy_symbios_texture::plank::PlankConfig;
        Shingle, SovereignShingleConfig, bevy_symbios_texture::shingle::ShingleConfig;
        Stucco, SovereignStuccoConfig, bevy_symbios_texture::stucco::StuccoConfig;
        Concrete, SovereignConcreteConfig, bevy_symbios_texture::concrete::ConcreteConfig;
        Metal, SovereignMetalConfig, bevy_symbios_texture::metal::MetalConfig;
        Pavers, SovereignPaversConfig, bevy_symbios_texture::pavers::PaversConfig;
        Ashlar, SovereignAshlarConfig, bevy_symbios_texture::ashlar::AshlarConfig;
        Cobblestone, SovereignCobblestoneConfig, bevy_symbios_texture::cobblestone::CobblestoneConfig;
        Thatch, SovereignThatchConfig, bevy_symbios_texture::thatch::ThatchConfig;
        Marble, SovereignMarbleConfig, bevy_symbios_texture::marble::MarbleConfig;
        Corrugated, SovereignCorrugatedConfig, bevy_symbios_texture::corrugated::CorrugatedConfig;
        Asphalt, SovereignAsphaltConfig, bevy_symbios_texture::asphalt::AsphaltConfig;
        Wainscoting, SovereignWainscotingConfig, bevy_symbios_texture::wainscoting::WainscotingConfig;
        Encaustic, SovereignEncausticConfig, bevy_symbios_texture::encaustic::EncausticConfig;
        SoftDisc, SovereignSoftDiscConfig, bevy_symbios_texture::soft_disc::SoftDiscConfig;
        Spark, SovereignSparkConfig, bevy_symbios_texture::spark::SparkConfig;
        Snowflake, SovereignSnowflakeConfig, bevy_symbios_texture::snowflake::SnowflakeConfig;
        Puff, SovereignPuffConfig, bevy_symbios_texture::puff::PuffConfig;
        Ring, SovereignRingConfig, bevy_symbios_texture::ring::RingConfig;
        Petal, SovereignPetalConfig, bevy_symbios_texture::petal::PetalConfig;
        Shard, SovereignShardConfig, bevy_symbios_texture::shard::ShardConfig;
        LeafSprite, SovereignLeafSpriteConfig, bevy_symbios_texture::leaf_sprite::LeafSpriteConfig;
        Flame, SovereignFlameConfig, bevy_symbios_texture::flame::FlameConfig;
        Flower, SovereignFlowerConfig, bevy_symbios_texture::flower::FlowerConfig;
        GrassTuft, SovereignGrassTuftConfig, bevy_symbios_texture::grass::GrassTuftConfig;
        Frond, SovereignFrondConfig, bevy_symbios_texture::frond::FrondConfig;
        Reed, SovereignReedConfig, bevy_symbios_texture::reed::ReedConfig;
        Needle, SovereignNeedleConfig, bevy_symbios_texture::needle::NeedleConfig;
        Broadleaf, SovereignBroadleafConfig, bevy_symbios_texture::broadleaf::BroadleafConfig;
        Moss, SovereignMossConfig, bevy_symbios_texture::moss::MossConfig;
        Lichen, SovereignLichenConfig, bevy_symbios_texture::lichen::LichenConfig;
        Fabric, SovereignFabricConfig, bevy_symbios_texture::fabric::FabricConfig;
        Sand, SovereignSandConfig, bevy_symbios_texture::sand::SandConfig;
        Snow, SovereignSnowConfig, bevy_symbios_texture::snow::SnowConfig;
        Ice, SovereignIceConfig, bevy_symbios_texture::ice::IceConfig;
        Lava, SovereignLavaConfig, bevy_symbios_texture::lava::LavaConfig;
        CactusSkin, SovereignCactusSkinConfig, bevy_symbios_texture::cactus::CactusSkinConfig;
        CrackedEarth, SovereignCrackedEarthConfig, bevy_symbios_texture::cracked_earth::CrackedEarthConfig;
        Gravel, SovereignGravelConfig, bevy_symbios_texture::gravel::GravelConfig;
        ForestFloor, SovereignForestFloorConfig, bevy_symbios_texture::forest_floor::ForestFloorConfig;
        Enamel, SovereignEnamelConfig, bevy_symbios_texture::enamel::EnamelConfig;
        Obsidian, SovereignObsidianConfig, bevy_symbios_texture::obsidian::ObsidianConfig;
        Chitin, SovereignChitinConfig, bevy_symbios_texture::chitin::ChitinConfig;
        SolarPanel, SovereignSolarPanelConfig, bevy_symbios_texture::solar_panel::SolarPanelConfig;
        Parquet, SovereignParquetConfig, bevy_symbios_texture::parquet::ParquetConfig;
        Truchet, SovereignTruchetConfig, bevy_symbios_texture::truchet::TruchetConfig;
        ChainLink, SovereignChainLinkConfig, bevy_symbios_texture::chain_link::ChainLinkConfig;
        LogEnd, SovereignLogEndConfig, bevy_symbios_texture::log_end::LogEndConfig;
    );
    assert_eq!(checked, 57, "every procedural variant is checked");
}

/// Drive every number in an upstream config's JSON to an extreme the *wire*
/// can express, keeping each one's JSON type. `Fp` decodes from an integer
/// on an i32 grid, so `i32::MAX`-ish is the worst a record can say; counts
/// ride as `u32`.
fn extreme(value: Value, magnitude: i64) -> Value {
    match value {
        Value::Number(n) if n.is_f64() => Value::from(magnitude),
        Value::Number(_) => Value::from(if magnitude > 0 { u32::MAX } else { 0 }),
        Value::Bool(b) => Value::Bool(!b),
        // An enum can only hold a variant it declares; serde rejects the rest.
        Value::String(s) => Value::String(s),
        Value::Array(items) => {
            Value::Array(items.into_iter().map(|v| extreme(v, magnitude)).collect())
        }
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, extreme(v, magnitude)))
                .collect(),
        ),
        Value::Null => Value::Null,
    }
}

/// Every numeric field of every variant, driven past both ends of its
/// envelope on the wire and then sanitised, must come back inside it.
///
/// The assertion is the general one rather than a list of bounds: after
/// `sanitize`, clamping again may not move a single field. Before #1304 this
/// could only have been written as a hand list, and the hand list covered
/// sixty-two fields of the several hundred — `warp_octaves` and twenty-seven
/// other integer fields had no bound here at all.
#[test]
fn sanitising_a_hostile_record_lands_inside_the_envelope() {
    use symbios_overlands::pds::*;
    use symbios_texture::ClampToEnvelope;

    macro_rules! hostile {
        ( $( $Variant:ident, $Sov:ty, $Native:ty );+ $(;)? ) => {{
            let mut checked = 0usize;
            $(
                for magnitude in [2_000_000_000i64, -2_000_000_000] {
                    // Build the record the way a peer would: field names and
                    // shapes off the upstream config, values off the deep end.
                    let shape = serde_json::to_value(<$Sov>::default().to_native())
                        .expect("upstream config serialises");
                    let hostile: $Sov = serde_json::from_value(extreme(shape, magnitude))
                        .expect("a hostile record still decodes");

                    // Through the public record path, exactly as a
                    // generator arriving from a peer is cleaned.
                    let mut generator = Generator::from_kind(
                        GeneratorKind::default_primitive_for_tag("Cuboid")
                            .expect("the roster has a cuboid"),
                    );
                    generator
                        .kind
                        .material_mut()
                        .expect("a primitive has a material")
                        .texture = SovereignTextureConfig::$Variant(hostile);
                    sanitize_generator(&mut generator);

                    let texture = &generator
                        .kind
                        .material_mut()
                        .expect("a primitive has a material")
                        .texture;
                    let SovereignTextureConfig::$Variant(clean) = texture else {
                        panic!("{} changed variant under sanitize", stringify!($Variant));
                    };

                    let mut native = clean.to_native();
                    let settled = serde_json::to_value(&native).expect("serialises");
                    native.clamp_to_envelope();
                    assert_eq!(
                        settled,
                        serde_json::to_value(&native).expect("serialises"),
                        "{} at magnitude {magnitude} is still outside its envelope \
                         after sanitising",
                        stringify!($Variant)
                    );
                    checked += 1;
                }
            )+
            checked
        }};
    }

    let checked = hostile!(
        Leaf, SovereignLeafConfig, bevy_symbios_texture::leaf::LeafConfig;
        Twig, SovereignTwigConfig, bevy_symbios_texture::twig::TwigConfig;
        Bark, SovereignBarkConfig, bevy_symbios_texture::bark::BarkConfig;
        Window, SovereignWindowConfig, bevy_symbios_texture::window::WindowConfig;
        StainedGlass, SovereignStainedGlassConfig, bevy_symbios_texture::stained_glass::StainedGlassConfig;
        IronGrille, SovereignIronGrilleConfig, bevy_symbios_texture::iron_grille::IronGrilleConfig;
        Ground, SovereignGroundConfig, bevy_symbios_texture::ground::GroundConfig;
        Rock, SovereignRockConfig, bevy_symbios_texture::rock::RockConfig;
        Brick, SovereignBrickConfig, bevy_symbios_texture::brick::BrickConfig;
        Plank, SovereignPlankConfig, bevy_symbios_texture::plank::PlankConfig;
        Shingle, SovereignShingleConfig, bevy_symbios_texture::shingle::ShingleConfig;
        Stucco, SovereignStuccoConfig, bevy_symbios_texture::stucco::StuccoConfig;
        Concrete, SovereignConcreteConfig, bevy_symbios_texture::concrete::ConcreteConfig;
        Metal, SovereignMetalConfig, bevy_symbios_texture::metal::MetalConfig;
        Pavers, SovereignPaversConfig, bevy_symbios_texture::pavers::PaversConfig;
        Ashlar, SovereignAshlarConfig, bevy_symbios_texture::ashlar::AshlarConfig;
        Cobblestone, SovereignCobblestoneConfig, bevy_symbios_texture::cobblestone::CobblestoneConfig;
        Thatch, SovereignThatchConfig, bevy_symbios_texture::thatch::ThatchConfig;
        Marble, SovereignMarbleConfig, bevy_symbios_texture::marble::MarbleConfig;
        Corrugated, SovereignCorrugatedConfig, bevy_symbios_texture::corrugated::CorrugatedConfig;
        Asphalt, SovereignAsphaltConfig, bevy_symbios_texture::asphalt::AsphaltConfig;
        Wainscoting, SovereignWainscotingConfig, bevy_symbios_texture::wainscoting::WainscotingConfig;
        Encaustic, SovereignEncausticConfig, bevy_symbios_texture::encaustic::EncausticConfig;
        SoftDisc, SovereignSoftDiscConfig, bevy_symbios_texture::soft_disc::SoftDiscConfig;
        Spark, SovereignSparkConfig, bevy_symbios_texture::spark::SparkConfig;
        Snowflake, SovereignSnowflakeConfig, bevy_symbios_texture::snowflake::SnowflakeConfig;
        Puff, SovereignPuffConfig, bevy_symbios_texture::puff::PuffConfig;
        Ring, SovereignRingConfig, bevy_symbios_texture::ring::RingConfig;
        Petal, SovereignPetalConfig, bevy_symbios_texture::petal::PetalConfig;
        Shard, SovereignShardConfig, bevy_symbios_texture::shard::ShardConfig;
        LeafSprite, SovereignLeafSpriteConfig, bevy_symbios_texture::leaf_sprite::LeafSpriteConfig;
        Flame, SovereignFlameConfig, bevy_symbios_texture::flame::FlameConfig;
        Flower, SovereignFlowerConfig, bevy_symbios_texture::flower::FlowerConfig;
        GrassTuft, SovereignGrassTuftConfig, bevy_symbios_texture::grass::GrassTuftConfig;
        Frond, SovereignFrondConfig, bevy_symbios_texture::frond::FrondConfig;
        Reed, SovereignReedConfig, bevy_symbios_texture::reed::ReedConfig;
        Needle, SovereignNeedleConfig, bevy_symbios_texture::needle::NeedleConfig;
        Broadleaf, SovereignBroadleafConfig, bevy_symbios_texture::broadleaf::BroadleafConfig;
        Moss, SovereignMossConfig, bevy_symbios_texture::moss::MossConfig;
        Lichen, SovereignLichenConfig, bevy_symbios_texture::lichen::LichenConfig;
        Fabric, SovereignFabricConfig, bevy_symbios_texture::fabric::FabricConfig;
        Sand, SovereignSandConfig, bevy_symbios_texture::sand::SandConfig;
        Snow, SovereignSnowConfig, bevy_symbios_texture::snow::SnowConfig;
        Ice, SovereignIceConfig, bevy_symbios_texture::ice::IceConfig;
        Lava, SovereignLavaConfig, bevy_symbios_texture::lava::LavaConfig;
        CactusSkin, SovereignCactusSkinConfig, bevy_symbios_texture::cactus::CactusSkinConfig;
        CrackedEarth, SovereignCrackedEarthConfig, bevy_symbios_texture::cracked_earth::CrackedEarthConfig;
        Gravel, SovereignGravelConfig, bevy_symbios_texture::gravel::GravelConfig;
        ForestFloor, SovereignForestFloorConfig, bevy_symbios_texture::forest_floor::ForestFloorConfig;
        Enamel, SovereignEnamelConfig, bevy_symbios_texture::enamel::EnamelConfig;
        Obsidian, SovereignObsidianConfig, bevy_symbios_texture::obsidian::ObsidianConfig;
        Chitin, SovereignChitinConfig, bevy_symbios_texture::chitin::ChitinConfig;
        SolarPanel, SovereignSolarPanelConfig, bevy_symbios_texture::solar_panel::SolarPanelConfig;
        Parquet, SovereignParquetConfig, bevy_symbios_texture::parquet::ParquetConfig;
        Truchet, SovereignTruchetConfig, bevy_symbios_texture::truchet::TruchetConfig;
        ChainLink, SovereignChainLinkConfig, bevy_symbios_texture::chain_link::ChainLinkConfig;
        LogEnd, SovereignLogEndConfig, bevy_symbios_texture::log_end::LogEndConfig;
    );
    assert_eq!(checked, 57 * 2, "both ends of every variant");
}

/// The corpus covers every arm of the union, so a variant added without a
/// line here fails rather than going unpinned.
#[test]
fn every_texture_variant_is_pinned() {
    let labels: Vec<String> = corpus()
        .iter()
        .map(|l| l.split('\t').next().expect("label").to_string())
        .collect();
    assert_eq!(
        labels.len(),
        57 * 2 + 2,
        "fifty-seven procedural variants at two lines each, plus None and Referenced"
    );
}
