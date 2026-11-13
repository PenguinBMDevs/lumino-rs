//! 洋葱皮相关工具栏事件（全局开关 / 下拉开合 / 选择显示范围）
//!
//! 从 `update.rs` 拆出：那个文件是巨型事件分发器（已超 REF-001 的 400 行上限），
//! 新逻辑不再往里堆。语义完全等价——`update` 在所有"外部一动作即收起"的守卫
//! **之后**、大 `match` **之前**调用这里，命中即返回。

use super::{Event, Toolbar};

impl Toolbar {
    /// 处理洋葱皮事件；返回 `true` 表示事件已在此处理（调用方应直接返回）。
    ///
    /// 三条语义（不要合并，理由各不相同）：
    /// - `ToggleOnionSkin`：纯开/关，**不动模式**（关掉再打开仍按原范围显示）；
    /// - `OnionSkinModeSelected`：已实现的模式写入并**顺手打开开关**
    ///   （选完范围画面没变化，用户会当成功能坏了）；未实现的模式**不写入**
    ///   （否则留下一个什么都没做的勾选态，提示由 root handler 负责）；
    /// - 两者都收起菜单：用户完成了一次点击。
    pub(super) fn update_onion_skin_event(&mut self, event: &Event) -> bool {
        match event {
            Event::ToggleOnionSkin => {
                self.onion_skin_enabled = !self.onion_skin_enabled;
                tracing::debug!(
                    "工具栏: 洋葱皮显示切换为 {}",
                    if self.onion_skin_enabled {
                        "开"
                    } else {
                        "关"
                    }
                );
            }
            Event::ToggleOnionSkinMenu => {
                self.onion_skin_menu_open = !self.onion_skin_menu_open;
                tracing::debug!(
                    "工具栏: 洋葱皮选项下拉 {}",
                    if self.onion_skin_menu_open {
                        "打开"
                    } else {
                        "关闭"
                    }
                );
            }
            Event::CloseOnionSkinMenu => {
                self.onion_skin_menu_open = false;
            }
            Event::OnionSkinModeSelected(mode) => {
                if mode.is_implemented() {
                    let changed = self.onion_skin_mode != *mode;
                    self.onion_skin_mode = *mode;
                    self.onion_skin_enabled = true;
                    if changed {
                        tracing::debug!("工具栏: 洋葱皮显示范围切换为 {}", mode.as_str());
                    }
                }
                self.onion_skin_menu_open = false;
            }
            _ => return false,
        }
        true
    }
}
