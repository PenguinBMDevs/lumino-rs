use super::core::{
    GroupId, GroupSubState, MAX_PANEL_WIDTH, MIN_PANEL_WIDTH, Route, Sidebar, Track,
};
use crate::event as ui_event;
use crate::sidebar::Event;

impl Sidebar {
    pub fn update(&mut self, event: Event) -> bool {
        use Event::*;
        let prev_visible = self.panel_visible;
        let prev_route = self.route;
        let prev_group = self.active_group;
        match event {
            // ── 分组切换（核心逻辑） ──
            GroupToggled(group) => {
                self.handle_group_toggle(group);
            }
            // ── 路由/面板 ──
            RouteUpdated(r) => {
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
            PanelToggled(r) => {
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
            // ── 音轨 ──
            TrackSelected(id) => {
                tracing::debug!("Sidebar: 音轨选择 id={}", id);
                self.selected_track = id;
            }
            TrackMuteToggled(id) => {
                if let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) {
                    track.is_muted = !track.is_muted;
                }
            }
            AddTrack => {
                let new_id = self.tracks.len();
                self.tracks.push(Track {
                    id: new_id,
                    name: format!("Track {}", new_id),
                    is_conductor: false,
                    can_delete: true,
                    is_muted: false,
                });
                ui_event::emit(ui_event::Event::Window(
                    ui_event::window::Event::local_track_added(new_id),
                ));
            }
            // ── 调整宽度 ──
            ResizeDragStarted(_) => {
                self.is_resizing = true;
            }
            ResizeDragged(_) => {}
            ResizeDragEnded => {
                self.is_resizing = false;
            }
            // ── 子按钮切换 ──
            AutomationPanelToggled => {
                if self.route == Route::Arrangement {
                    // 从工程走带界面打开自动化面板：恢复钢琴卷帘状态并开启自动化
                    self.restore_piano_roll_state();
                    self.automation_panel_visible = true;
                    self.active_group = Some(GroupId::PianoRoll);
                } else {
                    self.automation_panel_visible = !self.automation_panel_visible;
                }
            }
            PianoRollToggled => {
                self.piano_roll_visible = !self.piano_roll_visible;
                if self.piano_roll_visible && self.route == Route::Arrangement {
                    self.restore_piano_roll_state();
                }
            }
        }
        // 最终保护
        if self.route == Route::Arrangement {
            self.panel_visible = false;
        }

        self.panel_visible != prev_visible
            || self.route != prev_route
            || self.active_group != prev_group
    }

    /// 分组切换：保存旧组状态 → 恢复新组状态，互斥
    fn handle_group_toggle(&mut self, group: GroupId) {
        // 如果点击的是已激活的分组
        if self.active_group == Some(group) {
            // 钢琴卷帘组：不允许通过再次点击关闭，始终保持钢琴卷帘可见
            if group == GroupId::PianoRoll {
                return;
            }
            // 其他分组：先保存当前状态，确保再次激活时能正确恢复
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
    fn enter_arrangement(&mut self) {
        self.save_group_state(GroupId::PianoRoll);
        self.panel_visible = false;
        self.automation_panel_visible = false;
        self.piano_roll_visible = false;
        self.route = Route::Arrangement;
    }

    /// 退出工程走带视图：恢复之前保存的钢琴卷帘子按钮状态。
    fn restore_piano_roll_state(&mut self) {
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
    fn save_group_state(&mut self, group: GroupId) {
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
    fn activate_group(&mut self, group: GroupId) {
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

    /// 取消激活分组
    fn deactivate_group(&mut self, group: GroupId) {
        match group {
            GroupId::PianoRoll => {
                self.piano_roll_visible = false;
                self.panel_visible = false;
                self.automation_panel_visible = false;
            }
            GroupId::Project => {
                // 退出工程走带时恢复钢琴卷帘状态并切回钢琴卷帘组
                self.piano_roll_visible = true;
                let state = &self.piano_roll_sub_state;
                self.panel_route = state.panel_route;
                self.panel_visible = state.panel_visible;
                self.automation_panel_visible = state.automation_panel_visible;
                self.route = if state.panel_visible {
                    state.panel_route
                } else {
                    Route::File
                };
                self.active_group = Some(GroupId::PianoRoll);
                return;
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

    /// 检查是否正在调整大小
    pub fn is_resizing(&self) -> bool {
        self.is_resizing
    }

    pub fn start_resize(&mut self, cursor_x: f32) {
        self.is_resizing = true;
        self.resize_start_x = cursor_x;
        self.resize_start_width = self.panel_width;
    }

    pub fn update_resize_position(&mut self, cursor_x: f32) {
        if self.is_resizing {
            let delta_x = cursor_x - self.resize_start_x;
            let new_width = self.resize_start_width + delta_x;
            self.panel_width = new_width.clamp(MIN_PANEL_WIDTH, MAX_PANEL_WIDTH);
        }
    }

    pub fn end_resize(&mut self) {
        self.is_resizing = false;
    }
}
