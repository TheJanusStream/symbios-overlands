# geodata fixtures

Live answers of GDI Berlin's services, recorded 2026-10-08 (the urban
structure's page on 2026-10-10), and truth rasters
derived from independent sources to score the decoders against. All of it is
under the Datenlizenz Deutschland - Zero - Version 2.0
(<https://www.govdata.de/dl-de/zero-2-0>), which allows any use without
attribution. Source: Senatsverwaltung fuer Stadtentwicklung, Bauen und Wohnen
Berlin, <https://gdi.berlin.de>.

| File | What | From |
| --- | --- | --- |
| `alkis_gebaeude_391200_5819700_600m.json` | the ALKIS buildings and building parts of the Museumsinsel square, the attributes `berlin::parse_buildings` reads | WFS `alkis_gebaeude:gebaeude` |
| `baumbestand_strassenbaeume_391200_5819700_600m.json`, `baumbestand_anlagenbaeume_391200_5819700_600m.json` | the square's street trees and park trees, the attributes `berlin::parse_trees` reads | WFS `baumbestand:strassenbaeume`, `baumbestand:anlagenbaeume` |
| `beleuchtung_beleuchtung_391200_5819700_600m.json` | the square's street lamps (and the lighting register's switch cabinets, which `berlin::parse_furniture` leaves out), every attribute | WFS `beleuchtung:beleuchtung` |
| `strassenbefahrung_*_391200_5819700_600m.json` | the square's other street furniture, one page per kind `berlin::FurnitureKind` reads, every attribute (the shelters' page is empty: the square has none) | WFS `strassenbefahrung:*` |
| `atkis_strassenachse_391200_5819700_600m.json`, `atkis_fahrbahnachse_391200_5819700_600m.json` | the ATKIS street and carriageway axes of the Museumsinsel square: one page of GeoJSON each, the attributes `berlin::parse_axes` reads | WFS `atkis:b08_ax_strassenachse_l`, `atkis:b07_ax_fahrbahnachse_l` |
| `ua_stadtstruktur_b_stadtstruktur_differenziert_2024_391200_5819700_600m.json` | the Environmental Atlas's blocks of the Museumsinsel square with their urban-structure types, the attributes `berlin::parse_blocks` reads | WFS `ua_stadtstruktur:b_stadtstruktur_differenziert_2024` |
| `dgm1_legend.json` | JSON legend of the terrain layer `c_dgm1` (served in Latin-1) | `GetLegendGraphic` |
| `dgm1_392000_5820000_1024m_256px.png` | terrain render, E 392000-393024, N 5820000-5821024, 4 m pixels | `GetMap` |
| `dgm1_391200_5819700_600m_300px.png` | terrain render over the land-use square (Museumsinsel, the Spree at 30-31 m), 2 m pixels | `GetMap` |
| `dgm1_391400_5819900_200m_100px.png`, `landuse_391400_5819900_200m_100px.png` | terrain and land use of a 200 m core in the middle of the Museumsinsel square, 2 m pixels | `GetMap` |
| `dgm1_391200_5819700_600m_64px.png`, `landuse_391200_5819700_600m_64px.png` | the same Museumsinsel square whole, at a far field's 9.4 m pixels | `GetMap` |
| `dgm1_truth_392000_5820000_1024m_4m.png` | the same square from the raw DGM1 tile 392_5820 (2025): 4 m block means in centimetres, 16-bit grey, north up | ATOM `dgm1/atom/DGM1_392_5820.zip` |
| `dom_legend.json` | JSON legend of the surface model `c_dom` (served in Latin-1) | `GetLegendGraphic` |
| `dom_391200_5819700_600m_300px.png`, `dom_391200_5819700_600m_150px.png` | surface model over the Museumsinsel square, 2 m and 4 m pixels | `GetMap` |
| `landuse_391200_5819700_600m_150px.png` | land use over the same square at a ring's 4 m pixels | `GetMap` |
| `landuse_legend.json` | JSON legend of land use `c_ua_realnutz_2015` | `GetLegendGraphic` |
| `landuse_391200_5819700_600m_300px.png` | land-use render, E 391200-391800, N 5819700-5820300 (Museumsinsel), 2 m pixels | `GetMap` |
| `landuse_truth_391200_5819700_600m_300px.png` | the WFS land-use blocks rasterised to those pixels: class code, 0 street space, 255 unscored | WFS `c_ua_realnutz_2015` |
| `storeys_legend_*.json` | JSON legends of the six storey-band layers | `GetLegendGraphic` |
| `storeys_391200_5819700_600m_300px.png` | storeys render of all six layers over the land-use square | `GetMap` |
| `storeys_truth_391200_5819700_600m_300px.png` | the WFS ALKIS footprints: band 1 (>10 storeys) to 6 (<1) of the whole building under a pixel, 0 no footprint, 255 unscored | WFS `alkis_gebaeude:gebaeude` |

The renders, legends and feature pages come from the URLs the crate's
builders emit;
`tests/decode.rs` holds the builders to those URLs, and
`tools/fixtures.sh` re-records them. `tools/truth.py` regenerates the truth
rasters and says where its inputs come from.
