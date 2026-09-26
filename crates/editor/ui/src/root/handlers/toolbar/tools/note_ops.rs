//! 工具栏音符编辑操作
//!
//! 量化、变速、翻转、移调、连奏、分割/合并等音符编辑操作。

use super::super::ToolbarHandler;
use crate::root::Root;
use lumino_editor_state::CollabTransformSyncEntry;

impl ToolbarHandler {
    /// 钢琴卷帘变速：作用于 `selected_notes`（单轨索引位图）
    ///
    /// 从 `handle_toolbar_speed_change` 抽出，使视图分派保持 `match` 扁平可读，
    /// 且两视图的变速路径各自独立成方法——将来任一路径演进不影响另一条。
    fn apply_piano_roll_speed_change(root: &mut Root, speed_factor: f32) {
        let selected_count = root.editor.editor_state.interaction.selected_notes.len();

        if root.editor.editor_state.data.current_track_note_count() == 0 {
            tracing::debug!("Root: 没有音符需要变速");
            return;
        }

        // 必须有选中音符才能变速（无选中时对整个音轨变速是灾难性的）
        if selected_count == 0 {
            tracing::debug!("Root: 没有选中音符，不执行变速");
            return;
        }

        tracing::info!(
            "Root: 变速配置 - 速度因子: {}, 选中 {} 个音符",
            speed_factor,
            selected_count,
        );

        let modified = root.editor.apply_speed_change(speed_factor);

        if modified > 0 {
            tracing::info!("Root: 变速完成，修改了 {} 个音符", modified);
            root.update_playback_notes();
            root.editor.clear_notes_changed();
        } else {
            tracing::debug!("Root: 没有音符被变速（长度未变化）");
        }
    }

