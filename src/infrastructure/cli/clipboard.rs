use arboard::{Clipboard, ImageData};
use base64::{engine::general_purpose, Engine as _};

#[derive(Debug)]
#[allow(dead_code)]
pub enum ClipboardError {
    NotAvailable(String),
    NotAnImage,
    UnsupportedFormat,
    ReadError(String),
}

pub struct ClipboardReader {
    clipboard: Clipboard,
}

impl ClipboardReader {
    pub fn new() -> Result<Self, ClipboardError> {
        Clipboard::new()
            .map(|clipboard| Self { clipboard })
            .map_err(|e| ClipboardError::NotAvailable(e.to_string()))
    }

    /// Try to read an image from clipboard
    /// Returns (base64_data, mime_type, size_bytes)
    pub fn read_image(&mut self) -> Result<(String, String, usize), ClipboardError> {
        let image_data = self
            .clipboard
            .get_image()
            .map_err(|_| ClipboardError::NotAnImage)?;

        // Convert arboard ImageData to PNG bytes
        let png_bytes = self.image_data_to_png(&image_data)?;
        let size = png_bytes.len();

        // Base64 encode
        let base64_data = general_purpose::STANDARD.encode(&png_bytes);

        Ok((base64_data, "image/png".to_string(), size))
    }

    fn image_data_to_png(&self, image_data: &ImageData) -> Result<Vec<u8>, ClipboardError> {
        // Convert RGBA bytes to PNG using image crate
        let width = image_data.width;
        let height = image_data.height;
        let bytes = &image_data.bytes;

        let img = image::RgbaImage::from_raw(width as u32, height as u32, bytes.to_vec()).ok_or(
            ClipboardError::ReadError("Invalid image dimensions".to_string()),
        )?;

        let mut png_bytes = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .map_err(|e| ClipboardError::ReadError(e.to_string()))?;

        Ok(png_bytes)
    }

    pub fn get_text(&mut self) -> Result<String, ClipboardError> {
        self.clipboard
            .get_text()
            .map_err(|e| ClipboardError::ReadError(e.to_string()))
    }

    /// Try to read file paths from clipboard (when files are copied in Finder)
    pub fn get_files(&mut self) -> Result<Vec<std::path::PathBuf>, ClipboardError> {
        self.clipboard
            .get()
            .file_list()
            .map_err(|e| ClipboardError::ReadError(format!("Failed to read file list: {}", e)))
    }
}

pub fn format_size(bytes: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = KB * 1024;

    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{} KB", bytes / KB)
    } else {
        format!("{} bytes", bytes)
    }
}
