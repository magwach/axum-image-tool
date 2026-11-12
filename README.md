# axum-image-tool

A small Axum web app that loads an uploaded image, then optionally rotates, resizes, and converts it to grayscale.

## API

`POST /process` (multipart/form-data)

| Field       | Required | Description                                              |
|-------------|----------|----------------------------------------------------------|
| `image`     | yes      | The image file (PNG, JPEG, GIF, BMP, WebP, ...)          |
| `width`     | no       | Target width in px (1-8192)                              |
| `height`    | no       | Target height in px (1-8192)                             |
| `rotate`    | no       | Clockwise rotation: `0`, `90`, `180`, `270`              |
| `grayscale` | no       | `true` / `on` / `1` to convert to grayscale              |
| `format`    | no       | Output format: `png` (default) or `jpeg`                 |

Resizing preserves aspect ratio. With only one of width/height, the other is
computed; with both, the image is fitted inside that box. Rotation is applied
first, so width/height describe the final output.

### Examples

```bash
# Resize to 300px wide, rotate 90°, grayscale, save as JPEG
curl -F image=@photo.png -F width=300 -F rotate=90 -F grayscale=true \
     -F format=jpeg http://127.0.0.1:3000/process -o out.jpg
```

## Test

```bash
cargo test
```

## Layout

- `src/main.rs`: routes, multipart handling, and the `transform` function
- `src/index.html`: upload form served at `/`