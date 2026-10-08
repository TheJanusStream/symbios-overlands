# geodata fixtures

Live answers of GDI Berlin's services, recorded 2026-10-08, and truth rasters
derived from independent sources to score the decoders against. All of it is
under the Datenlizenz Deutschland - Zero - Version 2.0
(<https://www.govdata.de/dl-de/zero-2-0>), which allows any use without
attribution. Source: Senatsverwaltung fuer Stadtentwicklung, Bauen und Wohnen
Berlin, <https://gdi.berlin.de>.

| File | What | From |
| --- | --- | --- |
| `dgm1_legend.json` | JSON legend of the terrain layer `c_dgm1` (served in Latin-1) | `GetLegendGraphic` |
| `dgm1_392000_5820000_1024m_256px.png` | terrain render, E 392000-393024, N 5820000-5821024, 4 m pixels | `GetMap` |
| `dgm1_391200_5819700_600m_300px.png` | terrain render over the land-use square (Museumsinsel, the Spree at 30-31 m), 2 m pixels | `GetMap` |
| `dgm1_truth_392000_5820000_1024m_4m.png` | the same square from the raw DGM1 tile 392_5820 (2025): 4 m block means in centimetres, 16-bit grey, north up | ATOM `dgm1/atom/DGM1_392_5820.zip` |
| `landuse_legend.json` | JSON legend of land use `c_ua_realnutz_2015` | `GetLegendGraphic` |
| `landuse_391200_5819700_600m_300px.png` | land-use render, E 391200-391800, N 5819700-5820300 (Museumsinsel), 2 m pixels | `GetMap` |
| `landuse_truth_391200_5819700_600m_300px.png` | the WFS land-use blocks rasterised to those pixels: class code, 0 street space, 255 unscored | WFS `c_ua_realnutz_2015` |
| `storeys_legend_*.json` | JSON legends of the six storey-band layers | `GetLegendGraphic` |
| `storeys_391200_5819700_600m_300px.png` | storeys render of all six layers over the land-use square | `GetMap` |
| `storeys_truth_391200_5819700_600m_300px.png` | the WFS ALKIS footprints: band 1 (>10 storeys) to 6 (<1) of the whole building under a pixel, 0 no footprint, 255 unscored | WFS `alkis_gebaeude:gebaeude` |

The renders and legends come from the URLs the crate's builders emit;
`tests/decode.rs` holds the builders to those URLs, and
`tools/fixtures.sh` re-records them. `tools/truth.py` regenerates the truth
rasters and says where its inputs come from.
