//! Clipboard image paste bridge (ADR-010).
//!
//! Windows terminals forward only clipboard *text* to the PTY on Ctrl+V, so a
//! TUI like Claude Code never sees a screenshot. terminal-f bridges this: the
//! frontend catches an image paste, the image lands here as bytes, we persist
//! it under `<config>/paste/`, and the frontend pastes the file *path* into
//! the pane (bracketed) — the same shape as dragging an image file onto a
//! terminal, which image-aware TUIs already handle.

use std::fs;
use std::path::{Path, PathBuf};

/// Keep at most this many pasted images; oldest are pruned on each save.
pub const PASTE_KEEP: usize = 20;
/// Reject absurd payloads (a 4K screenshot PNG is ~10-20 MB at worst).
pub const PASTE_MAX_BYTES: usize = 32 * 1024 * 1024;

pub fn ext_for_mime(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        _ => "png", // Windows screenshots arrive as image/png
    }
}

/// Persist pasted image bytes as `img-<stamp>.<ext>` in `dir`, pruning old
/// pastes so the folder stays bounded. Returns the written path.
pub fn save_paste_bytes(
    dir: &Path,
    bytes: &[u8],
    ext: &str,
    stamp: u64,
) -> Result<PathBuf, String> {
    if bytes.is_empty() {
        return Err("pasted image is empty".into());
    }
    if bytes.len() > PASTE_MAX_BYTES {
        return Err(format!(
            "pasted image too large ({} bytes; max {PASTE_MAX_BYTES})",
            bytes.len()
        ));
    }
    fs::create_dir_all(dir).map_err(|e| format!("paste dir error: {e}"))?;
    prune(dir, PASTE_KEEP.saturating_sub(1));
    // Same-millisecond pastes: suffix a counter instead of overwriting.
    let mut path = dir.join(format!("img-{stamp}.{ext}"));
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("img-{stamp}-{n}.{ext}"));
        n += 1;
    }
    fs::write(&path, bytes).map_err(|e| format!("paste write error: {e}"))?;
    Ok(path)
}

/// Remove oldest `img-*` files beyond `keep`. Names embed the timestamp, so
/// lexicographic-by-length-then-name ordering matches age ordering.
fn prune(dir: &Path, keep: usize) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("img-"))
        })
        .collect();
    files.sort_by(|a, b| {
        let (an, bn) = (name(a), name(b));
        an.len().cmp(&bn.len()).then_with(|| an.cmp(&bn))
    });
    while files.len() > keep {
        let _ = fs::remove_file(files.remove(0));
    }
}

fn name(p: &Path) -> &str {
    p.file_name().and_then(|n| n.to_str()).unwrap_or("")
}

/// Encode raw RGBA8 pixels as a PNG byte stream (clipboard images arrive as
/// RGBA from arboard; the pasted file must be a real image format).
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    if (width as usize) * (height as usize) * 4 != rgba.len() {
        return Err(format!(
            "clipboard image size mismatch: {width}x{height} vs {} bytes",
            rgba.len()
        ));
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc
            .write_header()
            .map_err(|e| format!("png encode error: {e}"))?;
        writer
            .write_image_data(rgba)
            .map_err(|e| format!("png encode error: {e}"))?;
    }
    Ok(out)
}

/// What the OS clipboard currently holds, for the paste bridge (ADR-010).
#[derive(Debug, serde::Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "camelCase")]
pub enum PasteContent {
    /// Plain text — the frontend pastes it as-is (bracketed by xterm).
    Text(String),
    /// A clipboard image, persisted to disk — frontend pastes the file path.
    ImagePath(String),
    /// Nothing usable on the clipboard.
    None,
}

/// Write plain text to the OS clipboard — the smart-copy path (Ctrl+C with a
/// selection, Ctrl+Shift+C, right-click copy). Writing in Rust via arboard,
/// like `read_clipboard`, sidesteps WebView clipboard permission/focus prompts.
pub fn write_clipboard_text(text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("clipboard error: {e}"))?;
    cb.set_text(text.to_owned())
        .map_err(|e| format!("clipboard set error: {e}"))
}

