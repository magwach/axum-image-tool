use std::io::Cursor;

use axum::{
    extract::{DefaultBodyLimit, Multipart},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Router,
};
use image::{imageops::FilterType, DynamicImage, ImageFormat};

const MAX_UPLOAD_BYTES: usize = 20 * 1024 * 1024;
const MAX_DIMENSION: u32 = 8192;

#[derive(Debug)]
struct Options {
    width: Option<u32>,
    height: Option<u32>,
    rotate: u32,
    grayscale: bool,
    format: ImageFormat,
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

#[derive(Debug)]
struct AppError {
    status: StatusCode,
    message: String,
}

impl AppError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.status, self.message).into_response()
    }
}

fn transform(bytes: &[u8], opts: &Options) -> Result<Vec<u8>, AppError> {
    let mut img = image::load_from_memory(bytes)
        .map_err(|e| AppError::bad_request(format!("could not decode image: {e}")))?;

    img = match opts.rotate {
        0 => img,
        90 => img.rotate90(),
        180 => img.rotate180(),
        270 => img.rotate270(),
        other => {
            return Err(AppError::bad_request(format!(
                "rotate must be 0, 90, 180 or 270 (got {other})"
            )))
        }
    };

    if opts.width.is_some() || opts.height.is_some() {
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

    if opts.format == ImageFormat::Jpeg
        && !matches!(img, DynamicImage::ImageLuma8(_) | DynamicImage::ImageRgb8(_))
    {
        img = DynamicImage::ImageRgb8(img.to_rgb8());
    }

    // Encode.
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), opts.format)
        .map_err(|e| AppError::internal(format!("could not encode image: {e}")))?;
    Ok(out)
}

fn scale(value: u32, numerator: u32, denominator: u32) -> u32 {
    ((value as f64 * numerator as f64 / denominator as f64).round() as u32).max(1)
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../tests/index.html"))
}

async fn process_handler(mut multipart: Multipart) -> Result<Response, AppError> {
    let mut file: Option<Vec<u8>> = None;
    let mut opts = Options::default();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::bad_request(e.to_string()))?
    {
        let name = field.name().unwrap_or_default().to_string();

        if name == "image" {
            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::bad_request(e.to_string()))?;
            if !data.is_empty() {
                file = Some(data.to_vec());
            }
            continue;
        }

        let text = field
            .text()
            .await
            .map_err(|e| AppError::bad_request(e.to_string()))?;
        let text = text.trim();

        match name.as_str() {
            "width" => opts.width = parse_dimension("width", text)?,
            "height" => opts.height = parse_dimension("height", text)?,
            "rotate" if !text.is_empty() => {
                opts.rotate = text
                    .parse()
                    .map_err(|_| AppError::bad_request("rotate must be a number"))?;
            }
            "grayscale" => {
                opts.grayscale = matches!(text.to_lowercase().as_str(), "on" | "true" | "1" | "yes");
            }
            "format" if !text.is_empty() => {
                opts.format = match text.to_lowercase().as_str() {
                    "png" => ImageFormat::Png,
                    "jpg" | "jpeg" => ImageFormat::Jpeg,
                    other => {
                        return Err(AppError::bad_request(format!(
                            "unsupported format '{other}' (use png or jpeg)"
                        )))
                    }
                };
            }
            _ => {}
        }
    }

    let bytes = file.ok_or_else(|| AppError::bad_request("missing 'image' file field"))?;
    let format = opts.format;

    // Image work is CPU-bound, so keep it off the async executor threads.
    let output = tokio::task::spawn_blocking(move || transform(&bytes, &opts))
        .await
        .map_err(|e| AppError::internal(e.to_string()))??;

    Ok(([(header::CONTENT_TYPE, format.to_mime_type())], output).into_response())
}

fn parse_dimension(name: &str, text: &str) -> Result<Option<u32>, AppError> {
    if text.is_empty() {
        return Ok(None);
    }
    let value: u32 = text
        .parse()
        .map_err(|_| AppError::bad_request(format!("{name} must be a positive integer")))?;
    if value == 0 || value > MAX_DIMENSION {
        return Err(AppError::bad_request(format!(
            "{name} must be between 1 and {MAX_DIMENSION}"
        )));
    }
    Ok(Some(value))
}


fn app() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/process", post(process_handler))
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES))
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .expect("failed to bind to port 3000");
    println!("Listening on http://127.0.0.1:3000");
    axum::serve(listener, app()).await.expect("server error");
}


#[cfg(test)]
mod tests {
    use super::*;

    fn sample_png(w: u32, h: u32) -> Vec<u8> {
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(w, h, image::Rgb([200, 30, 30])));
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png).unwrap();
        buf
    }

    #[test]
    fn resizes_keeping_aspect_ratio() {
        let opts = Options { width: Some(50), ..Default::default() };
        let out = image::load_from_memory(&transform(&sample_png(100, 40), &opts).unwrap()).unwrap();
        assert_eq!((out.width(), out.height()), (50, 20));
    }

    #[test]
    fn rotates_90_degrees() {
        let opts = Options { rotate: 90, ..Default::default() };
        let out = image::load_from_memory(&transform(&sample_png(100, 40), &opts).unwrap()).unwrap();
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
}