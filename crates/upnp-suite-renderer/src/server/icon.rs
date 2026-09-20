//! Loads a user-supplied device icon file (PNG/JPEG/GIF) from disk for
//! UPnP's `iconList`/`icon` element. The file's dimensions and mimetype are
//! sniffed directly from its bytes (rather than trusted from its extension
//! or asked for separately) so the declared `width`/`height`/`mimetype`
//! always matches what's actually served.

use std::fmt;
use std::path::Path;

#[derive(Debug)]
pub enum IconError {
    Io(std::io::Error),
    UnrecognizedFormat,
}

impl fmt::Display for IconError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "failed to read icon file: {e}"),
            Self::UnrecognizedFormat => {
                write!(f, "icon file isn't a recognized PNG, JPEG, or GIF image")
            }
        }
    }
}

impl std::error::Error for IconError {}

impl From<std::io::Error> for IconError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// A device icon ready to serve: the raw file bytes plus what `device.xml`'s
/// `iconList` needs to describe it.
#[derive(Debug)]
pub struct DeviceIcon {
    pub bytes: Vec<u8>,
    pub mimetype: &'static str,
    /// File extension (with leading dot) matching `mimetype`, used to give
    /// the served URL a conventional-looking path (`/icon.png`) — some
    /// control points sniff an icon's type from the URL rather than trusting
    /// the HTTP `Content-Type`.
    pub extension: &'static str,
    pub width: u32,
    pub height: u32,
}

/// Reads `path` and sniffs its format/dimensions from its own bytes.
pub fn load_icon(path: &Path) -> Result<DeviceIcon, IconError> {
    let bytes = std::fs::read(path)?;
    let (mimetype, extension) = sniff_format(&bytes).ok_or(IconError::UnrecognizedFormat)?;
    let (width, height) = sniff_dimensions(&bytes).ok_or(IconError::UnrecognizedFormat)?;
    Ok(DeviceIcon {
        bytes,
        mimetype,
        extension,
        width,
        height,
    })
}

fn sniff_format(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]) {
        Some(("image/png", ".png"))
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(("image/jpeg", ".jpg"))
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(("image/gif", ".gif"))
    } else {
        None
    }
}

fn sniff_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]) {
        png_dimensions(bytes)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        jpeg_dimensions(bytes)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        gif_dimensions(bytes)
    } else {
        None
    }
}

/// PNG's `IHDR` chunk immediately follows the 8-byte signature (4-byte chunk
/// length + 4-byte "IHDR" type), then width/height as big-endian `u32`s.
fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let width = bytes.get(16..20)?;
    let height = bytes.get(20..24)?;
    Some((
        u32::from_be_bytes(width.try_into().ok()?),
        u32::from_be_bytes(height.try_into().ok()?),
    ))
}

/// Scans JPEG markers for a start-of-frame segment (any of the SOFn markers
/// except the DHT/JPG/DAC markers, which share the C0-CF range but aren't
/// frame headers) and reads its height/width fields — the only place JPEG
/// records pixel dimensions.
fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut pos = 2; // past the SOI marker (FFD8)
    while pos + 4 <= bytes.len() {
        if bytes[pos] != 0xFF {
            pos += 1;
            continue;
        }
        let marker = bytes[pos + 1];
        // Standalone markers (no length/payload follow).
        if marker == 0xD8 || marker == 0xD9 || (0xD0..=0xD7).contains(&marker) {
            pos += 2;
            continue;
        }
        let segment_len = u16::from_be_bytes(bytes.get(pos + 2..pos + 4)?.try_into().ok()?);
        let is_sof =
            (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC;
        if is_sof {
            let height = bytes.get(pos + 5..pos + 7)?;
            let width = bytes.get(pos + 7..pos + 9)?;
            return Some((
                u16::from_be_bytes(width.try_into().ok()?) as u32,
                u16::from_be_bytes(height.try_into().ok()?) as u32,
            ));
        }
        pos += 2 + segment_len as usize;
    }
    None
}

/// GIF's logical screen descriptor (width then height, little-endian `u16`s)
/// immediately follows the 6-byte "GIF87a"/"GIF89a" header.
fn gif_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let width = bytes.get(6..8)?;
    let height = bytes.get(8..10)?;
    Some((
        u16::from_le_bytes(width.try_into().ok()?) as u32,
        u16::from_le_bytes(height.try_into().ok()?) as u32,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A byte layout matching just enough of PNG's signature + IHDR chunk
    /// for `png_dimensions` to read — not a decodable image.
    fn fake_png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![137, 80, 78, 71, 13, 10, 26, 10]; // signature
        bytes.extend_from_slice(&13u32.to_be_bytes()); // IHDR chunk length
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes
    }

    fn fake_jpeg(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xD8]; // SOI
        bytes.extend_from_slice(&[0xFF, 0xC0]); // SOF0
        bytes.extend_from_slice(&17u16.to_be_bytes()); // segment length
        bytes.push(8); // precision
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.push(0); // rest of the segment, irrelevant to dimension sniffing
        bytes
    }

    fn fake_gif(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = b"GIF89a".to_vec();
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
        bytes
    }

    #[test]
    fn sniffs_png_format_and_dimensions() {
        let bytes = fake_png(120, 48);
        assert_eq!(sniff_format(&bytes), Some(("image/png", ".png")));
        assert_eq!(sniff_dimensions(&bytes), Some((120, 48)));
    }

    #[test]
    fn sniffs_jpeg_format_and_dimensions() {
        let bytes = fake_jpeg(64, 32);
        assert_eq!(sniff_format(&bytes), Some(("image/jpeg", ".jpg")));
        assert_eq!(sniff_dimensions(&bytes), Some((64, 32)));
    }

    #[test]
    fn sniffs_gif_format_and_dimensions() {
        let bytes = fake_gif(16, 16);
        assert_eq!(sniff_format(&bytes), Some(("image/gif", ".gif")));
        assert_eq!(sniff_dimensions(&bytes), Some((16, 16)));
    }

    #[test]
    fn rejects_unrecognized_bytes() {
        let bytes = b"not an image".to_vec();
        assert_eq!(sniff_format(&bytes), None);
        assert_eq!(sniff_dimensions(&bytes), None);
    }

    #[test]
    fn load_icon_reports_missing_file() {
        let err = load_icon(Path::new("/nonexistent/path/to/icon.png")).unwrap_err();
        assert!(matches!(err, IconError::Io(_)));
    }

    #[test]
    fn load_icon_reads_a_real_png_from_disk() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "upnp_receiver_test_icon_{}.png",
            std::process::id()
        ));
        std::fs::write(&path, fake_png(32, 32)).unwrap();

        let icon = load_icon(&path).unwrap();
        assert_eq!(icon.mimetype, "image/png");
        assert_eq!(icon.extension, ".png");
        assert_eq!(icon.width, 32);
        assert_eq!(icon.height, 32);

        std::fs::remove_file(&path).unwrap();
    }
}
