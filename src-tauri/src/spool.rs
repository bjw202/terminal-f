//! Per-session output spool file (M2.2, roadmap §2.5).
//!
//! The in-memory ring buffer is bounded (1 MiB, oldest-drop) so a slow or
//! reconnecting control-API broker would miss data. For panes the user marks
//! `allowObserve`, the reader thread *also* appends decoded output to a spool
//! file. The control API serves observation from this file using a byte
//! offset as the cursor, so a broker never silently loses data within the
//! spool cap.
//!
//! The spool is bounded too: once `cap_bytes` is reached, further output is
//! dropped and `full` is set (surfaced to the broker). It is not a
//! full-fidelity archive beyond the cap — that is an explicit limit.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Default spool cap per session.
pub const SPOOL_CAP_BYTES: u64 = 16 * 1024 * 1024;

pub struct SpoolWriter {
    file: File,
    path: PathBuf,
    written: u64,
    cap: u64,
    full: bool,
}

impl SpoolWriter {
    pub fn create(path: &Path, cap: u64) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Truncate: a session's spool starts fresh (session ids are unique).
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;
        Ok(Self {
            file,
            path: path.to_path_buf(),
            written: 0,
            cap,
            full: false,
        })
    }

    /// Append bytes, stopping at the cap. Returns bytes actually written.
    pub fn append(&mut self, bytes: &[u8]) -> usize {
        if self.full || bytes.is_empty() {
            return 0;
        }
        let remaining = self.cap.saturating_sub(self.written);
        let take = (bytes.len() as u64).min(remaining) as usize;
        if take == 0 {
            self.full = true;
            return 0;
        }
        // Best-effort: a write error just stops spooling for this session.
        if self.file.write_all(&bytes[..take]).is_err() {
            self.full = true;
            return 0;
        }
        let _ = self.file.flush();
        self.written += take as u64;
        if take < bytes.len() {
            self.full = true;
        }
        take
    }

    pub fn len(&self) -> u64 {
        self.written
    }

    pub fn is_full(&self) -> bool {
        self.full
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Result of reading a slice of a spool from a byte offset.
pub struct SpoolRead {
    pub data: String,
    /// New cursor (byte offset) for the next read.
    pub offset: u64,
    /// Total bytes currently in the spool.
    pub total: u64,
}

/// Read up to `max` bytes from `path` starting at byte `from`. UTF-8 is
/// decoded lossily at the slice boundary (brokers get text; exactness of a
/// split multibyte char across reads is not guaranteed — acceptable for
/// human/LLM consumption).
pub fn read_spool(path: &Path, from: u64, max: usize) -> std::io::Result<SpoolRead> {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SpoolRead {
                data: String::new(),
                offset: 0,
                total: 0,
            })
        }
        Err(e) => return Err(e),
    };
    let total = file.metadata()?.len();
    let start = from.min(total);
    file.seek(SeekFrom::Start(start))?;
    let want = ((total - start) as usize).min(max);
    let mut buf = vec![0u8; want];
    let n = file.read(&mut buf)?;
    buf.truncate(n);
    Ok(SpoolRead {
        data: String::from_utf8_lossy(&buf).into_owned(),
        offset: start + n as u64,
        total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        std::env::temp_dir().join(format!("termf-spool-{}", crate::model::new_id())).join("s.log")
    }

    #[test]
    fn append_and_read_by_offset() {
        let path = tmp();
        let mut w = SpoolWriter::create(&path, SPOOL_CAP_BYTES).unwrap();
        w.append(b"hello ");
        w.append(b"world");
        assert_eq!(w.len(), 11);

        let r = read_spool(&path, 0, 1024).unwrap();
        assert_eq!(r.data, "hello world");
        assert_eq!(r.offset, 11);
        assert_eq!(r.total, 11);

        // incremental read from a cursor
        let r2 = read_spool(&path, 6, 1024).unwrap();
        assert_eq!(r2.data, "world");
        assert_eq!(r2.offset, 11);

        // reading at EOF returns empty, offset unchanged
        let r3 = read_spool(&path, 11, 1024).unwrap();
        assert_eq!(r3.data, "");
        assert_eq!(r3.offset, 11);

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn respects_cap_and_reports_full() {
        let path = tmp();
        let mut w = SpoolWriter::create(&path, 10).unwrap();
        assert_eq!(w.append(b"12345"), 5);
        assert!(!w.is_full());
        assert_eq!(w.append(b"67890EXTRA"), 5, "only 5 bytes fit under cap 10");
        assert!(w.is_full());
        assert_eq!(w.append(b"more"), 0, "no writes once full");
        assert_eq!(w.len(), 10);
        let r = read_spool(&path, 0, 1024).unwrap();
        assert_eq!(r.data, "1234567890");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn read_max_bounds_slice() {
        let path = tmp();
        let mut w = SpoolWriter::create(&path, SPOOL_CAP_BYTES).unwrap();
        w.append(b"abcdefghij");
        let r = read_spool(&path, 0, 4).unwrap();
        assert_eq!(r.data, "abcd");
        assert_eq!(r.offset, 4);
        assert_eq!(r.total, 10);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn read_missing_file_is_empty() {
        let r = read_spool(Path::new("Z:/nope/x.log"), 0, 100).unwrap();
        assert_eq!(r.total, 0);
        assert_eq!(r.data, "");
    }
}