    /// 处理量化
    pub(crate) fn handle_toolbar_quantize(&self, root: &mut Root, event: &crate::toolbar::Event) {
        if !matches!(event, crate::toolbar::Event::Quantize) {
            return;
        }

        if !Self::arrangement_batch_gate(root, "量化") {
            return;
        }

        tracing::info!("Root: 执行量化操作");

        // 使用当前视觉网格线间隔作为量化网格，与显示保持一致
        let zoom_x = root.editor.editor_state.view.zoom_x;
        let ppq = root.editor.editor_state.view.ppq as f32;
        let grid_size = crate::editor::grid::utils::adaptive_grid_gap(zoom_x, ppq);

        if root.editor.editor_state.data.current_track_note_count() == 0 {
            tracing::debug!("Root: 没有音符需要量化");
            return;
        }

        let config = lumino_midi_loader::quantize::QuantizeConfig::new(grid_size, 1.0);

        // 获取选中音符索引（无选中则量化全部）
        let selected_indices: Vec<usize> = {
            let selected = &root.editor.editor_state.interaction.selected_notes;
            if selected.is_empty() {
                (0..root.editor.editor_state.data.current_track_note_count()).collect()
            } else {
                let mut v: Vec<usize> = selected.iter().collect();
                v.sort();
                v
            }
        };

        tracing::info!(
            "Root: 量化配置 - 网格大小: {} ticks, 目标音符: {} (选中 {} 个)",
            grid_size,
            selected_indices.len(),
            root.editor.editor_state.interaction.selected_notes.len(),
        );

        let snapshot = crate::editor::history::EditorSnapshot::new(
            std::sync::Arc::new(root.editor.editor_state.data.current_track_notes().clone()),
            root.editor.editor_state.data.current_track,
            root.editor.editor_state.data.automation_lanes.clone(),
        );
        root.editor.editor_state.data.history.push(snapshot);

        let mut quantizable_notes: Vec<lumino_midi_loader::quantize::QuantizableNote> =
            selected_indices
                .iter()
                .map(|&i| {
                    let note = &root.editor.editor_state.data.current_track_notes()[i];
                    lumino_midi_loader::quantize::QuantizableNote::new(
                        note.start_tick as f32,
                        (note.end_tick - note.start_tick) as f32,
                    )
                })
                .collect();

        // 2026-09 协作修复：量化会改变 tick/length，须广播旧→新让 B 端同步。
        // 提前捕获每个选中音符的旧状态（vel/ch 不变，key 不变）。
        let old_notes: Vec<(f32, u16, f32, u8, u8)> = selected_indices
            .iter()
            .map(|&i| {
                let note = &root.editor.editor_state.data.current_track_notes()[i];
                (
                    note.start_tick as f32,
                    note.key as u16,
                    (note.end_tick - note.start_tick) as f32,
                    note.velocity,
                    note.channel,
                )
            })
            .collect();

        let modified_count =
            lumino_midi_loader::quantize::quantize_notes(&mut quantizable_notes, &config);

        if modified_count > 0 {
            // 主选择漂移防护：量化可越过未量化音符 → 重排会位移选中索引（按值捕获）。
            let identity = root.editor.capture_selection_identity();
            // 量化仅改 tick/length（按值）：直接改 document，不记 ID。
            for (pos, &i) in selected_indices.iter().enumerate() {
                if let Some(note) = root
                    .editor
                    .editor_state
                    .data
                    .document
                    .as_mut()
                    .and_then(|doc| {
                        doc.track_notes_mut(root.editor.editor_state.data.current_track)
                    })
                    .and_then(|track| track.get_mut(i))
                {
                    let new_tick = lumino_editor_state::f32_to_tick(quantizable_notes[pos].tick);
                    let new_length =
                        lumino_editor_state::f32_to_tick(quantizable_notes[pos].length);
                    note.end_tick = new_tick.saturating_add(new_length.max(1));
                    note.start_tick = new_tick;
                }
            }

            // 子集量化可跨越未量化音符的 tick → 破坏「按 start_tick 升序」不变式
            // （window_range/position_of 窗口二分依赖，破坏后渲染/命中会漏检音符）
            // → 立即恢复；重排时按受影响闭区间增量更新（替代全量重建）。
            root.editor
                .editor_state
                .data
                .restore_current_track_sorted_incremental(&selected_indices);
            root.editor.remap_selection_by_identity(&identity);

            // 2026-09 去 ID 协作修复：仅对真正变化的音符发「删旧 + 加新」（按值，key/vel/ch 不变）。
            let track = root.editor.editor_state.data.current_track;
            let mut entries: Vec<CollabTransformSyncEntry> = Vec::new();
            for (pos, old) in old_notes.iter().enumerate() {
                let new_tick = quantizable_notes[pos].tick;
                let new_length = quantizable_notes[pos].length;
                if (new_tick, new_length) != (old.0, old.2) {
                    entries.push((false, old.0, old.1, old.2, old.3, old.4, track));
                    entries.push((true, new_tick, old.1, new_length, old.3, old.4, track));
                }
            }
            root.editor
                .editor_state
                .data
                .push_collab_transform_entries(entries);
            root.editor.broadcast_pending_collab_transform_sync();

            root.editor.mark_notes_changed();
            tracing::info!("Root: 量化完成，修改了 {} 个音符", modified_count);
        } else {
            root.editor.editor_state.data.history.discard_last();
            tracing::debug!("Root: 没有音符被量化");
        }

        if root.editor.notes_changed() {
            root.update_playback_notes();
            root.editor.clear_notes_changed();
        }
    }

