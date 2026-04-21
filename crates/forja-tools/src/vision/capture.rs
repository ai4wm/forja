use async_trait::async_trait;
#[cfg(not(feature = "vision"))]
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
#[cfg(feature = "vision")]
use image::imageops::crop_imm;
#[cfg(feature = "vision")]
use image::{DynamicImage, ImageFormat, RgbaImage};
#[cfg(feature = "vision")]
use std::io::Cursor;
#[cfg(feature = "vision")]
use xcap::Monitor;

#[async_trait]
pub trait ScreenCaptureBackend: Send + Sync + 'static {
    async fn capture_full(&self) -> std::result::Result<Vec<u8>, String>;
    async fn capture_region(
        &self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> std::result::Result<Vec<u8>, String>;
}

#[cfg(feature = "vision")]
pub struct XcapBackend;

#[cfg(feature = "vision")]
impl XcapBackend {
    pub fn new() -> Self {
        Self
    }

    fn capture_monitor_image(&self) -> std::result::Result<RgbaImage, String> {
        let monitor = Monitor::all()
            .map_err(|error| format!("Failed to enumerate monitors: {error}"))?
            .into_iter()
            .next()
            .ok_or_else(|| "No monitor available for capture".to_string())?;

        monitor
            .capture_image()
            .map_err(|error| format!("Failed to capture monitor image: {error}"))
    }
}

#[cfg(feature = "vision")]
impl Default for XcapBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "vision")]
#[async_trait]
impl ScreenCaptureBackend for XcapBackend {
    async fn capture_full(&self) -> std::result::Result<Vec<u8>, String> {
        encode_png(self.capture_monitor_image()?)
    }

    async fn capture_region(
        &self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> std::result::Result<Vec<u8>, String> {
        if x < 0 || y < 0 {
            return Err("Capture region coordinates must be non-negative".to_string());
        }

        let image = self.capture_monitor_image()?;
        let start_x = u32::try_from(x).map_err(|_| "Invalid x coordinate".to_string())?;
        let start_y = u32::try_from(y).map_err(|_| "Invalid y coordinate".to_string())?;
        let end_x = start_x
            .checked_add(width)
            .ok_or_else(|| "Capture region x overflow".to_string())?;
        let end_y = start_y
            .checked_add(height)
            .ok_or_else(|| "Capture region y overflow".to_string())?;

        if end_x > image.width() || end_y > image.height() {
            return Err("Capture region is out of screen bounds".to_string());
        }

        let cropped = crop_imm(&image, start_x, start_y, width, height).to_image();
        encode_png(cropped)
    }
}

pub(super) fn transparent_png() -> std::result::Result<Vec<u8>, String> {
    #[cfg(feature = "vision")]
    {
        let image = RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 0]));
        encode_png(image)
    }

    #[cfg(not(feature = "vision"))]
    {
        BASE64_STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADElEQVR4nGNgYGAAAAAEAAHIokmRAAAAAElFTkSuQmCC")
            .map_err(|error| format!("Failed to decode mock PNG: {error}"))
    }
}

#[cfg(feature = "vision")]
fn encode_png(image: RgbaImage) -> std::result::Result<Vec<u8>, String> {
    let mut buffer = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut buffer, ImageFormat::Png)
        .map_err(|error| format!("Failed to encode PNG: {error}"))?;

    Ok(buffer.into_inner())
}
