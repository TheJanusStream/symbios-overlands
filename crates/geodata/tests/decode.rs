//! The decoders against live GDI Berlin renders (`tests/fixtures`, recorded
//! 2026-10-08), scored against truth that never passed through a render:
//! the raw DGM for terrain, the WFS vectors for land use and storeys. And
//! the water settled on a decoded square against the river's known level.

use geodata::berlin::{self, LandUse, StoreyBand};
use geodata::legend::{ClassLegend, parse_class_legend, parse_value_legend};
use geodata::raster::{CLASS_NONE, decode_classes, decode_png, decode_terrain};
use geodata::request::{Bbox, FeatureQuery, get_features, get_legend, get_map};
use geodata::water;

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// An 8-bit grey truth raster's values (all of them are 300 x 300).
fn grey8(name: &str) -> Vec<u8> {
    decode_png(&fixture(name), 300, 300)
        .unwrap()
        .pixels
        .iter()
        .map(|p| p[0])
        .collect()
}

/// The terrain truth: 16-bit grey centimetres, as metres.
fn truth_heights() -> Vec<f32> {
    let bytes = fixture("dgm1_truth_392000_5820000_1024m_4m.png");
    let mut reader = png::Decoder::new(std::io::Cursor::new(&bytes[..]))
        .read_info()
        .unwrap();
    let mut buffer = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut buffer).unwrap();
    assert_eq!(
        (frame.width, frame.height, frame.bit_depth),
        (256, 256, png::BitDepth::Sixteen)
    );
    buffer
        .chunks_exact(2)
        .map(|be| f32::from(u16::from_be_bytes([be[0], be[1]])) / 100.0)
        .collect()
}

const TERRAIN_BOX: Bbox = Bbox {
    min_e: 392_000,
    min_n: 5_820_000,
    max_e: 393_024,
    max_n: 5_821_024,
};

const MUSEUMSINSEL: Bbox = Bbox {
    min_e: 391_200,
    min_n: 5_819_700,
    max_e: 391_800,
    max_n: 5_820_300,
};

