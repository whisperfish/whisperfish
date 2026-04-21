use fs2::FileExt;
use image::codecs::jpeg::JpegEncoder;
use image::{
    ColorType, DynamicImage, ExtendedColorType, GenericImageView, ImageDecoder, ImageReader,
};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentQuality {
    High,
    Standard,
    Low,
}

impl AttachmentQuality {
    pub const fn as_str(&self) -> &'static str {
        match self {
            AttachmentQuality::High => "high",
            AttachmentQuality::Standard => "standard",
            AttachmentQuality::Low => "low",
        }
    }
}

impl From<&str> for AttachmentQuality {
    fn from(value: &str) -> Self {
        match value {
            "high" => AttachmentQuality::High,
            "standard" => AttachmentQuality::Standard,
            "low" => AttachmentQuality::Low,
            x => {
                tracing::warn!("Unknown AttachmentQuality value {x}, returning standard");
                AttachmentQuality::Standard
            }
        }
    }
}

impl From<AttachmentQuality> for String {
    fn from(quality: AttachmentQuality) -> Self {
        quality.as_str().to_string()
    }
}

impl AsRef<str> for AttachmentQuality {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

// Maximum dimensions (width or height) for resized images.
// Signal Android even has target sizes list, but we don't yet implement that.
//
// Signal Android uses limits of:
// LEVEL_1: 1600px, 1.0MB, q70
// LEVEL_2: 2048px, 1.5MB, q75
// LEVEL_3: 4096px, 3.0MB, q75
// https://github.com/signalapp/Signal-Android/commit/c53abe09416e189396484a248f717a269d880c6f#diff-aff97af8f1d56c0c910bc2a1b48de39539724d3fdb262eaa099d20b39fdf9335R113

const HQ_MAX_DIMENSION: u32 = 4096;
const SQ_MAX_DIMENSION: u32 = 2048;
const LQ_MAX_DIMENSION: u32 = 1600;

const MB: u64 = 1024 * 1024;
const HQ_MAX_SIZE: u64 = 3 * MB;
const SQ_MAX_SIZE: u64 = 3 * MB / 2;
const LQ_MAX_SIZE: u64 = MB;

const JPEG_QUALITY: u8 = 80;

#[derive(Debug)]
pub enum ResizeResult {
    NoAction,
    Resized(PathBuf),
}

#[derive(Debug)]
pub enum ResizeError {
    IoError(std::io::Error),
    ImageError(image::ImageError),
    UnsupportedFormat,
}

impl From<std::io::Error> for ResizeError {
    fn from(err: std::io::Error) -> Self {
        ResizeError::IoError(err)
    }
}

impl From<image::ImageError> for ResizeError {
    fn from(err: image::ImageError) -> Self {
        ResizeError::ImageError(err)
    }
}

/// Resize the to-be-attached image file so that
/// * its width nor height doesn't exceed the selected maximum
/// * its file size doesn't exceed the selected maximum
///
/// User makes the quality selection in application settings,
/// which determines the maximum file size and dimension used.
///
/// If shrinking is necessary, the shrunken copy of the file is saved
/// in the storage directory passed to the function, and the full path is returned.
/// If there are errors opening, reading, decoding, resizing, encoding or saving
/// the file, the error is propagated.
///
/// If the file is already small enough, no action is taken and no path is returned.
pub fn shrink_attachment(
    input_path: &Path,
    storage_dir: &Path,
    quality: AttachmentQuality,
) -> Result<ResizeResult, ResizeError> {
    let size_file = fs::File::open(input_path)?;
    let size = size_file.allocated_size()?;
    drop(size_file);

    let mut decoder = ImageReader::open(input_path)?.into_decoder()?;
    let orientation = decoder.orientation();
    let original_image = DynamicImage::from_decoder(decoder)?;

    let mut scaled_img = maybe_resize(original_image, size, quality);

    if let Ok(orientation) = orientation {
        scaled_img.apply_orientation(orientation);
    }

    let file_name = Uuid::new_v4().as_simple().to_string();
    let mut file_path = storage_dir.join(Path::new(&file_name));
    file_path.set_extension("jpg");

    // JPEG wants RGB8 - convert if needed.
    let scaled_image_bytes = if scaled_img.color() == ColorType::Rgb8 {
        scaled_img.as_bytes()
    } else {
        tracing::debug!("Image not in RGB8 color, converting");
        &scaled_img.to_rgb8()
    };

    let mut output_file = fs::File::create(&file_path)?;
    let mut encoder = JpegEncoder::new_with_quality(&mut output_file, JPEG_QUALITY);

    // TODO: Repeat resize with descending sizes until size <= max_size. And make it async. #765
    encoder.encode(
        scaled_image_bytes,
        scaled_img.width(),
        scaled_img.height(),
        ExtendedColorType::Rgb8,
    )?;
    tracing::debug!(
        "Attachment image '{}' re-encoded to '{}'",
        input_path.to_string_lossy(),
        file_path.to_string_lossy()
    );
    Ok(ResizeResult::Resized(file_path))
}

fn maybe_resize(img: DynamicImage, size: u64, quality: AttachmentQuality) -> DynamicImage {
    let (width, height) = img.dimensions();

    let (max_dimension, max_size) = match quality {
        AttachmentQuality::High => (HQ_MAX_DIMENSION, HQ_MAX_SIZE),
        AttachmentQuality::Standard => (SQ_MAX_DIMENSION, SQ_MAX_SIZE),
        AttachmentQuality::Low => (LQ_MAX_DIMENSION, LQ_MAX_SIZE),
    };

    if width > max_dimension || height > max_dimension {
        // Not using resize_exact preserves the aspect ratio precisely
        // but can result in a sligtly reduced image dimensions.
        tracing::debug!("Image file has too large dimension ({width}|‚{height} > {max_dimension}), recompressing.");
        img.resize(
            max_dimension,
            max_dimension,
            image::imageops::FilterType::Lanczos3,
        )
    } else if size > max_size {
        // Image is smaller in dimension but too large in file size.
        // We'll delegate the responsibility to JPEG compression phase.
        // Keeping this branch here so we can keep the size stuff for later.
        tracing::debug!("Image file is too large ({size} > {max_size}), recompressing.");
        img
    } else {
        // Image is small enough both in dimensions and file size.
        // We want to re-encode it anyway so it gets stripped of
        // metadata and gets saved to attachments.
        tracing::debug!("Image size and dimension ok, recompressing anyway.");
        img
    }
}

#[rustfmt::skip]
#[cfg(test)]
mod tests {
    use tempfile::{TempDir, TempPath};

