use image::{
    ImageDecoder, ImageFormat, ImageReader, codecs::jpeg::JpegEncoder, imageops::FilterType,
};
use sha2::{Digest, Sha256};
use std::io::Cursor;

pub const MAX_UPLOAD: usize = 5_000_000;
pub const MAX_STORED: usize = 500_000;
pub const MAX_WIDTH: u32 = 1920;
pub const MAX_HEIGHT: u32 = 1200;

pub struct Prepared {
    pub bytes: Vec<u8>,
    pub filename: String,
}

pub fn owned_name(name: &str) -> bool {
    name.strip_prefix("custom-")
        .and_then(|s| s.strip_suffix(".jpg"))
        .is_some_and(|hash| {
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        })
}

fn dimensions(width: u32, height: u32) -> Result<(), String> {
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || u64::from(width) * u64::from(height) > 16_000_000
    {
        return Err("图片最多 1600 万像素，任一边不能超过 8192 像素".into());
    }
    Ok(())
}

pub fn prepare(bytes: &[u8]) -> Result<Prepared, String> {
    if bytes.is_empty() || bytes.len() > MAX_UPLOAD {
        return Err("请选择不超过 5 MB 的图片".into());
    }
    let format =
        image::guess_format(bytes).map_err(|_| "图片格式无效，请选择 JPEG、PNG 或 WebP")?;
    if !matches!(
        format,
        ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP
    ) {
        return Err("仅支持 JPEG、PNG 或 WebP 图片".into());
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader
        .into_decoder()
        .map_err(|_| "无法读取图片或图片尺寸过大")?;
    let (width, height) = decoder.dimensions();
    dimensions(width, height)?;
    let orientation = decoder.orientation().map_err(|_| "图片方向信息无效")?;
    let mut image =
        image::DynamicImage::from_decoder(decoder).map_err(|_| "图片已损坏或解码失败")?;
    image.apply_orientation(orientation);
    if image.width() > MAX_WIDTH || image.height() > MAX_HEIGHT {
        image = image.resize(MAX_WIDTH, MAX_HEIGHT, FilterType::Triangle);
    }
    // Flatten transparency before JPEG encoding; stored files contain no original metadata.
    let rgba = image.to_rgba8();
    let mut rgb = image::RgbImage::new(rgba.width(), rgba.height());
    for (src, dst) in rgba.pixels().zip(rgb.pixels_mut()) {
        let alpha = u32::from(src[3]);
        for c in 0..3 {
            dst[c] = ((u32::from(src[c]) * alpha + 18 * (255 - alpha)) / 255) as u8;
        }
    }
    for _ in 0..5 {
        for quality in [85, 75, 65, 55, 45] {
            let mut output = Vec::new();
            JpegEncoder::new_with_quality(&mut output, quality)
                .encode_image(&rgb)
                .map_err(|_| "图片压缩失败")?;
            if output.len() <= MAX_STORED {
                let filename = format!("custom-{:x}.jpg", Sha256::digest(&output));
                return Ok(Prepared {
                    bytes: output,
                    filename,
                });
            }
        }
        rgb = image::imageops::resize(
            &rgb,
            (rgb.width() * 3 / 4).max(1),
            (rgb.height() * 3 / 4).max(1),
            FilterType::Triangle,
        );
    }
    Err("图片无法压缩到 500 KB 以内，请换一张图片".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_oversize_and_unsupported_uploads() {
        assert!(prepare(&[]).is_err());
        assert!(prepare(b"not an image").is_err());
        assert!(prepare(&vec![0; MAX_UPLOAD + 1]).is_err());
        assert!(prepare(b"GIF89a\x01\x00\x01\x00").is_err());
    }

    #[test]
    fn rejects_decompression_bombs_before_decoding() {
        assert!(dimensions(8193, 1).is_err());
        assert!(dimensions(5000, 4000).is_err());
        assert!(dimensions(0, 100).is_err());
        assert!(dimensions(4000, 4000).is_ok());
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(8193, 1)
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        assert!(prepare(png.get_ref()).is_err());
    }

    #[test]
    fn normalizes_png_to_bounded_jpeg_and_immutable_owned_filename() {
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2400, 1600)
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        let result = prepare(png.get_ref()).unwrap();
        assert!(result.bytes.len() <= MAX_STORED);
        assert_eq!(
            image::guess_format(&result.bytes).unwrap(),
            ImageFormat::Jpeg
        );
        let decoded = image::load_from_memory(&result.bytes).unwrap();
        assert!(decoded.width() <= MAX_WIDTH && decoded.height() <= MAX_HEIGHT);
        assert!(owned_name(&result.filename));
        assert_eq!(result.filename, prepare(png.get_ref()).unwrap().filename);
    }

    #[test]
    fn never_accepts_arbitrary_paths() {
        for name in [
            "../settings.json",
            "custom-../secret.jpg",
            "C:\\photo.jpg",
            "custom-abcd.jpg",
            "https://example.com/image.jpg",
        ] {
            assert!(!owned_name(name));
        }
    }

    #[test]
    fn bundled_photos_fit_the_download_budget() {
        for bytes in [
            include_bytes!("../../../public/wallpapers/forest.jpg").as_slice(),
            include_bytes!("../../../public/wallpapers/mountain.jpg").as_slice(),
            include_bytes!("../../../public/wallpapers/night.jpg").as_slice(),
        ] {
            assert!(bytes.len() <= 300_000);
            let image = image::load_from_memory(bytes).unwrap();
            assert_eq!((image.width(), image.height()), (1600, 1000));
        }
    }
}
