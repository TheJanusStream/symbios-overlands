//! GeoServer's JSON legends (`GetLegendGraphic&format=application/json`).
//!
//! A legend names the exact colour of every class a layer is drawn with,
//! which is what lets [`crate::raster`] read a render back as data. Two
//! shapes are read here:
//!
//! - a raster layer's **colour map** of type `intervals` (the terrain): each
//!   entry's colour covers the values above the previous entry's quantity up
//!   to its own, so a colour decodes to a value range ([`ValueLegend`]);
//! - a vector layer's **rules** with polygon fills (land use, storeys): each
//!   fill colour is one class, named by the rule's attribute filter
//!   ([`ClassLegend`]).
//!
//! GDI Berlin serves these bodies in Latin-1 while calling them JSON, so a
//! body that is not UTF-8 is read as Latin-1 rather than refused.

use std::borrow::Cow;

use serde::Deserialize;

/// The most classes a [`ClassLegend`] may hold: class ids are a `u8`, with
/// `0` meaning nothing drawn and `255` kept by the decoder as a marker.
pub const MAX_CLASSES: usize = 254;

/// A raster layer's colour map, as the value range each colour stands for.
#[derive(Clone, Debug, PartialEq)]
pub struct ValueLegend {
    /// One class per distinct colour, in the order of each colour's first
    /// entry. Entries that share a colour are merged into one class spanning
    /// all of them: in the terrain legend those are always neighbours, and a
    /// colour shared by entries apart covers the whole span between them,
    /// since a render cannot tell them apart.
    pub classes: Vec<ValueClass>,
}

/// One colour of a [`ValueLegend`] and the values it covers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ValueClass {
    /// The colour, sRGB.
    pub rgb: [u8; 3],
    /// Values above this...
    pub lo: f64,
    /// ...up to this. `lo` is `-inf` for a class open below.
    pub hi: f64,
}

/// A vector layer's polygon fills, one class per fill.
#[derive(Clone, Debug, PartialEq)]
pub struct ClassLegend {
    /// In legend order; a decoded class id `k` is `classes[k - 1]`.
    pub classes: Vec<FillClass>,
}

/// One fill of a [`ClassLegend`].
#[derive(Clone, Debug, PartialEq)]
pub struct FillClass {
    /// The fill colour, sRGB.
    pub rgb: [u8; 3],
    /// The fill's opacity, `0..=1`.
    pub opacity: f32,
    /// The layer the rule belongs to.
    pub layer: String,
    /// The rule's name, if it has one.
    pub name: Option<String>,
    /// The attribute of a `[attr = 'value']` filter.
    pub attribute: Option<String>,
    /// The value of a `[attr = 'value']` filter.
    pub value: Option<String>,
}

/// Why a legend could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LegendError {
    /// Not JSON, or not a legend.
    Json(String),
    /// A legend with no layer, no colour map, or no class in it.
    Empty,
    /// A colour map of a type other than `intervals`.
    ColorMapType(String),
    /// A colour that is not `#rrggbb`.
    Colour(String),
    /// A quantity that is not a number.
    Quantity(String),
    /// Quantities that do not ascend.
    Order,
    /// More fills than [`MAX_CLASSES`].
    TooManyClasses(usize),
}

impl std::fmt::Display for LegendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LegendError::Json(e) => write!(f, "legend is not legend JSON: {e}"),
            LegendError::Empty => write!(f, "legend has no classes"),
            LegendError::ColorMapType(t) => write!(f, "colour map type {t:?} is not intervals"),
            LegendError::Colour(c) => write!(f, "colour {c:?} is not #rrggbb"),
            LegendError::Quantity(q) => write!(f, "quantity {q:?} is not a number"),
            LegendError::Order => write!(f, "colour map quantities do not ascend"),
            LegendError::TooManyClasses(n) => write!(f, "{n} fills, more than {MAX_CLASSES}"),
        }
    }
}

impl std::error::Error for LegendError {}