    use super::*;

    const XLARGE_DIMENSION: u32 = 4200;
    const LARGE_DIMENSION: u32 = 2200;
    const STANDARD_DIMENSION: u32 = 1700;
    const SMALL_DIMENSION: u32 = 1200;
    const TINY_DIMENSION: u32 = 100; // Used as smaller width/height as well

    const XLARGE_SIZE: u64 = 4 * MB;
    const LARGE_SIZE: u64 = 2 * MB;
    const STANDARD_SIZE: u64 = (1.25 * (MB as f32)) as u64;
    const SMALL_SIZE: u64 = (0.8 * (MB as f32)) as u64;

    #[test]
    fn test_test_image_sizes() {
        // Sanity check for the rest of the tests.
        // Use variables to keep Clippy happy
        // and compiler from optimizing away `assert!(true)`
        let tiny_dimension = TINY_DIMENSION;
        let small_dimension = SMALL_DIMENSION;
        let low_quality_dimension = LQ_MAX_DIMENSION;
        let standard_dimension = STANDARD_DIMENSION;
        let standard_quality_dimension = SQ_MAX_DIMENSION;
        let large_dimension = LARGE_DIMENSION;
        let high_quality_dimension = HQ_MAX_DIMENSION;
        let xlarge_dimension = XLARGE_DIMENSION;
        assert!(tiny_dimension < small_dimension);
        assert!(small_dimension < low_quality_dimension);
        assert!(low_quality_dimension < standard_dimension);
        assert!(standard_dimension < standard_quality_dimension);
        assert!(standard_quality_dimension < large_dimension);
        assert!(large_dimension < high_quality_dimension);
        assert!(high_quality_dimension < xlarge_dimension);

        let small_size = SMALL_SIZE;
        let lq_max_size = LQ_MAX_SIZE;
        let standard_size = STANDARD_SIZE;
        let sq_max_size = SQ_MAX_SIZE;
        let large_size = LARGE_SIZE;
        let hq_max_size = HQ_MAX_SIZE;
        let xlarge_size = XLARGE_SIZE;
        assert!(small_size < lq_max_size);
        assert!(lq_max_size < standard_size);
        assert!(standard_size < sq_max_size);
        assert!(sq_max_size < large_size);
        assert!(large_size < hq_max_size);
        assert!(hq_max_size < xlarge_size);
    }

