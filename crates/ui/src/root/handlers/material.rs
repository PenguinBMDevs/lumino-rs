//! 素材库交互处理器（右侧栏素材面板）
//!
//! 素材（.lmmaterial）的完整生命周期：
//! - 列表扫描（内置 + 用户配置目录）；
//! - 本地导入（复制到用户素材目录）；
//! - 拖出放置（加载到内存 → 预览跟随鼠标 → √/× 确认写入）。

use crate::root::Root;
use crate::sidebar;

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
    pub(super) fn start_material_scan(&mut self) {
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
}
