//! 素材库交互处理器（右侧栏素材面板）
//!
//! 素材（.lmmaterial）的完整生命周期：
//! - 列表扫描（内置 + 用户配置目录）；
//! - 本地导入（复制到用户素材目录）；
//! - 拖出放置（加载到内存 → 预览跟随鼠标 → √/× 确认写入）；
//! - 右键菜单（重命名 / 删除 / 上传到云）。

use lumino_message::MaterialContextMenuItem;

use crate::root::Root;
use crate::sidebar;
use crate::toast::ToastLevel;

impl Root {
    /// 素材项按下：立即进入拖出跟随模式（预览跟随鼠标）
    ///
    /// 素材预览在扫描时已预解析缓存（`MaterialEntry.preview`），
    /// 此处**同步**启动——按下即生效，不依赖异步轮询（修复拖放失效：
    /// 此前异步加载 + 消息驱动 poll，素材就绪时无消息触发轮询，拖放无响应）。
    pub(super) fn start_material_drag(&mut self, index: usize) {
        let Some(entry) = self.right_sidebar.materials.entries.get(index) else {
            return;
        };
        let Some(preview) = entry.preview.clone() else {
            tracing::warn!("素材 {} 无可用的放置预览，拖出已忽略", entry.name);
            return;
        };
        self.editor
            .editor_state
            .image_to_midi
            .begin_material_follow(preview, 0.0);
        self.editor
            .invalidate_caches(lumino_ui_editor::CacheInvalidation::ALL);
        tracing::info!("素材 {} 已进入拖出跟随模式", entry.name);
    }

    /// 清理过期的素材拖出跟随（鼠标已释放且未在卷帘内确认放置）
    ///
    /// 素材拖出 = 右侧栏按下 → 移入卷帘松手放置；若在右侧栏/空白处松手
    /// （卷帘 released 不会触发），跟随预览会残留——本方法兜底取消。
    pub(crate) fn cancel_stale_material_follow(&mut self) {
        use lumino_editor_state::ImageToMidiMode;
        let i2m = &self.editor.editor_state.image_to_midi;
        if i2m.mode == ImageToMidiMode::Selecting && i2m.drag_follow.is_some() {
            self.editor
                .editor_state
                .image_to_midi
                .cancel_material_follow();
            self.editor
                .invalidate_caches(lumino_ui_editor::CacheInvalidation::ALL);
            tracing::debug!("素材拖出已取消（未在卷帘内放置）");
        }
    }

