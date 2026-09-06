//! Keep ownership separate from the active merge queue. Moving a queue into a
//! fallible merge must not orphan its files, or the partially written output.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(crate) struct OwnedRuns {
    paths: BTreeSet<PathBuf>,
}

impl OwnedRuns {
    /// Register immediately after exclusive creation, before any fallible I/O.
    pub(crate) fn register(&mut self, path: &Path) {
        self.paths.insert(path.to_path_buf());
    }

    pub(crate) fn remove(&mut self, path: &Path) -> Result<(), String> {
        if !self.paths.contains(path) {
            return Err(format!(
                "refusing to remove unowned sort run: {}",
                path.display()
            ));
        }
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        self.paths.remove(path);
        Ok(())
    }
}

impl Drop for OwnedRuns {
    fn drop(&mut self) {
        // Best effort on exceptional exits. Successful merges remove runs
        // eagerly; failed removals remain registered for this final attempt.
        for path in &self.paths {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_preserves_unowned_files() {
        let dir = std::env::temp_dir().join(format!("turbo-owned-runs-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let owned = dir.join("owned.run");
        let unowned = dir.join("input.bam");
        fs::write(&owned, b"partial output").unwrap();
        fs::write(&unowned, b"input").unwrap();
        {
            let mut runs = OwnedRuns::default();
            runs.register(&owned);
            assert!(runs.remove(&unowned).is_err());
        }
        assert!(!owned.exists());
        assert_eq!(fs::read(&unowned).unwrap(), b"input");
        fs::remove_file(&unowned).unwrap();
        fs::remove_dir(&dir).unwrap();
    }
}
