//! 路由/面板切换处理 — RouteUpdated、PanelToggled、AutomationPanelToggled、
//! PianoRollToggled、RollBarToggled

use crate::sidebar::core::{GroupId, RollBarButton, Route, Sidebar};

impl Sidebar {
    /// 处理路由更新事件
    pub(super) fn handle_route_updated(&mut self, r: Route) {
        // 音频渲染：切换独立面板（主界面钢琴卷帘区域）
        if r == Route::AudioExport {
            self.audio_export_visible = !self.audio_export_visible;
            self.video_export_visible = false;
            self.route = if self.audio_export_visible {
                Route::AudioExport
            } else {
                Route::File
            };
            self.panel_visible = false;
            self.piano_roll_visible = false;
        } else if r == Route::VideoExport {
            // 视频渲染：切换独立面板（与音频互斥，不影响钢琴卷帘状态）
            self.video_export_visible = !self.video_export_visible;
            self.audio_export_visible = false;
            self.route = if self.video_export_visible {
                Route::VideoExport
            } else {
                Route::File
            };
            self.panel_visible = false;
        } else {
            // 工程走带（Arrangement）与其他钢琴卷帘界面按钮互斥：
            // 进入走带前保存当前钢琴卷帘状态，离开时恢复。
            if self.route == Route::Arrangement && r != Route::Arrangement {
                self.restore_piano_roll_state();
                if self.active_group == Some(GroupId::Project) {
                    self.active_group = Some(GroupId::PianoRoll);
                }
            } else if r == Route::Arrangement && self.route != Route::Arrangement {
                self.enter_arrangement();
                self.active_group = Some(GroupId::Project);
            }
            self.route = r;
        }
    }

    /// 处理面板切换事件
    pub(super) fn handle_panel_toggled(&mut self, r: Route) {
        // 子按钮只能在对应分组激活时操作
        let not_allowed = self.active_group != Some(GroupId::PianoRoll)
            && matches!(r, Route::File | Route::Automation);
        if not_allowed {
            // 跨组点击 PianoRoll 子按钮：先切回 PianoRoll 组，始终打开面板
            self.handle_group_toggle(GroupId::PianoRoll);
            self.panel_visible = true;
            self.panel_route = r;
            self.route = r;
        } else if r == Route::Arrangement {
            // 工程走带：通过 GroupToggled 切换分组，此处仅兜底
            self.route = Route::Arrangement;
        } else if self.route == Route::Arrangement {
            // 在工程走带界面点击其他子按钮：先恢复钢琴卷帘状态，再打开目标面板
            self.restore_piano_roll_state();
            self.panel_visible = true;
            self.panel_route = r;
            self.route = r;
        } else if self.panel_visible && self.panel_route == r {
            self.panel_visible = false;
        } else {
            self.panel_visible = true;
            self.panel_route = r;
            self.route = r;
        }
    }

    /// 处理自动化面板切换
    pub(super) fn handle_automation_panel_toggled(&mut self) {
        if self.route == Route::Arrangement {
            // 从工程走带界面打开自动化面板：恢复钢琴卷帘状态并开启自动化
            self.restore_piano_roll_state();
            self.automation_panel_visible = true;
            self.active_group = Some(GroupId::PianoRoll);
        } else {
            self.automation_panel_visible = !self.automation_panel_visible;
        }
    }

    /// 处理钢琴卷帘面板切换
    pub(super) fn handle_piano_roll_toggled(&mut self) {
        self.piano_roll_visible = !self.piano_roll_visible;
        if self.piano_roll_visible && self.route == Route::Arrangement {
            self.restore_piano_roll_state();
        }
    }

    /// 处理卷帘面板底部按钮切换（横向/纵向卷帘）
    ///
    /// 互斥语义：点击未激活的按钮 → 该按钮点亮、另一个熄灭；
    /// 再次点击已激活的按钮 → 关闭（两个按钮均熄灭）。
    pub(super) fn handle_roll_bar_toggled(&mut self, button: RollBarButton) {
        self.roll_bar_active = match self.roll_bar_active {
            Some(active) if active == button => None,
            _ => Some(button),
        };
    }
}
