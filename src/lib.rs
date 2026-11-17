//! SigmaOS-apt-file — Debian Contents-index file→package search.
//!
//! Inspired by Debian's `apt-file`: searches the `Contents` index (which maps
//! every shipped file path to the package(s) containing it) so users can ask
//! "which package provides /usr/bin/foo?". Parses `Contents-<arch>` stanza
//! blocks (`Package`, `Path` continuation lists) into an index and answers
//! path/glob lookups.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::BTreeMap;

/// Errors raised by this component.
#[derive(Debug, thiserror::Error)]
pub enum ComponentError {
    /// A stanza was missing its `Package` field.
    #[error("stanza missing Package field")]
    MissingPackage,
}

/// Result alias for component operations.
pub type ComponentResult<T> = Result<T, ComponentError>;

/// One `Contents` stanza: a package and the file paths it ships.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackageFiles {
    /// Source/binary package name.
    pub package: String,
    /// File paths shipped by the package (sorted, deduplicated).
    pub paths: Vec<String>,
}

/// The Contents index: package → paths, plus a path → packages reverse map.
#[derive(Debug, Default)]
pub struct ContentsIndex {
    by_package: BTreeMap<String, Vec<String>>,
    by_path: BTreeMap<String, Vec<String>>,
}

impl ContentsIndex {
    /// Empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse `Contents-<arch>` text. Records are deb822-like stanzas:
    /// `Package: name` then ` ` -continued `Path` lines listing files.
    pub fn parse(input: &str) -> ComponentResult<Self> {
        let mut idx = ContentsIndex::new();
        let mut cur_name: Option<String> = None;
        let mut cur_paths: Vec<String> = Vec::new();
        let mut last_was_path = false;

        let mut flush = |name: &mut Option<String>, paths: &mut Vec<String>| {
            if let Some(n) = name.take() {
                paths.sort();
                paths.dedup();
                for p in paths.iter() {
                    idx.by_path.entry(p.clone()).or_default().push(n.clone());
                }
                idx.by_package.insert(n, std::mem::take(paths));
            } else {
                paths.clear();
            }
        };

        for raw in input.lines() {
            let line = raw.trim_end();
            if line.trim().is_empty() {
                last_was_path = false;
                continue;
            }
            if let Some(rest) = line.strip_prefix("Package:") {
                flush(&mut cur_name, &mut cur_paths);
                cur_name = Some(rest.trim().to_string());
                last_was_path = false;
            } else if line.starts_with(' ') || line.starts_with('\t') {
                // Continuation: a path line inside a Path field.
                let p = line.trim();
                if !p.is_empty() && !p.starts_with("Path:") {
                    cur_paths.push(p.to_string());
                    last_was_path = true;
                }
            } else if line.starts_with("Path:") {
                last_was_path = true;
            } else {
                last_was_path = false;
            }
        }
        flush(&mut cur_name, &mut cur_paths);
        Ok(idx)
    }

    /// All packages in the index.
    pub fn packages(&self) -> Vec<&str> {
        self.by_package.keys().map(String::as_str).collect()
    }

    /// Packages that ship exactly `path`.
    pub fn owners(&self, path: &str) -> Vec<&str> {
        self.by_path
            .get(path)
            .map(|v| v.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }

    /// Paths shipped by `package`.
    pub fn files_of(&self, package: &str) -> &[String] {
        self.by_package.get(package).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Packages owning any path containing `needle` (case-sensitive substring).
    pub fn search(&self, needle: &str) -> Vec<(&str, Vec<&str>)> {
        let mut out: Vec<(&str, Vec<&str>)> = Vec::new();
        for (path, pkgs) in &self.by_path {
            if path.contains(needle) {
                out.push((path.as_str(), pkgs.iter().map(String::as_str).collect()));
            }
        }
        out
    }

    /// Number of indexed packages.
    pub fn len(&self) -> usize {
        self.by_package.len()
    }

    /// Whether empty.
    pub fn is_empty(&self) -> bool {
        self.by_package.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTENTS: &str = "Package: bash\n /bin/bash\n /bin/sh\n\nPackage: coreutils\n /bin/ls\n /bin/cat\n /usr/bin/dir\n\nPackage: zlib1g\n /lib/x86_64-linux-gnu/libz.so.1\n";

    #[test]
    fn parses_packages_and_paths() {
        let idx = ContentsIndex::parse(CONTENTS).unwrap();
        assert_eq!(idx.len(), 3);
        assert_eq!(idx.packages(), vec!["bash", "coreutils", "zlib1g"]);
        assert_eq!(idx.files_of("bash"), &["/bin/bash", "/bin/sh"]);
    }

    #[test]
    fn owners_lookup() {
        let idx = ContentsIndex::parse(CONTENTS).unwrap();
        assert_eq!(idx.owners("/bin/ls"), vec!["coreutils"]);
        assert!(idx.owners("/nonexistent").is_empty());
    }

    #[test]
    fn substring_search() {
        let idx = ContentsIndex::parse(CONTENTS).unwrap();
        let hits = idx.search("/bin/");
        // /bin/bash,/bin/sh (bash); /bin/ls,/bin/cat (coreutils); /usr/bin/dir no
        assert!(hits.iter().any(|(p, _)| *p == "/bin/ls"));
        assert!(hits.iter().any(|(p, _)| *p == "/bin/bash"));
    }

    #[test]
    fn empty_ok() {
        assert!(ContentsIndex::parse("").unwrap().is_empty());
    }
}