#[test]
fn fixtures_are_what_the_builders_ask_for() {
    // tools/fixtures.sh spells these URLs out; the builders must still
    // emit them, or the fixtures no longer stand for what the app fetches.
    let wms = "https://gdi.berlin.de/services/wms";
    let tail = "&styles=&crs=EPSG:25833";
    let png = "&format=image/png&transparent=true&format_options=antialias:none";
    assert_eq!(
        get_map(&berlin::TERRAIN, TERRAIN_BOX, 256, 256),
        format!(
            "{wms}/dgm1?service=WMS&version=1.3.0&request=GetMap&layers=c_dgm1{tail}\
             &bbox=392000,5820000,393024,5821024&width=256&height=256{png}"
        )
    );
    assert_eq!(
        get_map(&berlin::TERRAIN, MUSEUMSINSEL, 300, 300),
        format!(
            "{wms}/dgm1?service=WMS&version=1.3.0&request=GetMap&layers=c_dgm1{tail}\
             &bbox=391200,5819700,391800,5820300&width=300&height=300{png}"
        )
    );
    assert_eq!(
        get_map(&berlin::SURFACE, MUSEUMSINSEL, 300, 300),
        format!(
            "{wms}/dom?service=WMS&version=1.3.0&request=GetMap&layers=c_dom{tail}\
             &bbox=391200,5819700,391800,5820300&width=300&height=300{png}"
        )
    );
    assert_eq!(
        get_map(&berlin::LAND_USE, MUSEUMSINSEL, 300, 300),
        format!(
            "{wms}/ua_flaechennutzung_2015?service=WMS&version=1.3.0&request=GetMap\
             &layers=c_ua_realnutz_2015{tail}&bbox=391200,5819700,391800,5820300\
             &width=300&height=300{png}"
        )
    );
    assert_eq!(
        get_map(&berlin::STOREYS, MUSEUMSINSEL, 300, 300),
        format!(
            "{wms}/gebaeude_geschosse?service=WMS&version=1.3.0&request=GetMap\
             &layers=a_geschosszahl_mehr_10,b_geschosszahl_7_10,c_geschosszahl_5_6,\
             d_geschosszahl_3_4,e_geschosszahl_1_2,f_geschosszahl_unter_1{tail}\
             &bbox=391200,5819700,391800,5820300&width=300&height=300{png}"
        )
    );
    let axes = FeatureQuery {
        properties: berlin::AXIS_PROPERTIES,
        count: Some(berlin::AXIS_PAGE),
        start_index: None,
    };
    let wfs = "https://gdi.berlin.de/services/wfs";
    let page = "&outputFormat=application/json\
                &bbox=391200,5819700,391800,5820300,urn:ogc:def:crs:EPSG::25833\
                &propertyName=uuid,brf,fsz,ftr,fkt,wdm,geom&count=2000";
    assert_eq!(
        get_features(&berlin::STREET_AXES, MUSEUMSINSEL, &axes),
        format!(
            "{wfs}/atkis?service=WFS&version=2.0.0&request=GetFeature\
             &typeNames=atkis:b08_ax_strassenachse_l{page}"
        )
    );
    assert_eq!(
        get_features(&berlin::CARRIAGEWAY_AXES, MUSEUMSINSEL, &axes),
        format!(
            "{wfs}/atkis?service=WFS&version=2.0.0&request=GetFeature\
             &typeNames=atkis:b07_ax_fahrbahnachse_l{page}"
        )
    );
    let page = |properties, count| FeatureQuery {
        properties,
        count: Some(count),
        start_index: None,
    };
    let bbox = "&bbox=391200,5819700,391800,5820300,urn:ogc:def:crs:EPSG::25833";
    let get = "?service=WFS&version=2.0.0&request=GetFeature";
    assert_eq!(
        get_features(
            &berlin::BUILDINGS,
            MUSEUMSINSEL,
            &page(berlin::BUILDING_PROPERTIES, berlin::BUILDING_PAGE)
        ),
        format!(
            "{wfs}/alkis_gebaeude{get}&typeNames=alkis_gebaeude:gebaeude\
             &outputFormat=application/json{bbox}&propertyName=uuid,gfk,aog,bezeich,geom\
             &count=3000"
        )
    );
    assert_eq!(
        get_features(
            &berlin::PARK_TREES,
            MUSEUMSINSEL,
            &page(berlin::TREE_PROPERTIES, berlin::TREE_PAGE)
        ),
        format!(
            "{wfs}/baumbestand{get}&typeNames=baumbestand:anlagenbaeume\
             &outputFormat=application/json{bbox}\
             &propertyName=gisid,gattung,baumhoehe,kronedurch,stammumfg,geom&count=6000"
        )
    );
    assert_eq!(
        get_features(
            &berlin::FurnitureKind::Bench.layer(),
            MUSEUMSINSEL,
            &page(&[], berlin::FURNITURE_PAGE)
        ),
        format!(
            "{wfs}/strassenbefahrung{get}&typeNames=strassenbefahrung:bj_sitzbank\
             &outputFormat=application/json{bbox}&count=3000"
        )
    );
    assert_eq!(
        get_features(
            &berlin::FurnitureKind::Lamp.layer(),
            MUSEUMSINSEL,
            &page(&[], berlin::FURNITURE_PAGE)
        ),
        format!(
            "{wfs}/beleuchtung{get}&typeNames=beleuchtung:beleuchtung\
             &outputFormat=application/json{bbox}&count=3000"
        )
    );
    assert_eq!(
        get_legend(&berlin::TERRAIN, 0),
        format!(
            "{wms}/dgm1?service=WMS&version=1.3.0&request=GetLegendGraphic&layer=c_dgm1\
             &format=application/json"
        )
    );
    assert_eq!(
        get_legend(&berlin::STOREYS, 5),
        format!(
            "{wms}/gebaeude_geschosse?service=WMS&version=1.3.0&request=GetLegendGraphic\
             &layer=f_geschosszahl_unter_1&format=application/json"
        )
    );
}

