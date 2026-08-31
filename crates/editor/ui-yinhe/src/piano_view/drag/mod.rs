//! 拖拽交互 — 对应 `yinhe piano_view/drag/` 9 文件 + `pencil.rs` / `marquee.rs`
//!
//! - `state`  — 复用 `lumino_editor_state::DragState`（ghost 方案，BitVec）替代
//!   yinhe `ui.data().get_persisted` 持久化
//! - `hit`    — 命中测试（Move / Resize / SelectionEdge）
//! - `marquee`— 框选拖动 Press→Move→Release（≥3px 有效）
//! - `pencil` — 铅笔工具 Create/Move/Resize 状态机

pub mod hit;
pub mod marquee;
pub mod pencil;
pub mod state;

pub use hit::{HitKind, HitNote, hit_test_note, hit_test_sel_edge, rect_has_notes};
pub use marquee::{MarqueeResult, marquee_move, marquee_press, marquee_release};
pub use pencil::{
    HitNote as PencilHitNote, PencilDrag, PencilHitMode, PencilState, valid_pencil_track,
};
pub use state::PianoDragState;
