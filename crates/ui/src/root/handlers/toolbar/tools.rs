//! 工具栏工具选择与音符编辑操作
//!
//! 包括工具选择、精度同步、撤销/重做、量化、变速、翻转、移调、分割/合并等。

use super::ToolbarHandler;
use crate::root::Root;

impl ToolbarHandler {
    /// 同步工具状态到编辑器
    pub(crate) fn sync_toolbar_tool_state(&self, root: &mut Root, event: &crate::toolbar::Event) {
        if let crate::toolbar::Event::ToolSelected(tool) = event {
            root.editor.set_tool(*tool);
        }
    }

    /// 同步精度设置到编辑器
    pub(crate) fn sync_toolbar_precision(&self, root: &mut Root, event: &crate::toolbar::Event) {
        if let crate::toolbar::Event::PrecisionChanged(precision) = event {
            let ticks = (*precision).as_ticks(root.editor.editor_state.view.ppq);
            root.editor.set_snap_precision(ticks);
            root.editor.set_default_note_length(ticks);
            tracing::debug!(
                "Root: 音符精度同步为 {} ticks (PPQ={})",
                ticks,
                root.editor.editor_state.view.ppq
            );
        }
    }

    /// 同步自动滚动模式到编辑器
    pub(crate) fn sync_auto_scroll_mode(&self, root: &mut Root, event: &crate::toolbar::Event) {
        if matches!(event, crate::toolbar::Event::AutoScrollModeChanged) {
            // 同步自动滚动模式到 editor（toolbar 已经切换了模式，这里同步到 editor）
            root.editor
                .set_auto_scroll_config(lumino_core::storage::config::AutoScrollConfig {
                    mode: root.toolbar.auto_scroll_mode,
                    ..root.editor.editor_state.auto_scroll
                });
            tracing::debug!(
                "Root: 自动滚动模式同步为 {:?}",
                root.toolbar.auto_scroll_mode
            );
        }
    }

    /// 处理撤销/重做
    pub(crate) fn handle_toolbar_undo_redo(&self, _root: &mut Root, event: &crate::toolbar::Event) {
        if matches!(event, crate::toolbar::Event::Undo) {
            tracing::info!("Root: 触发撤销操作");
            crate::event::emit(crate::event::Event::Menu(crate::event::menu::Event::Edit(
                crate::event::menu::edit::Event::Undo,
            )));
        }
        if matches!(event, crate::toolbar::Event::Redo) {
            tracing::info!("Root: 触发重做操作");
            crate::event::emit(crate::event::Event::Menu(crate::event::menu::Event::Edit(
                crate::event::menu::edit::Event::Redo,
            )));
        }
    }

    /// 处理协作对话框
    pub(crate) fn handle_toolbar_collaboration(
        &self,
        _root: &mut Root,
        event: &crate::toolbar::Event,
    ) {
        if matches!(event, crate::toolbar::Event::OpenCollaborationDialog) {
            tracing::info!("Root: 触发打开协作对话框");
            crate::event::emit(crate::event::Event::Window(
                crate::event::window::Event::open_collaboration_dialog(),
            ));
        }
    }

    /// 处理内存监控对话框
    pub(crate) fn handle_toolbar_memory_monitor(
        &self,
        _root: &mut Root,
        event: &crate::toolbar::Event,
    ) {
        if matches!(event, crate::toolbar::Event::OpenMemoryMonitorDialog) {
            tracing::info!("Root: 触发打开内存监控对话框");
            crate::event::emit(crate::event::Event::Window(
                crate::event::window::Event::open_memory_monitor_dialog(),
            ));
        }
    }

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

        if root.editor.editor_state.data.notes.is_empty() {
            tracing::debug!("Root: 没有音符需要量化");
            return;
        }

        let config = lumino_midi_loader::quantize::QuantizeConfig::new(grid_size, 1.0);

        // 获取选中音符索引（无选中则量化全部）
        let selected_indices: Vec<usize> = {
            let selected = &root.editor.editor_state.interaction.selected_notes;
            if selected.is_empty() {
                (0..root.editor.editor_state.data.notes.len()).collect()
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
            root.editor.editor_state.data.notes.clone(),
            root.editor.editor_state.data.current_track,
            root.editor.editor_state.data.automation_lanes.clone(),
        );
        root.editor.editor_state.data.history.push(snapshot);

        let mut quantizable_notes: Vec<lumino_midi_loader::quantize::QuantizableNote> =
            selected_indices
                .iter()
                .map(|&i| {
                    let note = &root.editor.editor_state.data.notes[i];
                    lumino_midi_loader::quantize::QuantizableNote::new(note.tick, note.length)
                })
                .collect();

        let modified_count =
            lumino_midi_loader::quantize::quantize_notes(&mut quantizable_notes, &config);

        if modified_count > 0 {
            for (pos, &i) in selected_indices.iter().enumerate() {
                if let Some(note) = root.editor.editor_state.data.notes.get_mut(i) {
                    note.tick = quantizable_notes[pos].tick;
                    note.length = quantizable_notes[pos].length;
                }
            }

            root.editor.mark_notes_changed();
            tracing::info!("Root: 量化完成，修改了 {} 个音符", modified_count);
        } else {
            root.editor.editor_state.data.history.undo(
                crate::editor::history::EditorSnapshot::new(
                    root.editor.editor_state.data.notes.clone(),
                    root.editor.editor_state.data.current_track,
                    root.editor.editor_state.data.automation_lanes.clone(),
                ),
            );
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
        let notes = &root.editor.editor_state.data.notes;
        let selected = &root.editor.editor_state.interaction.selected_notes;

        if notes.is_empty() {
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
    pub(crate) fn handle_toolbar_transpose(&self, root: &mut Root, event: &crate::toolbar::Event) {
        let semitones = match event {
            crate::toolbar::Event::TransposeUp => 1,
            crate::toolbar::Event::TransposeDown => -1,
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
                    if let Some(note) = root.editor.editor_state.data.notes.get(idx) {
                        let split_tick = note.tick + note.length / 2.0;
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