#[test]
fn terrain_decodes_within_a_quarter_metre_of_the_raw_model() {
    let legend = parse_value_legend(&fixture("dgm1_legend.json")).unwrap();
    // 100 entries: one transparent, two colours shared by neighbours.
    assert_eq!(legend.classes.len(), 97);
    let image = decode_png(&fixture("dgm1_392000_5820000_1024m_256px.png"), 256, 256).unwrap();
    let grid = decode_terrain(&image, &legend).unwrap();
    assert_eq!((grid.transparent, grid.unmatched), (0, 0));

    let truth = truth_heights();
    let n = truth.len() as f64;
    let error = |heights: &[f32], truth: &[f32]| {
        let (sum, abs) = heights
            .iter()
            .zip(truth)
            .fold((0.0, 0.0), |(s, a), (&h, &t)| {
                let e = f64::from(h - t);
                (s + e, a + e.abs())
            });
        (sum / n, abs / n)
    };
    let (bias, mean_abs) = error(&grid.heights, &truth);
    assert!(mean_abs < 0.30, "mean abs error {mean_abs:.3} m");
    assert!(bias.abs() < 0.25, "bias {bias:.3} m");

    // Row 0 is north: against the truth flipped north-south the decode is
    // metres out, so a flip in either would show.
    let flipped: Vec<f32> = truth.chunks_exact(256).rev().flatten().copied().collect();
    assert!(error(&grid.heights, &flipped).1 > 1.0);

    // The smoothing removes the one-metre terraces: where the ground is
    // flat (a truth step under 0.2 m between neighbours), a step of half a
    // metre or more is false. Class midpoints make many; the decode few.
    let midpoints: Vec<f32> = image
        .pixels
        .iter()
        .map(|p| {
            let class = legend
                .classes
                .iter()
                .find(|c| c.rgb == [p[0], p[1], p[2]])
                .unwrap();
            ((class.lo + class.hi) / 2.0) as f32
        })
        .collect();
    let false_steps = |heights: &[f32]| {
        let (mut flat, mut steps) = (0u32, 0u32);
        for row in 0..256 {
            for col in 0..255 {
                let i = row * 256 + col;
                if (truth[i + 1] - truth[i]).abs() < 0.2 {
                    flat += 1;
                    steps += u32::from((heights[i + 1] - heights[i]).abs() >= 0.5);
                }
            }
        }
        f64::from(steps) / f64::from(flat)
    };
    let (raw, smoothed) = (false_steps(&midpoints), false_steps(&grid.heights));
    assert!(raw > 0.04, "midpoints should terrace: {raw:.4}");
    assert!(smoothed < 0.015, "decode leaves {smoothed:.4} false steps");
    assert!(
        error(&midpoints, &truth).1 > mean_abs,
        "smoothing must help"
    );
}

#[test]
fn land_use_decodes_to_the_wfs_blocks() {
    let legend = parse_class_legend(&fixture("landuse_legend.json")).unwrap();
    assert_eq!(legend.classes.len(), LandUse::ALL.len());
    let table = berlin::land_use_table(&legend);
    assert!(
        table[1..].iter().all(Option::is_some),
        "every fill is a known class"
    );

    let image = decode_png(&fixture("landuse_391200_5819700_600m_300px.png"), 300, 300).unwrap();
    let grid = decode_classes(&image, &legend).unwrap();
    // Only the block outlines miss the legend.
    assert!(f64::from(grid.unmatched) / (300.0 * 300.0) < 0.06);

    let truth = grey8("landuse_truth_391200_5819700_600m_300px.png");
    let (mut scored, mut agree) = (0u32, 0u32);
    for (i, &t) in truth.iter().enumerate() {
        if t == 255 {
            continue;
        }
        scored += 1;
        let code = table[usize::from(grid.classes[i])].map_or(0, LandUse::code);
        agree += u32::from(code == u16::from(t));
    }
    let share = f64::from(agree) / f64::from(scored);
    assert!(share > 0.995, "{share:.4} of {scored} pixels agree");
    // The Museumsinsel: the Spree, the Lustgarten, the museums, and streets.
    for class in [LandUse::Water, LandUse::Park, LandUse::PublicSpecial] {
        assert!(
            grid.classes
                .iter()
                .any(|&c| table[usize::from(c)] == Some(class))
        );
    }
    assert!(grid.classes.contains(&CLASS_NONE));
}