    /// 处理音符变速
    ///
    /// - 普通点击：直接使用当前 speed_factor 执行变速
    /// - Ctrl+点击：打开变速对话框，让用户输入自定义倍率
    pub(crate) fn handle_toolbar_speed_change(
        &self,
        root: &mut Root,
        event: &crate::toolbar::Event,
    ) {
        if !matches!(event, crate::toolbar::Event::SpeedChange) {
            return;
        }

        // Ctrl+点击：打开独立对话框窗口
        if root.toolbar.ctrl_pressed {
            tracing::info!("Root: Ctrl+点击变速按钮，打开变速对话框窗口");
            crate::event::emit(crate::event::Event::Window(
                crate::event::window::Event::open_speed_change_dialog(),
            ));
            return;
        }

        // 普通点击：直接执行变速
        tracing::info!("Root: 执行音符变速操作");

        let speed_factor = root.toolbar.speed_factor;

        // 视图仲裁（收口）：`match` 而非 `if`——将来新增编辑视图时此处编译失败，
        // 强制为新视图补齐批量操作路径，不会静默落到卷帘分支误伤整轨
        match root.edit_view() {
            lumino_ui_editor::EditView::Arrangement => {
                // ---------- 工程走带：作用于 arrange_selection（跨轨） ----------
                // 不在此重复判空：`arrange_apply_speed_change` 内部已按
                // `arrange_selection` 取数并在无命中音符时返回 0 + 日志。
                // 调用方再判一次既是冗余，也会形成「自行判选区」的旁路——
                // 正是视图仲裁收口要消灭的写法。
                tracing::info!("Root: 工程走带变速 - 速度因子: {}", speed_factor);

                let modified = root.editor.arrange_apply_speed_change(speed_factor);

                if modified > 0 {
                    tracing::info!("Root: 工程走带变速完成，修改了 {} 个音符", modified);
                    root.update_playback_notes();
                    root.editor.clear_notes_changed();
                } else {
                    tracing::debug!("Root: 工程走带变速 - 没有音符被修改（无走带选区或长度未变）");
                }
            }
            lumino_ui_editor::EditView::PianoRoll => {
                Self::apply_piano_roll_speed_change(root, speed_factor);
            }
        }
    }

    /// 处理垂直翻转
    pub(crate) fn handle_toolbar_flip_vertical(
        &self,
        root: &mut Root,
        event: &crate::toolbar::Event,
    ) {
        if !matches!(event, crate::toolbar::Event::FlipVertical) {
            return;
        }

        if !Self::arrangement_batch_gate(root, "垂直翻转") {
            return;
        }

        tracing::info!("Root: 执行垂直翻转操作");

        let modified = root.editor.flip_selected_notes_vertical();

        if modified > 0 {
            tracing::info!("Root: 垂直翻转完成，修改了 {} 个音符", modified);
            root.update_playback_notes();
            root.editor.clear_notes_changed();
        } else {
            tracing::debug!("Root: 没有音符被翻转（无选中音符）");
        }
    }

    /// 处理水平翻转
    pub(crate) fn handle_toolbar_flip_horizontal(
        &self,
        root: &mut Root,
        event: &crate::toolbar::Event,
    ) {
        let mode = match event {
            crate::toolbar::Event::FlipHorizontal(mode) => *mode,
            _ => return,
        };

        if !Self::arrangement_batch_gate(root, "水平翻转") {
            return;
        }

        tracing::info!("Root: 执行水平翻转操作，模式: {:?}", mode);

        let modified = root.editor.flip_selected_notes_horizontal(mode);

        if modified > 0 {
            tracing::info!("Root: 水平翻转完成，修改了 {} 个音符", modified);
            root.update_playback_notes();
            root.editor.clear_notes_changed();
        } else {
            tracing::debug!("Root: 没有音符被翻转（无选中音符）");
        }
    }

    /// 处理移调操作
    ///
    /// - 普通点击：按 ±1 半音移调选中音符
    /// - Ctrl+点击：按 ±12 半音（一个八度）移调选中音符
    pub(crate) fn handle_toolbar_transpose(&self, root: &mut Root, event: &crate::toolbar::Event) {
        let semitones = match event {
            crate::toolbar::Event::TransposeUp(s) => *s,
            crate::toolbar::Event::TransposeDown(s) => -*s,
            _ => return,
        };

        if !Self::arrangement_batch_gate(root, "移调") {
            return;
        }

        // 必须有选中音符才能移调
        if root
            .editor
            .editor_state
            .interaction
            .selected_notes
            .is_empty()
        {
            tracing::debug!("Root: 没有选中音符，不执行移调");
            return;
        }

        tracing::info!("Root: 执行移调操作，半音数: {}", semitones);

        let modified = root.editor.transpose_selected(semitones);

        if modified > 0 {
            tracing::info!("Root: 移调完成，修改了 {} 个音符", modified);
            root.update_playback_notes();
            root.editor.clear_notes_changed();
        } else {
            tracing::debug!("Root: 没有音符被移调");
        }
    }
}
