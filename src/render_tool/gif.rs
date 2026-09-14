//! GIF output for the tool's clips, and the PNG frame dump beside it.
//!
//! One global palette for the whole clip rather than a palette per frame:
//! a per-frame fit re-decides every colour every frame, and the resulting
//! shimmer across a still sky is the thing people recognise as "a GIF". The
//! palette is a NeuQuant fit over a pixel sample drawn evenly from every
//! frame, then each frame is mapped through it with an ordered (Bayer)
//! dither - ordered, not error-diffused, because a pattern fixed to the
//! pixel grid is identical frame to frame where the picture is, which keeps
//! the frame differencing honest: a pixel whose index did not change since
//! the previous frame is written as the transparent slot, and the decoder
//! keeps what it had. A clip with a still camera and one moving body then
//! costs what the body costs.
//!
//! The lookup from a dithered colour to its palette index goes through a
//! 6-bit-per-channel table filled on demand, so a 900 × 500 × 60 clip
//! maps in well under a second instead of running the network search per
//! pixel.

use std::borrow::Cow;
use std::fs::File;
use std::io::BufWriter;

use color_quant::NeuQuant;

/// The palette slot every diff frame uses for "unchanged". Reserved: the
/// quantiser fits one colour fewer than the table holds.
const TRANSPARENT: u8 = 255;
/// Colours the palette fit is asked for - the table minus the reserved slot.
const COLOURS: usize = 255;
/// NeuQuant's sampling factor: 1 is the slowest and best, 30 the fastest.
const SAMPLE_FACTOR: i32 = 5;
/// Pixels the palette fit sees at most, drawn with an even stride across the
/// clip. Enough that a colour on a few dozen pixels of one frame still
/// registers; small enough that fitting is a fraction of the encode.
const FIT_SAMPLE_PIXELS: usize = 2_000_000;
/// The default ordered-dither amplitude (`--dither`), in 8-bit steps: the
/// peak-to-peak spread added to a colour before it is looked up. Six smooths
/// a sky gradient on a 255-colour table without much grain; every step above
/// it costs LZW, because a dither pattern is exactly the high-frequency
/// detail the codec cannot fold. Zero is a plain nearest-colour map.
pub(super) const DEFAULT_DITHER: f32 = 6.0;
/// The 8 × 8 Bayer threshold matrix, values 0..64.
const BAYER: [[u8; 8]; 8] = [
    [0, 32, 8, 40, 2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
];

/// A fitted palette plus the lookup table that maps a colour into it.
struct Quantizer {
    /// Ordered-dither amplitude, see [`DEFAULT_DITHER`].
    dither: f32,
    fit: NeuQuant,
    /// 256 × RGB: the fitted colours, then black in the reserved slot.
    palette: Vec<u8>,
    /// 64³ cells keyed by the top six bits of each channel; a cell holds the
    /// palette index of its centre colour, or [`TRANSPARENT`] until asked.
    lut: Vec<u8>,
}

impl Quantizer {
    /// Fit a palette to a sample of every frame's pixels.
    fn fit(frames: &[Vec<u8>], dither: f32) -> Self {
        let total: usize = frames.iter().map(|f| f.len() / 4).sum();
        let stride = (total / FIT_SAMPLE_PIXELS).max(1);
        let mut sample = Vec::with_capacity((total / stride + 1) * 4);
        for frame in frames {
            for px in frame.chunks_exact(4).step_by(stride) {
                sample.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
        }
        let fit = NeuQuant::new(SAMPLE_FACTOR, COLOURS, &sample);
        let mut palette = fit.color_map_rgb();
        palette.resize(256 * 3, 0);
        Self {
            dither,
            fit,
            palette,
            lut: vec![TRANSPARENT; 64 * 64 * 64],
        }
    }

    /// Palette index for a colour, through the cell table.
    #[inline]
    fn index(&mut self, r: u8, g: u8, b: u8) -> u8 {
        let key = ((r as usize >> 2) << 12) | ((g as usize >> 2) << 6) | (b as usize >> 2);
        let cached = self.lut[key];
        if cached != TRANSPARENT {
            return cached;
        }
        // The cell's centre, so every colour in it maps to one index.
        let centre = [(r & !3) + 2, (g & !3) + 2, (b & !3) + 2, 255];
        let idx = self.fit.index_of(&centre).min(COLOURS - 1) as u8;
        self.lut[key] = idx;
        idx
    }

    /// Map one RGBA frame to palette indices, dithered.
    fn quantize(&mut self, rgba: &[u8], width: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(rgba.len() / 4);
        for (i, px) in rgba.chunks_exact(4).enumerate() {
            let (x, y) = (i % width, i / width);
            let offset = ((BAYER[y & 7][x & 7] as f32 + 0.5) / 64.0 - 0.5) * self.dither;
            let d = |c: u8| (c as f32 + offset).round().clamp(0.0, 255.0) as u8;
            out.push(self.index(d(px[0]), d(px[1]), d(px[2])));
        }
        out
    }
}

/// Encode RGBA `frames` (all `width × height`, row-major, no padding) as a
/// looping GIF at `delay_cs` centiseconds a frame, dithered by `dither`
/// steps (see [`DEFAULT_DITHER`]).
pub(super) fn write_gif(
    path: &str,
    width: u32,
    height: u32,
    frames: &[Vec<u8>],
    delay_cs: u16,
    dither: f32,
) -> Result<(), String> {
    if frames.is_empty() {
        return Err("no frames to encode".into());
    }
    let expect = (width * height * 4) as usize;
    if let Some((i, f)) = frames.iter().enumerate().find(|(_, f)| f.len() != expect) {
        return Err(format!(
            "frame {i} is {} bytes, expected {expect} for {width}×{height} RGBA",
            f.len()
        ));
    }
    let (w16, h16) = (
        u16::try_from(width).map_err(|_| format!("width {width} exceeds GIF's 65535"))?,
        u16::try_from(height).map_err(|_| format!("height {height} exceeds GIF's 65535"))?,
    );
    let mut q = Quantizer::fit(frames, dither.max(0.0));
    let file = File::create(path).map_err(|e| format!("create {path}: {e}"))?;
    let mut enc = gif::Encoder::new(BufWriter::new(file), w16, h16, &q.palette)
        .map_err(|e| format!("gif header: {e}"))?;
    enc.set_repeat(gif::Repeat::Infinite)
        .map_err(|e| format!("gif loop extension: {e}"))?;
    let mut previous: Option<Vec<u8>> = None;
    for rgba in frames {
        let indices = q.quantize(rgba, width as usize);
        let (buffer, transparent) = match &previous {
            None => (indices.clone(), None),
            Some(prev) => (
                indices
                    .iter()
                    .zip(prev)
                    .map(|(&now, &was)| if now == was { TRANSPARENT } else { now })
                    .collect(),
                Some(TRANSPARENT),
            ),
        };
        let frame = gif::Frame {
            delay: delay_cs,
            dispose: gif::DisposalMethod::Keep,
            transparent,
            width: w16,
            height: h16,
            buffer: Cow::Owned(buffer),
            ..gif::Frame::default()
        };
        enc.write_frame(&frame)
            .map_err(|e| format!("gif frame: {e}"))?;
        previous = Some(indices);
    }
    drop(enc);
    Ok(())
}

/// Write every frame as `<dir>/frame-NNN.png`, creating `dir`.
pub(super) fn write_png_frames(
    dir: &str,
    width: u32,
    height: u32,
    frames: &[Vec<u8>],
) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("mkdir {dir}: {e}"))?;
    for (i, rgba) in frames.iter().enumerate() {
        let path = format!("{dir}/frame-{i:03}.png");
        image::save_buffer(&path, rgba, width, height, image::ExtendedColorType::Rgba8)
            .map_err(|e| format!("write {path}: {e}"))?;
    }
    Ok(())
}

