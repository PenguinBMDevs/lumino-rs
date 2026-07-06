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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_snapshot(notes: Vec<Note>, current_track: usize) -> EditorSnapshot {
        EditorSnapshot::new(Vector::from(notes), current_track)
    }

    #[test]
    fn test_history_new_is_empty() {
        let h = History::new();
        assert!(!h.can_undo());
        assert!(!h.can_redo());
    }

    #[test]
    fn test_history_push_and_undo() {
        let mut h = History::new();
        let s1 = make_snapshot(vec![Note::new(0.0, 60, 480.0)], 0);
        let s2 = make_snapshot(
            vec![Note::new(0.0, 64, 480.0), Note::new(480.0, 67, 240.0)],
            0,
        );

        h.push(s1);
        assert!(h.can_undo());
        assert!(!h.can_redo());

        // undo 返回上一个状态，当前状态放入 redo
        let restored = h.undo(s2).expect("undo 应返回 Some");
        assert_eq!(restored.notes.len(), 1);
        assert_eq!(restored.notes[0].key, 60);
        assert!(h.can_redo());
    }

    #[test]
    fn test_history_undo_empty() {
        let mut h = History::new();
        let current = make_snapshot(vec![], 0);
        assert!(h.undo(current).is_none());
    }

    #[test]
    fn test_history_redo_empty() {
        let mut h = History::new();
        let current = make_snapshot(vec![], 0);
        assert!(h.redo(current).is_none());
    }

    #[test]
    fn test_history_redo_after_undo() {
        let mut h = History::new();
        let s1 = make_snapshot(vec![Note::new(0.0, 60, 480.0)], 0);
        let s2 = make_snapshot(vec![Note::new(0.0, 64, 480.0)], 0);

        h.push(s1);
        let _ = h.undo(s2);
        assert!(h.can_redo());

        let restored = h.redo(make_snapshot(vec![], 0)).expect("redo 应返回 Some");
        assert_eq!(restored.notes.len(), 1);
        assert_eq!(restored.notes[0].key, 64);
    }

    #[test]
    fn test_history_new_push_clears_redo() {
        let mut h = History::new();
        h.push(make_snapshot(vec![Note::new(0.0, 60, 480.0)], 0));
        let s2 = make_snapshot(vec![Note::new(0.0, 64, 480.0)], 0);
        let _ = h.undo(s2);

        // 新操作应清空 redo 栈
        h.push(make_snapshot(vec![Note::new(0.0, 67, 480.0)], 0));
        assert!(!h.can_redo());
    }

    #[test]
    fn test_history_max_size() {
        let mut h = History::new();
        h.max_size = 3;
        for i in 0..5 {
            h.push(make_snapshot(
                vec![Note::new(i as f32 * 10.0, 60, 480.0)],
                0,
            ));
        }
        // 栈大小不应超过 max_size
        assert_eq!(h.undo_stack.len(), 3);
    }

    #[test]
    fn test_history_clear() {
        let mut h = History::new();
        h.push(make_snapshot(vec![Note::new(0.0, 60, 480.0)], 0));
        h.push(make_snapshot(vec![Note::new(0.0, 64, 480.0)], 0));
        h.clear();
        assert!(!h.can_undo());
        assert!(!h.can_redo());
    }

    #[test]
    fn test_history_undo_redo_roundtrip() {
        let mut h = History::new();
        let original = make_snapshot(vec![Note::new(0.0, 60, 480.0)], 0);
        let modified = make_snapshot(vec![Note::new(0.0, 64, 480.0)], 0);

        h.push(original);
        let restored = h.undo(modified).expect("undo");
        assert_eq!(restored.notes[0].key, 60);

        let redone = h.redo(restored).expect("redo");
        assert_eq!(redone.notes[0].key, 64);
    }

    #[test]
    fn test_editor_snapshot_new() {
        let notes = vec![Note::new(10.0, 72, 960.0)];
        let snap = EditorSnapshot::new(Vector::from(notes), 1);
        assert_eq!(snap.current_track, 1);
        assert_eq!(snap.notes.len(), 1);
    }
}
