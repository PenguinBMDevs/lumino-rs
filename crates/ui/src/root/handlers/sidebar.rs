//! 侧边栏事件与模式切换处理器
//!
//! 处理 `Message::Sidebar` 以及 `Message::ModeToggled`，
//! 负责侧边栏路由、音轨选择、分组切换与 AppMode 同步。

use crate::root::Root;
use crate::sidebar;
use crate::titlebar::mode_toggle::AppMode;
use lumino_core::storage::config::TrackAddBehavior;

impl Root {
    /// 处理模式切换（编辑器 ↔ 瀑布流）
    pub(crate) fn handle_mode_toggle(&mut self) -> bool {
        use crate::sidebar::GroupId;

        let target_mode = match self.state.current_mode {
            AppMode::Editor => AppMode::Waterfall,
            AppMode::Waterfall => AppMode::Editor,
        };
        if target_mode == AppMode::Waterfall {
            // 通过分组系统切换
            self.sidebar
                .update(sidebar::Event::GroupToggled(GroupId::Waterfall));
        } else {
            // 从瀑布流转回 → 恢复钢琴卷帘组
            self.sidebar
                .update(sidebar::Event::GroupToggled(GroupId::PianoRoll));
        }
        let target_progress = match target_mode {
            AppMode::Editor => 0.0,
            AppMode::Waterfall => 1.0,
        };
        self.state.current_mode = target_mode;
        self.state.toggle_animation.animate_to(target_progress);
        true
    }