    #[test]
    fn test_xlarge_image() {
        let img = DynamicImage::new_rgb8(XLARGE_DIMENSION, TINY_DIMENSION);
        let res = maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::High);
        assert_eq!(res.dimensions(), (4096, 98));
        let res = maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::Standard);
        assert_eq!(res.dimensions(), (2048, 49));
        let res = maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::Low);
        assert_eq!(res.dimensions(), (1600, 38));

        let img = DynamicImage::new_rgb8(TINY_DIMENSION, XLARGE_DIMENSION);
        let res = maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::High);
        assert_eq!(res.dimensions(), (98, 4096));
        let res = maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::Standard);
        assert_eq!(res.dimensions(), (49, 2048));
        let res = maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::Low);
        assert_eq!(res.dimensions(), (38, 1600));
    }

    #[test]
    fn test_large_image() {
        let img = DynamicImage::new_rgb8(LARGE_DIMENSION, TINY_DIMENSION);
        let res = maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::High);
        assert_eq!(res, img);
        let res = maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::Standard);
        assert_eq!(res.dimensions(), (2048, 93));
        let res = maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::Low);
        assert_eq!(res.dimensions(), (1600, 73));

        let img = DynamicImage::new_rgb8(TINY_DIMENSION, LARGE_DIMENSION);
        let res = maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::High);
        assert_eq!(res, img);
        let res = maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::Standard);
        assert_eq!(res.dimensions(), (93, 2048));
        let res = maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::Low);
        assert_eq!(res.dimensions(), (73, 1600));
    }

    #[test]
    fn test_medium_image() {
        let img = DynamicImage::new_rgb8(STANDARD_DIMENSION, TINY_DIMENSION);
        assert_eq!(img, maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::High));
        assert_eq!(img, maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::Standard));
        let res = maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::Low);
        assert_eq!(res.dimensions(), (1600, 94));

        let img = DynamicImage::new_rgb8(TINY_DIMENSION, STANDARD_DIMENSION);
        assert_eq!(img, maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::High));
        assert_eq!(img, maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::Standard));
        let res = maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::Low);
        assert_eq!(res.dimensions(), (94, 1600));
    }

    #[test]
    fn test_small_image() {
        let img = DynamicImage::new_rgb8(SMALL_DIMENSION, TINY_DIMENSION);
        assert_eq!(img, maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::High));
        assert_eq!(img, maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::Standard));
        assert_eq!(img, maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::Low));

        let img = DynamicImage::new_rgb8(TINY_DIMENSION, SMALL_DIMENSION);
        assert_eq!(img, maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::High));
        assert_eq!(img, maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::Standard));
        assert_eq!(img, maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::Low));
    }

    #[test]
    fn test_image_size_limits() {
        let img = DynamicImage::new_rgb8(TINY_DIMENSION, TINY_DIMENSION);

        // Small enough
        assert_eq!(maybe_resize(img.clone(), SMALL_SIZE, AttachmentQuality::Low), img);
        assert_eq!(maybe_resize(img.clone(), STANDARD_SIZE, AttachmentQuality::Standard), img);
        assert_eq!(maybe_resize(img.clone(), LARGE_SIZE, AttachmentQuality::High), img);

        // Needs resizing
        assert_eq!(maybe_resize(img.clone(), STANDARD_SIZE, AttachmentQuality::Low), img);
        assert_eq!(maybe_resize(img.clone(), LARGE_SIZE, AttachmentQuality::Standard), img);
        assert_eq!(maybe_resize(img.clone(), XLARGE_SIZE, AttachmentQuality::High), img);
    }

    #[test]
    fn test_rgba8_png_to_rgb8_jpg() {
        let input_dir = TempDir::new().unwrap();
        let storage_dir = TempDir::new().unwrap();
        let input_path = input_dir.path().join("large_input.png");

        let img = image::ImageBuffer::from_fn(1700, 1000, |x, y| {
            image::Rgba([
                (x % 256) as u8,
                (y % 256) as u8,
                ((x + y) % 256) as u8,
                128u8,
            ])
        });
        img.save(&input_path).unwrap();
        let _output_path = TempPath::from_path(&input_path);

        let size_file = fs::File::open(&input_path).unwrap();
        let size = size_file.allocated_size().unwrap();
        drop(size_file);

        let (orig_w, orig_h) = img.dimensions();
        assert!(orig_w < SQ_MAX_DIMENSION && orig_h < SQ_MAX_DIMENSION);
        assert!(size > SQ_MAX_SIZE);

        match shrink_attachment(&input_path, storage_dir.path(), AttachmentQuality::Standard)
            .unwrap()
        {
            ResizeResult::Resized(resized_path) => {
                assert!(resized_path.to_string_lossy().ends_with("jpg"));

                let output_img = image::open(&resized_path).unwrap();
                let _resized_path = TempPath::from_path(&resized_path);
                let (w, h) = output_img.dimensions();

                let size_file = fs::File::open(resized_path).unwrap();
                let size = size_file.allocated_size().unwrap();
                drop(size_file);

                // Only format and file size changes
                assert_eq!(w, orig_w);
                assert_eq!(h, orig_h);
                assert!(size <= SQ_MAX_SIZE);
            }
            ResizeResult::NoAction => {
                panic!("should have resized the image because of file size")
            }
        }

        assert!(orig_w > LQ_MAX_DIMENSION || orig_h > LQ_MAX_DIMENSION);
        assert!(size > LQ_MAX_SIZE);
        match shrink_attachment(&input_path, storage_dir.path(), AttachmentQuality::Low).unwrap() {
            ResizeResult::Resized(resized_path) => {
                assert!(resized_path.to_string_lossy().ends_with("jpg"));

                let output_img = image::open(&resized_path).unwrap();
                let _resized_path = TempPath::from_path(&resized_path);
                let (w, h) = output_img.dimensions();

                let size_file = fs::File::open(resized_path).unwrap();
                let size = size_file.allocated_size().unwrap();
                drop(size_file);

                // Dimensions and size are changed
                assert!(w <= LQ_MAX_DIMENSION);
                assert!(h <= LQ_MAX_DIMENSION);
                assert!(size <= LQ_MAX_SIZE);
            }
            ResizeResult::NoAction => {
                panic!("should have resized the image because of file size")
            }
        }
    }
}
