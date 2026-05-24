use anyhow::{Context, Result};

/// Write UTF-8 text to the system clipboard.
pub fn set_text(text: String) -> Result<()> {
    let mut cb = arboard::Clipboard::new().context("open clipboard")?;
    cb.set_text(text).context("set text")?;
    Ok(())
}

/// Decode PNG bytes and write as a bitmap to the system clipboard.
pub fn set_image(png_bytes: Vec<u8>) -> Result<()> {
    let img = image::load_from_memory(&png_bytes)
        .context("decode PNG")?
        .to_rgba8();

    let (width, height) = img.dimensions();
    let pixels = img.into_raw(); // RGBA u8 vec

    let img_data = arboard::ImageData {
        width: width as usize,
        height: height as usize,
        bytes: pixels.into(),
    };

    let mut cb = arboard::Clipboard::new().context("open clipboard")?;
    cb.set_image(img_data).context("set image")?;
    Ok(())
}