    /// 处理侧边栏事件
    ///
    /// 返回是否需要重新渲染（仅供调用方决策；`Root::update` 始终将
    /// Sidebar 消息视为已处理，不会因无需重绘而把消息继续路由）
    pub(crate) fn handle_sidebar_event(&mut self, event: sidebar::Event) -> bool {
        // 窗口最大化/还原期间阻止路由被意外切换
        if self.window_resize_guard
            && matches!(
                &event,
                sidebar::Event::RouteUpdated(_) | sidebar::Event::GroupToggled(_)
            )
        {
            tracing::warn!("Root: 窗口最大化/还原期间忽略路由切换");
            return false;
        }

        // 自动化面板切换始终触发重绘
        if matches!(&event, sidebar::Event::AutomationPanelToggled) {
            self.sidebar.update(event);
            return true;
        }

        // 钢琴卷帘切换始终触发重绘
        if matches!(&event, sidebar::Event::PianoRollToggled) {
            // 互斥：打开钢琴卷帘时退出瀑布流模式
            if !self.sidebar.piano_roll_visible {
                self.state.current_mode = AppMode::Editor;
                self.state.toggle_animation.animate_to(0.0);
            }
            self.sidebar.update(event);
            return true;
        }

        // 先检查是否是音轨切换
        let track_selected_idx = if let sidebar::Event::TrackSelected(idx) = &event {
            Some(*idx)
        } else {
            None
        };

        // 更新 sidebar，获取是否需要重新渲染
        let needs_redraw = self.sidebar.update(event.clone());

        // 音轨新增类事件：同步扩展 document（单一权威源）。
        // 2026-08 修复：sidebar 新建音轨只更新 UI 列表，document 未扩轨，
        // 导致新音轨 insert_note 越界静默失败（音符无法放置）。
        // 必须在发射 TrackSelected 之前完成，保证切换后立即可编辑。
        if matches!(
            &event,
            sidebar::Event::AddTrack
                | sidebar::Event::TrackAddAbove(_)
                | sidebar::Event::TrackAddBelow(_)
        ) {
            self.ensure_sidebar_tracks_in_document();
        }

        // 音轨结构变化（拖拽排序 / 新增 / 删除）→ 同步视觉位置映射。
        // sidebar.tracks 顺序即视觉顺序，track_visual_order 是走带交互层
        // 把视觉位置转换为 document 音轨索引的依据，不同步会导致排序后
        // 走带编辑（添加/框选/移动/擦除/切割）落在错误的音轨上。
        let track_structure_changed = matches!(
            &event,
            sidebar::Event::AddTrack
                | sidebar::Event::TrackAddAbove(_)
                | sidebar::Event::TrackAddBelow(_)
                | sidebar::Event::TrackReorderEnded(_)
                | sidebar::Event::TrackContextMenuItemClicked(
                    _,
                    lumino_message::TrackContextMenuItem::Delete
                )
        ) || self.sidebar.pending_track_deletion.is_some();
        if track_structure_changed {
            self.sync_track_visual_order();
        }

        // 消费 sidebar 中待删除音轨请求，构造 payload 转发给 Runner 写入 .lmdeltrack
        // 必须在 sidebar.update 之后调用——此时 pending_track_deletion 才被设置。
        self.forward_pending_track_deletion();

        // 消费 sidebar 中"找回删除音轨"对话框打开请求，转发给 Runner 打开对话框
        self.forward_pending_recover_track_dialog();

        // 分组切换 → 同步 AppMode（必须在 sidebar.update 之后，因为 active_group 在那里改变）
        if matches!(&event, sidebar::Event::GroupToggled(_)) {
            match self.sidebar.active_group {
                Some(sidebar::GroupId::Waterfall) => {
                    self.state.current_mode = AppMode::Waterfall;
                    self.state.toggle_animation.animate_to(1.0);
                }
                _ => {
                    self.state.current_mode = AppMode::Editor;
                    self.state.toggle_animation.animate_to(0.0);
                }
            }
        }

        // 音频导出面板打开时，从设置自动填充音色库路径（用户选择可覆盖）
        if matches!(
            &event,
            sidebar::Event::RouteUpdated(sidebar::Route::AudioExport)
        ) && self.sidebar.audio_export_visible
            && self.state.audio_export_dialog.soundfont_path.is_empty()
        {
            self.state.audio_export_dialog.soundfont_path = self.settings.synth.soundfont_path.clone();
        }

        // 更新画布偏移
        let sidebar_width = self.sidebar.width() as f32;
        let current_offset_y = self.editor.editor_state.canvas.offset_y;
        self.editor
            .set_canvas_offset(iced_core::Point::new(sidebar_width, current_offset_y));

        // 如果是音轨切换，发送 Core 事件
        if let Some(track_idx) = track_selected_idx {
            tracing::debug!("Root: 发射音轨选择事件，音轨 {}", track_idx);
            crate::event::emit(crate::event::Event::Menu(crate::event::menu::Event::File(
                crate::event::menu::file::Event::TrackSelected(track_idx),
            )));
        }

        // 如果是添加音轨，根据用户设置决定是否切换到新音轨
        if matches!(&event, sidebar::Event::AddTrack) {
            if self.settings.editing.track_add_behavior == TrackAddBehavior::AutoSwitch {
                let track_idx = self
                    .sidebar
                    .tracks
                    .last()
                    .map(|track| track.id)
                    .unwrap_or(0);
                self.sidebar.selected_track = track_idx;
                tracing::debug!("Root: 添加音轨后自动选中新音轨 {}", track_idx);
                crate::event::emit(crate::event::Event::Menu(crate::event::menu::Event::File(
                    crate::event::menu::file::Event::TrackSelected(track_idx),
                )));
            } else {
                tracing::debug!(
                    "Root: 添加音轨，保持当前音轨 {} 不变",
                    self.sidebar.selected_track
                );
            }
        }

        needs_redraw
    }
}

