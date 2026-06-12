//! History module for undo/redo functionality

use crate::note::Note;
use im::Vector;

/// A snapshot of the editor state for undo/redo
#[derive(Debug, Clone)]
pub struct EditorSnapshot {
    pub notes: Vector<Note>,
    pub current_track: usize,
}

impl EditorSnapshot {
    pub fn new(notes: Vector<Note>, current_track: usize) -> Self {
        Self {
            notes,
            current_track,
        }
    }
}

/// History manager for undo/redo
#[derive(Debug)]
pub struct History {
    undo_stack: Vec<EditorSnapshot>,
    redo_stack: Vec<EditorSnapshot>,
    max_size: usize,
}

impl History {
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_size: 100,
        }
    }

    /// Push a new snapshot to the undo stack
    pub fn push(&mut self, snapshot: EditorSnapshot) {
        self.undo_stack.push(snapshot);
        // Clear redo stack when new action is performed
        self.redo_stack.clear();
        // Limit the undo stack size
        if self.undo_stack.len() > self.max_size {
            self.undo_stack.remove(0);
        }
    }

    /// Undo the last action and return the previous state
    pub fn undo(&mut self, current_state: EditorSnapshot) -> Option<EditorSnapshot> {
        if self.undo_stack.is_empty() {
            return None;
        }
        // Push current state to redo stack
        self.redo_stack.push(current_state);
        // Pop from undo stack
        self.undo_stack.pop()
    }

    /// Redo the last undone action
    pub fn redo(&mut self, current_state: EditorSnapshot) -> Option<EditorSnapshot> {
        if self.redo_stack.is_empty() {
            return None;
        }
        // Push current state to undo stack
        self.undo_stack.push(current_state);
        // Pop from redo stack
        self.redo_stack.pop()
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}