#[test]
fn storeys_decode_to_the_alkis_footprints() {
    let legends = berlin::STOREYS.layers.iter().map(|layer| {
        parse_class_legend(&fixture(&format!("storeys_legend_{layer}.json"))).unwrap()
    });
    let legend = ClassLegend::concat(legends).unwrap();
    let table = berlin::storey_table(&legend);
    let bands: Vec<Option<StoreyBand>> = StoreyBand::ALL.into_iter().map(Some).collect();
    assert_eq!(table[0], None);
    assert_eq!(table[1..], bands[..], "class id k is the k-th band");

    let image = decode_png(&fixture("storeys_391200_5819700_600m_300px.png"), 300, 300).unwrap();
    let grid = decode_classes(&image, &legend).unwrap();
    let truth = grey8("storeys_truth_391200_5819700_600m_300px.png");
    let (mut buildings, mut agree, mut empty, mut none) = (0u32, 0u32, 0u32, 0u32);
    for (i, &t) in truth.iter().enumerate() {
        match t {
            1..=6 => {
                buildings += 1;
                agree += u32::from(grid.classes[i] == t);
            }
            0 => {
                empty += 1;
                none += u32::from(grid.classes[i] == CLASS_NONE);
            }
            _ => {}
        }
    }
    // The storeys layer and the footprints are separate ALKIS products, so
    // a few pixels differ; 99 % agree at 2 m pixels (measured 2026-10-08).
    let share = f64::from(agree) / f64::from(buildings);
    assert!(
        share > 0.97,
        "{share:.4} of {buildings} building pixels agree"
    );
    let clear = f64::from(none) / f64::from(empty);
    assert!(
        clear > 0.95,
        "{clear:.4} of {empty} empty pixels decode as none"
    );
}

#[test]
fn the_spree_at_the_museumsinsel_settles_at_its_level() {
    // The land-use square's terrain and land use, decoded on one 2 m grid.
    let terrain = parse_value_legend(&fixture("dgm1_legend.json")).unwrap();
    let image = decode_png(&fixture("dgm1_391200_5819700_600m_300px.png"), 300, 300).unwrap();
    let grid = decode_terrain(&image, &terrain).unwrap();
    assert_eq!((grid.transparent, grid.unmatched), (0, 0));
    let mut heights = grid.heights;
    let legend = parse_class_legend(&fixture("landuse_legend.json")).unwrap();
    let table = berlin::land_use_table(&legend);
    let image = decode_png(&fixture("landuse_391200_5819700_600m_300px.png"), 300, 300).unwrap();
    let wet: Vec<bool> = decode_classes(&image, &legend)
        .unwrap()
        .classes
        .iter()
        .map(|&c| table[usize::from(c)] == Some(LandUse::Water))
        .collect();
    let before = heights.clone();
    let settled = water::settle(&mut heights, &wet, 300, 300, 2.0).unwrap();
    // The Spree below the Muehlendamm lock lies at about 30.5 m; the
    // terrain layer draws it flat in the 30-31 m class (measured 30.54 m).
    assert!(
        (30.3..30.8).contains(&settled.level),
        "level {}",
        settled.level
    );
    // Every body mapped here is the Spree or an arm of it, and is drawn as
    // water: the smoothed shore trades a few pixels each way (10,504 drawn
    // of 10,525 mapped).
    let mapped = wet.iter().filter(|&&w| w).count() as u32;
    assert_eq!(settled.stranded, 0);
    assert!(
        settled.wet.abs_diff(mapped) * 100 < mapped,
        "{settled:?} of {mapped}"
    );
    // Out of the shore's reach - nothing but water, or no water, within
    // two pixels - the rules hold exactly: the water below the level, the
    // land above it, and no land lifted by a metre.
    let only = |i: usize, want: bool| {
        let (x, y) = ((i % 300) as i64, (i / 300) as i64);
        (-2..=2).all(|dy| {
            (-2..=2).all(|dx| {
                let (nx, ny) = (x + dx, y + dy);
                !(0..300).contains(&nx)
                    || !(0..300).contains(&ny)
                    || wet[(ny * 300 + nx) as usize] == want
            })
        })
    };
    let crest = settled.level + water::FREEBOARD_M;
    for (i, (&now, &was)) in heights.iter().zip(&before).enumerate() {
        if wet[i] && only(i, true) {
            assert!(now < settled.level, "water pixel {i} at {now}");
        } else if !wet[i] && only(i, false) {
            assert!(
                now >= crest && now - was < 1.0,
                "land pixel {i}: {was} to {now}"
            );
        }
    }
}

