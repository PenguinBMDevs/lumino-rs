pub mod error;
pub mod font_scanner;
pub mod history;
pub mod note;
pub mod pattern;
pub mod spatial_index;
pub mod storage;

pub use error::{CoreError, Result};
pub use font_scanner::{FontInfo, scan_system_fonts};
pub use history::{EditorSnapshot, History};
pub use note::Note;
pub use pattern::Pattern;
pub use spatial_index::{NoteRef, NoteSpatialIndex};
