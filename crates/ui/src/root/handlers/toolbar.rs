//! 工具栏事件处理器

use crate::message::Message;
use crate::root::Root;
use crate::root::handlers::MessageHandler;

/// 工具栏消息处理器
///
/// 注意：此处理器处理工具栏事件，但对于播放控制，
/// 它直接将消息转发给专门的处理器，而不是递归调用 update。
pub struct ToolbarHandler;

impl ToolbarHandler {
    pub fn new() -> Self {
        Self
    }

    fn handle_toolbar_event(&self, root: &mut Root, event: crate::toolbar::Event) {
        // 先让 toolbar 更新自身状态（包括自动滚动模式切换）
        root.toolbar.update(event.clone());

        // 处理播放控制 - 直接执行，不通过消息循环
        self.handle_toolbar_playback(root, &event);

        // 同步工具状态
        self.sync_toolbar_tool_state(root, &event);

        // 同步精度设置
        self.sync_toolbar_precision(root, &event);

        // 同步自动滚动模式（在 toolbar 更新之后）
        self.sync_auto_scroll_mode(root, &event);

        // 处理撤销/重做
        self.handle_toolbar_undo_redo(root, &event);

        // 处理量化
        self.handle_toolbar_quantize(root, &event);

        // 处理协作对话框
        self.handle_toolbar_collaboration(root, &event);

        // 处理工程设置对话框
        self.handle_toolbar_project_settings(root, &event);

        // 处理录制
        self.handle_toolbar_recording(root, &event);
    }

    fn handle_toolbar_playback(&self, root: &mut Root, event: &crate::toolbar::Event) {
        match event {
            crate::toolbar::Event::Play => {
                Self::do_play(root);
            }
            crate::toolbar::Event::Pause => {
                Self::do_pause(root);
            }
            crate::toolbar::Event::Stop => {
                Self::do_stop(root);
            }
            crate::toolbar::Event::ToggleLoop => {
                Self::do_toggle_loop(root);
            }
            _ => {}
        }
    }

    /// 执行播放逻辑
    fn do_play(root: &mut Root) {
        if root.playback_manager.is_none() {
            Self::init_playback_manager(root);
        }

        if let Some(manager) = &mut root.playback_manager {
            manager.play();
            root.toolbar.is_playing = true;
            tracing::info!("Root: 开始播放");
        }
    }

    /// 执行暂停逻辑
    fn do_pause(root: &mut Root) {
        if let Some(manager) = &mut root.playback_manager {
            manager.pause();
            root.toolbar.is_playing = false;
            tracing::info!("Root: 暂停播放");
        }
    }

    /// 执行停止逻辑
    fn do_stop(root: &mut Root) {
        if let Some(manager) = &mut root.playback_manager {
            manager.stop();
            root.toolbar.is_playing = false;
            root.editor.playback_position = 0.0;
            tracing::info!("Root: 停止播放");
        }
    }

    /// 执行循环切换逻辑
    fn do_toggle_loop(root: &mut Root) {
        use crate::message::LoopRangeAction;

        let action = LoopRangeAction::Toggle;
        root.handle_loop_range_action(action);
    }

    /// 初始化播放管理器
    fn init_playback_manager(root: &mut Root) {
        use crate::playback::PlaybackManager;

        let division = root.editor.editor_state.view.ppq;
        let mut manager = PlaybackManager::new(division);

        // 先创建空的 manager，让 update_playback_notes 能工作
        manager.set_current_track_notes(Vec::new());
        root.playback_manager = Some(manager);

        // 通过 update_playback_notes 填充所有音轨的音符（含 document 懒加载）
        root.update_playback_notes();

        // 用缓存的 MIDI 输出连接
        if let Some(output) = root.pending_midi_output.take()
            && let Some(manager) = &mut root.playback_manager
        {
            manager.set_midi_output(output);
        }

        // 应用缓存的 tempo 变化
        if let Some(changes) = root.pending_tempo_changes.take()
            && let Some(manager) = &mut root.playback_manager
        {
            manager.set_tempo_changes(changes);
        }

        if let Some(_manager) = &root.playback_manager {
            tracing::info!(
                "Root: 播放管理器已初始化 (division={}, 过滤阈值={})",
                division,
                root.velocity_filter_threshold,
            );
        }

        // 同步循环状态到播放引擎（用户可能在创建管理器前已开启循环）
        if let Some(loop_range) = &root.editor.loop_range
            && loop_range.enabled()
            && let Some(manager) = &mut root.playback_manager
        {
            manager.set_looping(true);
            manager.set_loop_range(loop_range.start_tick(), loop_range.end_tick());
            tracing::debug!(
                "Root: 循环状态已同步到播放引擎 [{:.2}, {:.2}]",
                loop_range.start_tick(),
                loop_range.end_tick(),
            );
        }
    }

