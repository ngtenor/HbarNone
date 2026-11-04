//! Reads a QR code from a screenshot or photo, so a signing request can be
//! loaded without pasting. The image is decoded in memory and never kept.

use std::path::Path;

use image::{ImageReader, Limits};

/// Larger files are not screenshots of a QR code.
const MAX_FILE_BYTES: u64 = 25 * 1024 * 1024;
const MAX_SIDE_PIXELS: u32 = 8_000;
const MAX_DECODE_BYTES: u64 = 256 * 1024 * 1024;

/// Removes the quotes Windows adds when a file is dragged into a terminal.
pub fn clean_path(input: &str) -> &str {
    input.trim().trim_matches(|c| c == '"' || c == '\'')
}

/// Decodes the first QR code found in the PNG or JPEG at `path`.
pub fn read_qr_text(path: &Path) -> Result<String, String> {
    let size = std::fs::metadata(path)
        .map_err(|_| "That file could not be opened. Check the path.".to_string())?
        .len();
    if size > MAX_FILE_BYTES {
        return Err("That file is too large to be a screenshot of a QR code.".to_string());
    }
    let mut reader = ImageReader::open(path)
        .map_err(|_| "That file could not be opened. Check the path.".to_string())?
        .with_guessed_format()
        .map_err(|_| "That file could not be read.".to_string())?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE_PIXELS);
    limits.max_image_height = Some(MAX_SIDE_PIXELS);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let gray = reader
        .decode()
        .map_err(|_| "That file is not a PNG or JPEG picture the signer can read.".to_string())?
        .into_luma8();

    let (width, height) = (gray.width() as usize, gray.height() as usize);
    let mut prepared = rqrr::PreparedImage::prepare_from_greyscale(width, height, |x, y| {
        gray.get_pixel(x as u32, y as u32).0[0]
    });
    for grid in prepared.detect_grids() {
        if let Ok((_, content)) = grid.decode() {
            return Ok(content);
        }
    }
    Err(
        "No QR code could be read in that picture. Use a sharp, full-size screenshot or photo \
         with the whole code and a clear border."
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GrayImage, Luma};
    use qrcode::{Color, EcLevel, QrCode};

    fn render(text: &str, scale: u32, border: u32, invert: bool) -> GrayImage {
        let code = QrCode::with_error_correction_level(text.as_bytes(), EcLevel::L).unwrap();
        let modules = code.width() as u32;
        let side = (modules + 2 * border) * scale;
        let (light, dark) = if invert { (0u8, 255u8) } else { (255u8, 0u8) };
        let mut picture = GrayImage::from_pixel(side, side, Luma([light]));
        for y in 0..modules {
            for x in 0..modules {
                if code.to_colors()[(y * modules + x) as usize] == Color::Dark {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            picture.put_pixel(
                                (x + border) * scale + dx,
                                (y + border) * scale + dy,
                                Luma([dark]),
                            );
                        }
                    }
                }
            }
        }
        picture
    }

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("hv-signer-{}-{name}", std::process::id()))
    }

    #[test]
    fn reads_a_request_from_png_screenshots_at_several_sizes() {
        let request = format!("HVTX1-REQ:{}", "0A0F0A0608B5B4A8D7061205".repeat(14));
        for scale in [2, 3, 5, 8] {
            let path = temp(&format!("shot{scale}.png"));
            render(&request, scale, 4, false).save(&path).unwrap();
            assert_eq!(read_qr_text(&path).unwrap(), request, "scale {scale}");
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn reads_a_jpeg_photo() {
        let request = "HVTX1-REQ:0A0F0A0608B5B4A8D706";
        let path = temp("photo.jpg");
        render(request, 6, 4, false).save(&path).unwrap();
        assert_eq!(read_qr_text(&path).unwrap(), request);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn refuses_pictures_without_a_code_and_files_that_are_not_pictures() {
        let blank = temp("blank.png");
        GrayImage::from_pixel(200, 200, Luma([255]))
            .save(&blank)
            .unwrap();
        assert!(read_qr_text(&blank).unwrap_err().contains("No QR code"));
        let _ = std::fs::remove_file(blank);

        let text = temp("notes.png");
        std::fs::write(&text, b"HVTX1-REQ:not a picture").unwrap();
        assert!(
            read_qr_text(&text)
                .unwrap_err()
                .contains("not a PNG or JPEG")
        );
        let _ = std::fs::remove_file(text);

        assert!(read_qr_text(Path::new("no-such-file.png")).is_err());
    }

    #[test]
    fn strips_quotes_from_dragged_paths() {
        assert_eq!(
            clean_path("  \"C:\\Users\\me\\shot.png\" "),
            "C:\\Users\\me\\shot.png"
        );
        assert_eq!(clean_path("'/home/me/shot.png'"), "/home/me/shot.png");
    }
}
