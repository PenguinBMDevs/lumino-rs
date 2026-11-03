//! 溢出菜单交互逻辑
//!
//! 计算可见/隐藏分组，并把隐藏分组分发给 [`super::items`] 展开为菜单项。
//!
//! 拆分口径（REF-001）：**宽度预算 / 折叠决策**留在本文件；
//! **分组 → 菜单项**的构造全部在 `items.rs`。

use lumino_extras::i18n::Language;

use crate::toolbar::Toolbar;
use crate::toolbar::overflow::state::{OverflowMenuItem, ToolbarGroup};

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

        // 工程走带模式没有洋葱皮（该层只在钢琴卷帘里绘制）：整组既不参与布局，
        // 也不进溢出菜单——否则会留下一个点了没任何效果的菜单项。
        let order: Vec<ToolbarGroup> = ToolbarGroup::ORDER
            .iter()
            .copied()
            .filter(|group| !arrangement_mode || *group != ToolbarGroup::OnionSkin)
            .collect();

        let total = Self::total_width_for_groups(&order, tools_width);
        if total <= available_width {
            return (order, Vec::new());
        }

        let mut candidates = order.clone();
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

        let visible: Vec<ToolbarGroup> = order
            .into_iter()
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
            ToolbarGroup::OnionSkin => self.onion_skin_overflow_items(t),
            ToolbarGroup::AutoScroll => self.auto_scroll_overflow_items(t),
            ToolbarGroup::Collaboration => self.collaboration_overflow_items(t),
        }
    }
}