    fn sync_toolbar_tool_state(&self, root: &mut Root, event: &crate::toolbar::Event) {
        if let crate::toolbar::Event::ToolSelected(tool) = event {
            root.editor.set_tool(*tool);
        }
    }

    fn sync_toolbar_precision(&self, root: &mut Root, event: &crate::toolbar::Event) {
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

    fn handle_toolbar_undo_redo(&self, _root: &mut Root, event: &crate::toolbar::Event) {
        if matches!(event, crate::toolbar::Event::Undo) {
            tracing::info!("Root: 触发撤销操作");
            lumino_core::event::emit(lumino_core::event::Event::Menu(
                lumino_core::event::menu::Event::Edit(lumino_core::event::menu::edit::Event::Undo),
            ));
        }
        if matches!(event, crate::toolbar::Event::Redo) {
            tracing::info!("Root: 触发重做操作");
            lumino_core::event::emit(lumino_core::event::Event::Menu(
                lumino_core::event::menu::Event::Edit(lumino_core::event::menu::edit::Event::Redo),
            ));
        }
    }

    fn handle_toolbar_collaboration(&self, _root: &mut Root, event: &crate::toolbar::Event) {
        if matches!(event, crate::toolbar::Event::OpenCollaborationDialog) {
            tracing::info!("Root: 触发打开协作对话框");
            lumino_core::event::emit(lumino_core::Event::Window(
                lumino_core::event::window::Event::OpenCollaborationDialog,
            ));
        }
    }

    fn handle_toolbar_project_settings(&self, _root: &mut Root, event: &crate::toolbar::Event) {
        if matches!(event, crate::toolbar::Event::OpenProjectSettingsDialog) {
            tracing::info!("Root: 触发打开工程设置对话框");
            lumino_core::event::emit(lumino_core::Event::Window(
                lumino_core::event::window::Event::OpenProjectSettingsDialog,
            ));
        }
    }

    fn handle_toolbar_recording(&self, root: &mut Root, event: &crate::toolbar::Event) {
        match event {
            crate::toolbar::Event::Record => {
                root.start_recording();
            }
            crate::toolbar::Event::RecordStop => {
                root.stop_recording();
            }
            _ => {}
        }
    }

    fn handle_toolbar_quantize(&self, root: &mut Root, event: &crate::toolbar::Event) {
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

        let config = lumino_core::midi::quantize::QuantizeConfig::new(grid_size, 1.0);

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
        );
        root.editor.editor_state.data.history.push(snapshot);

        let mut quantizable_notes: Vec<lumino_core::midi::quantize::QuantizableNote> =
            selected_indices
                .iter()
                .map(|&i| {
                    let note = &root.editor.editor_state.data.notes[i];
                    lumino_core::midi::quantize::QuantizableNote::new(note.tick, note.length)
                })
                .collect();

        let modified_count =
            lumino_core::midi::quantize::quantize_notes(&mut quantizable_notes, &config);

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
                ),
            );
            tracing::debug!("Root: 没有音符被量化");
        }

        if root.editor.notes_changed() {
            root.update_playback_notes();
            root.editor.clear_notes_changed();
        }
    }

    fn sync_auto_scroll_mode(&self, root: &mut Root, event: &crate::toolbar::Event) {
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
}

impl Default for ToolbarHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageHandler for ToolbarHandler {
    fn handle(&mut self, root: &mut Root, msg: Message) -> Option<Message> {
        match msg {
            Message::Toolbar(event) => {
                self.handle_toolbar_event(root, event);
                None
            }
            other => Some(other),
        }
    }
}