/// Read a raster layer's `intervals` colour map. The first entry has no
/// lower bound and becomes a class open below; entries drawn at opacity `0`
/// are no class at all (the render leaves them transparent).
pub fn parse_value_legend(body: &[u8]) -> Result<ValueLegend, LegendError> {
    let doc = parse(body)?;
    let map = doc
        .legend
        .iter()
        .flat_map(|layer| &layer.rules)
        .flat_map(|rule| &rule.symbolizers)
        .find_map(|s| s.raster.as_ref())
        .map(|raster| &raster.colormap)
        .ok_or(LegendError::Empty)?;
    let kind = map.kind.as_deref().unwrap_or("ramp");
    if kind != "intervals" {
        return Err(LegendError::ColorMapType(kind.to_owned()));
    }
    let mut classes: Vec<ValueClass> = Vec::new();
    let mut previous = f64::NEG_INFINITY;
    for entry in &map.entries {
        let quantity = number(&entry.quantity)?;
        if quantity < previous {
            return Err(LegendError::Order);
        }
        let opacity = match &entry.opacity {
            Some(o) => number(o)?,
            None => 1.0,
        };
        if opacity > 0.0 {
            let rgb = colour(&entry.color)?;
            match classes.iter_mut().find(|c| c.rgb == rgb) {
                Some(class) => {
                    class.lo = class.lo.min(previous);
                    class.hi = class.hi.max(quantity);
                }
                None => classes.push(ValueClass {
                    rgb,
                    lo: previous,
                    hi: quantity,
                }),
            }
        }
        previous = quantity;
    }
    if classes.is_empty() {
        return Err(LegendError::Empty);
    }
    Ok(ValueLegend { classes })
}

/// Read a vector layer's polygon-fill rules. Rules with no fill (an outline
/// rule, say) are skipped: what they draw decodes as no class.
pub fn parse_class_legend(body: &[u8]) -> Result<ClassLegend, LegendError> {
    let doc = parse(body)?;
    let mut classes = Vec::new();
    for layer in &doc.legend {
        for rule in &layer.rules {
            let Some(polygon) = rule.symbolizers.iter().find_map(|s| s.polygon.as_ref()) else {
                continue;
            };
            let Some(fill) = &polygon.fill else {
                continue;
            };
            let (attribute, value) = rule.filter.as_deref().and_then(equality).unzip();
            classes.push(FillClass {
                rgb: colour(fill)?,
                opacity: match &polygon.fill_opacity {
                    // f32 can overflow a finite f64 to inf; clamp to the range.
                    Some(o) => (number(o)? as f32).clamp(0.0, 1.0),
                    None => 1.0,
                },
                layer: layer.layer_name.clone(),
                name: rule.name.clone(),
                attribute,
                value,
            });
        }
    }
    if classes.is_empty() {
        return Err(LegendError::Empty);
    }
    if classes.len() > MAX_CLASSES {
        return Err(LegendError::TooManyClasses(classes.len()));
    }
    Ok(ClassLegend { classes })
}

impl ClassLegend {
    /// The legends of several layers drawn together, as one: the layers'
    /// classes in the order given, which should be the draw order.
    pub fn concat(legends: impl IntoIterator<Item = ClassLegend>) -> Result<Self, LegendError> {
        let classes: Vec<FillClass> = legends.into_iter().flat_map(|l| l.classes).collect();
        if classes.len() > MAX_CLASSES {
            return Err(LegendError::TooManyClasses(classes.len()));
        }
        Ok(ClassLegend { classes })
    }

    /// The class id (`1..`) of the fill whose filter value is `value`, if it
    /// is among the first [`MAX_CLASSES`] (a decoder refuses a legend with
    /// more).
    pub fn id_of(&self, value: &str) -> Option<u8> {
        let index = self
            .classes
            .iter()
            .take(MAX_CLASSES)
            .position(|c| c.value.as_deref() == Some(value))?;
        Some(index as u8 + 1)
    }
}

/// The body as text: UTF-8 if it is, Latin-1 otherwise (every byte is a
/// Latin-1 character, so this cannot fail).
fn text(body: &[u8]) -> Cow<'_, str> {
    match std::str::from_utf8(body) {
        Ok(s) => Cow::Borrowed(s),
        Err(_) => Cow::Owned(body.iter().map(|&b| char::from(b)).collect()),
    }
}

fn parse(body: &[u8]) -> Result<Doc, LegendError> {
    serde_json::from_str(&text(body)).map_err(|e| LegendError::Json(e.to_string()))
}

fn colour(s: &str) -> Result<[u8; 3], LegendError> {
    // Six hex digits exactly: `from_str_radix` alone would take a sign.
    let hex = s
        .strip_prefix('#')
        .filter(|h| h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit()));
    let channel = |i: usize| hex.and_then(|h| u8::from_str_radix(h.get(i..i + 2)?, 16).ok());
    match (channel(0), channel(2), channel(4)) {
        (Some(r), Some(g), Some(b)) => Ok([r, g, b]),
        _ => Err(LegendError::Colour(s.to_owned())),
    }
}

fn number(v: &serde_json::Value) -> Result<f64, LegendError> {
    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
    .filter(|q: &f64| q.is_finite())
    .ok_or_else(|| LegendError::Quantity(v.to_string()))
}

