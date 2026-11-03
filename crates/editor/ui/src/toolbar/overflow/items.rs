//! 溢出菜单「分组 → 菜单项」展开
//!
//! 从 `interaction.rs` 按职责拆出（REF-001：单文件 ≤ 400 行）：本模块只回答
//! **"某分组被折叠时，它的菜单项长什么样"**；宽度预算与可见/隐藏分组计算仍留在
//! `interaction.rs`。
//!
//! 拆分的实际收益：新增分组（本轮 = 洋葱皮）时只需在 `interaction.rs` 的 dispatch
//! 加一行、在本模块加一个 builder——两边都不会因为"加三行就顶破 400 行"而被迫
//! 把逻辑塞进别处。

use lumino_core::storage::config::AutoScrollMode;
use lumino_extras::i18n::Language;

use crate::Message;
use crate::resources::icon;
use crate::toolbar::overflow::state::OverflowMenuItem;
use crate::toolbar::{Event, FlipHorizontalMode, Tool, Toolbar};

/// 修饰键事件元组（翻转水平事件、移调 tooltip+事件：下/上）
type ModifierEvents = (&'static str, Message, &'static str, Message, Message);

/// 快速构造溢出菜单项的辅助函数（消除重复样板代码）
fn menu_item(
    icon: icon::Icon,
    tooltip: &'static str,
    on_press: Message,
    enabled: bool,
) -> OverflowMenuItem {
    OverflowMenuItem {
        icon,
        tooltip,
        on_press,
        enabled,
    }
}

impl Toolbar {
    /// 录制分组溢出项
    pub(super) fn record_overflow_items(
        &self,
        t: &'static lumino_extras::i18n::MainTranslations,
    ) -> Vec<OverflowMenuItem> {
        vec![menu_item(
            icon::PlayCircle,
            if self.is_recording {
                t.record_stop
            } else {
                t.record_start
            },
            if self.is_recording {
                Event::record_stop()
            } else {
                Event::record()
            },
            true,
        )]
    }

    /// 播放控制分组溢出项（快退/播放·暂停/快进）
    pub(super) fn playback_overflow_items(
        &self,
        t: &'static lumino_extras::i18n::MainTranslations,
    ) -> Vec<OverflowMenuItem> {
        vec![
            menu_item(
                icon::SkipBackward,
                t.skip_backward,
                Event::skip_backward(),
                true,
            ),
            menu_item(
                if self.is_playing {
                    icon::Pause
                } else {
                    icon::Play
                },
                if self.is_playing { t.pause } else { t.play },
                if self.is_playing {
                    Event::pause()
                } else {
                    Event::play()
                },
                true,
            ),
            menu_item(
                icon::SkipForward,
                t.skip_forward,
                Event::skip_forward(),
                true,
            ),
        ]
    }

    /// 循环切换分组溢出项
    pub(super) fn loop_overflow_items(
        &self,
        t: &'static lumino_extras::i18n::MainTranslations,
    ) -> Vec<OverflowMenuItem> {
        vec![menu_item(
            if self.is_looping {
                icon::ArrowsLeftRight
            } else {
                icon::Ban
            },
            if self.is_looping {
                t.loop_on
            } else {
                t.loop_off
            },
            Event::toggle_loop(),
            true,
        )]
    }

    /// 撤销/重做分组溢出项
    pub(super) fn undo_redo_overflow_items(
        &self,
        t: &'static lumino_extras::i18n::MainTranslations,
    ) -> Vec<OverflowMenuItem> {
        vec![
            menu_item(icon::Undo, t.undo, Event::undo(), true),
            menu_item(icon::Redo, t.redo, Event::redo(), true),
            menu_item(
                icon::Clock,
                lumino_core::tr!("撤销历史"),
                Event::open_history_dialog(),
                true,
            ),
        ]
    }

    /// 自动滚动分组溢出项
    pub(super) fn auto_scroll_overflow_items(
        &self,
        t: &'static lumino_extras::i18n::MainTranslations,
    ) -> Vec<OverflowMenuItem> {
        vec![menu_item(
            match self.auto_scroll_mode {
                AutoScrollMode::FixedIndicatorLeft => icon::ArrowsLeftRight,
                AutoScrollMode::ScrollingIndicator => icon::Scroll,
                AutoScrollMode::Off => icon::Ban,
            },
            t.auto_scroll_tooltip,
            Event::auto_scroll_mode_changed(),
            true,
        )]
    }

    /// 洋葱皮分组溢出项
    ///
    /// 只放**开/关**一项：按钮右侧下拉三角的「洋葱皮选项」尚在预留阶段，
    /// 不在这里预埋半成品条目（点不动的菜单项比没有更糟）。
    pub(super) fn onion_skin_overflow_items(
        &self,
        t: &'static lumino_extras::i18n::MainTranslations,
    ) -> Vec<OverflowMenuItem> {
        vec![menu_item(
            icon::OnionSkin,
            t.onion_skin_tooltip,
            Event::toggle_onion_skin(),
            true,
        )]
    }