#[test]
fn the_surface_model_stands_the_berliner_dom_90_m_over_its_square() {
    let legend = parse_value_legend(&fixture("dom_legend.json")).unwrap();
    let image = decode_png(&fixture("dom_391200_5819700_600m_300px.png"), 300, 300).unwrap();
    let surface = decode_terrain(&image, &legend).unwrap();
    assert_eq!((surface.transparent, surface.unmatched), (0, 0));
    let ground = decode_terrain(
        &decode_png(&fixture("dgm1_391200_5819700_600m_300px.png"), 300, 300).unwrap(),
        &parse_value_legend(&fixture("dgm1_legend.json")).unwrap(),
    )
    .unwrap();
    // The highest thing standing on the Museumsinsel square: the dome of the
    // Berliner Dom, at E 391513 N 5819981, 98 m tall to its lantern cross.
    let (top, at) =
        surface.heights.iter().enumerate().fold(
            (f32::MIN, 0),
            |(m, a), (i, &h)| if h > m { (h, i) } else { (m, a) },
        );
    let (col, row) = (at % 300, at / 300);
    assert_eq!(
        (391_200 + 2 * col as i64 + 1, 5_820_300 - 2 * row as i64 - 1),
        (391_513, 5_819_981)
    );
    let standing = top - ground.heights[at];
    assert!(
        (85.0..100.0).contains(&standing),
        "{standing} m over the ground"
    );
    // Less the terrain, open ground stands at about nothing.
    let above: Vec<f32> = surface
        .heights
        .iter()
        .zip(&ground.heights)
        .map(|(s, g)| s - g)
        .collect();
    let mut sorted = above.clone();
    sorted.sort_by(f32::total_cmp);
    assert!(
        sorted[sorted.len() / 20] > -2.0,
        "a twentieth below {}",
        sorted[sorted.len() / 20]
    );
}

