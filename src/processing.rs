use std::io::Cursor;

use image::{ imageops::FilterType, DynamicImage, ImageFormat };

use crate::error::AppError;

pub const BASE_PRICE_KES: i64 = 1;
pub const RESIZE_PRICE_KES: i64 = 1;
pub const ROTATE_PRICE_KES: i64 = 1;
pub const GRAYSCALE_PRICE_KES: i64 = 1;

#[derive(Debug)]
pub struct Options {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub rotate: u32,
    pub grayscale: bool,
    pub format: ImageFormat,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            rotate: 0,
            grayscale: false,
            format: ImageFormat::Png,
        }
    }
}

impl Options {
    fn resizes(&self) -> bool {
        self.width.is_some() || self.height.is_some()
    }

    pub fn cost_kes(&self) -> i64 {
        let mut cost = BASE_PRICE_KES;
        if self.resizes() {
            cost += RESIZE_PRICE_KES;
        }
        if self.rotate != 0 {
            cost += ROTATE_PRICE_KES;
        }
        if self.grayscale {
            cost += GRAYSCALE_PRICE_KES;
        }
        cost
    }

    pub fn operations(&self) -> String {
        let mut ops = Vec::new();
        if self.resizes() {
            ops.push("resize");
        }
        if self.rotate != 0 {
            ops.push("rotate");
        }
        if self.grayscale {
            ops.push("grayscale");
        }
        if ops.is_empty() {
            "convert".to_string()
        } else {
            ops.join(",")
        }
    }
}


pub fn transform(bytes: &[u8], opts: &Options) -> Result<Vec<u8>, AppError> {
    let mut img = image
        ::load_from_memory(bytes)
        .map_err(|e| AppError::bad_request(format!("could not decode image: {e}")))?;

    img = match opts.rotate {
        0 => img,
        90 => img.rotate90(),
        180 => img.rotate180(),
        270 => img.rotate270(),
        other => {
            return Err(
                AppError::bad_request(format!("rotate must be 0, 90, 180 or 270 (got {other})"))
            );
        }
    };

    if opts.resizes() {
        let (w, h) = (img.width(), img.height());
        let (target_w, target_h) = match (opts.width, opts.height) {
            (Some(tw), Some(th)) => (tw, th),
            (Some(tw), None) => (tw, scale(h, tw, w)),
            (None, Some(th)) => (scale(w, th, h), th),
            (None, None) => unreachable!(),
        };
        img = img.resize(target_w, target_h, FilterType::Lanczos3);
    }

    if opts.grayscale {
        img = img.grayscale();
    }

    if
        opts.format == ImageFormat::Jpeg &&
        !matches!(img, DynamicImage::ImageLuma8(_) | DynamicImage::ImageRgb8(_))
    {
        img = DynamicImage::ImageRgb8(img.to_rgb8());
    }

    let mut out = Vec::new();
    img
        .write_to(&mut Cursor::new(&mut out), opts.format)
        .map_err(|e| AppError::internal(format!("could not encode image: {e}")))?;
    Ok(out)
}

fn scale(value: u32, numerator: u32, denominator: u32) -> u32 {
    ((((value as f64) * (numerator as f64)) / (denominator as f64)).round() as u32).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_png(w: u32, h: u32) -> Vec<u8> {
        let img = DynamicImage::ImageRgb8(
            image::RgbImage::from_pixel(w, h, image::Rgb([200, 30, 30]))
        );
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png).unwrap();
        buf
    }

    #[test]
    fn resizes_keeping_aspect_ratio() {
        let opts = Options { width: Some(50), ..Default::default() };
        let out = image
            ::load_from_memory(&transform(&sample_png(100, 40), &opts).unwrap())
            .unwrap();
        assert_eq!((out.width(), out.height()), (50, 20));
    }

    #[test]
    fn rotates_90_degrees() {
        let opts = Options { rotate: 90, ..Default::default() };
        let out = image
            ::load_from_memory(&transform(&sample_png(100, 40), &opts).unwrap())
            .unwrap();
        assert_eq!((out.width(), out.height()), (40, 100));
    }

    #[test]
    fn converts_to_grayscale() {
        let opts = Options { grayscale: true, ..Default::default() };
        let out = image::load_from_memory(&transform(&sample_png(10, 10), &opts).unwrap()).unwrap();
        assert!(!out.color().has_color());
    }

    #[test]
    fn rejects_invalid_rotation() {
        let opts = Options { rotate: 45, ..Default::default() };
        assert!(transform(&sample_png(10, 10), &opts).is_err());
    }

    #[test]
    fn rejects_non_image_data() {
        assert!(transform(b"definitely not an image", &Options::default()).is_err());
    }

    #[test]
    fn prices_each_operation() {
        assert_eq!(Options::default().cost_kes(), 1);

        let opts = Options {
            width: Some(50),
            rotate: 90,
            grayscale: true,
            ..Default::default()
        };
        assert_eq!(opts.cost_kes(), 4);
        assert_eq!(opts.operations(), "resize,rotate,grayscale");
    }
}
