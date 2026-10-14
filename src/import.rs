//! Bounded, deterministic audio discovery. Run on an I/O thread, never in the UI.
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

pub const AUDIO_EXTENSIONS: &[&str] = &["mp3", "wav", "flac", "ogg", "opus", "m4a", "aac"];
pub const MAX_FILES: usize = 1_000;
const MAX_ENTRIES: usize = 100_000;
const MAX_ISSUES: usize = 8;

#[derive(Default)]
pub struct ImportReport {
    pub files: Vec<PathBuf>,
    pub skipped: usize,
    pub issues: Vec<String>,
    pub truncated: bool,
}

impl ImportReport {
    fn issue(&mut self, path: &Path, error: impl std::fmt::Display) {
        self.skipped += 1;
        if self.issues.len() < MAX_ISSUES {
            self.issues.push(format!("{}: {error}", path.display()));
        }
    }
}

pub fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            AUDIO_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

/// Folder symlinks/junctions are skipped to avoid cycles and leaving the selected tree.
/// Canonical paths deduplicate repeated files and overlapping folder selections.
pub fn discover(paths: Vec<PathBuf>, recursive: bool) -> ImportReport {
    let mut report = ImportReport::default();
    let mut seen = HashSet::new();
    let mut folders = HashSet::new();
    let mut stack = paths.into_iter().rev().collect::<Vec<_>>();
    let mut visited = 0;
    let mut scanned_entries = 0;
    while let Some(path) = stack.pop() {
        if visited == MAX_ENTRIES || report.files.len() == MAX_FILES {
            report.truncated = true;
            break;
        }
        visited += 1;
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                report.issue(&path, error);
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            report.skipped += 1;
            continue;
        }
        if metadata.is_dir() {
            let canonical = match path.canonicalize() {
                Ok(path) => path,
                Err(error) => {
                    report.issue(&path, error);
                    continue;
                }
            };
            if !folders.insert(canonical) {
                report.skipped += 1;
                continue;
            }
            let entries = match fs::read_dir(&path) {
                Ok(entries) => entries,
                Err(error) => {
                    report.issue(&path, error);
                    continue;
                }
            };
            let mut children = Vec::new();
            for entry in entries {
                if scanned_entries == MAX_ENTRIES {
                    report.truncated = true;
                    break;
                }
                scanned_entries += 1;
                match entry {
                    Ok(entry) => match entry.file_type() {
                        Ok(kind) if !kind.is_dir() || recursive => children.push(entry.path()),
                        Ok(_) => {}
                        Err(error) => report.issue(&entry.path(), error),
                    },
                    Err(error) => report.issue(&path, error),
                }
            }
            children.sort();
            stack.extend(children.into_iter().rev());
        } else if metadata.is_file() && is_audio(&path) {
            match path.canonicalize() {
                Ok(path) if seen.insert(path.clone()) => report.files.push(path),
                Ok(_) => report.skipped += 1,
                Err(error) => report.issue(&path, error),
            }
        } else {
            report.skipped += 1;
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_filter_sort_recurse_and_deduplicate() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("nested");
        fs::create_dir(&nested).unwrap();
        for name in ["b.WAV", "a.opus", "notes.txt", "nested/c.mp3"] {
            fs::write(root.path().join(name), []).unwrap();
        }
        let flat = discover(vec![root.path().into()], false);
        assert_eq!(flat.files.len(), 2);
        assert_eq!(flat.files[0].file_name().unwrap(), "a.opus");
        assert_eq!(flat.files[1].file_name().unwrap(), "b.WAV");
        let recursive = discover(
            vec![root.path().into(), nested, root.path().join("a.opus")],
            true,
        );
        assert_eq!(recursive.files.len(), 3);
        assert_eq!(recursive.skipped, 3); // text file, repeated folder, repeated audio
    }

    #[test]
    fn missing_empty_and_unsupported_sources_are_reported() {
        let root = tempfile::tempdir().unwrap();
        assert!(discover(vec![root.path().into()], true).files.is_empty());
        fs::write(root.path().join("notes.txt"), []).unwrap();
        let report = discover(vec![root.path().join("missing"), root.path().into()], true);
        assert!(report.files.is_empty());
        assert_eq!(report.skipped, 2);
        assert_eq!(report.issues.len(), 1);
        assert!(report.issues[0].contains("missing"));
    }

    #[test]
    fn discovery_caps_large_batches() {
        let root = tempfile::tempdir().unwrap();
        for index in 0..MAX_FILES + 2 {
            fs::write(root.path().join(format!("{index:04}.wav")), []).unwrap();
        }
        let report = discover(vec![root.path().into()], false);
        assert_eq!(report.files.len(), MAX_FILES);
        assert!(report.truncated);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_cycles_are_skipped() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("audio.wav"), []).unwrap();
        std::os::unix::fs::symlink(root.path(), root.path().join("loop")).unwrap();
        let report = discover(vec![root.path().into()], true);
        assert_eq!(report.files.len(), 1);
        assert_eq!(report.skipped, 1);
    }
}