    /// 开始后台扫描素材列表（内置 + 用户配置目录），完成后刷新面板
    pub(crate) fn start_material_scan(&mut self) {
        self.right_sidebar.materials.scanning = true;
        let user_dir = crate::right_sidebar::user_materials_dir();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let entries = crate::right_sidebar::scan_materials(&user_dir);
            let _ = tx.send(entries);
        });
        self.pending_material_scan = Some(rx);
    }

    /// 轮询素材扫描结果（后台扫描完成后刷新素材列表）
    pub(crate) fn poll_material_scan(&mut self) {
        let rx = match self.pending_material_scan.as_ref() {
            Some(rx) => rx,
            None => return,
        };
        let entries = match rx.try_recv() {
            Ok(entries) => entries,
            Err(_) => return, // Empty / Disconnected
        };
        self.pending_material_scan = None;
        self.right_sidebar.materials.scanning = false;
        self.right_sidebar.materials.entries = entries;
        tracing::info!(
            "素材库扫描完成：{} 个素材（内置 + 本地）",
            self.right_sidebar.materials.entries.len()
        );
    }

    /// 从本地选取 .lmmaterial 素材文件并导入
    ///
    /// 导入流程：文件对话框选择 → 复制到用户素材目录 → 重新扫描列表。
    pub(super) fn import_material_from_local(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("选择要导入的素材文件")
            .add_filter("Lumino 素材", &["lmmaterial"])
            .add_filter("所有文件", &["*"])
            .pick_file()
        else {
            return;
        };

        // 校验素材格式（从 metadata 判断是否为素材文件）
        let valid_material = lumino_export::load_project(&path)
            .map(|p| p.metadata.is_material_file())
            .unwrap_or(false);
        if !valid_material {
            tracing::error!("导入失败：{} 不是素材文件（.lmmaterial）", path.display());
            self.toast.push(
                crate::toast::ToastLevel::Error,
                "素材导入失败：不是有效的素材文件",
            );
            return;
        }

        let user_dir = crate::right_sidebar::user_materials_dir();
        match crate::right_sidebar::copy_material_to_user_dir(&path, &user_dir) {
            Ok(dest) => {
                tracing::info!("素材已导入并复制到用户素材目录: {:?}", dest);
                self.toast
                    .push(crate::toast::ToastLevel::Success, "素材已导入");
                // 重新扫描列表
                self.start_material_scan();
            }
            Err(e) => {
                tracing::error!("素材复制失败: {e}");
                self.toast.push(
                    crate::toast::ToastLevel::Error,
                    "素材导入失败：复制文件出错",
                );
            }
        }
    }

    /// 确认放置生成（I2M / 素材共用）：按逐轨写入/自动建轨策略写入 document
    ///
    /// - 轨 0 写入当前音轨；
    /// - 轨 1+ 优先复用现有非当前音轨，数量不足时才新建缺失数量的音轨
    ///   （sidebar + document 同步扩轨）；
    /// - 使用 `CreateOp` 操作日志记录（跨轨撤销/重做）。
    ///
    /// 素材放置复用此路径：素材音轨数 = preview.tracks.len()，
    /// Y 向偏移已由 `track_screen_notes` 应用（`note_screen_key`）。
    pub(super) fn handle_i2m_placement_confirm(&mut self) {
        use lumino_editor_state::ImageToMidiMode;

        // 快照放置状态（避免与后续 &mut self 借用冲突）
        let i2m = self.editor.editor_state.image_to_midi.clone();
        if i2m.mode != ImageToMidiMode::Placing {
            return;
        }
        let Some(preview) = &i2m.preview else {
            return;
        };

        let current_track = self.editor.editor_state.data.current_track;
        // 收集每轨音符（区域映射后的屏幕 tick/key/length）
        let mut tracks_data: Vec<Vec<(f32, u8, f32)>> = Vec::with_capacity(preview.tracks.len());
        let mut total_notes = 0usize;
        for (idx, _) in preview.tracks.iter().enumerate() {
            let notes = i2m.track_screen_notes(idx);
            total_notes += notes.len();
            tracks_data.push(notes);
        }
        if total_notes == 0 {
            return;
        }

        // 音轨分配策略：轨 0 始终写入当前音轨；轨 1+ 优先复用现有非当前音轨
        // （按侧边栏顺序取用），数量不足时才新建缺失数量的音轨——避免多次
        // 放置都无脑新建 N-1 条轨道，导致音轨无限膨胀。
        let needed_extra = preview.tracks.len().saturating_sub(1);
        let reused_tracks: Vec<usize> = self
            .sidebar
            .tracks
            .iter()
            .map(|t| t.id)
            .filter(|id| *id != current_track)
            .take(needed_extra)
            .collect();
        let deficit = needed_extra.saturating_sub(reused_tracks.len());

        // 自动建轨：仅为不足的数量新建音轨（sidebar + document 同步）
        let before: std::collections::HashSet<usize> =
            self.sidebar.tracks.iter().map(|t| t.id).collect();
        for _ in 0..deficit {
            self.sidebar.update(sidebar::Event::AddTrack);
        }
        let new_track_ids: Vec<usize> = self
            .sidebar
            .tracks
            .iter()
            .filter(|t| !before.contains(&t.id))
            .map(|t| t.id)
            .collect();

        // 逐轨写入（轨 0 → 当前轨，轨 1+ → 复用的现有轨或新建轨）
        let mut create_ops: Vec<lumino_note_core::history::CreateOp> = Vec::new();
        let mut affected = std::collections::HashSet::new();
        for (color_idx, notes) in tracks_data.iter().enumerate() {
            if notes.is_empty() {
                continue;
            }
            let target_track = if color_idx == 0 {
                current_track
            } else {
                let reuse_idx = color_idx - 1;
                reused_tracks
                    .get(reuse_idx)
                    .copied()
                    .or_else(|| new_track_ids.get(reuse_idx - reused_tracks.len()).copied())
                    .unwrap_or(current_track)
            };
            if !self.editor.editor_state.data.ensure_track(target_track) {
                continue;
            }
            for &(tick, key, length) in notes {
                // 批量归一化：区域等比映射产生亚 tick 数值（如 12418.724），
                // 写入前统一 round 为整数 tick/长度——既保证 note_to_event 对
                // tick 与 tick+length 的 round 结果一致（长度不变形），也从源头
                // 消除非整数 tick（f32_to_tick 因此走快速路径，零日志、零阻塞）。
                let tick = tick.round();
                let length = length.round().max(1.0);
                let note = lumino_note_core::note::Note::new(tick, u16::from(key), length);
                let event = lumino_editor_state::note_to_event(note.clone());
                if self
                    .editor
                    .editor_state
                    .data
                    .insert_note(target_track, note)
                {
                    create_ops.push(lumino_note_core::history::CreateOp {
                        track_id: target_track as u32,
                        note: event,
                    });
                }
            }
            affected.insert(target_track);
        }

        // 历史记录（跨轨撤销）+ 标记变化（洋葱皮增量：明确受影响音轨）
        if !create_ops.is_empty() {
            self.editor
                .editor_state
                .data
                .history
                .push_note_create(create_ops);
            self.editor
                .editor_state
                .data
                .mark_track_notes_changed_for(Some(affected));
        }

        // 清除放置模式，还原显示区域
        self.editor.editor_state.image_to_midi.cancel();
        self.right_sidebar.converting = false;
        // 完全还原工具：切回转换前的工具（√ 写入成功后流程结束）
        if let Some(tool) = self.i2m_restore_tool.take() {
            self.toolbar.current_tool = tool;
            self.editor.set_tool(tool);
        }
        // 清理放置前残留的交互状态：写入改变了音符索引，残留的选中集合与
        // pending_drag_state 仍指向写入前的索引，保留会导致后续调整音符长度时
        // 触发批量 ResizingSelection（连带周围音符长度改变）或 ghost 误偏移。
        self.editor.editor_state.interaction.selected_notes.clear();
        self.editor.clear_pending_drag();
        self.editor.mark_notes_changed();
        self.update_playback_notes();
        self.editor.clear_notes_changed();
        self.editor
            .invalidate_caches(lumino_ui_editor::CacheInvalidation::ALL);

        tracing::info!("放置写入完成：{} 个音符", total_notes);
    }

    // ── 素材右键菜单 ──

    /// 处理打开素材右键菜单
    pub(super) fn open_material_context_menu(&mut self, index: usize) {
        if self.right_sidebar.materials.entries.get(index).is_none() {
            return;
        }
        // 互斥：关闭其他浮动状态
        self.right_sidebar.materials.add_menu_open = false;
        self.right_sidebar.materials.renaming_material = None;
        self.right_sidebar.materials.pending_delete = None;
        // 快照当前鼠标位置（面板局部坐标）作为菜单弹出位置；
        // 菜单打开期间该位置冻结，不跟随鼠标移动
        self.right_sidebar.materials.context_menu_pos =
            self.right_sidebar.materials.context_cursor_pos;
        self.right_sidebar.materials.context_menu_target = Some(index);
    }

    /// 处理关闭素材右键菜单
    pub(super) fn close_material_context_menu(&mut self) {
        self.right_sidebar.materials.context_menu_target = None;
        self.right_sidebar.materials.context_menu_pos = None;
    }

    /// 处理点击素材右键菜单项
    pub(super) fn handle_material_context_menu_item_clicked(
        &mut self,
        index: usize,
        item: MaterialContextMenuItem,
    ) {
        self.right_sidebar.materials.context_menu_target = None;
        match item {
            MaterialContextMenuItem::Rename => {
                // 仅用户素材可重命名（内置素材的按钮已置灰，此处为防御）
                if let Some(entry) = self.right_sidebar.materials.entries.get(index)
                    && entry.path.is_some()
                {
                    self.right_sidebar.materials.renaming_material =
                        Some((index, entry.name.clone()));
                }
            }
            MaterialContextMenuItem::Delete => {
                // 仅用户素材可删除（内置素材的按钮已置灰，此处为防御）
                if let Some(entry) = self.right_sidebar.materials.entries.get(index)
                    && entry.path.is_some()
                {
                    // 进入删除确认态：主窗口叠加覆盖层弹窗展示确认卡片
                    self.right_sidebar.materials.pending_delete = Some(index);
                    self.right_sidebar.materials.pending_delete_name = Some(entry.name.clone());
                }
            }
            MaterialContextMenuItem::UploadToCloud => {
                // 仅用户素材可上传到云（内置素材为程序资产，按钮已置灰，此处为防御）
                if let Some(entry) = self.right_sidebar.materials.entries.get(index)
                    && entry.path.is_some()
                {
                    self.upload_material_to_cloud(index);
                }
            }
        }
    }

    /// 处理素材重命名输入变化
    pub(super) fn handle_material_rename_input_changed(&mut self, value: String) {
        if let Some((_, buffer)) = &mut self.right_sidebar.materials.renaming_material {
            *buffer = value;
        }
    }

    /// 处理取消素材重命名
    pub(super) fn cancel_material_rename(&mut self) {
        self.right_sidebar.materials.renaming_material = None;
    }

    /// 处理确认素材重命名
    ///
    /// 流程：加载工程 → 写入新名称（文件 + metadata 同步）→ 删除旧文件 → 重新扫描。
    /// 与素材显示名规则一致：`metadata.project.name` 优先，故必须双改。
    pub(super) fn confirm_material_rename(&mut self) {
        let Some((index, buffer)) = self.right_sidebar.materials.renaming_material.take() else {
            return;
        };
        let new_name = buffer.trim().replace(['/', '\\'], "_");
        if new_name.is_empty() || new_name == "." || new_name == ".." {
            self.toast.push(ToastLevel::Error, "素材名称不能为空");
            return;
        }
        let Some(entry) = self.right_sidebar.materials.entries.get(index) else {
            return;
        };
        let Some(old_path) = &entry.path else {
            self.toast.push(ToastLevel::Error, "内置素材不可重命名");
            return;
        };
        // 新路径 = 用户素材目录 / 新名称.lmmaterial（与导入落点一致）
        let user_dir = crate::right_sidebar::user_materials_dir();
        let new_path = user_dir.join(format!("{new_name}.lmmaterial"));
        if new_path.exists() {
            self.toast.push(ToastLevel::Error, "已存在同名素材");
            return;
        }
        // 加载工程 → 以新名称重新保存（同步 metadata.project.name）→ 删除旧文件
        match lumino_export::load_project(old_path) {
            Ok(project) => {
                if let Err(e) = lumino_export::save_material(&project, &new_name, &new_path) {
                    self.toast
                        .push(ToastLevel::Error, format!("素材重命名失败：{e}"));
                    return;
                }
                if let Err(e) = std::fs::remove_file(old_path) {
                    // 新文件已保存；旧文件删除失败会导致列表出现两份，提示用户处理
                    tracing::warn!("素材重命名后旧文件删除失败: {e}");
                    self.toast.push(
                        ToastLevel::Error,
                        format!("素材已保存为新名称，但旧文件删除失败：{e}"),
                    );
                } else {
                    self.toast.push(ToastLevel::Success, "素材已重命名");
                }
                self.start_material_scan();
            }
            Err(e) => {
                self.toast.push(
                    ToastLevel::Error,
                    format!("素材重命名失败：无法读取原文件 {e}"),
                );
            }
        }
    }

    /// 处理取消素材删除确认
    ///
    /// 覆盖层确认卡片的[取消]按钮/点击遮罩调用，清除确认态。
    pub(super) fn cancel_material_delete(&mut self) {
        self.right_sidebar.materials.pending_delete = None;
        self.right_sidebar.materials.pending_delete_name = None;
    }

    /// 处理确认素材删除（删除本地文件并重新扫描）
    ///
    /// `index` 必须与当前待确认索引一致（防御：只允许确认当前卡片对应的素材项）。
    /// 覆盖层确认卡片的[删除]按钮调用。
    pub(super) fn confirm_material_delete(&mut self, index: usize) {
        if self.right_sidebar.materials.pending_delete != Some(index) {
            return;
        }
        self.right_sidebar.materials.pending_delete = None;
        self.right_sidebar.materials.pending_delete_name = None;
        let Some(entry) = self.right_sidebar.materials.entries.get(index) else {
            return;
        };
        let Some(path) = &entry.path else {
            self.toast.push(ToastLevel::Error, "内置素材不可删除");
            return;
        };
        match std::fs::remove_file(path) {
            Ok(()) => {
                tracing::info!("素材已删除: {}", path.display());
                self.toast.push(ToastLevel::Success, "素材已删除");
                self.start_material_scan();
            }
            Err(e) => {
                tracing::error!("素材删除失败: {e}");
                self.toast
                    .push(ToastLevel::Error, format!("素材删除失败：{e}"));
            }
        }
    }

    /// 上传素材到云：设置待办并打开云存储文件管理面板（选择上传位置）
    ///
    /// - 无在线连接：runner 分流弹出云存储连接面板引导配置；
    /// - 有在线连接：打开云浏览面板（保存模式），用户选择目录后点"保存到此处"。
    ///
    /// 仅用户素材可上传（调用方已做防御门；内置素材无磁盘路径，不可上传）。
    pub(super) fn upload_material_to_cloud(&mut self, index: usize) {
        let Some(entry) = self.right_sidebar.materials.entries.get(index) else {
            return;
        };
        if !entry.valid {
            self.toast.push(ToastLevel::Error, "素材无效，无法上传");
            return;
        }
        let Some(path) = &entry.path else {
            self.toast.push(ToastLevel::Error, "内置素材不可上传");
            return;
        };
        // 远程文件名：素材显示名 + 扩展名；过滤路径分隔符（防止创建子路径）
        let file_name = format!("{}.lmmaterial", entry.name.replace(['/', '\\'], "_"));

        // 设置上传待办（云浏览面板"保存到此处"时消费）
        self.cloud.pending_upload = Some(crate::state::cloud_state::PendingUpload {
            local_path: path.to_string_lossy().into_owned(),
            file_name,
        });
        // 云入口分流：无连接 → runner 弹出连接面板；已连接 → 浏览面板（保存模式）
        crate::event::emit(crate::event::Event::cloud(
            crate::event::cloud::Event::OpenCloudPanel {
                intent: "material_upload".to_string(),
            },
        ));
        tracing::info!("素材 {} 上传到云流程已启动", entry.name);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::right_sidebar::MaterialSource;
    use crate::root::Root;
    use lumino_core::storage::config::UiConfig;

    fn create_root() -> Root {
        Root::new(&UiConfig::default())
    }

    /// 创建唯一临时目录（无 tempfile 依赖，测试后由操作系统清理）
    fn make_tmp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lumino_mat_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("临时目录应创建成功");
        dir
    }

    /// 构造用户素材条目（可指定磁盘路径）
    fn user_entry(path: Option<PathBuf>) -> crate::right_sidebar::MaterialEntry {
        crate::right_sidebar::MaterialEntry {
            name: "测试素材".into(),
            author: String::new(),
            source: MaterialSource::User,
            path,
            data: None,
            multi_track: false,
            track_count: 1,
            valid: true,
            preview: None,
        }
    }

    #[test]
    fn test_open_context_menu_sets_target_and_clears_others() {
        let mut root = create_root();
        root.right_sidebar.materials.entries.push(user_entry(None));
        root.right_sidebar.materials.renaming_material = Some((0, "旧名".into()));
        root.right_sidebar.materials.pending_delete = Some(0);
        root.right_sidebar.materials.add_menu_open = true;

        root.open_material_context_menu(0);
        assert_eq!(root.right_sidebar.materials.context_menu_target, Some(0));
        assert!(root.right_sidebar.materials.renaming_material.is_none());
        assert!(root.right_sidebar.materials.pending_delete.is_none());
        assert!(!root.right_sidebar.materials.add_menu_open);
    }

    #[test]
    fn test_open_context_menu_ignores_invalid_index() {
        let mut root = create_root();
        root.open_material_context_menu(99);
        assert!(root.right_sidebar.materials.context_menu_target.is_none());
    }

    #[test]
    fn test_context_menu_rename_starts_inline_edit() {
        let mut root = create_root();
        root.right_sidebar
            .materials
            .entries
            .push(user_entry(Some(PathBuf::from("C:/tmp/a.lmmaterial"))));
        root.open_material_context_menu(0);

        root.handle_material_context_menu_item_clicked(0, MaterialContextMenuItem::Rename);
        assert!(root.right_sidebar.materials.context_menu_target.is_none());
        assert_eq!(
            root.right_sidebar.materials.renaming_material,
            Some((0, "测试素材".into()))
        );
    }

    #[test]
    fn test_context_menu_rename_ignored_for_builtin() {
        // 内置素材无磁盘路径：菜单按钮已置灰，此处验证防御逻辑
        let mut root = create_root();
        root.right_sidebar.materials.entries.push(crate::right_sidebar::MaterialEntry {
            name: "内置".into(),
            author: String::new(),
            source: MaterialSource::BuiltIn,
            path: None,
            data: None,
            multi_track: false,
            track_count: 1,
            valid: true,
            preview: None,
        });
        root.open_material_context_menu(0);

        root.handle_material_context_menu_item_clicked(0, MaterialContextMenuItem::Rename);
        assert!(root.right_sidebar.materials.renaming_material.is_none());
    }

    #[test]
    fn test_rename_input_updates_buffer() {
        let mut root = create_root();
        root.right_sidebar.materials.renaming_material = Some((0, "旧名".into()));
        root.handle_material_rename_input_changed("新名".into());
        assert_eq!(
            root.right_sidebar.materials.renaming_material,
            Some((0, "新名".into()))
        );
    }

    #[test]
    fn test_rename_confirmed_empty_name_rejected() {
        let mut root = create_root();
        root.right_sidebar.materials.renaming_material = Some((0, "   ".into()));
        root.confirm_material_rename();
        // 空名被拒绝：不 panic，编辑态已清除
        assert!(root.right_sidebar.materials.renaming_material.is_none());
    }

    #[test]
    fn test_confirm_rename_missing_entry_noop() {
        let mut root = create_root();
        root.right_sidebar.materials.renaming_material = Some((99, "新名".into()));
        root.confirm_material_rename();
        assert!(root.right_sidebar.materials.renaming_material.is_none());
    }

    #[test]
    fn test_context_menu_delete_enters_confirm_state() {
        let mut root = create_root();
        root.right_sidebar
            .materials
            .entries
            .push(user_entry(Some(PathBuf::from("C:/tmp/a.lmmaterial"))));
        root.open_material_context_menu(0);

        root.handle_material_context_menu_item_clicked(0, MaterialContextMenuItem::Delete);
        assert!(root.right_sidebar.materials.context_menu_target.is_none());
        // 确认态 + 素材名快照（独立对话框窗口展示用）
        assert_eq!(root.right_sidebar.materials.pending_delete, Some(0));
        assert_eq!(
            root.right_sidebar.materials.pending_delete_name.as_deref(),
            Some("测试素材")
        );
    }

    #[test]
    fn test_delete_sets_confirm_snapshot() {
        // 右键删除：确认态 + 素材名快照（覆盖层确认卡片展示用）
        let mut root = create_root();
        root.right_sidebar
            .materials
            .entries
            .push(user_entry(Some(PathBuf::from("C:/tmp/a.lmmaterial"))));
        root.open_material_context_menu(0);
        root.handle_material_context_menu_item_clicked(0, MaterialContextMenuItem::Delete);
        assert_eq!(root.right_sidebar.materials.pending_delete, Some(0));
        assert_eq!(
            root.right_sidebar.materials.pending_delete_name.as_deref(),
            Some("测试素材")
        );
    }

    #[test]
    fn test_cancel_delete_clears_snapshot() {
        let mut root = create_root();
        root.right_sidebar
            .materials
            .entries
            .push(user_entry(Some(PathBuf::from("C:/tmp/a.lmmaterial"))));
        root.open_material_context_menu(0);
        root.handle_material_context_menu_item_clicked(0, MaterialContextMenuItem::Delete);
        root.cancel_material_delete();
        assert!(root.right_sidebar.materials.pending_delete.is_none());
        assert!(root.right_sidebar.materials.pending_delete_name.is_none());
    }

    #[test]
    fn test_confirm_delete_removes_file() {
        let dir = make_tmp_dir();
        let file = dir.join("a.lmmaterial");
        std::fs::write(&file, b"lmpj").expect("写入临时素材失败");

        let mut root = create_root();
        root.right_sidebar
            .materials
            .entries
            .push(user_entry(Some(file.clone())));
        root.right_sidebar.materials.pending_delete = Some(0);

        root.confirm_material_delete(0);
        assert!(!file.exists(), "素材文件应被删除");
        assert!(root.right_sidebar.materials.pending_delete.is_none());
    }

    #[test]
    fn test_confirm_delete_wrong_index_ignored() {
        let dir = make_tmp_dir();
        let file = dir.join("a.lmmaterial");
        std::fs::write(&file, b"lmpj").expect("写入临时素材失败");

        let mut root = create_root();
        root.right_sidebar
            .materials
            .entries
            .push(user_entry(Some(file.clone())));
        root.right_sidebar.materials.pending_delete = Some(0);

        // 索引不匹配：防御性忽略（不删除）
        root.confirm_material_delete(1);
        assert!(file.exists(), "索引不匹配时不应删除文件");
        assert_eq!(root.right_sidebar.materials.pending_delete, Some(0));
    }

    #[test]
    fn test_open_context_menu_snapshots_cursor_pos() {
        let mut root = create_root();
        root.right_sidebar.materials.entries.push(user_entry(None));
        root.right_sidebar.materials.update_cursor_pos(120.0, 80.0);

        root.open_material_context_menu(0);
        // 菜单位置 = 打开瞬间的光标位置快照（面板局部坐标）
        assert_eq!(
            root.right_sidebar.materials.context_menu_pos,
            Some((120.0, 80.0))
        );
        // 菜单打开期间光标移动：弹出位置保持冻结，不跟随鼠标漂移
        root.right_sidebar.materials.update_cursor_pos(300.0, 200.0);
        assert_eq!(
            root.right_sidebar.materials.context_menu_pos,
            Some((120.0, 80.0))
        );
    }

    #[test]
    fn test_close_context_menu_clears_snapshot() {
        let mut root = create_root();
        root.right_sidebar.materials.entries.push(user_entry(None));
        root.right_sidebar.materials.update_cursor_pos(10.0, 20.0);
        root.open_material_context_menu(0);

        root.close_material_context_menu();
        assert!(root.right_sidebar.materials.context_menu_target.is_none());
        assert!(root.right_sidebar.materials.context_menu_pos.is_none());
        // 实时光标位置保留，供下次打开菜单使用
        assert_eq!(
            root.right_sidebar.materials.context_cursor_pos,
            Some((10.0, 20.0))
        );
    }

    #[test]
    fn test_upload_to_cloud_sets_pending_upload_for_user_material() {
        let mut root = create_root();
        let path = PathBuf::from("C:/tmp/素材.lmmaterial");
        root.right_sidebar
            .materials
            .entries
            .push(user_entry(Some(path.clone())));
        root.open_material_context_menu(0);

        root.handle_material_context_menu_item_clicked(0, MaterialContextMenuItem::UploadToCloud);
        let pending = root.cloud.pending_upload.expect("应设置上传待办");
        assert_eq!(pending.local_path, path.to_string_lossy());
        assert_eq!(pending.file_name, "测试素材.lmmaterial");
    }

    #[test]
    fn test_upload_to_cloud_builtin_rejected() {
        // 内置素材不支持上传到云（按钮已置灰，此处验证防御逻辑）
        let mut root = create_root();
        root.right_sidebar.materials.entries.push(crate::right_sidebar::MaterialEntry {
            name: "内置素材".into(),
            author: String::new(),
            source: MaterialSource::BuiltIn,
            path: None,
            data: Some(&[0x4C, 0x4D, 0x50, 0x4A]), // LMPJ
            multi_track: false,
            track_count: 1,
            valid: true,
            preview: None,
        });
        root.open_material_context_menu(0);

        root.handle_material_context_menu_item_clicked(0, MaterialContextMenuItem::UploadToCloud);
        assert!(
            root.cloud.pending_upload.is_none(),
            "内置素材不应设置上传待办"
        );
    }

    #[test]
    fn test_upload_to_cloud_invalid_material_rejected() {
        let mut root = create_root();
        root.right_sidebar.materials.entries.push(crate::right_sidebar::MaterialEntry {
            name: "坏素材".into(),
            author: String::new(),
            source: MaterialSource::BuiltIn,
            path: None,
            data: None,
            multi_track: false,
            track_count: 0,
            valid: false,
            preview: None,
        });
        root.open_material_context_menu(0);

        root.handle_material_context_menu_item_clicked(0, MaterialContextMenuItem::UploadToCloud);
        assert!(root.cloud.pending_upload.is_none(), "无效素材不应设置上传待办");
    }
}