    /// 协作分组溢出项
    pub(super) fn collaboration_overflow_items(
        &self,
        t: &'static lumino_extras::i18n::MainTranslations,
    ) -> Vec<OverflowMenuItem> {
        vec![menu_item(
            icon::Users,
            t.collaboration_tooltip,
            Event::open_collaboration_dialog(),
            true,
        )]
    }

    /// 展开 Tools 分组为溢出菜单项
    pub(super) fn tools_overflow_items(
        &self,
        language: Language,
        has_selection: bool,
        arrangement_mode: bool,
    ) -> Vec<OverflowMenuItem> {
        let t = lumino_extras::i18n::main_translations(language);
        if arrangement_mode {
            return self.arrangement_tools_items(t);
        }

        let mut items = self.basic_selection_tool_items(t);
        items.append(&mut self.action_tool_items(t));
        let ev = self.setup_modifier_events(t);
        items.append(&mut self.modifier_tool_items(t, has_selection, ev));
        items
    }

    /// 工程走带模式下工具项（指针/曲线/橡皮）
    fn arrangement_tools_items(
        &self,
        t: &'static lumino_extras::i18n::MainTranslations,
    ) -> Vec<OverflowMenuItem> {
        vec![
            menu_item(
                icon::MousePointer,
                t.tool_pointer,
                Event::tool_selected(Tool::Pointer),
                true,
            ),
            menu_item(
                icon::Curve,
                t.tool_curve,
                Event::tool_selected(Tool::Curve),
                true,
            ),
            menu_item(
                icon::Eraser,
                t.tool_eraser,
                Event::tool_selected(Tool::Eraser),
                true,
            ),
        ]
    }

    /// 基础工具项（指针/铅笔/橡皮/曲线：无需选中，无修饰键）
    fn basic_selection_tool_items(
        &self,
        t: &'static lumino_extras::i18n::MainTranslations,
    ) -> Vec<OverflowMenuItem> {
        vec![
            menu_item(
                icon::MousePointer,
                t.tool_pointer,
                Event::tool_selected(Tool::Pointer),
                true,
            ),
            menu_item(
                icon::Pencil,
                t.tool_pencil,
                Event::tool_selected(Tool::Pencil),
                true,
            ),
            menu_item(
                icon::Eraser,
                t.tool_eraser,
                Event::tool_selected(Tool::Eraser),
                true,
            ),
            menu_item(
                icon::Curve,
                t.tool_curve,
                Event::tool_selected(Tool::Curve),
                true,
            ),
        ]
    }

    /// 动作工具项（量化/变速/分割/合并/连奏：无需选中，无修饰键）
    fn action_tool_items(
        &self,
        t: &'static lumino_extras::i18n::MainTranslations,
    ) -> Vec<OverflowMenuItem> {
        vec![
            menu_item(icon::Quantize, t.tool_quantize, Event::quantize(), true),
            menu_item(icon::Speed, t.tool_speed, Event::speed_change(), true),
            menu_item(icon::Split, t.tool_split, Event::split(), true),
            menu_item(icon::Glue, t.tool_glue, Event::glue(), true),
            menu_item(icon::Tie, t.tool_tie, Event::tie(), true),
        ]
    }

    /// 构造修饰键相关事件与 tooltip（翻转/移调）
    fn setup_modifier_events(
        &self,
        t: &'static lumino_extras::i18n::MainTranslations,
    ) -> ModifierEvents {
        let ctrl = self.ctrl_pressed;
        let shift = self.shift_pressed;

        let (transpose_down_tooltip, transpose_down_event) = if ctrl {
            (t.tool_transpose_down_octave, Event::transpose_down(12))
        } else {
            (t.tool_transpose_down, Event::transpose_down(1))
        };
        let (transpose_up_tooltip, transpose_up_event) = if ctrl {
            (t.tool_transpose_up_octave, Event::transpose_up(12))
        } else {
            (t.tool_transpose_up, Event::transpose_up(1))
        };
        let flip_horizontal_event = if shift {
            Event::flip_horizontal(FlipHorizontalMode::Right)
        } else if ctrl {
            Event::flip_horizontal(FlipHorizontalMode::Left)
        } else {
            Event::flip_horizontal(FlipHorizontalMode::Center)
        };

        (
            transpose_down_tooltip,
            transpose_down_event,
            transpose_up_tooltip,
            transpose_up_event,
            flip_horizontal_event,
        )
    }

    /// 需要选中项的修饰工具（翻转垂直/翻转水平/移调下/移调上）
    fn modifier_tool_items(
        &self,
        t: &'static lumino_extras::i18n::MainTranslations,
        has_selection: bool,
        ev: ModifierEvents,
    ) -> Vec<OverflowMenuItem> {
        vec![
            menu_item(
                icon::FlipVertical,
                t.tool_flip_vertical,
                Event::flip_vertical(),
                has_selection,
            ),
            menu_item(
                icon::FlipHorizontal,
                t.tool_flip_horizontal,
                ev.4,
                has_selection,
            ),
            menu_item(icon::TransposeDown, ev.0, ev.1, has_selection),
            menu_item(icon::TransposeUp, ev.2, ev.3, has_selection),
        ]
    }
}
