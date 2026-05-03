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

        // 处理协作对话框
        self.handle_toolbar_collaboration(root, &event);
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

    /// 初始化播放管理器
    fn init_playback_manager(root: &mut Root) {
        use crate::playback::PlaybackManager;

        let division = root.editor.state.ppq;
        let mut manager = PlaybackManager::new(division);

        // 先创建空的 manager，让 update_playback_notes 能工作
        manager.set_current_track_notes(Vec::new());
        root.playback_manager = Some(manager);

<<<<<<< HEAD
        // 设置音符：合并当前音轨和所有其他音轨的音符，应用力度过滤
        let mut notes: Vec<NoteEvent> = root
            .editor
            .notes
            .iter()
            .filter(|note| note.velocity > velocity_threshold)
            .map(|note| NoteEvent {
                tick: note.tick,
                channel: note.channel,
                key: note.key as u8,
                velocity: note.velocity,
                length: note.length,
            })
            .collect();
=======
        // 通过 update_playback_notes 填充所有音轨的音符（含 document 懒加载）
        root.update_playback_notes();
>>>>>>> feat/memory-for-loader

        // 用缓存的 MIDI 输出连接
        if let Some(output) = root.pending_midi_output.take()
            && let Some(manager) = &mut root.playback_manager {
                manager.set_midi_output(output);
            }
<<<<<<< HEAD
            for note in track_notes {
                if note.velocity > velocity_threshold {
                    notes.push(NoteEvent {
                        tick: note.tick,
                        channel: note.channel,
                        key: note.key as u8,
                        velocity: note.velocity,
                        length: note.length,
                    });
                }
            }
        }

        let total_notes = notes.len();

        // 收集所有音轨的 MIDI 控制事件（CC/PC/PB）
        let mut midi_events: Vec<crate::playback::MidiTrackEvent> = Vec::new();
        for (_track_idx, events) in &root.track_midi_events {
            midi_events.extend(events.clone());
        }
        // 按 tick 排序
        midi_events.sort_by(|a, b| a.tick.total_cmp(&b.tick));
        let total_midi_events = midi_events.len();
        manager.set_notes(notes);
        if !midi_events.is_empty() {
            manager.set_midi_events(midi_events);
        }
=======
>>>>>>> feat/memory-for-loader

        // 应用缓存的 tempo 变化
        if let Some(changes) = root.pending_tempo_changes.take()
            && let Some(manager) = &mut root.playback_manager {
                manager.set_tempo_changes(changes);
            }

        if let Some(_manager) = &root.playback_manager {
            tracing::info!(
                "Root: 播放管理器已初始化 (division={}, 过滤阈值={})",
                division,
                root.velocity_filter_threshold,
            );
        }
<<<<<<< HEAD

        root.playback_manager = Some(manager);
        tracing::info!(
            "Root: 播放管理器已初始化 (division={}, 总音符={}, MIDI事件={}, 过滤阈值={})",
            division,
            total_notes,
            total_midi_events,
            velocity_threshold
        );
=======
>>>>>>> feat/memory-for-loader
    }

    fn sync_toolbar_tool_state(&self, root: &mut Root, event: &crate::toolbar::Event) {
        if let crate::toolbar::Event::ToolSelected(tool) = event {
            root.editor.set_tool(*tool);
        }
    }

    fn sync_toolbar_precision(&self, root: &mut Root, event: &crate::toolbar::Event) {
        if let crate::toolbar::Event::PrecisionChanged(precision) = event {
            let ticks = (*precision).as_ticks(root.editor.state.ppq);
            root.editor.state.snap_precision = ticks;
            root.editor.state.default_note_length = ticks;
            tracing::debug!(
                "Root: 音符精度同步为 {} ticks (PPQ={})",
                ticks,
                root.editor.state.ppq
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
            lumino_core::event::emit(lumino_core::event::Event::Window(
                lumino_core::event::window::Event::OpenCollaborationDialog,
            ));
        }
    }

    fn sync_auto_scroll_mode(&self, root: &mut Root, event: &crate::toolbar::Event) {
        if matches!(event, crate::toolbar::Event::AutoScrollModeChanged) {
            // 同步自动滚动模式到 editor（toolbar 已经切换了模式，这里同步到 editor）
            root.editor
                .set_auto_scroll_config(lumino_core::storage::config::AutoScrollConfig {
                    mode: root.toolbar.auto_scroll_mode,
                    ..*root.editor.auto_scroll_config()
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