/// `[attr = 'value']` as `(attr, value)`; anything else as `None`.
fn equality(filter: &str) -> Option<(String, String)> {
    let inner = filter.trim().strip_prefix('[')?.strip_suffix(']')?;
    let (attr, value) = inner.split_once('=')?;
    let value = value.trim().strip_prefix('\'')?.strip_suffix('\'')?;
    Some((attr.trim().to_owned(), value.to_owned()))
}

#[derive(Deserialize)]
struct Doc {
    #[serde(rename = "Legend")]
    legend: Vec<LayerLegend>,
}

#[derive(Deserialize)]
struct LayerLegend {
    #[serde(rename = "layerName", default)]
    layer_name: String,
    #[serde(default)]
    rules: Vec<Rule>,
}

#[derive(Deserialize)]
struct Rule {
    name: Option<String>,
    filter: Option<String>,
    #[serde(default)]
    symbolizers: Vec<Symbolizer>,
}

#[derive(Deserialize)]
struct Symbolizer {
    #[serde(rename = "Raster")]
    raster: Option<RasterSymbolizer>,
    #[serde(rename = "Polygon")]
    polygon: Option<PolygonSymbolizer>,
}

#[derive(Deserialize)]
struct RasterSymbolizer {
    colormap: ColorMap,
}

#[derive(Deserialize)]
struct ColorMap {
    #[serde(default)]
    entries: Vec<Entry>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Deserialize)]
