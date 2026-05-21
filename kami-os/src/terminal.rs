//! Embedded XRPC terminal — shell for invoking XRPC commands.
//!
//! Renders a scrollback buffer via kami-text. Input is routed through
//! the XRPC client to atproto.gftd.ai and results displayed inline.

use serde::{Deserialize, Serialize};

/// Terminal window state.
#[derive(Debug, Clone)]
pub struct TerminalState {
    /// Command history (most recent last).
    pub history: Vec<String>,
    /// Output lines (scrollback buffer).
    pub output: Vec<TerminalLine>,
    /// Current input line being typed.
    pub input: String,
    /// Cursor position in input.
    pub cursor: usize,
    /// History navigation index (None = current input).
    pub history_index: Option<usize>,
    /// Scroll offset in output (0 = bottom).
    pub scroll_offset: usize,
    /// Maximum scrollback lines.
    pub max_scrollback: usize,
}

/// A line in the terminal output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalLine {
    /// Line text content.
    pub text: String,
    /// Line type (affects color).
    pub kind: LineKind,
}

/// Terminal line type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineKind {
    /// User command (prefixed with "> ").
    Command,
    /// XRPC response (normal output).
    Output,
    /// Error message (red).
    Error,
    /// System message (gray).
    System,
}

impl TerminalState {
    /// Create a new terminal with welcome message.
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            output: vec![TerminalLine {
                text: "GFTD OS Terminal — XRPC shell. Type `help` for commands.".to_string(),
                kind: LineKind::System,
            }],
            input: String::new(),
            cursor: 0,
            history_index: None,
            scroll_offset: 0,
            max_scrollback: 10000,
        }
    }

    /// Submit the current input line. Returns the command string.
    pub fn submit(&mut self) -> String {
        let cmd = self.input.clone();
        self.output.push(TerminalLine {
            text: format!("> {cmd}"),
            kind: LineKind::Command,
        });
        if !cmd.is_empty() {
            self.history.push(cmd.clone());
        }
        self.input.clear();
        self.cursor = 0;
        self.history_index = None;
        self.trim_scrollback();
        cmd
    }

    /// Append output lines from an XRPC response.
    pub fn append_output(&mut self, text: &str, kind: LineKind) {
        for line in text.lines() {
            self.output.push(TerminalLine {
                text: line.to_string(),
                kind,
            });
        }
        self.trim_scrollback();
    }

    /// Navigate history up.
    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let idx = match self.history_index {
            None => self.history.len() - 1,
            Some(0) => 0,
            Some(i) => i - 1,
        };
        self.history_index = Some(idx);
        self.input = self.history[idx].clone();
        self.cursor = self.input.len();
    }

    /// Navigate history down.
    pub fn history_down(&mut self) {
        match self.history_index {
            None => {}
            Some(i) if i >= self.history.len() - 1 => {
                self.history_index = None;
                self.input.clear();
                self.cursor = 0;
            }
            Some(i) => {
                let idx = i + 1;
                self.history_index = Some(idx);
                self.input = self.history[idx].clone();
                self.cursor = self.input.len();
            }
        }
    }

    /// Trim output to max scrollback.
    fn trim_scrollback(&mut self) {
        if self.output.len() > self.max_scrollback {
            let excess = self.output.len() - self.max_scrollback;
            self.output.drain(..excess);
        }
    }
}

impl Default for TerminalState {
    fn default() -> Self {
        Self::new()
    }
}
