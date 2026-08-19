//! 溢出菜单交互逻辑
//!
//! 计算可见/隐藏分组，将隐藏分组展开为菜单项列表。

use lumino_core::storage::config::AutoScrollMode;
use lumino_extras::i18n::Language;

use crate::Message;
use crate::resources::icon;
use crate::toolbar::overflow::state::{OverflowMenuItem, ToolbarGroup};
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
    /// 根据可用宽度计算可见分组与隐藏分组
    pub fn compute_overflow_groups(
        &self,
        available_width: f32,
        arrangement_mode: bool,
    ) -> (Vec<ToolbarGroup>, Vec<ToolbarGroup>) {
        let tools_width = if arrangement_mode {
            200.0
        } else {
            ToolbarGroup::Tools.width()
        };

        let total = Self::total_width_for_groups(ToolbarGroup::ORDER, tools_width);
        if total <= available_width {
            return (ToolbarGroup::ORDER.to_vec(), Vec::new());
        }

        let mut candidates = ToolbarGroup::ORDER.to_vec();
        candidates.sort_by_key(|g| g.collapse_priority());

        let mut hidden_set = Vec::new();
        let mut remaining_width = total;
        for group in candidates {
            if remaining_width <= available_width {
                break;
            }
            let width = if group == ToolbarGroup::Tools {
                tools_width
            } else {
                group.width()
            };
            remaining_width -= width + group.spacing_after();
            hidden_set.push(group);
        }

        let visible: Vec<ToolbarGroup> = ToolbarGroup::ORDER
            .iter()
            .copied()
            .filter(|g| !hidden_set.contains(g))
            .collect();

        (visible, hidden_set)
    }

    /// 计算一组分组的总宽度（含间距）
    fn total_width_for_groups(groups: &[ToolbarGroup], tools_width: f32) -> f32 {
        groups
            .iter()
            .map(|g| {
                let width = if *g == ToolbarGroup::Tools {
                    tools_width
                } else {
                    g.width()
                };
                width + g.spacing_after()
            })
            .sum()
    }

    /// 将某个隐藏分组展开为溢出菜单项列表
    pub fn group_overflow_items(
        &self,
        group: ToolbarGroup,
        has_selection: bool,
        language: Language,
        arrangement_mode: bool,
    ) -> Vec<OverflowMenuItem> {
        let t = lumino_extras::i18n::main_translations(language);
        match group {
            ToolbarGroup::Record => self.record_overflow_items(t),
            ToolbarGroup::Playback => self.playback_overflow_items(t),
            ToolbarGroup::Loop => self.loop_overflow_items(t),
            ToolbarGroup::UndoRedo => self.undo_redo_overflow_items(t),
            ToolbarGroup::Dashboard => Vec::new(),
            ToolbarGroup::Tools => {
                self.tools_overflow_items(language, has_selection, arrangement_mode)
            }
            ToolbarGroup::AutoScroll => self.auto_scroll_overflow_items(t),
            ToolbarGroup::Collaboration => vec![menu_item(
                icon::Users,
                t.collaboration_tooltip,
                Event::open_collaboration_dialog(),
                true,
            )],
        }
    }

    // ── 各分组溢出项展开（拆分自 group_overflow_items） ──

    /// 录制分组溢出项
    fn record_overflow_items(
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
    fn playback_overflow_items(
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
    fn loop_overflow_items(
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
    fn undo_redo_overflow_items(
        &self,
        t: &'static lumino_extras::i18n::MainTranslations,
    ) -> Vec<OverflowMenuItem> {
        vec![
            menu_item(icon::Undo, t.undo, Event::undo(), true),
            menu_item(icon::Redo, t.redo, Event::redo(), true),
        ]
    }

    /// 自动滚动分组溢出项
    fn auto_scroll_overflow_items(
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

    /// 展开 Tools 分组为溢出菜单项
    fn tools_overflow_items(
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
