//! File rotation helpers.

/// A simple size-based rolling writer: rotates base file to base.1, shifts older to .2..keep, deletes oldest.
pub struct SizeRollingFile {
    dir: std::path::PathBuf,
    base_name: String,
    max_bytes: u64,
    keep: usize,
    file: std::fs::File,
    written: u64,
}

impl SizeRollingFile {
    fn base_path(&self) -> std::path::PathBuf {
        self.dir.join(&self.base_name)
    }
    fn open_base(dir: &std::path::Path, base_name: &str) -> std::io::Result<(std::fs::File, u64)> {
        let path = dir.join(base_name);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let sz = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        Ok((file, sz))
    }
    pub fn new(
        dir: std::path::PathBuf,
        base_name: String,
        max_bytes: u64,
        keep: usize,
    ) -> std::io::Result<Self> {
        std::fs::create_dir_all(&dir).ok();
        let (file, sz) = Self::open_base(&dir, &base_name)?;
        Ok(Self {
            dir,
            base_name,
            max_bytes,
            keep: keep.max(1),
            file,
            written: sz,
        })
    }
    fn rotate(&mut self) -> std::io::Result<()> {
        // Shift files: base.(keep-1) -> base.keep (delete), ..., base.1 -> base.2, base -> base.1
        if self.keep > 0 {
            let oldest = self.dir.join(format!("{}.{}", self.base_name, self.keep));
            let _ = std::fs::remove_file(&oldest);
            // Shift
            for idx in (1..=self.keep - 1).rev() {
                let from = self.dir.join(format!("{}.{}", self.base_name, idx));
                let to = self.dir.join(format!("{}.{}", self.base_name, idx + 1));
                if from.exists() {
                    let _ = std::fs::rename(&from, &to);
                }
            }
            // Move base -> .1
            let base = self.base_path();
            if base.exists() {
                let _ = std::fs::rename(&base, self.dir.join(format!("{}.1", self.base_name)));
            }
        }
        // Reopen base fresh
        let (f, _sz) = Self::open_base(&self.dir, &self.base_name)?;
        self.file = f;
        self.written = 0;
        Ok(())
    }
}

impl std::io::Write for SizeRollingFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // If write would exceed max_bytes, rotate first
        if self.written >= self.max_bytes || self.written + (buf.len() as u64) > self.max_bytes {
            self.rotate()?;
        }
        let n = self.file.write(buf)?;
        self.written += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn new_creates_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().to_path_buf();

        let _ = SizeRollingFile::new(path.clone(), "test.log".to_string(), 1024, 3).unwrap();

        assert!(path.join("test.log").exists());
    }

    #[test]
    fn write_under_threshold_no_rotation() {
        let dir = tempdir().unwrap();
        let path = dir.path().to_path_buf();

        let mut file = SizeRollingFile::new(path.clone(), "test.log".to_string(), 1024, 3).unwrap();
        file.write_all(b"hello world").unwrap();
        file.flush().unwrap();

        assert!(path.join("test.log").exists());
        assert!(!path.join("test.log.1").exists());
    }

    #[test]
    fn write_at_boundary_triggers_rotation() {
        let dir = tempdir().unwrap();
        let path = dir.path().to_path_buf();

        let mut file = SizeRollingFile::new(path.clone(), "test.log".to_string(), 20, 3).unwrap();
        file.write_all(b"this is first write!").unwrap();
        file.flush().unwrap();
        file.write_all(b"second write").unwrap();
        file.flush().unwrap();

        assert!(path.join("test.log").exists());
        assert!(path.join("test.log.1").exists());
    }

    #[test]
    fn rotate_shifts_files_correctly() {
        let dir = tempdir().unwrap();
        let path = dir.path().to_path_buf();

        let mut file = SizeRollingFile::new(path.clone(), "test.log".to_string(), 10, 3).unwrap();
        for i in 0..5 {
            file.write_all(format!("write-{:03}", i).as_bytes())
                .unwrap();
            file.flush().unwrap();
        }

        assert!(path.join("test.log").exists());
        assert!(path.join("test.log.1").exists());
    }

    #[test]
    fn retention_deletes_oldest_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().to_path_buf();

        let mut file = SizeRollingFile::new(path.clone(), "test.log".to_string(), 5, 2).unwrap();
        for i in 0..10 {
            file.write_all(format!("{:05}", i).as_bytes()).unwrap();
            file.flush().unwrap();
        }

        assert!(path.join("test.log").exists());
        assert!(!path.join("test.log.3").exists());
    }

    #[test]
    fn reopen_existing_file_resumes_written_bytes() {
        let dir = tempdir().unwrap();
        let path = dir.path().to_path_buf();

        {
            let mut file =
                SizeRollingFile::new(path.clone(), "test.log".to_string(), 100, 3).unwrap();
            file.write_all(b"initial content").unwrap();
            file.flush().unwrap();
        }

        {
            let mut file =
                SizeRollingFile::new(path.clone(), "test.log".to_string(), 100, 3).unwrap();
            file.write_all(b"more content").unwrap();
            file.flush().unwrap();
        }

        let content = std::fs::read_to_string(path.join("test.log")).unwrap();
        assert!(content.contains("initial content"));
        assert!(content.contains("more content"));
    }

    #[test]
    fn flush_writes_to_disk() {
        let dir = tempdir().unwrap();
        let path = dir.path().to_path_buf();

        let mut file = SizeRollingFile::new(path.clone(), "test.log".to_string(), 1024, 3).unwrap();
        file.write_all(b"test data").unwrap();
        file.flush().unwrap();

        let content = std::fs::read_to_string(path.join("test.log")).unwrap();
        assert_eq!(content, "test data");
    }

    #[test]
    fn base_path_returns_correct_path() {
        let dir = tempdir().unwrap();
        let path = dir.path().to_path_buf();

        let file = SizeRollingFile::new(path.clone(), "mylog.txt".to_string(), 1024, 3).unwrap();
        assert_eq!(file.base_path(), path.join("mylog.txt"));
    }

    #[test]
    fn keep_minimum_is_one() {
        let dir = tempdir().unwrap();
        let path = dir.path().to_path_buf();

        // Even if keep=0 is passed, it should be at least 1
        let file = SizeRollingFile::new(path.clone(), "test.log".to_string(), 1024, 0).unwrap();
        assert_eq!(file.keep, 1);
    }

    #[test]
    fn multiple_rotations_in_sequence() {
        let dir = tempdir().unwrap();
        let path = dir.path().to_path_buf();

        let mut file = SizeRollingFile::new(path.clone(), "test.log".to_string(), 10, 5).unwrap();

        // Write enough to trigger multiple rotations
        for i in 0..20 {
            file.write_all(format!("{:010}", i).as_bytes()).unwrap();
        }
        file.flush().unwrap();

        // Base file should exist
        assert!(path.join("test.log").exists());
        // At least some rotated files should exist
        assert!(path.join("test.log.1").exists());
    }

    #[test]
    fn empty_write_does_not_rotate() {
        let dir = tempdir().unwrap();
        let path = dir.path().to_path_buf();

        let mut file = SizeRollingFile::new(path.clone(), "test.log".to_string(), 10, 3).unwrap();
        file.write_all(b"").unwrap();
        file.flush().unwrap();

        assert!(path.join("test.log").exists());
        assert!(!path.join("test.log.1").exists());
    }
}
