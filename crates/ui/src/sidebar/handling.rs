use super::core::{MAX_PANEL_WIDTH, MIN_PANEL_WIDTH, Route, Sidebar, Track};
use crate::event as ui_event;
use crate::sidebar::Event;

impl Sidebar {
    pub fn update(&mut self, event: Event) -> bool {
        use Event::*;
        let prev_visible = self.panel_visible;
        let prev_route = self.route;
        match event {
            RouteUpdated(r) => {
                self.route = r;
                // 切换到音轨总览路由时，自动隐藏左侧面板
                if r == Route::Arrangement {
                    self.panel_visible = false;
                    // 互斥：打开工程走带时关闭钢琴卷帘
                    self.piano_roll_visible = false;
                }
            }
            PanelToggled(r) => {
                // 音轨总览模式下：点击其他路由按钮只切换路由，不打开面板
                if self.route == Route::Arrangement && r != Route::Arrangement {
                    self.route = r;
                } else if r == Route::Arrangement {
                    // 切换到音轨总览路由时，关闭面板
                    self.panel_visible = false;
                    // 互斥：打开工程走带时关闭钢琴卷帘
                    self.piano_roll_visible = false;
                    self.panel_route = r;
                    self.route = r;
                } else if r == Route::File && !self.piano_roll_visible {
                    // 互斥：音轨列表面板只能在钢琴卷帘模式下打开
                    // 钢琴卷帘关闭（如瀑布流模式）时不允许打开文件面板
                    self.route = r;
                } else if self.panel_visible && self.panel_route == r {
                    self.panel_visible = false;
                } else {
                    self.panel_visible = true;
                    self.panel_route = r;
                    self.route = r;
                }
            }
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
                // 添加新音轨
                let new_id = self.tracks.len();
                self.tracks.push(Track {
                    id: new_id,
                    name: format!("Track {}", new_id),
                    is_conductor: false,
                    can_delete: true,
                    is_muted: false,
                });
                self.selected_track = new_id;
                self.add_track_menu_open = false;

                // 发射协作同步事件
                ui_event::emit(ui_event::Event::Window(
                    ui_event::window::Event::local_track_added(new_id),
                ));
            }
            AddTrackMenuToggled => {
                self.add_track_menu_open = !self.add_track_menu_open;
            }
            ResizeDragStarted(_) => {
                self.is_resizing = true;
            }
            ResizeDragged(_) => {
                // 拖拽中的位置更新由 Host 通过 update_resize_position 处理
            }
            ResizeDragEnded => {
                self.is_resizing = false;
            }
            AutomationPanelToggled => {
                self.automation_panel_visible = !self.automation_panel_visible;
            }
            PianoRollToggled => {
                self.piano_roll_visible = !self.piano_roll_visible;
                // 互斥：打开钢琴卷帘时关闭工程走带，切回 File 路由并保持面板开启
                if self.piano_roll_visible && self.route == Route::Arrangement {
                    self.route = Route::File;
                    self.panel_route = Route::File;
                    self.panel_visible = true;
                }
            }
        }
        // 最终保护：音轨总览模式下强制关闭面板
        if self.route == Route::Arrangement {
            self.panel_visible = false;
        }

        // 当面板可见性变化或路由变化时，都需要重新渲染
        self.panel_visible != prev_visible || self.route != prev_route
    }

    /// 检查是否正在调整大小
    pub fn is_resizing(&self) -> bool {
        self.is_resizing
    }

    /// 开始调整大小，记录起始鼠标 X 坐标
    pub fn start_resize(&mut self, cursor_x: f32) {
        self.is_resizing = true;
        self.resize_start_x = cursor_x;
        self.resize_start_width = self.panel_width;
    }

    /// 更新拖拽位置（从外部传入当前鼠标 X 坐标）
    pub fn update_resize_position(&mut self, cursor_x: f32) {
        if self.is_resizing {
            let delta_x = cursor_x - self.resize_start_x;
            let new_width = self.resize_start_width + delta_x;
            self.panel_width = new_width.clamp(MIN_PANEL_WIDTH, MAX_PANEL_WIDTH);
        }
    }

    /// 结束调整大小
    pub fn end_resize(&mut self) {
        self.is_resizing = false;
    }
}
