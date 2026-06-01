use base64::{engine::general_purpose::STANDARD, Engine};
use image::{imageops::FilterType, DynamicImage, ImageFormat};
use rxing::Reader;
use tracing::instrument;

use crate::error::{AppError, Result};

const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024; // 10 MB

pub(crate) struct ImageValidationResult {
    pub(crate) decoded_qr: Option<String>,
    pub(crate) qr_match: bool,
}

struct ImageAttempt {
    label: &'static str,
    image: DynamicImage,
}

struct AllowedDataUrl {
    prefix: &'static str,
    magic: &'static [u8],
    image_format: ImageFormat,
}

const ALLOWED_DATA_URLS: &[AllowedDataUrl] = &[
    AllowedDataUrl {
        prefix: "data:image/jpeg;base64,",
        magic: &[0xFF, 0xD8, 0xFF],
        image_format: ImageFormat::Jpeg,
    },
    AllowedDataUrl {
        prefix: "data:image/jpg;base64,",
        magic: &[0xFF, 0xD8, 0xFF],
        image_format: ImageFormat::Jpeg,
    },
    AllowedDataUrl {
        prefix: "data:image/png;base64,",
        magic: &[0x89, 0x50, 0x4E, 0x47],
        image_format: ImageFormat::Png,
    },
];

/// Validates the `data:image/*;base64,` payload and optionally reads the QR code.
/// Format, magic byte and size checks are hard errors. QR okuma başarısız olursa
/// reddetmek yerine loglanır; `qr_match: false` ile devam edilir.
#[instrument(skip(image_data_url), fields(expected_qr_len = expected_qr.len()))]
pub(crate) fn validate_check_image(
    image_data_url: &str,
    expected_qr: &str,
) -> Result<ImageValidationResult> {
    let (base64_data, allowed) = parse_data_url(image_data_url)?;

    let bytes = STANDARD
        .decode(base64_data)
        .map_err(|_| AppError::invalid_input("gorsel base64 decode edilemedi"))?;

    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(AppError::invalid_input(
            "gorsel boyutu 10 MB sinirini asiyor",
        ));
    }

    validate_magic_bytes(allowed, &bytes)?;

    // Use in-memory decoder directly (avoids reader/guessing pitfalls).
    let img = image::load_from_memory_with_format(&bytes, allowed.image_format).map_err(|error| {
        tracing::warn!(%error, format = ?allowed.image_format, "gorsel decode edilemedi");
        AppError::invalid_input("gorsel acilamadi, bozuk veya desteklenmeyen format")
    })?;

    let attempts = build_preprocessed_attempts(&img);

    match try_read_data_matrix_with_attempts(&attempts) {
        Some(decoded_qr) => {
            let qr_match = decoded_qr.trim() == expected_qr.trim();
            if !qr_match {
                tracing::warn!(
                    expected = %expected_qr,
                    found = %decoded_qr,
                    "QR uyusmazligi tespit edildi"
                );
            }
            Ok(ImageValidationResult {
                decoded_qr: Some(decoded_qr),
                qr_match,
            })
        }
        None => {
            tracing::warn!(
                expected_qr = %expected_qr,
                "QR okunamadi (tum on-isleme denemeleri basarisiz), gorsel kabul edildi"
            );
            save_failed_images_for_debug(&attempts, expected_qr);
            Ok(ImageValidationResult {
                decoded_qr: None,
                qr_match: false,
            })
        }
    }
}

fn parse_data_url(image_data_url: &str) -> Result<(&str, &'static AllowedDataUrl)> {
    for allowed in ALLOWED_DATA_URLS {
        if let Some(stripped) = image_data_url.strip_prefix(allowed.prefix) {
            return Ok((stripped, allowed));
        }
    }

    Err(AppError::invalid_input(
        "desteklenmeyen gorsel formati, yalnizca JPEG ve PNG kabul edilir",
    ))
}

fn validate_magic_bytes(allowed: &AllowedDataUrl, bytes: &[u8]) -> Result<()> {
    if bytes.len() < allowed.magic.len() || &bytes[..allowed.magic.len()] != allowed.magic {
        return Err(AppError::invalid_input(
            "gorsel icerik tipi ile dosya icerigi uyusmuyor",
        ));
    }

    Ok(())
}

/// Başarısız olan tüm ara görselleri /tmp'e kaydeder, teşhis için kullanılır.
fn save_failed_images_for_debug(attempts: &[ImageAttempt], expected_qr: &str) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    for attempt in attempts {
        let path = format!("/tmp/failed_qr_{timestamp}_{}.jpg", attempt.label);
        match attempt.image.save(&path) {
            Ok(_) => tracing::warn!(
                path = %path,
                stage = attempt.label,
                expected_qr = %expected_qr,
                "QR basarisiz ara gorsel diske kaydedildi"
            ),
            Err(error) => tracing::warn!(
                error = %error,
                stage = attempt.label,
                "QR basarisiz ara gorsel kaydedilemedi"
            ),
        }
    }
}

/// Ham görsel için DataMatrix okumada kullanılacak ara görselleri üretir.
fn build_preprocessed_attempts(img: &DynamicImage) -> Vec<ImageAttempt> {
    let scaled = img.resize(img.width() * 3, img.height() * 3, FilterType::Lanczos3);
    let enhanced = scaled.adjust_contrast(50.0);
    let mut luma = scaled.to_luma8();
    for pixel in luma.pixels_mut() {
        pixel[0] = if pixel[0] >= 128 { 255 } else { 0 };
    }
    let threshold = DynamicImage::ImageLuma8(luma);

    vec![
        ImageAttempt {
            label: "original",
            image: img.clone(),
        },
        ImageAttempt {
            label: "scaled",
            image: scaled,
        },
        ImageAttempt {
            label: "enhanced",
            image: enhanced,
        },
        ImageAttempt {
            label: "threshold",
            image: threshold,
        },
    ]
}

/// Hazırlanan ara görseller üzerinde sırayla DataMatrix okumayı dener.
fn try_read_data_matrix_with_attempts(attempts: &[ImageAttempt]) -> Option<String> {
    for attempt in attempts {
        match try_read_data_matrix(&attempt.image) {
            Some(text) => {
                tracing::debug!(stage = attempt.label, "DataMatrix okundu");
                return Some(text);
            }
            None => {
                tracing::debug!(stage = attempt.label, "DataMatrix okunamadi");
            }
        }
    }

    None
}

fn try_read_data_matrix(img: &DynamicImage) -> Option<String> {
    let source = rxing::BufferedImageLuminanceSource::new(img.clone());
    let binarizer = rxing::common::HybridBinarizer::new(source);
    let mut bitmap = rxing::BinaryBitmap::new(binarizer);

    let mut hints = rxing::DecodingHintDictionary::new();
    hints.insert(
        rxing::DecodeHintType::TRY_HARDER,
        rxing::DecodeHintValue::TryHarder(true),
    );

    let mut reader = rxing::datamatrix::DataMatrixReader;
    reader
        .decode_with_hints(&mut bitmap, &hints)
        .ok()
        .map(|result| result.getText().to_string())
}
