//! File explorer — R2/IPFS backed file browser.
//!
//! Displays a tree/grid view of files stored in R2 (Cloudflare)
//! and IPFS (content-addressed). File operations go through
//! Magatama XRPC commands (ai.gftd.os.sync.*).

use serde::{Deserialize, Serialize};

/// File explorer state.
#[derive(Debug, Clone)]
pub struct FileExplorerState {
    /// Current directory path.
    pub current_path: String,
    /// Directory entries.
    pub entries: Vec<FileEntry>,
    /// View mode.
    pub view_mode: ViewMode,
    /// Selected entry indices.
    pub selected: Vec<usize>,
    /// Sort order.
    pub sort_by: SortField,
    /// Sort direction.
    pub sort_desc: bool,
}

/// A file or directory entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// File/directory name.
    pub name: String,
    /// Full path (R2 key or IPFS CID).
    pub path: String,
    /// Entry type.
    pub kind: EntryKind,
    /// File size in bytes (0 for directories).
    pub size: u64,
    /// Last modified ISO timestamp.
    pub modified_at: String,
    /// MIME type (empty for directories).
    pub mime_type: String,
    /// Content CID (IPFS, if available).
    pub cid: Option<String>,
}

/// File entry type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
}

/// File explorer view mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    /// Grid with thumbnails.
    Grid,
    /// Detailed list with columns.
    List,
}

/// Sort field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortField {
    Name,
    Size,
    Modified,
    Kind,
}

impl FileExplorerState {
    /// Create file explorer at root.
    pub fn new() -> Self {
        Self {
            current_path: "/".to_string(),
            entries: Vec::new(),
            view_mode: ViewMode::List,
            selected: Vec::new(),
            sort_by: SortField::Name,
            sort_desc: false,
        }
    }

    /// Navigate to a directory.
    pub fn navigate(&mut self, path: String) {
        self.current_path = path;
        self.entries.clear();
        self.selected.clear();
    }

    /// Navigate up one level.
    pub fn navigate_up(&mut self) {
        if self.current_path == "/" {
            return;
        }
        if let Some(pos) = self.current_path.rfind('/') {
            let parent = if pos == 0 { "/" } else { &self.current_path[..pos] };
            self.current_path = parent.to_string();
            self.entries.clear();
            self.selected.clear();
        }
    }

    /// Toggle selection of an entry.
    pub fn toggle_select(&mut self, index: usize) {
        if let Some(pos) = self.selected.iter().position(|&i| i == index) {
            self.selected.remove(pos);
        } else {
            self.selected.push(index);
        }
    }

    /// Get selected entries.
    pub fn selected_entries(&self) -> Vec<&FileEntry> {
        self.selected
            .iter()
            .filter_map(|&i| self.entries.get(i))
            .collect()
    }
}

impl Default for FileExplorerState {
    fn default() -> Self {
        Self::new()
    }
}
