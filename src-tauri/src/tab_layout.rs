//! Tab layout persistence: which tabs a window had, in order, and which one was active.
//!
//! Stored in `<app_data>/tab-layout.json` rather than `prefs.json` so a corrupted renderer pref
//! cannot lose the tab list. Restore is deliberately lazy: every saved tab is registered, but
//! only the active one is started, so reopening the app does not spawn N agent processes.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabSpec {
    pub cwd: String,
    #[serde(default, rename = "sessionPath")]
    pub session_path: Option<String>,
    #[serde(default = "default_kind")]
    pub kind: String,
}

fn default_kind() -> String {
    "agent".to_owned()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedLayout {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub tabs: Vec<TabSpec>,
    #[serde(default, rename = "activeIndex")]
    pub active_index: usize,
}

fn default_version() -> u32 {
    1
}

impl Default for PersistedLayout {
    fn default() -> Self {
        PersistedLayout {
            version: 1,
            tabs: Vec::new(),
            active_index: 0,
        }
    }
}

impl PersistedLayout {
    pub fn path(data_dir: &Path) -> PathBuf {
        data_dir.join("tab-layout.json")
    }

    /// Read the saved layout, dropping everything that no longer resolves on disk: a tab whose
    /// workspace directory is gone, and a session file that was deleted. A layout that ends up
    /// empty is the caller's cue to open the default tab.
    pub fn load(data_dir: &Path) -> Self {
        let path = Self::path(data_dir);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return PersistedLayout::default();
        };
        let Ok(mut layout) = serde_json::from_str::<PersistedLayout>(&text) else {
            return PersistedLayout::default();
        };
        layout.tabs.retain(|tab| Path::new(&tab.cwd).is_dir());
        for tab in &mut layout.tabs {
            if let Some(session) = &tab.session_path
                && !Path::new(session).is_file()
            {
                tab.session_path = None;
            }
        }
        if layout.tabs.is_empty() {
            layout.active_index = 0;
        } else if layout.active_index >= layout.tabs.len() {
            layout.active_index = 0;
        }
        layout
    }

    /// Atomic write: a crash mid-save must not leave a half-written layout behind.
    pub fn save(data_dir: &Path, layout: &PersistedLayout) -> Result<(), String> {
        let path = Self::path(data_dir);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(layout).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("romp-tab-layout-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = temp_dir("roundtrip");
        let workspace = dir.join("ws");
        std::fs::create_dir_all(&workspace).unwrap();
        let layout = PersistedLayout {
            version: 1,
            tabs: vec![
                TabSpec {
                    cwd: workspace.to_string_lossy().into_owned(),
                    session_path: None,
                    kind: "agent".into(),
                },
                TabSpec {
                    cwd: workspace.to_string_lossy().into_owned(),
                    session_path: None,
                    kind: "chat".into(),
                },
            ],
            active_index: 1,
        };
        PersistedLayout::save(&dir, &layout).unwrap();
        assert_eq!(PersistedLayout::load(&dir), layout);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn drops_tabs_whose_workspace_or_session_is_gone() {
        let dir = temp_dir("drops");
        let workspace = dir.join("ws");
        std::fs::create_dir_all(&workspace).unwrap();
        let layout = PersistedLayout {
            version: 1,
            tabs: vec![
                TabSpec {
                    cwd: dir.join("deleted-workspace").to_string_lossy().into_owned(),
                    session_path: None,
                    kind: "agent".into(),
                },
                TabSpec {
                    cwd: workspace.to_string_lossy().into_owned(),
                    session_path: Some(dir.join("gone.jsonl").to_string_lossy().into_owned()),
                    kind: "agent".into(),
                },
            ],
            active_index: 1,
        };
        PersistedLayout::save(&dir, &layout).unwrap();
        let loaded = PersistedLayout::load(&dir);
        assert_eq!(loaded.tabs.len(), 1, "the missing workspace tab is dropped");
        assert_eq!(loaded.tabs[0].session_path, None, "the dead session is cleared");
        assert_eq!(loaded.active_index, 0, "the index follows the surviving tab");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn unreadable_or_absent_file_yields_an_empty_layout() {
        let dir = temp_dir("empty");
        assert_eq!(PersistedLayout::load(&dir), PersistedLayout::default());
        std::fs::write(PersistedLayout::path(&dir), "{ not json").unwrap();
        assert_eq!(PersistedLayout::load(&dir), PersistedLayout::default());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn active_index_is_clamped() {
        let dir = temp_dir("clamp");
        let workspace = dir.join("ws");
        std::fs::create_dir_all(&workspace).unwrap();
        let layout = PersistedLayout {
            version: 1,
            tabs: vec![TabSpec {
                cwd: workspace.to_string_lossy().into_owned(),
                session_path: None,
                kind: "agent".into(),
            }],
            active_index: 7,
        };
        PersistedLayout::save(&dir, &layout).unwrap();
        assert_eq!(PersistedLayout::load(&dir).active_index, 0);
        std::fs::remove_dir_all(dir).ok();
    }
}