/// The Museumsinsel square's streets, as ATKIS draws them: 70 stretches of
/// street, 12 of them the middle of a street whose carriageways run apart
/// (the Karl-Liebknecht-Strasse and the Schlossplatz), drawn as 28
/// carriageways of their own - the Liebknechtbruecke among them - and the
/// pedestrian zone round the museums.
#[test]
fn the_museumsinsel_streets_read_as_atkis_draws_them() {
    let streets =
        berlin::parse_axes(&fixture("atkis_strassenachse_391200_5819700_600m.json")).unwrap();
    let carriageways =
        berlin::parse_axes(&fixture("atkis_fahrbahnachse_391200_5819700_600m.json")).unwrap();
    assert!(!streets.is_cut_short() && !carriageways.is_cut_short());
    assert_eq!((streets.axes.len(), carriageways.axes.len()), (70, 28));
    assert_eq!(streets.axes.iter().filter(|a| a.separated).count(), 12);
    assert_eq!(streets.axes.iter().filter(|a| a.pedestrian).count(), 12);
    assert!(
        carriageways
            .axes
            .iter()
            .all(|a| a.separated && !a.pedestrian)
    );
    // Every carriageway, and every street that is one, says how wide it is.
    let drawn = streets
        .axes
        .iter()
        .filter(|a| !a.separated)
        .chain(&carriageways.axes);
    assert!(
        drawn
            .clone()
            .all(|a| a.width.is_some_and(|w| (4.0..=16.0).contains(&w)))
    );
    assert!(drawn.flat_map(|a| &a.lines).all(|l| l.len() >= 2));
    // The Liebknechtbruecke, over the Spree beside the Dom, a federal road.
    let bridge = carriageways
        .axes
        .iter()
        .find(|a| a.uuid == "DEBEATKB10000FP5")
        .expect("the bridge");
    assert_eq!(
        (bridge.width, bridge.dedication),
        (Some(5.5), berlin::Dedication::Federal)
    );
    assert_eq!(bridge.lines[0][0], [391_606.037, 5_819_950.312_9]);
}

/// The Museumsinsel square's buildings, trees and street furniture, as the
/// city records them: 66 buildings and 173 parts of them - the museums,
/// the Dom, the Humboldt Forum's 20,000 m2 - 118 street trees and 349 park
/// trees, lindens most of them, and the furniture of its streets.
#[test]
fn the_museumsinsel_street_level_reads_as_the_city_records_it() {
    let buildings =
        berlin::parse_buildings(&fixture("alkis_gebaeude_391200_5819700_600m.json")).unwrap();
    assert!(!buildings.is_cut_short());
    let (parts, whole): (Vec<_>, Vec<_>) = buildings.buildings.iter().partition(|b| b.part);
    assert_eq!((whole.len(), parts.len()), (66, 173));
    let largest = whole.iter().map(|b| b.area()).fold(0.0, f64::max);
    assert!((20_000.0..21_000.0).contains(&largest), "{largest}");
    let uses: std::collections::BTreeMap<String, usize> =
        whole.iter().fold(Default::default(), |mut m, b| {
            *m.entry(format!("{:?}", b.usage())).or_default() += 1;
            m
        });
    assert_eq!(uses.get("Cultural"), Some(&14), "{uses:?}");
    assert_eq!(uses.get("Religious"), Some(&1), "the Dom");
    assert_eq!(uses.get("Underground"), Some(&2));

    let trees: Vec<berlin::InventoryTree> = ["strassenbaeume", "anlagenbaeume"]
        .iter()
        .flat_map(|layer| {
            berlin::parse_trees(&fixture(&format!(
                "baumbestand_{layer}_391200_5819700_600m.json"
            )))
            .unwrap()
            .trees
        })
        .collect();
    assert_eq!(trees.len(), 118 + 349);
    let lindens = trees
        .iter()
        .filter(|t| t.genus.as_deref() == Some("Tilia"))
        .count();
    assert_eq!(lindens, 74 + 160);
    assert!(
        trees
            .iter()
            .all(|t| t.height.is_none_or(|h| (1.0..45.0).contains(&h)))
    );

    let counts: Vec<(berlin::FurnitureKind, usize)> = berlin::FurnitureKind::ALL
        .iter()
        .map(|&kind| {
            let layer = kind.layer().type_name.replace(':', "_");
            let page = berlin::parse_furniture(
                kind,
                &fixture(&format!("{layer}_391200_5819700_600m.json")),
            )
            .unwrap();
            assert!(
                page.items.iter().all(|i| !i.id.is_empty()),
                "{kind:?} keyed"
            );
            (kind, page.items.len())
        })
        .collect();
    use berlin::FurnitureKind::*;
    assert_eq!(
        counts,
        vec![
            (Lamp, 186),
            (Bench, 40),
            (Bin, 45),
            (Bollard, 223),
            (Shelter, 0),
            (Sign, 144),
            (Fountain, 2),
            (Column, 53),
            (BikeRack, 141)
        ]
    );
}
