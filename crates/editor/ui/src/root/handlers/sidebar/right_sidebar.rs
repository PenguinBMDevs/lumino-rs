//! 右侧栏动作处理器
//!
//! 处理 `Message::RightSidebar`（图片转 MIDI / 素材库）相关动作。

use crate::root::Root;

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
            PianoWaterfallKeyCountToggled => {
                // 键数在 128 ⇄ 256 间切换，并清空缓存强制重绘
                let state = &mut self.right_sidebar.piano_waterfall;
                state.key_count = if state.key_count <= 128 { 256 } else { 128 };
                state.handle = None;
                state.cached_signature = None;
                tracing::info!("钢琴瀑布流键盘键数切换为 {}", state.key_count);
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
            PianoWaterfallClicked => {
                use crate::right_sidebar::RightSidebarPanel;
                // 互斥路由：已在钢琴瀑布流预览面板 → 收起；否则切换到该面板。
                // 与其他右侧栏按钮对称：避免其他面板打开后本按钮只收起面板、
                // active_panel 仍指向旧面板导致无法切回。
                if self
                    .right_sidebar
                    .is_panel_active(RightSidebarPanel::PianoWaterfall)
                {
                    self.right_sidebar.panel_visible = false;
                } else {
                    self.right_sidebar
                        .switch_panel(RightSidebarPanel::PianoWaterfall);
                }
                tracing::info!(
                    "右侧栏钢琴瀑布流预览按钮被点击，面板{}",
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
}
