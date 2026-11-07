//! Images: RGBA, 8 bits a channel. Editing them is plain Rust; only reading
//! and writing files needs the `image` feature (the `image` crate), so a
//! program that doesn't open or save images doesn't build it.

use std::sync::Arc;

/// Cheap to clone: the pixels are shared, and every operation makes new ones.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// Row by row, 4 bytes a pixel.
    pub pixels: Arc<Vec<u8>>,
}

/// Images can be very big: an operation that would make one with more
/// pixels than this makes an empty one instead.
pub const MAX_PIXELS: u64 = 1 << 28;

impl Image {
    pub fn new(width: i64, height: i64, color: [u8; 4]) -> Image {
        let (w, h) = (width.max(0) as u64, height.max(0) as u64);
        if w * h > MAX_PIXELS {
            return Image::default();
        }
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h {
            px.extend_from_slice(&color);
        }
        Image { width: w as u32, height: h as u32, pixels: Arc::new(px) }
    }

    fn from_fn(w: u32, h: u32, mut f: impl FnMut(u32, u32) -> [u8; 4]) -> Image {
        if w as u64 * h as u64 > MAX_PIXELS {
            return Image::default();
        }
        let mut px = Vec::with_capacity(w as usize * h as usize * 4);
        for y in 0..h {
            for x in 0..w {
                px.extend_from_slice(&f(x, y));
            }
        }
        Image { width: w, height: h, pixels: Arc::new(px) }
    }

    #[inline]
    pub fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        [self.pixels[i], self.pixels[i + 1], self.pixels[i + 2], self.pixels[i + 3]]
    }

    fn map(&self, mut f: impl FnMut([u8; 4]) -> [u8; 4]) -> Image {
        let mut px = Vec::with_capacity(self.pixels.len());
        // chunks_exact, not as_chunks: compiled programs build this with Rust 1.85
        for p in self.pixels.chunks_exact(4) {
            px.extend_from_slice(&f([p[0], p[1], p[2], p[3]]));
        }
        Image { width: self.width, height: self.height, pixels: Arc::new(px) }
    }
}

/// `#rgb`, `#rrggbb`, `#rrggbbaa`, or a name: black, white, red, green,
/// blue, yellow, cyan, magenta, gray, transparent. Anything else is black.
pub fn color(s: &str) -> [u8; 4] {
    let s = s.trim();
    let hex = |h: &str| u8::from_str_radix(h, 16).ok();
    if let Some(h) = s.strip_prefix('#') {
        let c = match h.len() {
            3 => h.chars().map(|c| hex(&c.to_string()).map(|v| v * 17)).collect::<Option<Vec<_>>>(),
            6 | 8 => (0..h.len() / 2).map(|i| h.get(i * 2..i * 2 + 2).and_then(hex)).collect(),
            _ => None,
        };
        if let Some(c) = c {
            return [c[0], c[1], c[2], *c.get(3).unwrap_or(&255)];
        }
    }
    match s.to_ascii_lowercase().as_str() {
        "white" => [255, 255, 255, 255],
        "red" => [255, 0, 0, 255],
        "green" => [0, 255, 0, 255],
        "blue" => [0, 0, 255, 255],
        "yellow" => [255, 255, 0, 255],
        "cyan" => [0, 255, 255, 255],
        "magenta" => [255, 0, 255, 255],
        "gray" | "grey" => [128, 128, 128, 255],
        "transparent" => [0, 0, 0, 0],
        _ => [0, 0, 0, 255],
    }
}

/// `#rrggbbaa`, what [`color`] reads.
pub fn color_text(c: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}{:02x}", c[0], c[1], c[2], c[3])
}

pub fn width(img: &Image) -> i64 {
    img.width as i64
}

pub fn height(img: &Image) -> i64 {
    img.height as i64
}

/// The color at X, Y, as `#rrggbbaa`: empty outside the image.
pub fn pixel(img: &Image, x: i64, y: i64) -> String {
    if x < 0 || y < 0 || x >= img.width as i64 || y >= img.height as i64 {
        return String::new();
    }
    color_text(img.at(x as u32, y as u32))
}

