"""Regenerate the truth rasters under tests/fixtures from independent sources.

The decode tests score the crate's decoders against data that never passed
through a WMS render:

- terrain: the raw ATKIS DGM1 tile 392_5820 (2025, ATOM download), block
  means at 4 m over the render's bbox, as a 16-bit PNG of centimetres;
- land use: the WFS land-use blocks, rasterised to the render's pixels, as
  the class code per pixel;
- storeys: the WFS ALKIS footprints, as the storey band of the one whole
  building (AX_Gebaeude) covering a pixel.

Inputs (all dl-de/zero-2.0), downloaded into a directory of their own:

    curl -o DGM1_392_5820.zip https://gdi.berlin.de/data/dgm1/atom/DGM1_392_5820.zip
    wfs() { curl -sS -G -o "$2.json" "https://gdi.berlin.de/services/wfs/${1%%:*}" \
      --data-urlencode service=WFS --data-urlencode version=2.0.0 \
      --data-urlencode request=GetFeature --data-urlencode "typeNames=$1" \
      --data-urlencode outputFormat=application/json \
      --data-urlencode "bbox=391200,5819700,391800,5820300,urn:ogc:def:crs:EPSG::25833"; }
    wfs ua_flaechennutzung_2015:c_ua_realnutz_2015 landuse
    wfs alkis_gebaeude:gebaeude buildings

Run from the repository root, isolated (the downloads are data, not code):

    python3 -I crates/geodata/tools/truth.py DGM1_392_5820.zip landuse.json \
        buildings.json crates/geodata/tests/fixtures

Needs numpy and Pillow. Every raster is north-up, like a WMS render. In the
class rasters 255 marks a pixel not to score: one a boundary crosses (at 4x
supersampling), and for storeys also one a building part covers or one
whose building has no storey count.
"""

import json
import sys
import zipfile

import numpy as np
from PIL import Image, ImageDraw

UNSCORED = 255


def terrain(zip_path, out):
    # The tile is 2000 x 2000 points at cell centres, E 392000-394000,
    # N 5820000-5822000; the render covers E 392000-393024, N 5820000-5821024.
    grid = np.full((2000, 2000), np.nan)
    with zipfile.ZipFile(zip_path) as z:
        name = next(n for n in z.namelist() if n.endswith(".xyz"))
        with z.open(name) as f:
            for line in f:
                x, y, h = line.split()
                col, row = int(float(x) - 392000), int(float(y) - 5820000)
                grid[row, col] = float(h)
    assert not np.isnan(grid).any(), "the tile has holes"
    window = grid[:1024, :1024][::-1]  # north up
    means = window.reshape(256, 4, 256, 4).mean(axis=(1, 3))
    cm = np.round(means * 100).astype(np.uint16)
    Image.fromarray(cm).save(f"{out}/dgm1_truth_392000_5820000_1024m_4m.png")


def raster(features, value_of, e0=391200.0, n1=5820300.0, px=2.0, width=300, ss=4):
    img = Image.new("I", (width * ss, width * ss), 0)
    draw = ImageDraw.Draw(img)

    def at(q):
        return ((q[0] - e0) / px * ss, (n1 - q[1]) / px * ss)

    for feature in features:
        value = value_of(feature)
        if value is None:
            continue
        geometry = feature["geometry"]
        polygons = geometry["coordinates"]
        if geometry["type"] == "Polygon":
            polygons = [polygons]
        for polygon in polygons:
            draw.polygon([at(q) for q in polygon[0]], fill=value)
            for hole in polygon[1:]:
                draw.polygon([at(q) for q in hole], fill=0)
    cells = np.asarray(img).reshape(width, ss, width, ss)
    first = cells[:, 0, :, 0]
    uniform = (cells == first[:, None, :, None]).all(axis=(1, 3))
    return np.where(uniform, first, UNSCORED)


def land_use(path, out):
    with open(path, encoding="utf-8") as f:
        blocks = json.load(f)["features"]

    def code(feature):
        p = feature["properties"]
        return int(p["woz"] or p["grz"] or 0) or None

    truth = raster(blocks, code).astype(np.uint8)
    Image.fromarray(truth).save(f"{out}/landuse_truth_391200_5819700_600m_300px.png")


def storeys(path, out):
    with open(path, encoding="utf-8") as f:
        footprints = json.load(f)["features"]

    def band(feature):
        p = feature["properties"]
        if p.get("bezeich") != "AX_Gebaeude":
            return None
        try:
            aog = int(p.get("aog"))
        except (TypeError, ValueError):
            return UNSCORED
        # 1 = more than 10 storeys ... 6 = under one, the layer order
        for k, floor in enumerate((11, 7, 5, 3, 1)):
            if aog >= floor:
                return k + 1
        return 6

    whole = raster(footprints, band)
    parts = raster(
        [f for f in footprints if f["properties"].get("bezeich") == "AX_Bauteil"],
        lambda f: 1,
    )
    covered = raster(footprints, lambda f: 1)
    truth = np.where(parts != 0, UNSCORED, whole)
    truth = np.where(covered == 0, 0, truth).astype(np.uint8)
    Image.fromarray(truth).save(f"{out}/storeys_truth_391200_5819700_600m_300px.png")


def main():
    zip_path, landuse_path, buildings_path, out = sys.argv[1:5]
    terrain(zip_path, out)
    land_use(landuse_path, out)
    storeys(buildings_path, out)


if __name__ == "__main__":
    main()
