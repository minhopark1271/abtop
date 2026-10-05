use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::SystemTime;

/// Codex persists `/rename` entries in an append-only session index, separate
/// from rollout logs. Refresh it independently so idle sessions can be renamed.
#[derive(Default)]
pub(super) struct SessionNameIndex {
    fingerprint: Option<(SystemTime, u64)>,
    names: HashMap<String, String>,
}

impl SessionNameIndex {
    pub(super) fn refresh(&mut self, path: &Path) {
        let Ok(metadata) = fs::symlink_metadata(path) else {
            self.fingerprint = None;
            self.names.clear();
            return;
        };
        if !metadata.is_file() {
            self.fingerprint = None;
            self.names.clear();
            return;
        }
        let fingerprint = metadata
            .modified()
            .ok()
            .map(|mtime| (mtime, metadata.len()));
        if fingerprint.is_some() && fingerprint == self.fingerprint {
            return;
        }
        let Ok(file) = fs::File::open(path) else {
            return;
        };
        let mut names = HashMap::new();
        for line in BufReader::new(file).lines() {
            let Ok(line) = line else {
                return;
            };
            let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line) else {
                // A concurrent append may leave an incomplete final line.
                continue;
            };
            let (Some(id), Some(name)) = (entry["id"].as_str(), entry["thread_name"].as_str())
            else {
                continue;
            };
            if id.is_empty() {
                continue;
            }
            let name = super::sanitize_terminal_text(name);
            let name = name.trim();
            // The last record for a session wins, including an empty rename.
            if name.is_empty() {
                names.remove(id);
            } else {
                names.insert(id.to_string(), name.to_string());
            }
        }
        self.names = names;
        self.fingerprint = fingerprint;
    }

    pub(super) fn get(&self, session_id: &str) -> Option<String> {
        self.names.get(session_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn latest_rename_wins_without_losing_other_sessions() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(file, r#"{{"id":"a","thread_name":"Old name"}}"#).unwrap();
        writeln!(file, r#"{{"id":"b","thread_name":"Other session"}}"#).unwrap();
        writeln!(file, "not json").unwrap();
        writeln!(file, r#"{{"id":"a","thread_name":"New name"}}"#).unwrap();
        writeln!(file, r#"{{"id":"a","unrelated":"ignored"}}"#).unwrap();

        let mut index = SessionNameIndex::default();
        index.refresh(file.path());
        assert_eq!(index.get("a").as_deref(), Some("New name"));
        assert_eq!(index.get("b").as_deref(), Some("Other session"));
        assert_eq!(index.get("unknown"), None);
    }

    #[test]
    fn refresh_tracks_partial_appends_rewrites_and_deletion() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(file, r#"{{"id":"a","thread_name":"Original"}}"#).unwrap();
        let mut index = SessionNameIndex::default();
        index.refresh(file.path());
        assert_eq!(index.get("a").as_deref(), Some("Original"));

        write!(file, r#"{{"id":"a","thread_name":"Renamed"#).unwrap();
        index.refresh(file.path());
        assert_eq!(index.get("a").as_deref(), Some("Original"));
        writeln!(file, r#" session"}}"#).unwrap();
        index.refresh(file.path());
        assert_eq!(index.get("a").as_deref(), Some("Renamed session"));

        fs::write(file.path(), "").unwrap();
        index.refresh(file.path());
        assert_eq!(index.get("a"), None);

        fs::write(file.path(), r#"{"id":"b","thread_name":"Restored"}"#).unwrap();
        index.refresh(file.path());
        assert_eq!(index.get("b").as_deref(), Some("Restored"));
        fs::remove_file(file.path()).unwrap();
        index.refresh(file.path());
        assert_eq!(index.get("b"), None);
    }

    #[test]
    fn names_are_terminal_safe_and_empty_names_restore_the_fallback() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(file, r#"{{"id":"a","thread_name":"Old name"}}"#).unwrap();
        writeln!(file, r#"{{"id":"a","thread_name":" \n\t "}}"#).unwrap();
        writeln!(
            file,
            r#"{{"id":"b","thread_name":"  Fix \u03bb\n\u001b\u202e\u2066  "}}"#
        )
        .unwrap();
        let mut index = SessionNameIndex::default();
        index.refresh(file.path());
        assert_eq!(index.get("a"), None);
        assert_eq!(index.get("b").as_deref(), Some("Fix \u{03bb}"));
    }
}