/// `--stitch`: read every `*.png` in each directory (sorted by name), in
/// the order the directories are given, and encode them as one GIF. All
/// frames must share one size - clips rendered with the same `--width` /
/// `--height` do by construction.
pub(super) fn stitch(dirs: &[String], out: &str, delay_cs: u16, dither: f32) -> Result<(), String> {
    let mut frames: Vec<Vec<u8>> = Vec::new();
    let mut size: Option<(u32, u32)> = None;
    for dir in dirs {
        let mut paths: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| format!("read {dir}: {e}"))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("png")))
            .collect();
        paths.sort();
        if paths.is_empty() {
            return Err(format!("{dir}: no .png frames"));
        }
        for path in paths {
            let img = image::open(&path)
                .map_err(|e| format!("{}: {e}", path.display()))?
                .to_rgba8();
            let dims = img.dimensions();
            match size {
                None => size = Some(dims),
                Some(s) if s != dims => {
                    return Err(format!(
                        "{}: {}×{} does not match the first frame's {}×{}",
                        path.display(),
                        dims.0,
                        dims.1,
                        s.0,
                        s.1
                    ));
                }
                _ => {}
            }
            frames.push(img.into_raw());
        }
    }
    let (w, h) = size.ok_or("no frames")?;
    write_gif(out, w, h, &frames, delay_cs, dither)?;
    println!("wrote {out} ({} frames, {w}×{h})", frames.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
        (0..w * h)
            .flat_map(|_| [rgb[0], rgb[1], rgb[2], 255])
            .collect()
    }

    /// Encode → decode. A red frame, the same frame again, then blue: the
    /// second is all "unchanged" so it decodes to red through the kept
    /// canvas, and the third really changes.
    #[test]
    fn a_clip_round_trips_with_its_delay_and_its_diff_frames() {
        let dir = std::env::temp_dir().join(format!("render-gif-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clip.gif");
        let (w, h) = (32u32, 24u32);
        let frames = vec![
            flat(w, h, [200, 30, 30]),
            flat(w, h, [200, 30, 30]),
            flat(w, h, [30, 30, 200]),
        ];
        write_gif(path.to_str().unwrap(), w, h, &frames, 8, DEFAULT_DITHER).unwrap();

        let mut opts = gif::DecodeOptions::new();
        opts.set_color_output(gif::ColorOutput::RGBA);
        let mut dec = opts.read_info(File::open(&path).unwrap()).unwrap();
        assert_eq!((dec.width() as u32, dec.height() as u32), (w, h));
        let mut decoded = Vec::new();
        while let Some(f) = dec.read_next_frame().unwrap() {
            assert_eq!(f.delay, 8);
            decoded.push((f.transparent, f.buffer.to_vec()));
        }
        assert_eq!(decoded.len(), 3);
        let near =
            |px: &[u8], rgb: [u8; 3]| (0..3).all(|c| (px[c] as i32 - rgb[c] as i32).abs() <= 24);
        assert!(
            decoded[0].0.is_none(),
            "the first frame carries no transparency"
        );
        assert!(
            near(&decoded[0].1[..4], [200, 30, 30]),
            "{:?}",
            &decoded[0].1[..4]
        );
        // The repeat frame is entirely the transparent slot: alpha 0 in RGBA
        // output, because nothing changed.
        assert_eq!(decoded[1].0, Some(TRANSPARENT));
        assert!(decoded[1].1.chunks_exact(4).all(|px| px[3] == 0));
        assert!(
            near(&decoded[2].1[..4], [30, 30, 200]),
            "{:?}",
            &decoded[2].1[..4]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_wrong_sized_frame_is_refused_by_name() {
        let err =
            write_gif("/nonexistent/x.gif", 4, 4, &[vec![0; 3]], 8, DEFAULT_DITHER).unwrap_err();
        assert!(err.contains("frame 0"), "{err}");
    }

    #[test]
    fn the_dither_is_fixed_to_the_grid_so_a_still_pixel_keeps_its_index() {
        let (w, h) = (16u32, 16u32);
        let a = flat(w, h, [120, 140, 160]);
        let mut q = Quantizer::fit(std::slice::from_ref(&a), DEFAULT_DITHER);
        let first = q.quantize(&a, w as usize);
        let second = q.quantize(&a, w as usize);
        assert_eq!(first, second);
    }
}
