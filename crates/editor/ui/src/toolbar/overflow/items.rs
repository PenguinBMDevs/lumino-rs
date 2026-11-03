//! 溢出菜单「折叠结果 → 菜单项」展开
//!
//! 从 `interaction.rs` 按职责拆出（REF-001：单文件 ≤ 400 行）：本模块只回答
//! **"折叠掉的东西该怎么出现在「更多」菜单里"**；宽度预算与折叠决策在
//! `interaction.rs`。
//!
//! 两种粒度都要覆盖：
//! - **整组折叠**（录制/播放/循环/仪表盘/右侧开关/协作）→ 各自的分组清单；
//! - **主工具栏逐项折叠** → 只把真正被折叠的那几个条目按视觉顺序补进菜单。
//!   菜单是"缺席者的补位"，不该把仍然可见的按钮再列一遍。

use lumino_core::storage::config::AutoScrollMode;
use lumino_extras::i18n::{Language, MainTranslations, main_translations};

use crate::Message;
use crate::resources::icon;
use crate::toolbar::overflow::state::{OverflowMenuItem, ToolItem, ToolbarGroup, ToolbarOverflow};
use crate::toolbar::{Event, FlipHorizontalMode, Tool, Toolbar};

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
    /// 把折叠结果展开成「更多」菜单条目（按工具栏显示顺序）
    ///
    /// 主工具栏特殊：部分折叠时只列**被折叠的条目**；整组折叠时列全部条目，
    /// 并补上历史上的「曲线」快捷入口（该按钮原本就不在主工具栏行内，
    /// 是菜单独有的选择路径，不能因为本轮改造把它弄丢）。
    pub fn overflow_menu_items(
        &self,
        overflow: &ToolbarOverflow,
        has_selection: bool,
        language: Language,
        arrangement_mode: bool,
    ) -> Vec<OverflowMenuItem> {
        let t = main_translations(language);

        let mut items = Vec::new();
        for group in ToolbarGroup::ORDER.iter().copied() {
            let tools_partially_hidden =
                group == ToolbarGroup::Tools && !overflow.hidden_tools.is_empty();
            if !overflow.hidden_groups.contains(&group) && !tools_partially_hidden {
                continue;
            }

            match group {
                // 走带模式的工具区整组折叠（无逐项）
                ToolbarGroup::Tools if arrangement_mode => {
                    items.extend(self.arrangement_tools_items(t));
                }
                ToolbarGroup::Tools => {
                    items.extend(self.tool_item_entries(&overflow.hidden_tools, t, has_selection));
                    if overflow.hidden_tools.len() == ToolItem::ALL.len() {
                        items.push(menu_item(
                            icon::Curve,
                            t.tool_curve,
                            Event::tool_selected(Tool::Curve),
                            true,
                        ));
                    }
                }
                other => items.extend(self.group_overflow_items(other, t)),
            }
        }
        items
    }

    /// 主工具栏被折叠条目 → 菜单项（顺序 = 视觉顺序，由调用方保证）
    fn tool_item_entries(
        &self,
        items: &[ToolItem],
        t: &'static MainTranslations,
        has_selection: bool,
    ) -> Vec<OverflowMenuItem> {
        let ctrl = self.ctrl_pressed;
        let shift = self.shift_pressed;

        let transpose_down = if ctrl {
            (t.tool_transpose_down_octave, Event::transpose_down(12))
        } else {
            (t.tool_transpose_down, Event::transpose_down(1))
        };
        let transpose_up = if ctrl {
            (t.tool_transpose_up_octave, Event::transpose_up(12))
        } else {
            (t.tool_transpose_up, Event::transpose_up(1))
        };
        let flip_horizontal = if shift {
            Event::flip_horizontal(FlipHorizontalMode::Right)
        } else if ctrl {
            Event::flip_horizontal(FlipHorizontalMode::Left)
        } else {
            Event::flip_horizontal(FlipHorizontalMode::Center)
        };

        items
            .iter()
            .map(|item| match item {
                ToolItem::Pointer => menu_item(
                    icon::MousePointer,
                    t.tool_pointer,
                    Event::tool_selected(Tool::Pointer),
                    true,
                ),
                ToolItem::PointerYSelect => menu_item(
                    icon::MousePointerYSelect,
                    t.tool_pointer_y_select,
                    Event::tool_selected(Tool::PointerYSelect),
                    true,
                ),
                ToolItem::Pencil => menu_item(
                    icon::Pencil,
                    t.tool_pencil,
                    Event::tool_selected(Tool::Pencil),
                    true,
                ),
                ToolItem::Eraser => menu_item(
                    icon::Eraser,
                    t.tool_eraser,
                    Event::tool_selected(Tool::Eraser),
                    true,
                ),
                ToolItem::DrawToolbox => menu_item(
                    icon::DrawToolbox,
                    t.tool_panel_tooltip,
                    Event::toggle_tool_panel(),
                    true,
                ),
                ToolItem::Quantize => {
                    menu_item(icon::Quantize, t.tool_quantize, Event::quantize(), true)
                }
                ToolItem::Speed => {
                    menu_item(icon::Speed, t.tool_speed, Event::speed_change(), true)
                }
                ToolItem::FlipVertical => menu_item(
                    icon::FlipVertical,
                    t.tool_flip_vertical,
                    Event::flip_vertical(),
                    has_selection,
                ),
                ToolItem::FlipHorizontal => menu_item(
                    icon::FlipHorizontal,
                    t.tool_flip_horizontal,
                    flip_horizontal.clone(),
                    has_selection,
                ),
                ToolItem::Split => menu_item(icon::Split, t.tool_split, Event::split(), true),
                ToolItem::Glue => menu_item(icon::Glue, t.tool_glue, Event::glue(), true),
                ToolItem::TransposeDown => menu_item(
                    icon::TransposeDown,
                    transpose_down.0,
                    transpose_down.1.clone(),
                    has_selection,
                ),
                ToolItem::TransposeUp => menu_item(
                    icon::TransposeUp,
                    transpose_up.0,
                    transpose_up.1.clone(),
                    has_selection,
                ),
                ToolItem::Tie => menu_item(icon::Tie, t.tool_tie, Event::tie(), true),
            })
            .collect()
    }

    /// 工程走带模式的工具项（指针/曲线/橡皮/变速）
    fn arrangement_tools_items(&self, t: &'static MainTranslations) -> Vec<OverflowMenuItem> {
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
            menu_item(icon::Speed, t.tool_speed, Event::speed_change(), true),
        ]
    }

    /// 整组折叠的分组 → 菜单项
    fn group_overflow_items(
        &self,
        group: ToolbarGroup,
        t: &'static MainTranslations,
    ) -> Vec<OverflowMenuItem> {
        match group {
            ToolbarGroup::Record => vec![menu_item(
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
            )],
            ToolbarGroup::Playback => vec![
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
            ],
            ToolbarGroup::Loop => vec![menu_item(
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
            )],
            ToolbarGroup::UndoRedo => vec![
                menu_item(icon::Undo, t.undo, Event::undo(), true),
                menu_item(icon::Redo, t.redo, Event::redo(), true),
                menu_item(
                    icon::Clock,
                    lumino_core::tr!("撤销历史"),
                    Event::open_history_dialog(),
                    true,
                ),
            ],
            // 仪表盘是纯显示信息，没有可点击的操作，折叠后不进菜单
            ToolbarGroup::Dashboard => Vec::new(),
            ToolbarGroup::OnionSkin => vec![menu_item(
                icon::OnionSkin,
                t.onion_skin_tooltip,
                Event::toggle_onion_skin(),
                true,
            )],
            ToolbarGroup::AutoScroll => vec![menu_item(
                match self.auto_scroll_mode {
                    AutoScrollMode::FixedIndicatorLeft => icon::ArrowsLeftRight,
                    AutoScrollMode::ScrollingIndicator => icon::Scroll,
                    AutoScrollMode::Off => icon::Ban,
                },
                t.auto_scroll_tooltip,
                Event::auto_scroll_mode_changed(),
                true,
            )],
            ToolbarGroup::Collaboration => vec![menu_item(
                icon::Users,
                t.collaboration_tooltip,
                Event::open_collaboration_dialog(),
                true,
            )],
            // 主工具栏与走带工具区由 `overflow_menu_items` 单独处理
            ToolbarGroup::Tools => Vec::new(),
        }
    }
}
