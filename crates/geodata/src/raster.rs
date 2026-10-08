//! Decoding WMS renders back into data.
//!
//! A render is a PNG whose every pixel is one legend colour, transparent, or
//! (for a vector layer) an outline. [`decode_terrain`] turns the terrain
//! layer's colour classes into heights; [`decode_classes`] turns a
//! categorical layer's fills into class ids. Both are pure and use only
//! IEEE-exact arithmetic in a fixed order, so every peer decoding the same
//! bytes gets the same bits.

use crate::legend::{ClassLegend, MAX_CLASSES, ValueLegend};

/// The most pixels a render may have: 2048 x 2048. Decoding one holds about
/// 8 bytes a pixel at its peak (the decoder's buffer beside the RGBA
/// pixels: 32 MiB at the cap), and decoding terrain from it about 20 more.
/// Wasm memory never shrinks, so ask for no more pixels than will be drawn.
pub const MAX_PIXELS: u64 = 2048 * 2048;

/// The most the PNG decoder may allocate beyond the pixels: its row buffers
/// and any ancillary chunks (text, ICC profile), whose compressed contents
/// could otherwise inflate to its 64 MiB default.
const PNG_ALLOCATION_LIMIT: usize = 8 << 20;

/// Passes of clamped smoothing [`decode_terrain`] runs. Measured at 4 m
/// pixels against the raw 1 m model (#1581, `tests/decode.rs`): 20 passes
/// take the mean error from 0.42 m (class midpoints) to 0.26 m, and false
/// one-metre steps on flat ground from 6 % of neighbouring pairs to 0.7 %;
/// more passes start to flatten real slopes.
pub const DEQUANTISE_PASSES: u32 = 20;

/// A legend class wider than this many typical class widths is treated as
/// open-ended: its values are taken to lie within one typical width of the
/// bound it shares with its neighbours. The terrain legend's lowest class
/// spans -7 m to 26 m, where Berlin's ground never is.
pub const OPEN_CLASS_FACTOR: f64 = 4.0;

/// How far, per channel, a pixel may be from a fill colour and still be that
/// fill. Half-transparent fills come back within one or two.
pub const MATCH_TOLERANCE: u8 = 4;

/// Passes of neighbour voting that [`decode_classes`] spends on pixels whose
/// colour is no fill (outlines, blends).
pub const FILL_PASSES: u32 = 3;

/// The class id of a pixel where nothing is drawn.
pub const CLASS_NONE: u8 = 0;

/// The marker for a pixel not yet resolved. Never in a returned grid.
const UNKNOWN: u8 = u8::MAX;

/// An 8-bit RGBA image, row 0 at the north edge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba8 {
    /// Columns.
    pub width: u32,
    /// Rows.
    pub height: u32,
    /// Row-major pixels, unpremultiplied.
    pub pixels: Vec<[u8; 4]>,
}

/// Heights decoded from a terrain render, metres above sea level (DHHN2016).
#[derive(Clone, Debug, PartialEq)]
pub struct HeightGrid {
    /// Columns.
    pub width: u32,
    /// Rows.
    pub height: u32,
    /// Row-major heights, row 0 at the north edge.
    pub heights: Vec<f32>,
    /// Pixels the render left transparent, filled from their neighbours.
    pub transparent: u32,
    /// Pixels of a colour the legend does not name, filled from their
    /// neighbours. More than a few means the legend is not this render's.
    pub unmatched: u32,
}

impl HeightGrid {
    /// The height at `(col, row)`.
    ///
    /// # Panics
    ///
    /// Outside the grid, like any index.
    pub fn at(&self, col: u32, row: u32) -> f32 {
        assert!(col < self.width, "column {col} of {}", self.width);
        self.heights[row as usize * self.width as usize + col as usize]
    }
}

/// Class ids decoded from a categorical render.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassGrid {
    /// Columns.
    pub width: u32,
    /// Rows.
    pub height: u32,
    /// Row-major class ids, row 0 at the north edge: [`CLASS_NONE`] where
    /// nothing is drawn, else `k` for the legend's `classes[k - 1]`.
    pub classes: Vec<u8>,
    /// Drawn pixels whose colour was no fill (outlines, blends), before
    /// they were resolved from their neighbours.
    pub unmatched: u32,
}

