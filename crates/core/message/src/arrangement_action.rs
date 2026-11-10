//! 工程走带交互的**框架无关动作**（UI 无关）
//!
//! 走带交互逻辑（选择 / 移动 / 框选 / 橡皮 / 曲线）原先直接产出 iced
//! `Message`，导致同一套逻辑无法被 egui 侧复用。这里把「交互产生了什么」抽成
//! 中立枚举：交互层只产出 `ArrangementAction`，由各 UI 侧在边界翻译成自己的
//! 消息（iced：`Message`；egui：动作通道）。
//!
//! 与 `Message` 一一对应，迁移时不需要重新推导语义。

use crate::types::message::Message;

/// 走带交互动作（框架无关）。
#[derive(Debug, Clone, PartialEq)]
pub enum ArrangementAction {
    /// 设置演奏指示线位置（tick）。
    CursorSet(f64),
    /// 选择矩形变更（tick_start, tick_end, track_lo, track_hi）。
    SelectionChanged(Option<(f64, f64, usize, usize)>),
    /// 清空选择。
    SelectionCleared,
    /// 移动选中的音符（tick / 音轨偏移）。
    MoveNotes {
        /// tick 偏移量。
        delta_ticks: i64,
        /// 音轨偏移量。
        delta_tracks: i32,
    },
    /// 擦除矩形范围内的音符。
    Erase {
        /// 起始 tick。
        tick_start: f64,
        /// 结束 tick。
        tick_end: f64,
        /// 起始音轨。
        track_lo: usize,
        /// 结束音轨。
        track_hi: usize,
    },
    /// 在指定 tick / 音轨处分割音符。
    Razor {
        /// 分割位置 tick。
        tick: f64,
        /// 分割音轨。
        track: usize,
    },
    /// 在指定音轨 tick 处添加音符。
    AddNote {
        /// 音轨索引。
        track: usize,
        /// 起始 tick。
        tick: f64,
        /// 音符持续时间（tick）。
        duration: f64,
        /// 键位。
        key: u8,
        /// 力度。
        velocity: u8,
    },
    /// ghost 音符预览列表更新（`(tick_start, tick_end, 视觉轨, key)`）。
    GhostNotes(Vec<(f64, f64, usize, u8)>),
    /// 拖拽中的框选矩形（实时预览）。
    DragSelectionRect(Option<(f64, f64, usize, usize)>),
    /// 选中最靠前的音轨（点击空白处时同步侧栏选中）。
    SelectTrack(usize),
    /// 横向滚动（像素）。
    ScrollX(f32),
    /// 纵向滚动（像素）。
    ScrollY(f32),
}

impl ArrangementAction {
    /// 转成 iced 侧 `Message`（iced 边界专用；egui 侧不使用）。
    ///
    /// `select_track` 用于把 [`Self::SelectTrack`] 翻译成具体的侧栏消息——
    /// 侧栏消息是 `Message` 的泛型参数 `S`，本 crate 无法直接构造。
    pub fn into_message<W, S, Se, T>(
        self,
        select_track: impl Fn(usize) -> Message<W, S, Se, T>,
    ) -> Message<W, S, Se, T> {
        match self {
            Self::CursorSet(tick) => Message::ArrangementCursorSet(tick),
            Self::SelectionChanged(rect) => Message::ArrangementSelectionChanged(rect),
            Self::SelectionCleared => Message::ArrangementSelectionCleared,
            Self::MoveNotes {
                delta_ticks,
                delta_tracks,
            } => Message::ArrangementMoveNotes {
                delta_ticks,
                delta_tracks,
            },
            Self::Erase {
                tick_start,
                tick_end,
                track_lo,
                track_hi,
            } => Message::ArrangementErase {
                tick_start,
                tick_end,
                track_lo,
                track_hi,
            },
            Self::Razor { tick, track } => Message::ArrangementRazor { tick, track },
            Self::AddNote {
                track,
                tick,
                duration,
                key,
                velocity,
            } => Message::ArrangementAddNote {
                track,
                tick,
                duration,
                key,
                velocity,
            },
            Self::GhostNotes(notes) => Message::ArrangementGhostNotesUpdated(notes),
            Self::DragSelectionRect(rect) => Message::ArrangementDragSelectionRect(rect),
            Self::SelectTrack(id) => select_track(id),
            Self::ScrollX(x) => Message::ArrangementScrollX(x),
            Self::ScrollY(y) => Message::ArrangementScrollY(y),
        }
    }

    /// 一批动作 → 一条 `Message`（单条时原样返回，多条用 `Batch`）。
    pub fn batch_into_message<W, S, Se, T>(
        actions: Vec<Self>,
        select_track: impl Fn(usize) -> Message<W, S, Se, T>,
    ) -> Option<Message<W, S, Se, T>> {
        match actions.len() {
            0 => None,
            1 => actions
                .into_iter()
                .next()
                .map(|action| action.into_message(&select_track)),
            _ => Some(Message::Batch(
                actions
                    .into_iter()
                    .map(|action| action.into_message(&select_track))
                    .collect(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Msg = Message<(), (), (), ()>;

    #[test]
    fn cursor_and_selection_map_one_to_one() {
        assert!(matches!(
            ArrangementAction::CursorSet(42.0).into_message::<(), (), (), ()>(|_| unreachable!()),
            Msg::ArrangementCursorSet(t) if (t - 42.0).abs() < f64::EPSILON
        ));
        assert!(matches!(
            ArrangementAction::SelectionCleared.into_message::<(), (), (), ()>(|_| unreachable!()),
            Msg::ArrangementSelectionCleared
        ));
        assert!(matches!(
            ArrangementAction::SelectionChanged(Some((1.0, 2.0, 3, 4)))
                .into_message::<(), (), (), ()>(|_| unreachable!()),
            Msg::ArrangementSelectionChanged(Some((a, b, 3, 4)))
                if (a - 1.0).abs() < f64::EPSILON && (b - 2.0).abs() < f64::EPSILON
        ));
    }

    #[test]
    fn batch_collapses_single_and_empty() {
        assert!(
            ArrangementAction::batch_into_message::<(), (), (), ()>(Vec::new(), |_| unreachable!())
                .is_none(),
            "空动作不应产出消息"
        );
        assert!(matches!(
            ArrangementAction::batch_into_message::<(), (), (), ()>(
                vec![ArrangementAction::ScrollX(1.0)],
                |_| unreachable!(),
            ),
            Some(Msg::ArrangementScrollX(_))
        ));
        assert!(matches!(
            ArrangementAction::batch_into_message::<(), (), (), ()>(
                vec![
                    ArrangementAction::ScrollX(1.0),
                    ArrangementAction::ScrollY(2.0),
                ],
                |_| unreachable!(),
            ),
            Some(Msg::Batch(items)) if items.len() == 2
        ));
    }
}
