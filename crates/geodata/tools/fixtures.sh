#!/bin/sh
# Re-record the live GDI Berlin answers under tests/fixtures (all dl-de/zero-2.0).
#
#   sh crates/geodata/tools/fixtures.sh <empty download dir>
#
# The URLs are exactly what the crate's builders emit (tests/decode.rs holds
# them to that), so the fixtures are what the app would fetch. Inspect the
# downloads before copying them into tests/fixtures. The terrain truth grid
# is not fetched here: see tests/fixtures/README.md.
set -eu
out=${1:?download dir}
b=https://gdi.berlin.de/services

get() {
  curl -sS -m 120 -o "$out/$1" -w "$1 %{http_code} %{content_type} %{size_download}B\n" "$2"
}

legend() { # service layer
  echo "$b/wms/$1?service=WMS&version=1.3.0&request=GetLegendGraphic&layer=$2&format=application/json"
}

map() { # service layers bbox size
  echo "$b/wms/$1?service=WMS&version=1.3.0&request=GetMap&layers=$2&styles=&crs=EPSG:25833&bbox=$3&width=$4&height=$4&format=image/png&transparent=true&format_options=antialias:none"
}

features() { # service type bbox properties count - a page of GeoJSON features
  echo "$b/wfs/$1?service=WFS&version=2.0.0&request=GetFeature&typeNames=$1:$2&outputFormat=application/json&bbox=$3,urn:ogc:def:crs:EPSG::25833${4:+&propertyName=$4}&count=$5"
}

axes() { # type bbox - a page of street axes, as berlin::parse_axes reads them
  echo "$b/wfs/atkis?service=WFS&version=2.0.0&request=GetFeature&typeNames=atkis:$1&outputFormat=application/json&bbox=$2,urn:ogc:def:crs:EPSG::25833&propertyName=uuid,brf,fsz,ftr,fkt,wdm,geom&count=2000"
}

get dgm1_legend.json "$(legend dgm1 c_dgm1)"
get dgm1_392000_5820000_1024m_256px.png "$(map dgm1 c_dgm1 392000,5820000,393024,5821024 256)"
get dgm1_391200_5819700_600m_300px.png "$(map dgm1 c_dgm1 391200,5819700,391800,5820300 300)"

# A 200 m core and the 600 m square around it, as a far-field region asks
# for them (the app's terrain tests, #1585).
get dgm1_391400_5819900_200m_100px.png "$(map dgm1 c_dgm1 391400,5819900,391600,5820100 100)"
get dgm1_391200_5819700_600m_64px.png "$(map dgm1 c_dgm1 391200,5819700,391800,5820300 64)"

get dom_legend.json "$(legend dom c_dom)"
get dom_391200_5819700_600m_300px.png "$(map dom c_dom 391200,5819700,391800,5820300 300)"
get dom_391200_5819700_600m_150px.png "$(map dom c_dom 391200,5819700,391800,5820300 150)"

get landuse_legend.json "$(legend ua_flaechennutzung_2015 c_ua_realnutz_2015)"
get landuse_391200_5819700_600m_150px.png \
  "$(map ua_flaechennutzung_2015 c_ua_realnutz_2015 391200,5819700,391800,5820300 150)"
get landuse_391400_5819900_200m_100px.png \
  "$(map ua_flaechennutzung_2015 c_ua_realnutz_2015 391400,5819900,391600,5820100 100)"
get landuse_391200_5819700_600m_64px.png \
  "$(map ua_flaechennutzung_2015 c_ua_realnutz_2015 391200,5819700,391800,5820300 64)"
get landuse_391200_5819700_600m_300px.png \
  "$(map ua_flaechennutzung_2015 c_ua_realnutz_2015 391200,5819700,391800,5820300 300)"

storeys=a_geschosszahl_mehr_10,b_geschosszahl_7_10,c_geschosszahl_5_6,d_geschosszahl_3_4,e_geschosszahl_1_2,f_geschosszahl_unter_1
for layer in $(echo $storeys | tr , ' '); do
  get "storeys_legend_$layer.json" "$(legend gebaeude_geschosse "$layer")"
done
get storeys_391200_5819700_600m_300px.png \
  "$(map gebaeude_geschosse $storeys 391200,5819700,391800,5820300 300)"

# The street and carriageway axes of the Museumsinsel square (#1595).
get atkis_strassenachse_391200_5819700_600m.json \
  "$(axes b08_ax_strassenachse_l 391200,5819700,391800,5820300)"
get atkis_fahrbahnachse_391200_5819700_600m.json \
  "$(axes b07_ax_fahrbahnachse_l 391200,5819700,391800,5820300)"

# The Museumsinsel square's buildings, trees and street furniture (#1588).
square=391200,5819700,391800,5820300
get alkis_gebaeude_391200_5819700_600m.json \
  "$(features alkis_gebaeude gebaeude $square uuid,gfk,aog,bezeich,geom 3000)"
for layer in strassenbaeume anlagenbaeume; do
  get "baumbestand_${layer}_391200_5819700_600m.json" \
    "$(features baumbestand "$layer" $square gisid,gattung,baumhoehe,kronedurch,stammumfg,geom 6000)"
done
get beleuchtung_beleuchtung_391200_5819700_600m.json \
  "$(features beleuchtung beleuchtung $square "" 3000)"
for layer in bj_sitzbank ah_abfallbehaelter_muellbox av_poller br_fahrgastunterstand \
  aa_verkehrszeichen bv_springbrunnen_zierbrunnen ag_werbesaeule bq_fahrradstaender; do
  get "strassenbefahrung_${layer}_391200_5819700_600m.json" \
    "$(features strassenbefahrung "$layer" $square "" 3000)"
done

# The Museumsinsel square's blocks by urban-structure type (#1600).
get ua_stadtstruktur_b_stadtstruktur_differenziert_2024_391200_5819700_600m.json \
  "$(features ua_stadtstruktur b_stadtstruktur_differenziert_2024 $square schluessel,typ,geom 1000)"