struct Entry {
    quantity: serde_json::Value,
    color: String,
    opacity: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct PolygonSymbolizer {
    fill: Option<String>,
    #[serde(rename = "fill-opacity")]
    fill_opacity: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raster_legend(entries: &str, kind: &str) -> String {
        format!(
            r#"{{"Legend":[{{"layerName":"t","rules":[{{"symbolizers":[{{"Raster":{{"colormap":{{"entries":[{entries}],"type":"{kind}"}}}}}}]}}]}}]}}"#
        )
    }

    #[test]
    fn intervals_become_ranges_and_shared_colours_merge() {
        let body = raster_legend(
            r##"{"quantity":"-7.00000001","color":"#AFF0E9","opacity":"0.0"},
                {"quantity":"25.99999999","color":"#AFF0E0","opacity":"1.0"},
                {"quantity":26.5,"color":"#FFFCFF"},
                {"quantity":"27.5","color":"#fffcff","opacity":"1.0"}"##,
            "intervals",
        );
        let legend = parse_value_legend(body.as_bytes()).unwrap();
        assert_eq!(
            legend.classes,
            vec![
                ValueClass {
                    rgb: [0xAF, 0xF0, 0xE0],
                    lo: -7.00000001,
                    hi: 25.99999999
                },
                ValueClass {
                    rgb: [0xFF, 0xFC, 0xFF],
                    lo: 25.99999999,
                    hi: 27.5
                },
            ]
        );
    }

    #[test]
    fn an_opaque_first_entry_is_open_below() {
        let body = raster_legend(r##"{"quantity":"10","color":"#000000"}"##, "intervals");
        let legend = parse_value_legend(body.as_bytes()).unwrap();
        assert_eq!(legend.classes[0].lo, f64::NEG_INFINITY);
    }

    #[test]
    fn value_legends_refuse_what_they_cannot_decode() {
        let ramp = raster_legend(r##"{"quantity":"1","color":"#000000"}"##, "ramp");
        assert_eq!(
            parse_value_legend(ramp.as_bytes()),
            Err(LegendError::ColorMapType("ramp".into()))
        );
        let down = raster_legend(
            r##"{"quantity":"2","color":"#000000"},{"quantity":"1","color":"#111111"}"##,
            "intervals",
        );
        assert_eq!(parse_value_legend(down.as_bytes()), Err(LegendError::Order));
        let bad = raster_legend(r##"{"quantity":"x","color":"#000000"}"##, "intervals");
        assert!(matches!(
            parse_value_legend(bad.as_bytes()),
            Err(LegendError::Quantity(_))
        ));
        let colour = raster_legend(r##"{"quantity":"1","color":"red"}"##, "intervals");
        assert!(matches!(
            parse_value_legend(colour.as_bytes()),
            Err(LegendError::Colour(_))
        ));
        assert!(matches!(
            parse_value_legend(b"not json"),
            Err(LegendError::Json(_))
        ));
        assert_eq!(
            parse_value_legend(br#"{"Legend":[]}"#),
            Err(LegendError::Empty)
        );
    }

    #[test]
    fn fills_become_classes_named_by_their_filter() {
        let body = br##"{"Legend":[{"layerName":"nutz","rules":[
            {"name":"100","filter":"[grz = '100']","symbolizers":[{"Polygon":{"fill":"#006500","fill-opacity":"1.0"}}]},
            {"filter":"[woz = '10']","symbolizers":[{"Polygon":{"fill":"#FFCC65","fill-opacity":"0.5"}}]},
            {"name":"Umrandung","symbolizers":[{"Polygon":{"stroke":"#ababab"}}]}
        ]}]}"##;
        let legend = parse_class_legend(body).unwrap();
        assert_eq!(legend.classes.len(), 2, "the outline rule is no class");
        let forest = &legend.classes[0];
        assert_eq!(forest.rgb, [0x00, 0x65, 0x00]);
        assert_eq!(forest.layer, "nutz");
        assert_eq!(forest.name.as_deref(), Some("100"));
        assert_eq!(forest.attribute.as_deref(), Some("grz"));
        assert_eq!(forest.value.as_deref(), Some("100"));
        assert_eq!(legend.classes[1].opacity, 0.5);
        assert_eq!(legend.id_of("10"), Some(2));
        assert_eq!(legend.id_of("11"), None);
    }

    #[test]
    fn latin1_bodies_are_read() {
        // "Geländemodell" in Latin-1: 0xE4 is not UTF-8 on its own.
        let mut body = br#"{"Legend":[{"layerName":"x","title":"Gel"#.to_vec();
        body.push(0xE4);
        body.extend_from_slice(
            br##"ndemodell","rules":[{"symbolizers":[{"Polygon":{"fill":"#010203"}}]}]}]}"##,
        );
        assert!(std::str::from_utf8(&body).is_err());
        let legend = parse_class_legend(&body).unwrap();
        assert_eq!(legend.classes[0].rgb, [1, 2, 3]);
    }

    #[test]
    fn concat_keeps_draw_order_and_the_class_cap() {
        let one = |fill: &str, value: &str| ClassLegend {
            classes: vec![FillClass {
                rgb: colour(fill).unwrap(),
                opacity: 1.0,
                layer: value.into(),
                name: None,
                attribute: Some("k".into()),
                value: Some(value.into()),
            }],
        };
        let both = ClassLegend::concat([one("#000001", "a"), one("#000002", "b")]).unwrap();
        assert_eq!(both.id_of("a"), Some(1));
        assert_eq!(both.id_of("b"), Some(2));
        let many = (0..=MAX_CLASSES).map(|i| one("#000000", &i.to_string()));
        assert_eq!(
            ClassLegend::concat(many),
            Err(LegendError::TooManyClasses(MAX_CLASSES + 1))
        );
    }

    #[test]
    fn filters_parse_only_as_equalities() {
        assert_eq!(
            equality("[geschoss_kategorie = 'Geschoss_gr_10']"),
            Some(("geschoss_kategorie".into(), "Geschoss_gr_10".into()))
        );
        assert_eq!(equality("[a > '1']"), None);
        assert_eq!(equality("a = '1'"), None);
    }

    #[test]
    fn colours_are_six_hex_digits_and_nothing_else() {
        assert_eq!(colour("#0aFf09"), Ok([0x0A, 0xFF, 0x09]));
        for bad in [
            "#+1+2+3", "# 1 2 3", "0AFF09", "#0AFF0", "#0AFF09A", "#0AFG09",
        ] {
            assert_eq!(colour(bad), Err(LegendError::Colour(bad.into())), "{bad}");
        }
    }

    #[test]
    fn fill_opacity_is_clamped_into_its_range() {
        let legend = |opacity: &str| {
            let body = format!(
                r##"{{"Legend":[{{"rules":[{{"symbolizers":[{{"Polygon":{{"fill":"#010203","fill-opacity":"{opacity}"}}}}]}}]}}]}}"##
            );
            parse_class_legend(body.as_bytes()).unwrap().classes[0].opacity
        };
        assert_eq!(legend("1e300"), 1.0);
        assert_eq!(legend("-2"), 0.0);
        assert_eq!(legend("0.5"), 0.5);
    }

    #[test]
    fn id_of_stops_at_the_class_cap() {
        let fill = |value: String| FillClass {
            rgb: [0, 0, 0],
            opacity: 1.0,
            layer: "l".into(),
            name: None,
            attribute: Some("k".into()),
            value: Some(value),
        };
        let legend = ClassLegend {
            classes: (0..=MAX_CLASSES + 1).map(|k| fill(k.to_string())).collect(),
        };
        assert_eq!(legend.id_of("0"), Some(1));
        assert_eq!(legend.id_of(&(MAX_CLASSES - 1).to_string()), Some(254));
        assert_eq!(legend.id_of(&MAX_CLASSES.to_string()), None);
        assert_eq!(legend.id_of(&(MAX_CLASSES + 1).to_string()), None);
    }
}