/// 处理右侧栏动作
impl Root {
    pub(crate) fn handle_right_sidebar_action(
        &mut self,
        action: lumino_message::RightSidebarAction,
    ) -> bool {
        use lumino_message::RightSidebarAction::*;
        match action {
            ImageToMidiClicked => {
                use crate::right_sidebar::RightSidebarPanel;
                // 互斥路由：已在图片转 MIDI 面板 → 收起；否则切换到图片转 MIDI 面板。
                // 与素材库按钮对称——避免素材库打开后本按钮只收起面板、
                // active_panel 仍指向素材库导致无法切回。
                if self
                    .right_sidebar
                    .is_panel_active(RightSidebarPanel::ImageToMidi)
                {
                    self.right_sidebar.panel_visible = false;
                } else {
                    self.right_sidebar
                        .switch_panel(RightSidebarPanel::ImageToMidi);
                }
                tracing::info!(
                    "右侧栏图片转MIDI按钮被点击，面板{}",
                    if self.right_sidebar.panel_visible {
                        "展开"
                    } else {
                        "收起"
                    }
                );
                true
            }
            SelectImageFile => {
                // 面板内文件选择按钮：弹出对话框，让用户选择 i2m-rs 支持的图片文件
                // （PNG/JPEG/BMP/GIF/WebP/SVG），选中后标注路径。
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("选择要转换为 MIDI 的图片")
                    .add_filter(
                        "图片文件",
                        &["png", "jpg", "jpeg", "bmp", "gif", "webp", "svg"],
                    )
                    .add_filter("PNG 图片", &["png"])
                    .add_filter("JPEG 图片", &["jpg", "jpeg"])
                    .add_filter("BMP 图片", &["bmp"])
                    .add_filter("GIF 图片", &["gif"])
                    .add_filter("WebP 图片", &["webp"])
                    .add_filter("SVG 矢量图", &["svg"])
                    .add_filter("所有文件", &["*"])
                    .pick_file()
                {
                    self.right_sidebar.set_selected_image_path(path.clone());
                    tracing::info!("已选择图片转 MIDI 源文件: {}", path.display());
                }
                true
            }
            ResizeDragStarted => {
                // 运行时拖拽锚点初始化由 Host 层拦截处理（用当前光标位置调用
                // start_resize）；此处仅为测试/内部直达路径的防御性兜底。
                self.right_sidebar.is_resizing = true;
                true
            }
            ResizeDragged => {
                // 拖拽中更新宽度
                true
            }
            ResizeDragEnded => {
                self.right_sidebar.end_resize();
                true
            }
            ConvertClicked => {
                // 面板内转换按钮：后台线程执行 i2m-rs 转换，
                // 完成后由 poll_pending_i2m 轮询接收并强制切换到 Y 向选择工具。
                let Some(path) = self.right_sidebar.selected_image_path.clone() else {
                    return true;
                };
                // 标记转换中：面板按钮禁用 + 编辑器进入等待框选阶段
                self.right_sidebar.converting = true;
                // 记录转换前的工具，√ 写入成功后还原
                self.i2m_restore_tool = Some(self.toolbar.current_tool);
                self.editor.editor_state.image_to_midi.begin_converting();
                // 后台线程执行转换，结果通过 channel 回传
                let thread_path = path.clone();
                let thread_config = self.right_sidebar.config.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let result =
                        crate::right_sidebar::convert::run_conversion(&thread_path, &thread_config);
                    let _ = tx.send(result);
                });
                self.pending_i2m = Some(rx);
                tracing::info!("已启动图片转 MIDI 后台转换: {}", path.display());
                true
            }
            PlacementConfirm => {
                // 确认生成：调用 i2m-rs 逻辑在内存中写入数据（m8 实现）
                self.handle_i2m_placement_confirm();
                true
            }
            PlacementCancel => {
                // 取消生成：彻底退出放置模式，清除预览并还原显示区域
                // （× 按钮语义 = 完全退出；"仅清除区域框重新框选"由
                //   按下空白处 / 切换工具 的 clear_region 路径承担）
                self.editor.editor_state.image_to_midi.cancel();
                self.right_sidebar.converting = false;
                // 还原工具：切回转换前的工具（与 √ 写入成功后行为一致）
                if let Some(tool) = self.i2m_restore_tool.take() {
                    self.toolbar.current_tool = tool;
                    self.editor.set_tool(tool);
                }
                // 清理交互残留并强制刷新渲染：预览实例需由渲染线程全量
                // 重建清除（invalidate_caches 仅清网格缓存，不驱动音符实例）
                self.editor.editor_state.interaction.selected_notes.clear();
                self.editor.clear_pending_drag();
                self.editor.mark_notes_changed();
                self.update_playback_notes();
                self.editor.clear_notes_changed();
                self.editor
                    .invalidate_caches(lumino_ui_editor::CacheInvalidation::ALL);
                tracing::info!("图片转 MIDI 放置已取消");
                true
            }
            I2mConfigTextChanged { field, text } => {
                // 面板参数输入：仅接受数字，apply_text 内部 clamp 并同步文本缓冲
                self.right_sidebar.config.apply_text(field, &text);
                true
            }
            I2mPaletteChanged(index) => {
                // 面板调色板算法切换（索引指向 PALETTE_ALGORITHMS）
                if index < crate::right_sidebar::PALETTE_ALGORITHMS.len() {
                    self.right_sidebar.config.palette_index = index;
                }
                true
            }
            MaterialLibraryClicked => {
                use crate::right_sidebar::RightSidebarPanel;
                // 互斥路由：已在素材库面板 → 收起；否则切换到素材库面板
                if self
                    .right_sidebar
                    .is_panel_active(RightSidebarPanel::Materials)
                {
                    self.right_sidebar.panel_visible = false;
                } else {
                    self.right_sidebar
                        .switch_panel(RightSidebarPanel::Materials);
                    // 首次打开：惰性扫描素材列表（内置 + 用户配置目录）
                    if !self.right_sidebar.materials.is_initialized() {
                        self.start_material_scan();
                    }
                }
                tracing::info!(
                    "右侧栏素材库按钮被点击，面板{}",
                    if self.right_sidebar.panel_visible {
                        "展开"
                    } else {
                        "收起"
                    }
                );
                true
            }
            MaterialAddClicked => {
                // 展开/收起"添加素材"下拉菜单
                self.right_sidebar.materials.add_menu_open =
                    !self.right_sidebar.materials.add_menu_open;
                true
            }
            MaterialDownloadFromWeb => {
                // 云存储入口（素材模式）：无连接时 runner 会弹出连接面板引导
                self.right_sidebar.materials.add_menu_open = false;
                crate::event::emit(crate::event::Event::cloud(
                    crate::event::cloud::Event::OpenCloudPanel {
                        intent: "material".to_string(),
                    },
                ));
                true
            }
            MaterialImportFromLocal => {
                self.right_sidebar.materials.add_menu_open = false;
                self.import_material_from_local();
                true
            }
            MaterialAddMenuClosed => {
                self.right_sidebar.materials.add_menu_open = false;
                true
            }
            MaterialDragStarted(index) => {
                // 素材拖出：后台加载素材，预览跟随鼠标（由 poll 轮询接管）
                self.start_material_drag(index);
                true
            }
            MaterialContextMenuOpened(index) => {
                self.open_material_context_menu(index);
                true
            }
            MaterialCursorMoved(x, y) => {
                // 记录鼠标在素材面板内的实时位置（右键菜单弹出位置的数据源）
                self.right_sidebar.materials.update_cursor_pos(x, y);
                true
            }
            MaterialContextMenuClosed => {
                self.close_material_context_menu();
                true
            }
            MaterialContextMenuItemClicked(index, item) => {
                self.handle_material_context_menu_item_clicked(index, item);
                true
            }
            MaterialRenameInputChanged(text) => {
                self.handle_material_rename_input_changed(text);
                true
            }
            MaterialRenameConfirmed => {
                self.confirm_material_rename();
                true
            }
            MaterialRenameCancelled => {
                self.cancel_material_rename();
                true
            }
            MaterialDeleteConfirmed(index) => {
                self.confirm_material_delete(index);
                true
            }
            MaterialDeleteCancelled => {
                self.cancel_material_delete();
                true
            }
        }
    }

    /// 确认图片转 MIDI 生成：按逐轨写入/自动建轨策略写入 document
    ///
    /// - 颜色 0 写入当前音轨；
    /// - 颜色 1+ 优先复用现有非当前音轨，数量不足时才新建缺失数量的音轨
    ///   （sidebar + document 同步扩轨）；
    pub(crate) fn poll_pending_i2m(&mut self) {
        let rx = match self.pending_i2m.as_ref() {
            Some(rx) => rx,
            None => return,
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(_) => return, // Empty / Disconnected
        };
        self.pending_i2m = None;
        match result {
            Ok(preview) => {
                self.editor.editor_state.image_to_midi.set_preview(preview);
                self.right_sidebar.converting = false;
                // 强制切换到 Y 向选择工具，用户用其框选生成区域
                let tool = crate::toolbar::Tool::PointerYSelect;
                self.toolbar.current_tool = tool;
                self.editor.set_tool(tool);
                self.editor
                    .invalidate_caches(lumino_ui_editor::CacheInvalidation::ALL);
                tracing::info!("图片转 MIDI 转换完成，已强制切换到 Y 向选择工具");
            }
            Err(err) => {
                self.editor.editor_state.image_to_midi.cancel();
                self.right_sidebar.converting = false;
                // 转换失败：流程结束，清除原工具记录
                self.i2m_restore_tool = None;
                tracing::error!("图片转 MIDI 转换失败: {err}");
            }
        }
    }
}