/// Read the OS clipboard directly (no browser events involved): text wins,
/// else an image is persisted under `dir` and its path returned. Reading in
/// Rust avoids both xterm's Ctrl+V interception and WebView clipboard
/// permission prompts.
pub fn read_clipboard(dir: &Path, stamp: u64) -> Result<PasteContent, String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("clipboard error: {e}"))?;
    if let Ok(text) = cb.get_text() {
        if !text.is_empty() {
            return Ok(PasteContent::Text(text));
        }
    }
    if let Ok(img) = cb.get_image() {
        let bytes = encode_png(img.width as u32, img.height as u32, &img.bytes)?;
        let path = save_paste_bytes(dir, &bytes, "png", stamp)?;
        return Ok(PasteContent::ImagePath(path.to_string_lossy().into_owned()));
    }
    Ok(PasteContent::None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes tests that touch the OS clipboard. arboard on Windows uses
    /// the Win32 clipboard (OLE) which has thread affinity; two clipboard tests
    /// running on separate harness threads at once corrupts the heap
    /// (STATUS_HEAP_CORRUPTION). Every clipboard test takes this lock first.
    static CLIPBOARD_LOCK: Mutex<()> = Mutex::new(());

    fn tmp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("termf-paste-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn saves_and_returns_path() {
        let dir = tmp();
        let p = save_paste_bytes(&dir, b"\x89PNG-ish", "png", 1000).unwrap();
        assert!(p.exists());
        assert_eq!(fs::read(&p).unwrap(), b"\x89PNG-ish");
        assert!(name(&p).starts_with("img-1000"));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn same_stamp_does_not_overwrite() {
        let dir = tmp();
        let a = save_paste_bytes(&dir, b"a", "png", 5).unwrap();
        let b = save_paste_bytes(&dir, b"b", "png", 5).unwrap();
        assert_ne!(a, b);
        assert_eq!(fs::read(&a).unwrap(), b"a");
        assert_eq!(fs::read(&b).unwrap(), b"b");
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn rejects_empty_and_oversized() {
        let dir = tmp();
        assert!(save_paste_bytes(&dir, b"", "png", 1).is_err());
        let big = vec![0u8; PASTE_MAX_BYTES + 1];
        assert!(save_paste_bytes(&dir, &big, "png", 1).is_err());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn prunes_oldest_beyond_cap() {
        let dir = tmp();
        for i in 0..(PASTE_KEEP as u64 + 5) {
            save_paste_bytes(&dir, b"x", "png", i).unwrap();
        }
        let mut names: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), PASTE_KEEP);
        // oldest stamps (0..5) were pruned
        names.sort();
        assert!(!names.iter().any(|n| n.starts_with("img-0.")));
        assert!(names.iter().any(|n| n.starts_with(&format!("img-{}.", PASTE_KEEP as u64 + 4))));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn encode_png_roundtrips() {
        // 2x2 opaque red
        let rgba = [255u8, 0, 0, 255].repeat(4);
        let bytes = encode_png(2, 2, &rgba).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let decoder = png::Decoder::new(&bytes[..]);
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (2, 2));
        assert_eq!(&buf[..info.buffer_size()], &rgba[..]);
    }

    #[test]
    fn encode_png_rejects_size_mismatch() {
        assert!(encode_png(2, 2, &[0u8; 3]).is_err());
    }

    /// Round-trips real OS clipboard text through read_clipboard, restoring
    /// the user's clipboard afterwards. Serialized implicitly (cargo test
    /// runs threads, but no other test touches the clipboard).
    #[test]
    fn read_clipboard_prefers_text() {
        let _guard = CLIPBOARD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tmp();
        let mut cb = match arboard::Clipboard::new() {
            Ok(cb) => cb,
            Err(_) => return, // headless CI without a clipboard: skip
        };
        let saved = cb.get_text().ok();
        cb.set_text("TERMF_CLIP_TEST").unwrap();
        let got = read_clipboard(&dir, 1).unwrap();
        // restore before asserting so a failure doesn't clobber the user
        match &saved {
            Some(t) => cb.set_text(t.clone()).ok(),
            None => cb.clear().ok().map(|_| ()),
        };
        match got {
            PasteContent::Text(t) => assert_eq!(t, "TERMF_CLIP_TEST"),
            other => panic!("expected text, got {other:?}"),
        }
        fs::remove_dir_all(dir).ok();
    }

    /// Round-trips text through write_clipboard_text -> read_clipboard,
    /// restoring the user's clipboard afterwards (same discipline as
    /// read_clipboard_prefers_text).
    #[test]
    fn write_then_read_round_trips_text() {
        let _guard = CLIPBOARD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tmp();
        let mut cb = match arboard::Clipboard::new() {
            Ok(cb) => cb,
            Err(_) => return, // headless CI without a clipboard: skip
        };
        let saved = cb.get_text().ok();
        drop(cb); // release before write_clipboard_text opens its own handle
        write_clipboard_text("TERMF_COPY_TEST").unwrap();
        let got = read_clipboard(&dir, 1).unwrap();
        // restore before asserting so a failure doesn't clobber the user
        let mut cb = arboard::Clipboard::new().unwrap();
        match &saved {
            Some(t) => cb.set_text(t.clone()).ok(),
            None => cb.clear().ok().map(|_| ()),
        };
        match got {
            PasteContent::Text(t) => assert_eq!(t, "TERMF_COPY_TEST"),
            other => panic!("expected text, got {other:?}"),
        }
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn mime_mapping() {
        assert_eq!(ext_for_mime("image/png"), "png");
        assert_eq!(ext_for_mime("image/jpeg"), "jpg");
        assert_eq!(ext_for_mime("anything"), "png");
    }
}
