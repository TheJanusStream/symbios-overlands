# Geodata regions

A geodata region is a square of real Berlin at real scale, dressed in the
region's theme: Berlin's ground, water, blocks, buildings and trees, built
from the catalogue the region would use anyway. The data is GDI Berlin's
(<https://gdi.berlin.de>), fetched when the region is visited. Epic #1580;
the API research and the owner's decisions are on #1579.

Everything used here is under the Datenlizenz Deutschland - Zero - Version
2.0 (<https://www.govdata.de/dl-de/zero-2-0>): any use, no attribution
required. The region info credits the source anyway. One layer seen during
research is not zero-licensed and must not be used: `rbs_strab` (CC BY 3.0
DE).

## Decisions (owner, 2026-10-08)

- **Themed**, not a faithful copy: the region's `ThemeArchetype` dresses
  real Berlin.
- **Drawn by the DID**, like the other seeded categories, and lockable in
  the seed GUI. The draw picks the source (procedural or Berlin) and, for
  Berlin, a square: a random size and a random position.
- **Every size**, 250 m to 19 km (log-uniform, so each doubling of size is
  equally likely). The owner asked for 20 km; Berlin's shape allows 19 (see
  [Coverage](#coverage-and-the-square)).
- **Real scale, 1 m = 1 m both ways.** What matters is a realistic
  perspective standing on the ground, not the relief seen from afar.
  Compressing a large square onto today's ~1 km world would either shrink
  the hills to mounds or make the slopes twenty times steeper. So the
  square is the region's real extent, and the detail falls off with
  distance instead:
  - a walkable street-level **core** about today's world (~1 km, centred on
    spawn, at most the square);
  - a **middle ring** at block level;
  - a coarse **far field** out to the square's edge, as a real horizon with
    no colliders.
- **Fetched at visit**, with the app's own cache, not baked into packs.
  Requests are sized to the pixels needed: the GDI viewer's zoom-fitting
  loading, used as distance LOD.
- **Saves keep seed + edits**, adopted per item (copy-on-write). Untouched
  content is derived again on every load. An item the owner moved or
  restyled becomes ordinary record content with a back-reference to its
  stable source id, and its original is suppressed. Content hashes of the
  fetched layers are stored at save, so a stale cache can be detected.

## How the data is read

GDI Berlin is GeoServer in EPSG:25833 (ETRS89 / UTM 33N, metres). It sends
`Access-Control-Allow-Origin: *`, so a browser can fetch it directly. There
is no coverage service (WCS), and client styles (`SLD_BODY`) are ignored.
Two kinds of request carry everything:

- **WMS `GetMap` decoded through its JSON legend.** A render of any square,
  at any pixel size, costs one small request. `GetLegendGraphic` with
  `format=application/json` names every class's exact colour, so the PNG
  reads back as data:
  - The terrain layer `c_dgm1` is drawn in one-metre height classes.
    Decoding each pixel's class and then smoothing within it gives 0.26 m
    mean error against the raw 1 m model, for 9 KB per 256 x 256 render.
  - Land use (`c_ua_realnutz_2015`) and building storeys
    (`gebaeude_geschosse`) decode to class ids. GeoServer's
    `format_options=antialias:none` keeps fills crisp.
- **WFS `GetFeature` as GeoJSON**, for street-level vectors with exact
  attributes: footprints, trees, water, blocks. It supports bbox,
  `propertyName`, `count`/`startIndex` paging, and `resultType=hits` to size
  a request first.

Measured on the recorded fixtures (`crates/geodata/tests/decode.rs`):

| Layer | Decoded against | Agreement |
| --- | --- | --- |
| terrain, 4 m pixels | raw DGM1, 4 m block means | 0.26 m mean error (class midpoints: 0.42 m); false one-metre steps on flat ground 0.7 % (midpoints: 6 %) |
| land use, 2 m pixels | WFS blocks | 99.9 % of pixels |
| storeys, 2 m pixels | WFS ALKIS footprints | 99 % of building pixels, 98 % of open ground |

Traps, each found the hard way:

- **Legends are Latin-1** while calling themselves JSON. The parser falls
  back to Latin-1 when a body is not UTF-8.
- **`intervals` colour maps are off by one in their labels.** An entry's
  colour covers the values above the previous entry's *quantity* up to its
  own; the human labels are a metre lower. The terrain legend's first
  opaque class spans -7 m to 26 m (Berlin's ground never goes that low), so
  the decoder treats a class far wider than typical as open-ended, within
  one typical width of its inner bound.
- **The storeys layer is street-level only.** Its outline is a pixel wide at
  any scale, so at 40 m per pixel two thirds of the render is outline. The
  middle ring needs the land-use blocks' urban-structure type instead
  (#1587).
- **No-store.** WMS answers say `Cache-Control: no-cache, no-store`, so the
  browser keeps nothing: the app caches by request URL itself (#1582).
  That is why the builders emit one canonical string per request.
- **Raw elevation exists but is heavy:** 2 km GeoTIFF tiles of 16 MB,
  uncompressed float32, one row per strip. They support HTTP `Range`, so
  any row span can be read exactly (~1 MB per km^2 at 4 m). Kept in reserve
  should the core ever need more than the decoded classes give.

## Coverage and the square

A square may lie only wholly inside Berlin, so every layer has data under
all of it. `crates/geodata/tools/coverage.py` rasterises the ALKIS state and
borough boundaries into a 250 m grid: 13,787 cells (862 km^2) count as
inside, each with its borough. That grid is checked in as a 5 KB run-length
table. "Wholly inside" holds to the tool's 25 m sampling: a cell may reach
about one sample past the border, a sliver the decoders fill from its
neighbours.

`Coverage::place` picks uniformly among *every* whole-metre position where a
square of the drawn size fits. It counts, per corner cell, the offsets that
give each footprint, so no part of Berlin is likelier than another and no
draw is rejected. A 19 km square has 751 positions; 19.01 km has none.
Squares from fixed draws are pinned in a test, and an independent Python
implementation (`crates/geodata/tools/place_check.py`) reproduces them. Changing the draw mapping moves every
seeded region and needs a migration, not a re-bless.

The two draws come from the region's own salted stream in the app
(`ChaCha8Rng::seed_from_u64(room_seed ^ SALT)`, like every other seeded
stream). The crate maps them to a square and owns no randomness.

## Code

`crates/geodata` (P0.1, #1581) is pure, with no I/O and no Bevy, so the app
and the wasm worker share it:

- `square`: `GeoSquare` and the log-uniform size draw;
- `berlin`: the host, the layer catalogue, `LandUse` (22 classes),
  `StoreyBand`, `Borough`, and `Coverage`;
- `request`: canonical `GetMap`, `GetLegendGraphic` and `GetFeature` URLs;
- `legend`: GeoServer JSON legends, as value ranges or fill classes;
- `raster`: PNG to RGBA; terrain to heights with clamped smoothing;
  categorical layers to class ids, with outlines resolved by neighbour vote.

`tests/fixtures/README.md` lists the recorded answers and the truth rasters.
`tools/fixtures.sh` and `tools/truth.py` regenerate them.

## Phases

Each phase is a sub-issue of #1580, in build order:

| Issue | Phase |
| --- | --- |
| #1581 | P0.1 `crates/geodata`: square, coverage, catalogue, requests, decoders |
| #1582 | P0.2 fetch + cache service (native + wasm), host allow-list, size caps |
| #1583 | P1.1 record schema `GeoSource` + sanitiser, wire fixtures, editor slice |
| #1584 | P1.2 real-scale terrain core from `c_dgm1`, collider, world digest |
| #1585 | P1.3 far-field terrain ring to the square's edge, no collider, fog |
| #1586 | P1.4 ground splat from land use, water from the water layers |
| #1587 | P2.1 derived-content stage with stable ids; block-level buildings |
| #1588 | P2.2 street-level core: footprints, trees, street furniture |
| #1589 | P3.1 the DID draws the source; seed-row locks; exact-square lock |
| #1590 | P3.2 owner edits over derived content; layer hashes |
| #1591 | P4 walkable area past the core (revive terrain streaming) |

The Berlin share of the source draw stays at 0 until P2 makes a themed
Berlin worth landing in. Before that, a Berlin region can only be chosen on
purpose.
