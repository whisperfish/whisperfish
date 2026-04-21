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
    // TODO: in-place resize for e.g. Whisperfish embedded camera - Option<storage_dir> perhaps?
    storage_dir: &Path,
    quality: AttachmentQuality,
) -> Result<ResizeResult, ResizeError> {
    let size_file = fs::File::open(input_path)?;
    let size = size_file.allocated_size()?;
    drop(size_file);

    let mut decoder = ImageReader::open(input_path)?.into_decoder()?;
    let orientation = decoder.orientation();
    let original_image = DynamicImage::from_decoder(decoder)?;

    let Some(mut scaled_img) = maybe_resize(original_image, size, quality) else {
        return Ok(ResizeResult::NoAction);
    };

    if let Ok(orientation) = orientation {
        scaled_img.apply_orientation(orientation);
    }

    let file_name = Uuid::new_v4().as_simple().to_string();
    let mut file_path = storage_dir.join(Path::new(&file_name));
    file_path.set_extension("jpg");

    let mut output_file = fs::File::create(&file_path)?;
    let mut encoder = JpegEncoder::new_with_quality(&mut output_file, JPEG_QUALITY);

    // JPEG wants RGB8 - convert if needed.
    let scaled_image_bytes = if scaled_img.color() == ColorType::Rgb8 {
        scaled_img.as_bytes()
    } else {
        &scaled_img.to_rgb8()
    };

    // TODO: Repeat resize with descending sizes until size <= max_size. And make it async. #765
    encoder.encode(
        scaled_image_bytes,
        scaled_img.width(),
        scaled_img.height(),
        ExtendedColorType::Rgb8,
    )?;

    Ok(ResizeResult::Resized(file_path))
}

fn maybe_resize(img: DynamicImage, size: u64, quality: AttachmentQuality) -> Option<DynamicImage> {
    let (width, height) = img.dimensions();

    let (max_dimension, max_size) = match quality {
        AttachmentQuality::High => (HQ_MAX_DIMENSION, HQ_MAX_SIZE),
        AttachmentQuality::Standard => (SQ_MAX_DIMENSION, SQ_MAX_SIZE),
        AttachmentQuality::Low => (LQ_MAX_DIMENSION, LQ_MAX_SIZE),
    };

    if width > max_dimension || height > max_dimension {
        // Not using resize_exact preserves the aspect ratio precisely
        // but can result in a sligtly reduced image dimensions.
        Some(img.resize(
            max_dimension,
            max_dimension,
            image::imageops::FilterType::Lanczos3,
        ))
    } else if size > max_size {
        // Image is smaller in dimension but too large in file size.
        // We'll delegate the responsibility to JPEG compression phase.
        Some(img)
    } else {
        None
    }
}