pub fn grayscale(img: &Image) -> Image {
    img.map(|[r, g, b, a]| {
        let l = ((r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000) as u8;
        [l, l, l, a]
    })
}

pub fn invert(img: &Image) -> Image {
    img.map(|[r, g, b, a]| [255 - r, 255 - g, 255 - b, a])
}

/// Mirrors it: left to right, or upside down if VERTICAL.
pub fn flip(img: &Image, vertical: bool) -> Image {
    let (w, h) = (img.width, img.height);
    Image::from_fn(w, h, |x, y| if vertical { img.at(x, h - 1 - y) } else { img.at(w - 1 - x, y) })
}

/// Turns it clockwise by QUARTERS quarter turns.
pub fn rotate(img: &Image, quarters: i64) -> Image {
    let (w, h) = (img.width, img.height);
    match quarters.rem_euclid(4) {
        1 => Image::from_fn(h, w, |x, y| img.at(y, h - 1 - x)),
        2 => Image::from_fn(w, h, |x, y| img.at(w - 1 - x, h - 1 - y)),
        3 => Image::from_fn(h, w, |x, y| img.at(w - 1 - y, x)),
        _ => img.clone(),
    }
}

/// Bilinear, to WIDTH x HEIGHT.
pub fn resize(img: &Image, width: i64, height: i64) -> Image {
    let (w, h) = (width.max(0) as u32, height.max(0) as u32);
    if img.width == 0 || img.height == 0 || w == 0 || h == 0 {
        return Image::new(w as i64, h as i64, [0, 0, 0, 0]);
    }
    let sx = img.width as f32 / w as f32;
    let sy = img.height as f32 / h as f32;
    Image::from_fn(w, h, |x, y| {
        let fx = ((x as f32 + 0.5) * sx - 0.5).max(0.0);
        let fy = ((y as f32 + 0.5) * sy - 0.5).max(0.0);
        let (x0, y0) = (fx as u32, fy as u32);
        let (x1, y1) = ((x0 + 1).min(img.width - 1), (y0 + 1).min(img.height - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let (a, b, c, d) = (img.at(x0, y0), img.at(x1, y0), img.at(x0, y1), img.at(x1, y1));
        let mut out = [0u8; 4];
        for i in 0..4 {
            let top = a[i] as f32 * (1.0 - tx) + b[i] as f32 * tx;
            let bottom = c[i] as f32 * (1.0 - tx) + d[i] as f32 * tx;
            out[i] = (top * (1.0 - ty) + bottom * ty).round() as u8;
        }
        out
    })
}

/// The part from X, Y, WIDTH x HEIGHT, cut to what's inside the image.
pub fn crop(img: &Image, x: i64, y: i64, width: i64, height: i64) -> Image {
    let x0 = x.clamp(0, img.width as i64) as u32;
    let y0 = y.clamp(0, img.height as i64) as u32;
    let w = width.clamp(0, img.width as i64 - x0 as i64) as u32;
    let h = height.clamp(0, img.height as i64 - y0 as i64) as u32;
    Image::from_fn(w, h, |px, py| img.at(x0 + px, y0 + py))
}

/// A box blur RADIUS pixels around, done twice (close to a gaussian).
pub fn blur(img: &Image, radius: i64) -> Image {
    let r = radius.clamp(0, 256) as i32;
    if r == 0 || img.width == 0 || img.height == 0 {
        return img.clone();
    }
    let mut out = img.clone();
    for _ in 0..2 {
        out = box_pass(&out, r, true);
        out = box_pass(&out, r, false);
    }
    out
}

fn box_pass(img: &Image, r: i32, horizontal: bool) -> Image {
    let (w, h) = (img.width as i32, img.height as i32);
    let (lines, len) = if horizontal { (h, w) } else { (w, h) };
    let mut px = vec![0u8; img.pixels.len()];
    let idx = |line: i32, i: i32| -> usize {
        let i = i.clamp(0, len - 1);
        let (x, y) = if horizontal { (i, line) } else { (line, i) };
        (y as usize * w as usize + x as usize) * 4
    };
    let n = (2 * r + 1) as u32;
    for line in 0..lines {
        let mut sum = [0u32; 4];
        for i in -r..=r {
            let p = idx(line, i);
            for c in 0..4 {
                sum[c] += img.pixels[p + c] as u32;
            }
        }
        for i in 0..len {
            let p = idx(line, i);
            for c in 0..4 {
                px[p + c] = ((sum[c] + n / 2) / n) as u8;
            }
            let (add, sub) = (idx(line, i + r + 1), idx(line, i - r));
            for c in 0..4 {
                sum[c] = sum[c] + img.pixels[add + c] as u32 - img.pixels[sub + c] as u32;
            }
        }
    }
    Image { width: img.width, height: img.height, pixels: Arc::new(px) }
}

fn channel(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// Adds AMOUNT (-1 to 1: from black to white) to every channel.
pub fn brightness(img: &Image, amount: f64) -> Image {
    let d = amount * 255.0;
    img.map(|[r, g, b, a]| [channel(r as f64 + d), channel(g as f64 + d), channel(b as f64 + d), a])
}

/// Spreads the channels away from the middle gray by FACTOR (1: as is,
/// 0: all gray).
pub fn contrast(img: &Image, factor: f64) -> Image {
    let f = |v: u8| channel((v as f64 - 127.5) * factor + 127.5);
    img.map(|[r, g, b, a]| [f(r), f(g), f(b), a])
}

/// Mixes every pixel with COLOR, by AMOUNT (0: as is, 1: all COLOR).
pub fn tint(img: &Image, color_: &str, amount: f64) -> Image {
    let c = color(color_);
    let t = amount.clamp(0.0, 1.0);
    let mix = |v: u8, c: u8| channel(v as f64 * (1.0 - t) + c as f64 * t);
    img.map(|[r, g, b, a]| [mix(r, c[0]), mix(g, c[1]), mix(b, c[2]), a])
}

/// Reads an image file: png, jpg, gif, bmp, qoi or tga. An empty image
/// if it can't.
#[cfg(feature = "image")]
pub fn open(path: &str) -> Image {
    match ::image::open(path) {
        Ok(img) => {
            let rgba = img.into_rgba8();
            let (width, height) = rgba.dimensions();
            Image { width, height, pixels: Arc::new(rgba.into_raw()) }
        }
        Err(_) => Image::default(),
    }
}

#[cfg(not(feature = "image"))]
pub fn open(_path: &str) -> Image {
    Image::default()
}

/// Writes IMG to PATH, in the format its extension says (png, jpg, bmp,
/// qoi, tga, gif). True if it could.
#[cfg(feature = "image")]
pub fn save(img: &Image, path: &str) -> bool {
    if img.width == 0 || img.height == 0 {
        return false;
    }
    let Some(buf) = ::image::RgbaImage::from_raw(img.width, img.height, img.pixels.to_vec()) else {
        return false;
    };
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        // jpeg has no alpha
        return ::image::DynamicImage::ImageRgba8(buf).into_rgb8().save(path).is_ok();
    }
    buf.save(path).is_ok()
}

#[cfg(not(feature = "image"))]
pub fn save(_img: &Image, _path: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits() {
        let img = Image::new(3, 2, color("#102030"));
        assert_eq!(pixel(&img, 0, 0), "#102030ff");
        assert_eq!(pixel(&img, 3, 0), "");
        let r = rotate(&img, 1);
        assert_eq!((r.width, r.height), (2, 3));
        assert_eq!(crop(&img, 1, 1, 9, 9).width, 2);
        assert_eq!(pixel(&invert(&img), 0, 0), "#efdfcfff");
        assert_eq!(pixel(&blur(&img, 2), 1, 1), "#102030ff");
        assert_eq!(resize(&img, 6, 4).width, 6);
        assert_eq!(color("#fff"), [255, 255, 255, 255]);
    }
}