impl ClassGrid {
    /// The class id at `(col, row)`.
    ///
    /// # Panics
    ///
    /// Outside the grid, like any index.
    pub fn at(&self, col: u32, row: u32) -> u8 {
        assert!(col < self.width, "column {col} of {}", self.width);
        self.classes[row as usize * self.width as usize + col as usize]
    }
}

/// Why a render could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Not a PNG the decoder can read.
    Png(String),
    /// A requested size that is empty or more than [`MAX_PIXELS`].
    Size {
        /// Columns.
        width: u32,
        /// Rows.
        height: u32,
    },
    /// A PNG of another size than the one requested: an error page, or a
    /// body that is not the render asked for.
    Mismatch {
        /// The requested `(width, height)`.
        expected: (u32, u32),
        /// What the PNG's header says.
        got: (u32, u32),
    },
    /// An image or legend whose parts disagree - pixels that do not fill
    /// `width x height`, more fills than class ids. Only one built by hand
    /// can be; the decoders and parsers here never make one.
    Malformed(&'static str),
    /// Not one pixel decoded to data.
    NoData,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Png(e) => write!(f, "not a readable PNG: {e}"),
            DecodeError::Size { width, height } => {
                write!(f, "a {width} x {height} render is empty or too large")
            }
            DecodeError::Mismatch { expected, got } => write!(
                f,
                "asked for a {} x {} render, got {} x {}",
                expected.0, expected.1, got.0, got.1
            ),
            DecodeError::Malformed(what) => write!(f, "malformed input: {what}"),
            DecodeError::NoData => write!(f, "no pixel of the render decoded to data"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Decode a PNG of any colour type to 8-bit RGBA. Palette images (what the
/// terrain layer serves) are expanded through their palette and `tRNS`.
///
/// The PNG must be `width x height`, the size its request asked for: any
/// other size is refused from the header, before a pixel is allocated, so a
/// few kilobytes claiming millions of pixels cost nothing.
pub fn decode_png(bytes: &[u8], width: u32, height: u32) -> Result<Rgba8, DecodeError> {
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(DecodeError::Size { width, height });
    }
    let png_error = |e: png::DecodingError| DecodeError::Png(e.to_string());
    let limits = png::Limits {
        bytes: PNG_ALLOCATION_LIMIT,
    };
    let mut decoder = png::Decoder::new_with_limits(std::io::Cursor::new(bytes), limits);
    decoder.set_transformations(
        png::Transformations::normalize_to_color8() | png::Transformations::ALPHA,
    );
    let mut reader = decoder.read_info().map_err(png_error)?;
    let got = reader.info().size();
    if got != (width, height) {
        return Err(DecodeError::Mismatch {
            expected: (width, height),
            got,
        });
    }
    let size = reader
        .output_buffer_size()
        .ok_or(DecodeError::Size { width, height })?;
    let mut buffer = vec![0; size];
    let frame = reader.next_frame(&mut buffer).map_err(png_error)?;
    let channels = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => {
            return Err(DecodeError::Png("palette was not expanded".into()));
        }
    };
    let mut pixels = Vec::with_capacity(width as usize * height as usize);
    for line in buffer.chunks_exact(frame.line_size).take(height as usize) {
        for p in line[..width as usize * channels].chunks_exact(channels) {
            pixels.push(match *p {
                [v] => [v, v, v, 255],
                [v, a] => [v, v, v, a],
                [r, g, b] => [r, g, b, 255],
                [r, g, b, a] => [r, g, b, a],
                _ => unreachable!("chunks_exact yields {channels} samples"),
            });
        }
    }
    Ok(Rgba8 {
        width,
        height,
        pixels,
    })
}

/// Heights from a render of a terrain layer drawn with `legend`.
///
/// Each pixel's colour names a class, and the class a height range. Taking
/// every pixel at its class midpoint leaves one-metre terraces on gentle
/// slopes, so the decode then runs [`DEQUANTISE_PASSES`] of smoothing in
/// which every pixel is pulled to its neighbours' mean but never out of its
/// own class: a smooth surface that the render would still draw the same.
/// Pixels with no class (transparent, or a colour the legend lacks) are
/// filled from their neighbours first and smoothed freely.
pub fn decode_terrain(image: &Rgba8, legend: &ValueLegend) -> Result<HeightGrid, DecodeError> {
    check_pixels(image)?;
    // Only classes with finite, ordered bounds in f32 are looked up; a pixel
    // of any other is unmatched and filled, so no height is ever infinite
    // and no clamp sees its bounds reversed.
    let mut lookup: Vec<(u32, f32, f32)> = bounded_classes(legend)
        .into_iter()
        .map(|(rgb, lo, hi)| (pack(rgb), lo as f32, hi as f32))
        .filter(|&(_, lo, hi)| lo.is_finite() && hi.is_finite() && lo <= hi)
        .collect();
    lookup.sort_by_key(|&(key, ..)| key);

    let count = image.pixels.len();
    let mut lo = vec![f32::NEG_INFINITY; count];
    let mut hi = vec![f32::INFINITY; count];
    let mut known = vec![false; count];
    let (mut transparent, mut unmatched) = (0, 0);
    for (i, &[r, g, b, a]) in image.pixels.iter().enumerate() {
        if a == 0 {
            transparent += 1;
            continue;
        }
        match lookup.binary_search_by_key(&pack([r, g, b]), |&(key, ..)| key) {
            Ok(at) => {
                (lo[i], hi[i]) = (lookup[at].1, lookup[at].2);
                known[i] = true;
            }
            Err(_) => unmatched += 1,
        }
    }
    if !known.contains(&true) {
        return Err(DecodeError::NoData);
    }

    let (width, height) = (image.width as usize, image.height as usize);
    let mut heights: Vec<f32> = (0..count)
        .map(|i| if known[i] { (lo[i] + hi[i]) / 2.0 } else { 0.0 })
        .collect();
    fill_from_neighbours(&mut heights, &mut known, width, height);
    for _ in 0..DEQUANTISE_PASSES {
        heights = smooth_within(&heights, &lo, &hi, width, height);
    }
    Ok(HeightGrid {
        width: image.width,
        height: image.height,
        heights,
        transparent,
        unmatched,
    })
}

/// Class ids from a render of a categorical layer drawn with `legend`.
///
/// A transparent pixel is [`CLASS_NONE`]; a pixel within
/// [`MATCH_TOLERANCE`] of a fill is that fill's class (comparing colour
/// only, so half-transparent fills match too). Anything else - an outline,
/// two fills blended - takes the most common class among its eight
/// neighbours, over [`FILL_PASSES`] passes (ties to the lower id); a pixel
/// still unresolved after that is [`CLASS_NONE`].
pub fn decode_classes(image: &Rgba8, legend: &ClassLegend) -> Result<ClassGrid, DecodeError> {
    check_pixels(image)?;
    if legend.classes.len() > MAX_CLASSES {
        return Err(DecodeError::Malformed("more fills than class ids"));
    }
    let fills: Vec<[u8; 3]> = legend.classes.iter().map(|c| c.rgb).collect();
    let mut classes: Vec<u8> = image
        .pixels
        .iter()
        .map(|&[r, g, b, a]| {
            if a == 0 {
                return CLASS_NONE;
            }
            nearest_fill(&fills, [r, g, b]).map_or(UNKNOWN, |k| k as u8 + 1)
        })
        .collect();
    let unmatched = classes.iter().filter(|&&c| c == UNKNOWN).count() as u32;

    let (width, height) = (image.width as usize, image.height as usize);
    for _ in 0..FILL_PASSES {
        if !classes.contains(&UNKNOWN) {
            break;
        }
        classes = vote_unknown(&classes, width, height);
    }
    for class in &mut classes {
        if *class == UNKNOWN {
            *class = CLASS_NONE;
        }
    }
    Ok(ClassGrid {
        width: image.width,
        height: image.height,
        classes,
        unmatched,
    })
}

/// Refuse an image whose pixels do not fill `width x height` exactly.
fn check_pixels(image: &Rgba8) -> Result<(), DecodeError> {
    let area = u64::from(image.width) * u64::from(image.height);
    if area == 0 || image.pixels.len() as u64 != area {
        return Err(DecodeError::Malformed("pixels do not fill width x height"));
    }
    Ok(())
}

/// The legend's classes with open-ended ones bounded (see
/// [`OPEN_CLASS_FACTOR`]): a class open below (the colour map's first
/// entry) or above is given one typical width; a finite class far wider
/// than typical is narrowed toward its neighbours - from below in the lower
/// half of the legend, from above in the upper.
fn bounded_classes(legend: &ValueLegend) -> Vec<([u8; 3], f64, f64)> {
    let mut widths: Vec<f64> = legend
        .classes
        .iter()
        .map(|c| c.hi - c.lo)
        .filter(|w| w.is_finite() && *w > 0.0)
        .collect();
    widths.sort_by(f64::total_cmp);
    let typical = widths.get(widths.len() / 2).copied().unwrap_or(1.0);
    let half = legend.classes.len() / 2;
    legend
        .classes
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let (mut lo, mut hi) = (c.lo, c.hi);
            if lo == f64::NEG_INFINITY {
                lo = hi - typical;
            } else if hi == f64::INFINITY {
                hi = lo + typical;
            } else if hi - lo > OPEN_CLASS_FACTOR * typical {
                if i < half {
                    lo = hi - typical;
                } else {
                    hi = lo + typical;
                }
            }
            (c.rgb, lo, hi)
        })
        .collect()
}

