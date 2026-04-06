//! 播放控制处理器

use crate::message::Message;
use crate::playback::NoteEvent;
use crate::root::Root;
use crate::root::handlers::MessageHandler;

/// 播放控制消息处理器
pub struct PlaybackHandler;

impl PlaybackHandler {
    pub fn new() -> Self {
        Self
    }

    fn handle_play(&self, root: &mut Root) {
        if root.playback_manager.is_none() {
            Self::init_playback_manager(root);
        }

        if let Some(manager) = &mut root.playback_manager {
            manager.play();
            tracing::info!("Root: 开始播放");
        }
    }

    fn handle_pause(&self, root: &mut Root) {
        if let Some(manager) = &mut root.playback_manager {
            manager.pause();
            tracing::info!("Root: 暂停播放");
        }
    }

    fn handle_stop(&self, root: &mut Root) {
        if let Some(manager) = &mut root.playback_manager {
            manager.stop();
            root.editor.playback_position = 0.0;
            tracing::info!("Root: 停止播放");
        }
    }

    fn handle_playback_tick(&self, root: &mut Root, tick: f32) {
        root.editor.playback_position = tick;

        // 更新自动滚动
        root.editor.update_auto_scroll(tick);

        if let Some(manager) = &mut root.playback_manager {
            manager.update();
        }
    }

    fn init_playback_manager(root: &mut Root) {
        let division = root.editor.state.ppq;
        let mut manager = crate::playback::PlaybackManager::new(division);

        // 设置音符：合并当前音轨和所有其他音轨的音符
        let mut notes: Vec<NoteEvent> = root
            .editor
            .notes
            .iter()
            .map(|note| NoteEvent {
                tick: note.tick,
                channel: 0,
                key: note.key as u8,
                velocity: 100,
                length: note.length,
            })
            .collect();

        // 添加其他音轨的音符
        for (track_idx, track_notes) in &root.editor.track_notes {
            // 跳过当前编辑的音轨（已经在 editor.notes 中）
            if *track_idx == root.editor.current_track {
                continue;
            }
            for note in track_notes {
                notes.push(NoteEvent {
                    tick: note.tick,
                    channel: 0,
                    key: note.key as u8,
                    velocity: 100,
                    length: note.length,
                });
            }
        }

        let total_notes = notes.len();
        manager.set_notes(notes);

        // 应用缓存的 tempo 变化
        if let Some(changes) = root.pending_tempo_changes.take() {
            manager.set_tempo_changes(changes);
        }

        // 应用缓存的 MIDI 输出连接
        if let Some(output) = root.pending_midi_output.take() {
            manager.set_midi_output(output);
        }

        root.playback_manager = Some(manager);
        tracing::info!(
            "Root: 播放管理器已初始化 (division={}, 总音符={})",
            division,
            total_notes
        );
    }
}

impl Default for PlaybackHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageHandler for PlaybackHandler {
    fn handle(&mut self, root: &mut Root, msg: Message) -> Option<Message> {
        match msg {
            Message::Play => {
                self.handle_play(root);
                None
            }
            Message::Pause => {
                self.handle_pause(root);
                None
            }
            Message::Stop => {
                self.handle_stop(root);
                None
            }
            Message::PlaybackTick(tick) => {
                self.handle_playback_tick(root, tick);
                None
            }
            other => Some(other),
        }
    }
}
