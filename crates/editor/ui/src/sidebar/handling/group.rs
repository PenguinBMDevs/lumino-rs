//! 分组切换处理 — GroupToggled 事件分发及分组状态管理
//!
//! 包含分组切换的核心逻辑：保存旧组状态 → 恢复新组状态，父按钮互斥。

use crate::sidebar::core::{GroupId, GroupSubState, Route, Sidebar};

impl Sidebar {
    /// 分组切换：保存旧组状态 → 恢复新组状态，互斥
    pub(crate) fn handle_group_toggle(&mut self, group: GroupId) {
        // 如果点击的是已激活的分组：再次点击 = 关闭该分组。
        //
        // 所有组（含钢琴卷帘 / 工程走带）统一走「关闭 → 空白主区域」这一条
        // 语义，关闭前保存子按钮状态，重新点开时恢复。父按钮不得把「关闭」
        // 实现成「切到另一个组」——否则用户点关闭会被强制跳转到别的面板。
        if self.active_group == Some(group) {
            self.save_group_state(group);
            self.deactivate_group(group);
            return;
        }

        // 保存当前激活组的状态（如果有）
        if let Some(old_group) = self.active_group {
            // 若正在工程走带界面，先恢复钢琴卷帘的原始状态再保存，
            // 避免把走带界面的关闭状态误存为钢琴卷帘状态。
            if old_group == GroupId::PianoRoll && self.route == Route::Arrangement {
                self.restore_piano_roll_state();
            }
            self.save_group_state(old_group);
        }

        // 切换到新分组
        self.activate_group(group);
    }

    /// 进入工程走带视图：保存钢琴卷帘状态并关闭音轨列表面板、自动化面板。
    pub(crate) fn enter_arrangement(&mut self) {
        self.save_group_state(GroupId::PianoRoll);
        self.panel_visible = false;
        self.automation_panel_visible = false;
        self.piano_roll_visible = false;
        self.route = Route::Arrangement;
    }

    /// 退出工程走带视图：恢复之前保存的钢琴卷帘子按钮状态。
    pub(crate) fn restore_piano_roll_state(&mut self) {
        let state = &self.piano_roll_sub_state;
        self.panel_route = state.panel_route;
        self.panel_visible = state.panel_visible;
        self.automation_panel_visible = state.automation_panel_visible;
        self.piano_roll_visible = true;
        self.route = if state.panel_visible {
            state.panel_route
        } else {
            Route::File
        };
    }

    /// 保存当前分组子按钮状态
    pub(crate) fn save_group_state(&mut self, group: GroupId) {
        match group {
            GroupId::PianoRoll => {
                self.piano_roll_sub_state = GroupSubState {
                    panel_visible: self.panel_visible
                        && !matches!(self.panel_route, Route::Automation | Route::Arrangement),
                    panel_route: self.panel_route,
                    automation_panel_visible: self.automation_panel_visible,
                };
            }
            GroupId::Project => {
                self.project_sub_state = GroupSubState {
                    panel_visible: self.panel_visible,
                    panel_route: self.panel_route,
                    automation_panel_visible: self.automation_panel_visible,
                };
            }
            GroupId::Renderer => {
                self.renderer_sub_state = GroupSubState {
                    panel_visible: self.panel_visible,
                    panel_route: self.panel_route,
                    automation_panel_visible: self.automation_panel_visible,
                };
            }
            GroupId::Waterfall => {
                // 瀑布流无子按钮，无需保存
            }
        }
    }

    /// 激活分组
    pub(crate) fn activate_group(&mut self, group: GroupId) {
        match group {
            GroupId::PianoRoll => {
                self.piano_roll_visible = true;
                // 恢复保存的子按钮状态
                let state = &self.piano_roll_sub_state;
                self.panel_route = state.panel_route;
                self.panel_visible = state.panel_visible;
                self.automation_panel_visible = state.automation_panel_visible;
                self.route = if state.panel_visible {
                    state.panel_route
                } else {
                    Route::File
                };
                // 切回钢琴卷帘组时清除渲染面板标志，确保主界面显示编辑器
                self.audio_export_visible = false;
                self.video_export_visible = false;
            }
            GroupId::Project => {
                // 工程走带：隐藏钢琴卷帘，显示走带视图
                self.piano_roll_visible = false;
                self.panel_visible = false;
                self.automation_panel_visible = false;
                self.route = Route::Arrangement;
                // 进入走带时清除渲染面板标志
                self.audio_export_visible = false;
                self.video_export_visible = false;
            }
            GroupId::Waterfall => {
                // 瀑布流：关闭钢琴卷帘
                self.piano_roll_visible = false;
                self.panel_visible = false;
                self.automation_panel_visible = false;
                self.route = Route::File;
                // 进入瀑布流时清除渲染面板标志
                self.audio_export_visible = false;
                self.video_export_visible = false;
            }
            GroupId::Renderer => {
                // 渲染组：当前无子按钮，保持基本状态
                self.piano_roll_visible = false;
                self.panel_visible = false;
                self.automation_panel_visible = false;
                self.route = Route::File;
            }
        }
        self.active_group = Some(group);
    }

    /// 取消激活分组（关闭父按钮）
    ///
    /// 关闭后主区域保持空白（由默认第一屏接管）；**不得**在这里切换到其他
    /// 分组——「关闭」被实现成「跳转」是用户明确报告过的缺陷。
    pub(crate) fn deactivate_group(&mut self, group: GroupId) {
        match group {
            GroupId::PianoRoll => {
                self.piano_roll_visible = false;
                self.panel_visible = false;
                self.automation_panel_visible = false;
            }
            GroupId::Project => {
                // 关闭工程走带 = 主区域回到空白态（由默认第一屏接管），
                // **不**替用户跳到钢琴卷帘：父按钮的语义是「开 / 关本组」，
                // 关闭时切到另一个组会让「关闭」变成「跳转」。
                //
                // 钢琴卷帘子按钮状态在进入走带前已存入 `piano_roll_sub_state`
                // （见 `handle_group_toggle` / `enter_arrangement`），用户主动
                // 点开卷帘组时由 `activate_group` 恢复，这里不能覆盖它。
                self.piano_roll_visible = false;
                self.panel_visible = false;
                self.automation_panel_visible = false;
                self.route = Route::File;
            }
            GroupId::Waterfall => {
                // 退出瀑布流时切回钢琴卷帘组（如果有保存状态）
                // 默认回到编辑器模式
            }
            GroupId::Renderer => {
                // 关闭渲染组：清除渲染面板标志
                self.audio_export_visible = false;
                self.video_export_visible = false;
            }
        }
        self.active_group = None;
    }
}
