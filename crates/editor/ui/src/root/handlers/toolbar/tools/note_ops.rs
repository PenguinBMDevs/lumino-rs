//! 工具栏音符编辑操作
//!
//! 量化、变速、翻转、移调、连奏、分割/合并等音符编辑操作。

use super::super::ToolbarHandler;
use crate::root::Root;

impl ToolbarHandler {
    /// 处理量化
    pub(crate) fn handle_toolbar_quantize(&self, root: &mut Root, event: &crate::toolbar::Event) {
        if !matches!(event, crate::toolbar::Event::Quantize) {
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
                let mut v: Vec<usize> = selected.iter().copied().collect();
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

            // 2026-09 协作修复：仅对真正变化的音符发「删旧 + 加新」（key/vel/ch 不变）。
            let track = root.editor.editor_state.data.current_track;
            let mut entries: Vec<(bool, u64, f32, u16, f32, u8, u8, usize)> = Vec::new();
            for (pos, old) in old_notes.iter().enumerate() {
                let new_tick = quantizable_notes[pos].tick;
                let new_length = quantizable_notes[pos].length;
                if (new_tick, new_length) != (old.0, old.2) {
                    // 量化后音符已落到 new_tick，按新位置反查其真实全局 ID。
                    let note_id =
                        root.editor
                            .editor_state
                            .data
                            .note_id_at(track, new_tick, old.1)
                            .unwrap_or(0);
                    entries.push((false, note_id, old.0, old.1, old.2, old.3, old.4, track));
                    entries.push((true, note_id, new_tick, old.1, new_length, old.3, old.4, track));
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

        if root.is_arrangement_mode() {
            // ---------- 工程走带模式：基于 arrange_selection（rect 框选） ----------
            if root.editor.editor_state.data.arrange_selection.is_empty() {
                tracing::debug!("Root: 工程走带变速 - 没有选中区域");
                return;
            }

            tracing::info!("Root: 工程走带变速 - 速度因子: {}", speed_factor,);

            let modified = root.editor.arrange_apply_speed_change(speed_factor);

            if modified > 0 {
                tracing::info!("Root: 工程走带变速完成，修改了 {} 个音符", modified);
                root.update_playback_notes();
                root.editor.clear_notes_changed();
            } else {
                tracing::debug!("Root: 工程走带变速 - 没有音符被修改");
            }
        } else {
            // ---------- 钢琴卷帘模式：基于 selected_notes（HashSet 选中索引） ----------
            let selected = &root.editor.editor_state.interaction.selected_notes;

            if root.editor.editor_state.data.current_track_note_count() == 0 {
                tracing::debug!("Root: 没有音符需要变速");
                return;
            }

            // 必须有选中音符才能变速（无选中时对整个音轨变速是灾难性的）
            if selected.is_empty() {
                tracing::debug!("Root: 没有选中音符，不执行变速");
                return;
            }

            tracing::info!(
                "Root: 变速配置 - 速度因子: {}, 选中 {} 个音符",
                speed_factor,
                root.editor.editor_state.interaction.selected_notes.len(),
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

    /// 处理连奏操作
    pub(crate) fn handle_toolbar_tie(&self, root: &mut Root, event: &crate::toolbar::Event) {
        if !matches!(event, crate::toolbar::Event::Tie) {
            return;
        }

        tracing::info!("Root: 执行音符连奏操作");

        // 必须有选中音符才能连奏
        if root
            .editor
            .editor_state
            .interaction
            .selected_notes
            .is_empty()
        {
            tracing::debug!("Root: 没有选中音符，不执行连奏");
            return;
        }

        let tied = root.editor.tie_selected_notes();

        if tied > 0 {
            tracing::info!("Root: 连奏完成，连接了 {} 个音符", tied);
            root.update_playback_notes();
            root.editor.clear_notes_changed();
        } else {
            tracing::debug!("Root: 没有音符被连奏（需至少 2 个同 Key 的选中音符）");
        }
    }

    /// 处理分割/合并操作
    pub(crate) fn handle_toolbar_split_glue(&self, root: &mut Root, event: &crate::toolbar::Event) {
        match event {
            crate::toolbar::Event::Split => {
                // 分割选中音符：在音符中间位置分割
                let selected: Vec<usize> = root
                    .editor
                    .editor_state
                    .interaction
                    .selected_notes
                    .iter()
                    .copied()
                    .collect();

                if selected.is_empty() {
                    tracing::debug!("Root: 分割操作 - 没有选中音符");
                    return;
                }

                let mut split_count = 0usize;
                // 从大到小处理，避免索引偏移
                let mut indices: Vec<usize> = selected;
                indices.sort_by(|a, b| b.cmp(a));
                indices.dedup();

                root.editor.push_history();

                for &idx in &indices {
                    if let Some(note) = root.editor.editor_state.data.current_track_notes().get(idx)
                    {
                        let split_tick =
                            note.start_tick as f32 + (note.end_tick - note.start_tick) as f32 / 2.0;
                        root.editor.split_note(idx, split_tick);
                        split_count += 1;
                    }
                }

                if split_count > 0 {
                    tracing::info!("Root: 分割完成 - 分割了 {} 个音符", split_count);
                    root.update_playback_notes();
                    root.editor.clear_notes_changed();
                    root.editor.editor_state.interaction.selected_notes.clear();
                }
            }
            crate::toolbar::Event::Glue => {
                let merged = root.editor.glue_selected_notes();
                if merged > 0 {
                    tracing::info!("Root: 合并完成 - 合并了 {} 组音符", merged);
                    root.update_playback_notes();
                    root.editor.clear_notes_changed();
                } else {
                    tracing::debug!("Root: 合并操作 - 没有可合并的音符");
                }
            }
            _ => {}
        }
    }
}