fn pack([r, g, b]: [u8; 3]) -> u32 {
    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

/// Give every pixel without data the mean of its known 4-neighbours, ring
/// by ring inwards from the known ones. Each ring reads only pixels known
/// before it, so the result does not depend on scan order.
fn fill_from_neighbours(values: &mut [f32], known: &mut [bool], width: usize, height: usize) {
    let mut ring: Vec<usize> = (0..values.len())
        .filter(|&i| !known[i] && neighbours4(i, width, height).any(|j| known[j]))
        .collect();
    let mut queued = known.to_vec();
    for &i in &ring {
        queued[i] = true;
    }
    while !ring.is_empty() {
        let filled: Vec<f32> = ring
            .iter()
            .map(|&i| {
                let (sum, n) = neighbours4(i, width, height)
                    .filter(|&j| known[j])
                    .fold((0.0f32, 0u32), |(s, n), j| (s + values[j], n + 1));
                sum / n as f32
            })
            .collect();
        for (&i, v) in ring.iter().zip(filled) {
            values[i] = v;
            known[i] = true;
        }
        let mut next = Vec::new();
        for &i in &ring {
            for j in neighbours4(i, width, height) {
                if !queued[j] {
                    queued[j] = true;
                    next.push(j);
                }
            }
        }
        next.sort_unstable();
        ring = next;
    }
}

/// One pass: each pixel becomes the mean of its 4-neighbours (an edge
/// pixel's missing neighbour counts as itself), clamped into `[lo, hi]`.
fn smooth_within(values: &[f32], lo: &[f32], hi: &[f32], width: usize, height: usize) -> Vec<f32> {
    let at = |x: usize, y: usize| values[y * width + x];
    let mut out = vec![0.0; values.len()];
    for y in 0..height {
        for x in 0..width {
            let i = y * width + x;
            let here = values[i];
            let west = if x > 0 { at(x - 1, y) } else { here };
            let east = if x + 1 < width { at(x + 1, y) } else { here };
            let north = if y > 0 { at(x, y - 1) } else { here };
            let south = if y + 1 < height { at(x, y + 1) } else { here };
            out[i] = ((west + east + north + south) / 4.0).clamp(lo[i], hi[i]);
        }
    }
    out
}

fn neighbours4(i: usize, width: usize, height: usize) -> impl Iterator<Item = usize> {
    let (x, y) = (i % width, i / width);
    [
        (x > 0).then(|| i - 1),
        (x + 1 < width).then(|| i + 1),
        (y > 0).then(|| i - width),
        (y + 1 < height).then(|| i + width),
    ]
    .into_iter()
    .flatten()
}

/// The index of the fill nearest `rgb`, if one is within
/// [`MATCH_TOLERANCE`] on every channel (ties to the earlier fill).
fn nearest_fill(fills: &[[u8; 3]], rgb: [u8; 3]) -> Option<usize> {
    fills
        .iter()
        .enumerate()
        .filter(|(_, f)| (0..3).all(|c| f[c].abs_diff(rgb[c]) <= MATCH_TOLERANCE))
        .min_by_key(|(_, f)| {
            (0..3)
                .map(|c| u32::from(f[c].abs_diff(rgb[c])).pow(2))
                .sum::<u32>()
        })
        .map(|(k, _)| k)
}

/// One pass: every unknown pixel with a known 8-neighbour takes the most
/// common known value around it (ties to the lowest).
fn vote_unknown(classes: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut out = classes.to_vec();
    for y in 0..height {
        for x in 0..width {
            let i = y * width + x;
            if classes[i] != UNKNOWN {
                continue;
            }
            let mut tally: [(u8, u8); 8] = [(0, 0); 8];
            let mut seen = 0;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if (dx, dy) == (0, 0) || nx < 0 || ny < 0 {
                        continue;
                    }
                    let (nx, ny) = (nx as usize, ny as usize);
                    if nx >= width || ny >= height {
                        continue;
                    }
                    let class = classes[ny * width + nx];
                    if class == UNKNOWN {
                        continue;
                    }
                    match tally[..seen].iter_mut().find(|(c, _)| *c == class) {
                        Some((_, n)) => *n += 1,
                        None => {
                            tally[seen] = (class, 1);
                            seen += 1;
                        }
                    }
                }
            }
            if let Some(&(class, _)) = tally[..seen]
                .iter()
                .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
            {
                out[i] = class;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::legend::{FillClass, ValueClass};

    fn image(width: u32, height: u32, pixels: Vec<[u8; 4]>) -> Rgba8 {
        Rgba8 {
            width,
            height,
            pixels,
        }
    }

    fn class(rgb: [u8; 3], lo: f64, hi: f64) -> ValueClass {
        ValueClass { rgb, lo, hi }
    }

    #[test]
    fn terrain_classes_decode_and_stay_inside_their_range() {
        // A ramp of four one-metre classes across 8 columns.
        let legend = ValueLegend {
            classes: (0..4)
                .map(|k| class([k as u8, 0, 0], 30.0 + k as f64, 31.0 + k as f64))
                .collect(),
        };
        let row: Vec<[u8; 4]> = (0..8).map(|x| [(x / 2) as u8, 0, 0, 255]).collect();
        let grid = decode_terrain(&image(8, 3, row.repeat(3)), &legend).unwrap();
        for y in 0..3 {
            for x in 0..8 {
                let k = (x / 2) as f32;
                let h = grid.at(x, y);
                assert!((30.0 + k..=31.0 + k).contains(&h), "{h} at {x}");
            }
        }
        // Smoothing turns the staircase into a ramp: no flat pair of columns.
        assert!(grid.at(1, 1) > grid.at(0, 1));
        assert!(grid.at(2, 1) > grid.at(1, 1));
        assert_eq!((grid.transparent, grid.unmatched), (0, 0));
    }

    #[test]
    fn terrain_holes_fill_from_their_neighbours() {
        let legend = ValueLegend {
            classes: vec![class([9, 9, 9], 40.0, 41.0)],
        };
        let mut pixels = vec![[9, 9, 9, 255]; 25];
        pixels[12] = [0, 0, 0, 0];
        pixels[13] = [1, 2, 3, 255];
        let grid = decode_terrain(&image(5, 5, pixels), &legend).unwrap();
        assert_eq!((grid.transparent, grid.unmatched), (1, 1));
        assert!((grid.at(2, 2) - 40.5).abs() < 1e-4);
        assert!((grid.at(3, 2) - 40.5).abs() < 1e-4);
    }

    #[test]
    fn a_render_with_nothing_on_it_is_no_data() {
        let legend = ValueLegend {
            classes: vec![class([9, 9, 9], 40.0, 41.0)],
        };
        let empty = image(2, 2, vec![[0, 0, 0, 0]; 4]);
        assert_eq!(decode_terrain(&empty, &legend), Err(DecodeError::NoData));
    }

    #[test]
    fn open_ended_classes_are_bounded_toward_their_neighbours() {
        let mut classes: Vec<ValueClass> = (0..9)
            .map(|k| class([k, 0, 0], 26.0 + f64::from(k), 27.0 + f64::from(k)))
            .collect();
        classes.insert(0, class([99, 0, 0], -7.0, 26.0));
        classes.push(class([98, 0, 0], 35.0, 90.0));
        classes.push(class([97, 0, 0], 90.0, 93.0));
        let bounded = bounded_classes(&ValueLegend { classes });
        assert_eq!(bounded[0], ([99, 0, 0], 25.0, 26.0));
        assert_eq!(bounded[10], ([98, 0, 0], 35.0, 36.0));
        assert_eq!(bounded[11], ([97, 0, 0], 90.0, 93.0), "3 m is not open");
    }

    fn fill(rgb: [u8; 3], value: &str) -> FillClass {
        FillClass {
            rgb,
            opacity: 1.0,
            layer: "l".into(),
            name: None,
            attribute: Some("k".into()),
            value: Some(value.into()),
        }
    }

    #[test]
    fn classes_match_fills_and_vote_away_outlines() {
        let legend = ClassLegend {
            classes: vec![fill([200, 0, 0], "red"), fill([0, 0, 200], "blue")],
        };
        let (r, b, grey, none) = (
            [202, 1, 0, 128],
            [0, 0, 200, 255],
            [127, 127, 127, 255],
            [0, 0, 0, 0],
        );
        // red | outline | blue, over a transparent last row
        let pixels = vec![
            r, r, grey, b, b, //
            r, r, grey, b, b, //
            r, r, grey, b, b, //
            none, none, none, none, none,
        ];
        let grid = decode_classes(&image(5, 4, pixels), &legend).unwrap();
        assert_eq!(grid.unmatched, 3);
        assert_eq!(grid.at(0, 0), 1);
        assert_eq!(grid.at(4, 2), 2);
        assert_eq!(grid.at(2, 3), CLASS_NONE);
        // The top of the outline sees two red and two blue: ties go to the
        // lower id. Its bottom sees three transparent pixels, which outvote
        // two of each fill.
        assert_eq!(grid.at(2, 0), 1);
        assert_eq!(grid.at(2, 2), CLASS_NONE);
        assert!(!grid.classes.contains(&UNKNOWN));
    }

    #[test]
    fn an_unresolvable_pixel_is_none() {
        let legend = ClassLegend {
            classes: vec![fill([200, 0, 0], "red")],
        };
        let grid = decode_classes(&image(1, 1, vec![[1, 2, 3, 255]]), &legend).unwrap();
        assert_eq!((grid.at(0, 0), grid.unmatched), (CLASS_NONE, 1));
    }

    fn encode(
        width: u32,
        color: png::ColorType,
        data: &[u8],
        palette: Option<(&[u8], &[u8])>,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, 1);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        if let Some((plte, trns)) = palette {
            encoder.set_palette(plte.to_vec());
            encoder.set_trns(trns.to_vec());
        }
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(data).unwrap();
        writer.finish().unwrap();
        out
    }

    #[test]
    fn png_decodes_every_colour_type_to_rgba() {
        let rgba = |bytes: Vec<u8>| decode_png(&bytes, 2, 1).unwrap().pixels;
        assert_eq!(
            rgba(encode(2, png::ColorType::Grayscale, &[7, 9], None)),
            vec![[7, 7, 7, 255], [9, 9, 9, 255]]
        );
        assert_eq!(
            rgba(encode(2, png::ColorType::Rgb, &[1, 2, 3, 4, 5, 6], None)),
            vec![[1, 2, 3, 255], [4, 5, 6, 255]]
        );
        assert_eq!(
            rgba(encode(
                2,
                png::ColorType::Indexed,
                &[1, 0],
                Some((&[10, 20, 30, 40, 50, 60], &[0, 255]))
            )),
            vec![[40, 50, 60, 255], [10, 20, 30, 0]]
        );
        assert!(matches!(
            decode_png(b"not a png", 2, 1),
            Err(DecodeError::Png(_))
        ));
    }

    #[test]
    fn pngs_of_another_size_than_asked_are_refused() {
        let bytes = encode(2, png::ColorType::Grayscale, &[7, 9], None);
        assert_eq!(
            decode_png(&bytes, 1, 2),
            Err(DecodeError::Mismatch {
                expected: (1, 2),
                got: (2, 1)
            })
        );
        assert_eq!(
            decode_png(&bytes, 0, 1),
            Err(DecodeError::Size {
                width: 0,
                height: 1
            })
        );
        assert_eq!(
            decode_png(&bytes, 2049, 2048),
            Err(DecodeError::Size {
                width: 2049,
                height: 2048
            })
        );
    }

    #[test]
    fn a_legend_of_one_class_decodes_finite_heights() {
        // A colour map with one opaque entry is one class open below; it
        // takes the fallback typical width rather than reaching -inf.
        let body = br##"{"Legend":[{"rules":[{"symbolizers":[{"Raster":{"colormap":
            {"type":"intervals","entries":[{"quantity":"10","color":"#090909"}]}}}]}]}]}"##;
        let legend = crate::legend::parse_value_legend(body).unwrap();
        let grid = decode_terrain(&image(2, 1, vec![[9, 9, 9, 255]; 2]), &legend).unwrap();
        assert_eq!(grid.heights, vec![9.5, 9.5]);
    }

    #[test]
    fn classes_without_finite_ordered_bounds_are_never_looked_up() {
        // Hand-built: reversed bounds would make the clamp panic, and bounds
        // past f32 would make heights infinite. Their pixels are unmatched
        // and filled from the good class beside them.
        let legend = ValueLegend {
            classes: vec![
                class([1, 1, 1], 30.0, 31.0),
                class([2, 2, 2], 5.0, 4.0),
                class([3, 3, 3], 1e300, 1e300),
            ],
        };
        let pixels = vec![[1, 1, 1, 255], [2, 2, 2, 255], [3, 3, 3, 255]];
        let grid = decode_terrain(&image(3, 1, pixels), &legend).unwrap();
        assert_eq!(grid.unmatched, 2);
        assert!(grid.heights.iter().all(|h| (30.0..=31.0).contains(h)));
    }

    #[test]
    fn images_that_do_not_fill_their_size_are_refused() {
        let terrain = ValueLegend {
            classes: vec![class([9, 9, 9], 40.0, 41.0)],
        };
        let fills = ClassLegend {
            classes: vec![fill([9, 9, 9], "a")],
        };
        let malformed = Err(DecodeError::Malformed("pixels do not fill width x height"));
        for bad in [
            image(3, 3, vec![[9, 9, 9, 255]; 4]),
            image(0, 4, vec![[9, 9, 9, 255]; 4]),
            image(2, 2, vec![]),
        ] {
            assert_eq!(decode_terrain(&bad, &terrain).map(|_| ()), malformed);
            assert_eq!(decode_classes(&bad, &fills).map(|_| ()), malformed);
        }
    }

    #[test]
    fn more_fills_than_class_ids_are_refused() {
        let legend = ClassLegend {
            classes: (0..=MAX_CLASSES)
                .map(|k| fill([k as u8, 0, 0], "x"))
                .collect(),
        };
        assert_eq!(
            decode_classes(&image(1, 1, vec![[0, 0, 0, 255]]), &legend),
            Err(DecodeError::Malformed("more fills than class ids"))
        );
    }

    #[test]
    #[should_panic(expected = "column 2 of 2")]
    fn a_column_past_the_edge_does_not_wrap_into_the_next_row() {
        let legend = ClassLegend {
            classes: vec![fill([9, 9, 9], "a")],
        };
        let grid = decode_classes(&image(2, 2, vec![[9, 9, 9, 255]; 4]), &legend).unwrap();
        grid.at(2, 0);
    }
}
