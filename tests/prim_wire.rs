//! Byte-level wire guard for the sixteen parametric primitives (#1188).
//!
//! Every primitive on the [`primitive_kind_tags`] roster serialises to one
//! flat JSON object — its own dimensional knobs followed by the shared
//! block (`solid`, `uv_mapping`, `material`, `faces`, `torture`) in that
//! order, the default-valued members of the block elided. Child room
//! records are content-addressed over exactly those bytes
//! ([`child_rkey`]), so a refactor that reorders one key or writes one
//! elided default rewrites every child record on the next publish.
//!
//! This test pins the bytes, not the shape: a corpus built through the
//! representation-independent surface (tag defaults, the shared accessors,
//! JSON injection) is serialised and compared line-for-line against
//! `tests/fixtures/prim_wire.jsonl`. Regenerate the fixture only when the
//! wire form is *meant* to move, with `PRIM_WIRE_BLESS=1`, and say so in
//! the commit.

use std::path::PathBuf;

use serde_json::{Value, json};
use symbios_overlands::pds::generator::{FaceKey, FaceOverride, UvMapping, primitive_kind_tags};
use symbios_overlands::pds::room::child_rkey;
use symbios_overlands::pds::{Fp, Fp3, Generator, GeneratorKind, sanitize_generator};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/prim_wire.jsonl")
}

/// One corpus line: a label, the bytes, and the content address those
/// bytes mint when the kind is a room child.
fn line(label: &str, kind: &GeneratorKind) -> String {
    let bytes = serde_json::to_string(kind).expect("primitive serialises");
    let g = Generator::from_kind(kind.clone());
    let rkey = child_rkey("corpus", &g);
    format!("{label}\t{rkey}\t{bytes}")
}

/// Re-encode `doc` through the type, optionally through the sanitiser.
fn through(doc: Value, sanitize: bool) -> GeneratorKind {
    let kind: GeneratorKind = serde_json::from_value(doc).expect("corpus doc decodes");
    if !sanitize {
        return kind;
    }
    let mut g = Generator::from_kind(kind);
    sanitize_generator(&mut g);
    g.kind
}

fn corpus() -> Vec<String> {
    let mut out = Vec::new();
    for tag in primitive_kind_tags() {
        let base = GeneratorKind::default_primitive_for_tag(tag).expect("roster has a default");
        out.push(line(&format!("{tag}/default"), &base));

        // Every shared field set to a non-default through the accessors.
        let mut dressed = base.clone();
        let mat = dressed.material_mut().expect("primitive");
        mat.base_color = Fp3([0.2, 0.4, 0.6]);
        mat.roughness = Fp(0.3);
        let t = dressed.torture_mut().expect("primitive");
        t.hollow = Fp(0.3);
        t.twist = Fp(0.25);
        dressed.faces_mut().expect("primitive").extend([
            FaceOverride {
                face: FaceKey::Top,
                material: symbios_overlands::pds::SovereignMaterialSettings {
                    base_color: Fp3([0.9, 0.1, 0.1]),
                    ..Default::default()
                },
                uv_mapping: Some(UvMapping::PlanarY),
            },
            FaceOverride {
                face: FaceKey::Wall,
                material: Default::default(),
                uv_mapping: None,
            },
        ]);
        out.push(line(&format!("{tag}/dressed"), &dressed));

        // `solid` and `uv_mapping` have no setter today; inject on the wire.
        let pristine = serde_json::to_value(&base).expect("serialises");
        for (name, uv) in [
            ("planar_y", "network.symbios.uv.planar_y"),
            ("box", "network.symbios.uv.box"),
            ("fit", "network.symbios.uv.fit"),
        ] {
            let mut doc = pristine.clone();
            doc["solid"] = json!(false);
            doc["uv_mapping"] = json!({ "$type": uv });
            // Raw: exactly what an overlands client re-encodes without
            // touching the record. Sanitised: what it publishes.
            out.push(line(
                &format!("{tag}/unsolid+{name}/sanitized"),
                &through(doc.clone(), true),
            ));
            // A value equal to the family's own default is never written
            // by this client; the raw re-encode is pinned only for the
            // two values that always land on the wire.
            if name == "planar_y" {
                out.push(line(
                    &format!("{tag}/unsolid+{name}/raw"),
                    &through(doc, false),
                ));
            }
        }
    }
    out
}

/// The corpus bytes match the blessed fixture line for line.
#[test]
fn primitive_wire_bytes_are_pinned() {
    let got = corpus();
    let path = fixture_path();
    if std::env::var_os("PRIM_WIRE_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, got.join("\n") + "\n").expect("write fixture");
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} — bless with PRIM_WIRE_BLESS=1", path.display()));
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
        "the primitive wire form moved ({} difference(s)); child rkeys would rewrite \
         on the next publish:\n{}",
        diffs.len(),
        diffs.join("\n")
    );
}

/// Every pinned line decodes back to the value that produced it, and
/// re-encodes to the same bytes — the fixture is a fixed point, not just a
/// snapshot.
#[test]
fn primitive_wire_fixture_is_a_fixed_point() {
    for l in corpus() {
        let (label, rest) = l.split_once('\t').unwrap();
        let (_, bytes) = rest.split_once('\t').unwrap();
        let kind: GeneratorKind = serde_json::from_str(bytes).expect("fixture line decodes");
        assert_eq!(
            serde_json::to_string(&kind).unwrap(),
            bytes,
            "{label}: decode→encode is not the identity"
        );
    }
}

/// The seeded rooms are a second, much wider corpus: every themed
/// catalogue entry is built through the `catalogue::items::util`
/// constructors, so one room's generators pin every primitive those
/// constructors emit. Pinned as each generator's content-addressed child
/// rkey — the very identity a publish compares — folded per seed in name
/// order, because the record's generator map itself iterates in hash
/// order and its bytes are not stable across processes.
#[test]
fn seeded_room_bytes_are_pinned() {
    use symbios_overlands::pds::RoomRecord;
    use symbios_overlands::seeded_defaults::fnv1a_64;

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/seeded_rooms.txt");
    let got: Vec<String> = (1..=12u64)
        .map(|seed| {
            let room = RoomRecord::default_for_seed(seed, "did:plc:corpus");
            let mut names: Vec<&String> = room.generators.keys().collect();
            names.sort();
            let rkeys: String = names
                .iter()
                .map(|name| format!("{name}={}\n", child_rkey(name, &room.generators[*name])))
                .collect();
            format!(
                "seed {seed}\t{:016x}\t{} generators",
                fnv1a_64(&rkeys),
                names.len()
            )
        })
        .collect();
    if std::env::var_os("PRIM_WIRE_BLESS").is_some() {
        std::fs::write(&path, got.join("\n") + "\n").expect("write fixture");
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} — bless with PRIM_WIRE_BLESS=1", path.display()));
    assert_eq!(
        want.trim_end().lines().collect::<Vec<_>>(),
        got.iter().map(String::as_str).collect::<Vec<_>>(),
        "a seeded room's bytes moved: some primitive a catalogue entry builds \
         serialises differently now"
    );
}
