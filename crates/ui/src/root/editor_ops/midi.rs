//! 编辑器操作 - MIDI 输出管理

use crate::root::Root;

impl Root {
    /// 加载 Tempo 变化事件到播放管理器
    /// tempo_changes: Vec<(tick, tempo_in_microseconds_per_quarter_note)>
    pub fn load_tempo_changes(&mut self, tempo_changes: Vec<(u32, u32)>) {
        tracing::debug!(
            "Root::load_tempo_changes: loading {} tempo changes",
            tempo_changes.len()
        );

        let tempo_change_list: Vec<crate::playback::TempoChange> = tempo_changes
            .into_iter()
            .map(|(tick, tempo)| crate::playback::TempoChange {
                tick: tick as f32,
                tempo,
            })
            .collect();

        // 如果有播放管理器，更新其 tempo timeline
        if let Some(manager) = &mut self.playback.manager {
            manager.update_tempo_changes(tempo_change_list);
            tracing::debug!("Root::load_tempo_changes: tempo changes updated in playback manager");
        } else {
            self.playback.pending_tempo_changes = Some(tempo_change_list);
            tracing::debug!(
                "Root::load_tempo_changes: playback manager not ready, cached tempo changes"
            );
        }
    }

    /// 设置 MIDI 输出连接
    pub fn set_midi_output(&mut self, output: Box<dyn lumino_midi_io::OutputConnection>) {
        if let Some(manager) = &mut self.playback.manager {
            manager.set_midi_output(output);
            tracing::info!("Root::set_midi_output: MIDI output connection set");
        } else {
            self.playback.pending_midi_output = Some(output);
            tracing::debug!(
                "Root::set_midi_output: playback manager not ready, cached MIDI output"
            );
        }
    }

    /// 清除 MIDI 输出连接
    pub fn clear_midi_output(&mut self) {
        if let Some(manager) = &mut self.playback.manager {
            manager.clear_midi_output();
            tracing::info!("Root::clear_midi_output: MIDI output connection cleared");
        }
        self.playback.pending_midi_output = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::note::Note;
    use crate::message::Message;
    use crate::playback::PlaybackState;
    use crate::toolbar;
    use lumino_core::storage::config::{AutoScrollConfig, AutoScrollMode, UiConfig};

    /// 模拟 MIDI 输出连接，用于测试 playback 流程
    struct MockOutput {
        _note_on_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
        _note_off_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
    }

    impl lumino_midi_io::OutputConnection for MockOutput {
        fn note_on(
            &mut self,
            _ch: u8,
            _key: u8,
            _vel: u8,
        ) -> std::result::Result<(), lumino_midi_io::Error> {
            self._note_on_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        }
        fn note_off(
            &mut self,
            _ch: u8,
            _key: u8,
            _vel: u8,
        ) -> std::result::Result<(), lumino_midi_io::Error> {
            self._note_off_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        }
        fn control_change(
            &mut self,
            _ch: u8,
            _controller: u8,
            _value: u8,
        ) -> std::result::Result<(), lumino_midi_io::Error> {
            Ok(())
        }
        fn program_change(
            &mut self,
            _ch: u8,
            _program: u8,
        ) -> std::result::Result<(), lumino_midi_io::Error> {
            Ok(())
        }
        fn pitch_bend(
            &mut self,
            _ch: u8,
            _value: f32,
        ) -> std::result::Result<(), lumino_midi_io::Error> {
            Ok(())
        }
        fn channel_pressure(
            &mut self,
            _ch: u8,
            _pressure: u8,
        ) -> std::result::Result<(), lumino_midi_io::Error> {
            Ok(())
        }
        fn poly_pressure(
            &mut self,
            _ch: u8,
            _key: u8,
            _pressure: u8,
        ) -> std::result::Result<(), lumino_midi_io::Error> {
            Ok(())
        }
        fn send_raw(&mut self, _data: [u8; 3]) -> std::result::Result<(), lumino_midi_io::Error> {
            Ok(())
        }
        fn close(self: Box<Self>) {}
    }

    /// 辅助函数：创建带默认配置的 Root
    fn create_root() -> Root {
        Root::new(&UiConfig::default())
    }

    /// 辅助函数：创建 MockOutput
    fn create_mock_output() -> Box<MockOutput> {
        Box::new(MockOutput {
            _note_on_count: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            _note_off_count: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
        })
    }

    /// 测试核心契约：
    /// set_midi_output 在无 playback_manager 时缓存到 pending_midi_output，
    /// Message::Toolbar(toolbar::Event::Play) 消费它并创建 playback_manager
    #[test]
    fn test_play_consumes_pending_midi_output() {
        let mut root = create_root();

        // 初始状态：干净
        assert!(root.playback.manager.is_none(), "初始无播放管理器");
        assert!(root.playback.pending_midi_output.is_none(), "初始无挂起 MIDI 输出");

        // 添加测试音符
        root.editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(0.0, 60, 480.0));
        root.editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(480.0, 64, 480.0));

        // 设置 MIDI 输出 → 因无管理器应缓存
        root.set_midi_output(create_mock_output());
        assert!(
            root.playback.pending_midi_output.is_some(),
            "无播放管理器时 set_midi_output 应缓存到 pending_midi_output"
        );
        assert!(
            root.playback.manager.is_none(),
            "pending 状态不应创建播放管理器"
        );

        // 发送 Play 消息 → 应消费缓存并创建管理器
        root.update(Message::Toolbar(toolbar::Event::Play));
        assert!(root.playback.manager.is_some(), "Play 消息应创建播放管理器");
        assert!(
            root.playback.pending_midi_output.is_none(),
            "Play 消息应消费 pending_midi_output"
        );

        // toolbar.is_playing 是同步设置的（在 manager.play() 之前），可立即验证
        assert!(root.toolbar.is_playing, "Play 后工具栏 playing 应为 true");

        // 注意：manager.state() 是异步的（引擎线程通过 mpsc 接收命令后更新），
        // 此处不立即验证 Playing 状态，避免竞态条件。
        // 通过 toolbar.is_playing 同步标志和 manager 存在性验证流程正确性。

        // 停止并自动清理
        root.update(Message::Toolbar(toolbar::Event::Stop));
        assert!(!root.toolbar.is_playing, "Stop 后工具栏 playing 应为 false");
    }

    /// 测试已存在播放管理器时，set_midi_output 直接传递不缓存
    #[test]
    fn test_set_midi_output_direct_when_manager_exists() {
        let mut root = create_root();
        root.editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(0.0, 60, 480.0));
        root.editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(480.0, 64, 480.0));

        // 先通过 Play 创建 playback_manager
        root.set_midi_output(create_mock_output());
        root.update(Message::Toolbar(toolbar::Event::Play));
        assert!(root.playback.manager.is_some());
        assert!(root.playback.pending_midi_output.is_none());

        // 此时再调用 set_midi_output：应直接传递给 manager，不缓存
        root.set_midi_output(create_mock_output());
        assert!(
            root.playback.pending_midi_output.is_none(),
            "有播放管理器时不应缓存 MIDI output"
        );

        root.update(Message::Toolbar(toolbar::Event::Stop));
    }

    /// 测试 clear_midi_output 的完整性：清除缓存 + 转发到管理器
    #[test]
    fn test_clear_midi_output_clears_pending() {
        let mut root = create_root();

        // 仅缓存（无管理器）
        root.set_midi_output(create_mock_output());
        assert!(root.playback.pending_midi_output.is_some());

        root.clear_midi_output();
        assert!(
            root.playback.pending_midi_output.is_none(),
            "clear_midi_output 应清除 pending"
        );

        // 有管理器时
        root.editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(0.0, 60, 480.0));
        root.set_midi_output(create_mock_output());
        root.update(Message::Toolbar(toolbar::Event::Play));
        root.clear_midi_output();
        assert!(
            root.playback.pending_midi_output.is_none(),
            "有管理器时 clear 后 pending 仍应为 None"
        );

        root.update(Message::Toolbar(toolbar::Event::Stop));
    }

    /// 测试完整的笔记生命周期：
    /// 创建笔记 → 设置 MIDI 输出 → 播放 → 停止
    #[test]
    fn test_full_playback_lifecycle() {
        let mut root = create_root();

        // 添加多个音符
        root.editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(0.0, 60, 480.0));
        root.editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(480.0, 64, 240.0));
        root.editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(720.0, 67, 480.0));

        // 设置 MIDI 输出
        root.set_midi_output(create_mock_output());
        assert!(root.playback.pending_midi_output.is_some());

        // 播放
        root.update(Message::Toolbar(toolbar::Event::Play));
        assert!(root.playback.manager.is_some(), "播放管理器应被创建");
        assert!(
            root.playback.pending_midi_output.is_none(),
            "pending MIDI 输出应被消费"
        );
        assert!(root.toolbar.is_playing, "播放后工具栏应标记为 playing");

        // 停止
        root.update(Message::Toolbar(toolbar::Event::Stop));

        // 验证停止后状态（manager.state() 是异步的，但发送 Command::Stop 后
        // 引擎线程会处理并更新状态。由于 Manager::stop() 是同步发送消息，
        // 引擎线程在下一个 1ms 睡眠周期处理它。此处给线程一点时间处理。）
        if let Some(ref manager) = root.playback.manager {
            // 等待引擎线程处理 Stop 命令
            for _ in 0..50 {
                if manager.state() == PlaybackState::Stopped {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            assert_eq!(
                manager.state(),
                PlaybackState::Stopped,
                "停止后应处于 Stopped 状态"
            );
        }
        assert!(!root.toolbar.is_playing, "停止后工具栏 playing 应为 false");
        assert!(
            (root.editor.playback_position - 0.0).abs() < f32::EPSILON,
            "停止后播放位置应重置为 0"
        );
    }

    /// 测试 update_playback_notes：音符变更后应能更新管理器中的音符
    #[test]
    fn test_update_playback_notes_after_note_change() {
        let mut root = create_root();

        // 先播放（此时无音符）
        root.set_midi_output(create_mock_output());
        root.update(Message::Toolbar(toolbar::Event::Play));
        assert!(root.playback.manager.is_some());

        // 添加音符并标记变更
        root.editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(0.0, 60, 480.0));
        root.editor.mark_notes_changed();

        // 触发音符更新（模拟 handle_editor_action 中的流程）
        if root.editor.notes_changed() {
            root.update_playback_notes();
            root.editor.clear_notes_changed();
        }

        // 管理器仍存在，pending 无缓存
        assert!(root.playback.manager.is_some());
        assert!(root.playback.pending_midi_output.is_none());

        root.update(Message::Toolbar(toolbar::Event::Stop));
    }

    /// 测试 load_tempo_changes 在无管理器时缓存
    #[test]
    fn test_tempo_changes_cached_when_no_manager() {
        let mut root = create_root();

        assert!(root.playback.pending_tempo_changes.is_none());

        root.load_tempo_changes(vec![(0, 500000)]); // 120 BPM
        assert!(
            root.playback.pending_tempo_changes.is_some(),
            "无管理器时 tempo changes 应缓存"
        );

        // 播放时应消费缓存的 tempo changes
        root.editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(0.0, 60, 480.0));
        root.set_midi_output(create_mock_output());
        root.update(Message::Toolbar(toolbar::Event::Play));

        assert!(
            root.playback.pending_tempo_changes.is_none(),
            "播放后 tempo changes 应被消费"
        );
        assert!(root.playback.manager.is_some());

        root.update(Message::Toolbar(toolbar::Event::Stop));
    }

    /// 测试 set_midi_output 和 set_playback_midi_output (Host 层) 的一致性
    #[test]
    fn test_host_set_playback_midi_output_flow() {
        let mut root = create_root();
        root.editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(0.0, 60, 480.0));

        // Host::set_playback_midi_output → Root::set_midi_output
        root.set_midi_output(create_mock_output());
        assert!(root.playback.pending_midi_output.is_some());

        root.update(Message::Toolbar(toolbar::Event::Play));
        assert!(root.playback.pending_midi_output.is_none());
        assert!(root.playback.manager.is_some());

        root.update(Message::Toolbar(toolbar::Event::Stop));
    }

    /// 端到端测试：验证引擎线程实际发送了 MIDI note_on 消息
    /// 这个测试暴露了一个关键问题：引擎线程的 update() 循环只在
    /// playback_manager.update() 被调用时运行，但主线程只在
    /// update_playback() 中调用它——而 update_playback() 只在
    /// PlaybackTick 消息中被调用！
    #[test]
    fn test_end_to_end_note_actually_plays() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::thread;
        use std::time::Duration;

        let mut root = create_root();

        // 添加一个在 tick 0 的音符
        root.editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(0.0, 60, 480.0));

        // 设置 MIDI 输出（使用可计数的 mock）
        let note_on_count = Arc::new(AtomicU32::new(0));
        let note_off_count = Arc::new(AtomicU32::new(0));

        let on_count = Arc::clone(&note_on_count);
        let off_count = Arc::clone(&note_off_count);

        let mock_output = Box::new(CountingMockOutput {
            note_on_count: on_count,
            note_off_count: off_count,
        });

        root.set_midi_output(mock_output);
        assert!(root.playback.pending_midi_output.is_some());

        // 发送 Play 消息
        root.update(Message::Toolbar(toolbar::Event::Play));
        assert!(root.playback.manager.is_some(), "播放管理器应被创建");
        assert!(
            root.playback.pending_midi_output.is_none(),
            "pending_midi_output 应被消费"
        );
        assert!(root.toolbar.is_playing, "工具栏应标记为 playing");

        // 关键：引擎线程需要时间来启动和发送 MIDI 消息
        // 但更重要的是：PlaybackManager::update() 必须在主线程中被调用
        // 才能驱动引擎线程的 tick 前进
        //
        // 在真实应用中，update_playback() 在 PlaybackTick 消息中被调用
        // 但在测试中我们需要手动调用它来驱动播放

        // 给引擎线程一点时间启动
        thread::sleep(Duration::from_millis(50));

        // 手动调用 update_playback 来驱动引擎
        // 在真实应用中，这由主循环的 PlaybackTick 消息触发
        for _ in 0..100 {
            root.update_playback();
            thread::sleep(Duration::from_millis(1));
        }

        let on_count = note_on_count.load(Ordering::Relaxed);
        let off_count = note_off_count.load(Ordering::Relaxed);

        tracing::info!("端到端测试: note_on={}, note_off={}", on_count, off_count);

        // 停止播放
        root.update(Message::Toolbar(toolbar::Event::Stop));

        // 断言：至少应该有一个 note_on 被发送
        // 如果这里是 0，说明整个播放管道有问题
        assert!(
            on_count > 0,
            "端到端播放失败：引擎线程没有发送任何 note_on 消息。\
             这意味着 pending_midi_output 没有被正确传递到引擎线程，\
             或者引擎线程没有正确发送 MIDI 消息。\
             实际 note_on={}, note_off={}",
            on_count,
            off_count
        );
    }

    /// 模拟真实用户场景：先画音符，再点击播放
    /// 这个测试更接近用户的实际使用流程
    #[test]
    fn test_draw_note_then_play() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::thread;
        use std::time::Duration;

        let mut root = create_root();

        // 步骤1：设置 MIDI 输出（模拟启动时的初始化）
        let note_on_count = Arc::new(AtomicU32::new(0));
        let note_off_count = Arc::new(AtomicU32::new(0));

        let mock_output = Box::new(CountingMockOutput {
            note_on_count: Arc::clone(&note_on_count),
            note_off_count: Arc::clone(&note_off_count),
        });

        root.set_midi_output(mock_output);
        assert!(
            root.playback.pending_midi_output.is_some(),
            "启动后 pending_midi_output 应有值"
        );

        // 步骤2：用户画一个音符（模拟鼠标操作）
        // 这会在 finish_drawing 中调用 mark_notes_changed
        // 然后在 handle_editor_action 中调用 update_playback_notes
        // 但此时 playback_manager 还不存在，所以 update_playback_notes 什么都不做
        root.editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(0.0, 60, 480.0));
        root.editor.mark_notes_changed();

        // 模拟 handle_editor_action 的处理
        if root.editor.notes_changed() {
            root.update_playback_notes();
            root.editor.clear_notes_changed();
        }

        // 此时 playback_manager 应该还不存在
        assert!(
            root.playback.manager.is_none(),
            "画音符后不应创建播放管理器"
        );

        // 步骤3：用户点击播放按钮
        root.update(Message::Toolbar(toolbar::Event::Play));

        // 播放管理器应该被创建
        assert!(
            root.playback.manager.is_some(),
            "点击播放后应创建播放管理器"
        );
        assert!(
            root.playback.pending_midi_output.is_none(),
            "pending_midi_output 应被消费"
        );

        // 给引擎线程时间启动
        thread::sleep(Duration::from_millis(50));

        // 驱动播放
        for _ in 0..100 {
            root.update_playback();
            thread::sleep(Duration::from_millis(1));
        }

        let on_count = note_on_count.load(Ordering::Relaxed);
        let off_count = note_off_count.load(Ordering::Relaxed);

        tracing::info!(
            "画音符后播放测试: note_on={}, note_off={}",
            on_count,
            off_count
        );

        // 停止
        root.update(Message::Toolbar(toolbar::Event::Stop));

        // 断言
        assert!(
            on_count > 0,
            "画音符后播放失败：没有发送 note_on。实际 note_on={}, note_off={}",
            on_count,
            off_count
        );
    }

    /// 测试场景：先播放（创建管理器），再画音符，再播放
    /// 这个场景测试 update_playback_notes 是否正常工作
    #[test]
    fn test_play_then_draw_then_play() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::thread;
        use std::time::Duration;

        let mut root = create_root();

        // 设置 MIDI 输出
        let note_on_count = Arc::new(AtomicU32::new(0));
        let note_off_count = Arc::new(AtomicU32::new(0));

        let mock_output = Box::new(CountingMockOutput {
            note_on_count: Arc::clone(&note_on_count),
            note_off_count: Arc::clone(&note_off_count),
        });

        root.set_midi_output(mock_output);

        // 步骤1：先添加一个音符并播放（创建播放管理器）
        root.editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(0.0, 60, 480.0));
        root.update(Message::Toolbar(toolbar::Event::Play));
        assert!(root.playback.manager.is_some(), "第一次播放应创建管理器");

        // 停止
        root.update(Message::Toolbar(toolbar::Event::Stop));
        thread::sleep(Duration::from_millis(50));

        // 步骤2：再画一个音符（模拟用户操作）
        root.editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(480.0, 64, 480.0));
        root.editor.mark_notes_changed();

        // 模拟 handle_editor_action 的处理
        if root.editor.notes_changed() {
            root.update_playback_notes();
            root.editor.clear_notes_changed();
        }

        // 步骤3：再次播放
        root.update(Message::Toolbar(toolbar::Event::Play));

        // 驱动播放
        thread::sleep(Duration::from_millis(50));
        for _ in 0..100 {
            root.update_playback();
            thread::sleep(Duration::from_millis(1));
        }

        let on_count = note_on_count.load(Ordering::Relaxed);
        let off_count = note_off_count.load(Ordering::Relaxed);

        tracing::info!(
            "先播再画再播测试: note_on={}, note_off={}",
            on_count,
            off_count
        );

        // 停止
        root.update(Message::Toolbar(toolbar::Event::Stop));

        // 断言：应该有两个 note_on（两个音符）
        assert!(
            on_count >= 2,
            "先播再画再播失败：期望至少2个 note_on，实际 note_on={}, note_off={}",
            on_count,
            off_count
        );
    }

    // ════════════════════════════════════════════════════════════════════════════
    // 录制功能测试
    // ════════════════════════════════════════════════════════════════════════════

    /// 测试 RecordingState 的状态切换
    #[test]
    fn test_recording_state_toggle() {
        use crate::editor::recording::RecordingState;

        let mut state = RecordingState::new();
        assert!(!state.is_recording, "初始不应录制");
        assert!(state.input_device_name.is_none());
        assert_eq!(state.arm_track, 0);
        assert!(state.pending_notes.is_empty());

        state.start(Some("Test Device".into()), 1);
        assert!(state.is_recording, "start() 后应进入录制状态");
        assert_eq!(state.input_device_name.as_deref(), Some("Test Device"));
        assert_eq!(state.arm_track, 1);

        state.stop();
        assert!(!state.is_recording, "stop() 后应退出录制状态");
        assert!(state.pending_notes.is_empty());

        // 测试节拍器切换
        assert!(!state.metronome_enabled, "节拍器默认关闭");
        state.toggle_metronome();
        assert!(state.metronome_enabled, "toggle_metronome 应打开节拍器");
        state.toggle_metronome();
        assert!(!state.metronome_enabled, "再次 toggle 应关闭节拍器");
    }

    /// 测试 poll_midi_input 在录制状态下处理 NoteOn 事件
    #[test]
    fn test_poll_midi_input_note_on() {
        let mut root = create_root();
        root.recording.is_recording = true;
        root.recording.started_at = Some(std::time::Instant::now());

        // 模拟 MIDI NoteOn：通道0，按键60（Middle C），力度100
        let midi_data = vec![0x90, 60, 100];
        {
            let mut buf = root.midi.input_buffer.lock().unwrap();
            buf.push_back(midi_data);
        }

        root.poll_midi_input();

        // 验证创建了一个音符（tick 基于墙钟时间，接近 0）
        assert_eq!(root.editor.editor_state.data.notes.len(), 1);
        let note = &root.editor.editor_state.data.notes[0];
        assert_eq!(note.key, 60);
        assert_eq!(note.velocity, 100);
        assert!(note.tick >= 0.0, "音符 tick 应 >= 0，实际 {}", note.tick);

        // 验证 pending_notes 追踪
        assert!(
            root.recording.pending_notes.contains_key(&60),
            "NoteOn 后应在 pending_notes 中追踪按键 60"
        );
    }

    /// 测试 poll_midi_input 处理 NoteOn + NoteOff 序列
    #[test]
    fn test_poll_midi_input_note_on_off() {
        let mut root = create_root();
        root.recording.is_recording = true;
        root.recording.started_at = Some(std::time::Instant::now());

        // 模拟 NoteOn 事件
        {
            let mut buf = root.midi.input_buffer.lock().unwrap();
            buf.push_back(vec![0x90, 60, 100]);
        }
        root.poll_midi_input();

        // 模拟 NoteOff
        {
            let mut buf = root.midi.input_buffer.lock().unwrap();
            buf.push_back(vec![0x80, 60, 0]);
        }
        root.poll_midi_input();

        // 验证音符长度已更新（基于墙钟时间，length > 0）
        assert_eq!(root.editor.editor_state.data.notes.len(), 1);
        let note = &root.editor.editor_state.data.notes[0];
        assert!(note.length > 0.0, "音符长度应大于 0，实际 {}", note.length);

        // 验证 pending_notes 已清除
        assert!(
            !root.recording.pending_notes.contains_key(&60),
            "NoteOff 后应从 pending_notes 移除按键 60"
        );
    }

    /// 测试收到 NoteOn with velocity=0 时当作 NoteOff 处理
    #[test]
    fn test_note_on_with_velocity_zero_treated_as_note_off() {
        let mut root = create_root();
        root.recording.is_recording = true;
        root.recording.started_at = Some(std::time::Instant::now());

        // 先发送 NoteOn
        {
            let mut buf = root.midi.input_buffer.lock().unwrap();
            buf.push_back(vec![0x90, 60, 100]);
        }
        root.poll_midi_input();

        assert!(root.recording.pending_notes.contains_key(&60));

        // 发送 velocity=0 的 NoteOn（MIDI 规范中视为 NoteOff）
        {
            let mut buf = root.midi.input_buffer.lock().unwrap();
            buf.push_back(vec![0x90, 60, 0]);
        }
        root.poll_midi_input();

        let note = &root.editor.editor_state.data.notes[0];
        assert!(
            note.length > 0.0,
            "velocity=0 的 NoteOn 应被当作 NoteOff 处理"
        );
        assert!(!root.recording.pending_notes.contains_key(&60));
    }

    /// 测试录制中不会插入重复的 NoteOn
    #[test]
    fn test_no_duplicate_note_on_while_pending() {
        let mut root = create_root();
        root.recording.is_recording = true;
        root.editor.playback_position = 0.0;

        // 发送两次相同的 NoteOn
        {
            let mut buf = root.midi.input_buffer.lock().unwrap();
            buf.push_back(vec![0x90, 60, 100]);
            buf.push_back(vec![0x90, 60, 90]); // 重复按键，不同力度
        }
        root.poll_midi_input();

        // 验证只创建了一个音符（重复 NoteOn 被忽略）
        assert_eq!(
            root.editor.editor_state.data.notes.len(),
            1,
            "重复 NoteOn 不应插入第二个音符"
        );
    }

    /// 测试未处于录制状态时 poll_midi_input 不处理数据
    #[test]
    fn test_poll_midi_input_no_op_when_not_recording() {
        let mut root = create_root();
        root.recording.is_recording = false;

        {
            let mut buf = root.midi.input_buffer.lock().unwrap();
            buf.push_back(vec![0x90, 60, 100]);
        }

        root.poll_midi_input();

        assert!(
            root.editor.editor_state.data.notes.is_empty(),
            "未录制时不应处理 MIDI 输入"
        );
    }

    /// 测试停止录制时处理残留的 pending 音符
    #[test]
    fn test_stop_recording_handles_pending_notes_internal() {
        let mut root = create_root();
        root.recording.is_recording = true;
        root.editor.playback_position = 100.0;

        // 手动模拟 note_on: 直接插入音符并追踪
        let note = crate::editor::note::Note::new(100.0, 60, 0.0);
        root.editor.editor_state.data.notes.push_back(note);
        root.recording.pending_notes.insert(60, 0);

        // 手动停止录制（不通过 start_recording - 需要 MIDI API）
        // 这里直接模拟 stop_recording 的核心逻辑：处理残留音符
        let default_length = root.editor.editor_state.view.default_note_length.max(1.0);
        for (_, note_idx) in root.recording.pending_notes.iter() {
            if let Some(note) = root.editor.editor_state.data.notes.get_mut(*note_idx)
                && note.length <= 0.0
            {
                note.length = default_length;
            }
        }
        root.recording.pending_notes.clear();
        root.recording.is_recording = false;

        // 验证残留音符被设置了默认长度
        assert!(root.recording.pending_notes.is_empty());
        let note = &root.editor.editor_state.data.notes[0];
        assert!(
            note.length > 0.0,
            "残留音符长度应被设置为默认长度，实际 {}",
            note.length
        );
    }

    /// 测试循环回绕全链路：引擎回绕 → playback_position → auto_scroll → 指示线位置
    ///
    /// 验证循环回绕后：
    /// 1. manager.current_tick() 回到 loop_start 附近
    /// 2. update_auto_scroll() 根据实际 tick 计算出正确的 scroll_x
    /// 3. get_playback_indicator_screen_x() 返回正确的屏幕坐标
    #[test]
    fn test_loop_wrapping_full_pipeline_position_verification() {
        use crate::playback::PlaybackManager;
        use std::time::{Duration, Instant};

        // ── 直接创建 PlaybackManager 和 Editor ──
        let mut manager = PlaybackManager::new(480);
        let mut editor = crate::editor::Editor::new();

        // 设置已知视口参数（固定值，用于精确位置计算）
        editor.editor_state.view.zoom_x = 2.0;
        editor.editor_state.view.keyboard_width = 60.0;
        editor.editor_state.view.scroll_x = 0.0;
        editor.editor_state.canvas.size = iced_core::Point::new(1280.0, 800.0);

        // ── 设置循环 ──
        manager.set_looping(true);
        manager.set_loop_range(100.0, 500.0);

        // ── 播放并等待引擎就绪 ──
        manager.play();
        // 轮询直到 tick 开始前进（引擎线程处理了 Play 命令）
        let deadline = Instant::now() + Duration::from_millis(200);
        let mut tick_before = manager.current_tick();
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
            let tick_now = manager.current_tick();
            if tick_now > tick_before {
                break; // 引擎开始播放了
            }
            tick_before = tick_now;
        }
        let tick_running = manager.current_tick();
        eprintln!("[DEBUG] tick after engine started: {:.1}", tick_running);
        assert!(
            tick_running > 0.0,
            "引擎应已开始播放，current_tick 应为正数，实际 = {}",
            tick_running,
        );

        // ── seek 到循环终点之后（600 > 500），触发回绕 ──
        manager.seek(600.0);

        // 轮询等待回绕发生（current_tick 应跳到 < loop_end）
        let deadline = Instant::now() + Duration::from_millis(200);
        let mut wrapped_tick = manager.current_tick();
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
            wrapped_tick = manager.current_tick();
            if wrapped_tick >= 80.0 && wrapped_tick <= 520.0 && wrapped_tick < 500.0 {
                // 回绕已发生：tick 在 100 附近
                break;
            }
        }
        eprintln!("[DEBUG] tick after seek+wrap: {:.1}", wrapped_tick);

        // ════════════════════════════════════════════════
        // 验证 1: 引擎层回绕
        // ════════════════════════════════════════════════
        assert!(
            wrapped_tick >= 80.0 && wrapped_tick <= 300.0,
            "引擎层回绕失败：current_tick 应接近 loop_start(100)，实际 = {}",
            wrapped_tick,
        );
        // 如果 tick 大于 500 则回绕完全没发生
        assert!(
            wrapped_tick < 500.0,
            "引擎层回绕失败：tick >= loop_end(500)，实际 = {}",
            wrapped_tick,
        );

        let current_tick = wrapped_tick;

        // ════════════════════════════════════════════════
        // 验证 2: FixedIndicatorLeft 模式
        // ════════════════════════════════════════════════
        {
            editor.set_auto_scroll_config(AutoScrollConfig {
                mode: AutoScrollMode::FixedIndicatorLeft,
                fixed_indicator_position: 300,
                ..Default::default()
            });

            // 模拟 update_playback_state: 同步引擎 tick 到编辑器
            editor.playback_position = current_tick;

            // update_auto_scroll(): target = 100 * 2.0 - 300 = -100 → clamp 到 0.0
            let scrolled = editor.update_auto_scroll(current_tick);
            assert!(scrolled, "FixedIndicatorLeft 模式 auto_scroll 应始终触发");
            assert!(
                (editor.editor_state.view.scroll_x - 0.0).abs() < f32::EPSILON,
                "scroll_x 应为 0.0 (100*2-300=-100 clamp to 0)，实际 = {}",
                editor.editor_state.view.scroll_x,
            );

            // get_playback_indicator_screen_x(): keyboard_width(60) + fixed_position(300) = 360
            let screen_x = editor.get_playback_indicator_screen_x();
            assert!(screen_x.is_some(), "指示线位置应存在");
            assert!(
                (screen_x.unwrap() - 360.0).abs() < f32::EPSILON,
                "FixedIndicatorLeft 模式指示线应在 360px，实际 = {}",
                screen_x.unwrap(),
            );
        }

        // ════════════════════════════════════════════════
        // 验证 3: ScrollingIndicator 模式（auto_scroll 不触发，指示线由 tick 计算）
        // ════════════════════════════════════════════════
        {
            editor.set_auto_scroll_config(AutoScrollConfig {
                mode: AutoScrollMode::ScrollingIndicator,
                page_trigger_offset: 100,
                page_return_position: 100,
                ..Default::default()
            });

            // 回绕后 tick=100，指示线在 viewport 左侧，不触发翻页
            let scrolled = editor.update_auto_scroll(current_tick);
            assert!(!scrolled, "ScrollingIndicator: 回绕后不应触发翻页滚动");

            // 指示线位置 = tick * zoom_x - scroll_x + keyboard_width
            let expected_indicator_x = current_tick * 2.0 - 0.0 + 60.0;
            let screen_x = editor.get_playback_indicator_screen_x();
            assert!(screen_x.is_some(), "指示线位置应存在");
            assert!(
                (screen_x.unwrap() - expected_indicator_x).abs() < 1.0,
                "ScrollingIndicator 模式指示线应在 {:.0}px ({}*2+60)，实际 = {}",
                expected_indicator_x,
                current_tick,
                screen_x.unwrap(),
            );
        }

        // ════════════════════════════════════════════════
        // 验证 4: Off 模式（无 auto_scroll，指示线由 tick 计算）
        // ════════════════════════════════════════════════
        {
            editor.set_auto_scroll_config(AutoScrollConfig {
                mode: AutoScrollMode::Off,
                ..Default::default()
            });

            let scrolled = editor.update_auto_scroll(current_tick);
            assert!(!scrolled, "Off 模式 auto_scroll 不应触发");

            let expected_indicator_x = current_tick * 2.0 - 0.0 + 60.0;
            let screen_x = editor.get_playback_indicator_screen_x();
            assert!(screen_x.is_some(), "指示线位置应存在");
            assert!(
                (screen_x.unwrap() - expected_indicator_x).abs() < 1.0,
                "Off 模式指示线应在 {:.0}px ({}*2+60)，实际 = {}",
                expected_indicator_x,
                current_tick,
                screen_x.unwrap(),
            );
        }

        // 清理
        manager.stop();
    }

    /// 测试 Bug 2 场景：先开启循环再创建播放管理器，循环状态应同步到引擎
    ///
    /// 模拟用户先点击反复开启按钮，再点击播放的流程：
    /// 1. Editor 中循环已启用（但 PlaybackManager 还不存在）
    /// 2. 创建 PlaybackManager 后将循环状态同步过去（即 init_playback_manager 中的修复）
    /// 3. 播放后 seek 到循环终点外，验证回绕正确触发
    #[test]
    fn test_loop_synced_to_new_playback_manager() {
        use crate::playback::PlaybackManager;
        use std::time::{Duration, Instant};

        let mut manager = PlaybackManager::new(480);
        manager.set_current_track_notes(Vec::new());

        // ═══ 模拟：Editor 中循环已开启但 manager 不存在 ═══
        let mut editor = crate::editor::Editor::new();
        if let Some(lr) = &mut editor.loop_range {
            lr.set_range(100.0, 500.0);
            lr.enable();
        }
        // 此时 editor.loop_range 为 enabled, [100, 500]
        // 但 manager 还不知道循环
        // 模拟 fix: 创建 manager 后同步循环状态
        if let Some(lr) = &editor.loop_range
            && lr.enabled()
        {
            manager.set_looping(true);
            manager.set_loop_range(lr.start_tick(), lr.end_tick());
        }

        // ═══ 播放并验证回绕 ═══
        manager.play();
        let deadline = Instant::now() + Duration::from_millis(200);
        let mut tick_before = manager.current_tick();
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
            let tick_now = manager.current_tick();
            if tick_now > tick_before {
                break;
            }
            tick_before = tick_now;
        }
        assert!(manager.current_tick() > 0.0, "引擎应已开始播放");

        // seek 到循环终点后 → 应触发回绕
        manager.seek(600.0);
        let deadline = Instant::now() + Duration::from_millis(200);
        let mut wrapped_tick = manager.current_tick();
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
            wrapped_tick = manager.current_tick();
            if wrapped_tick >= 80.0 && wrapped_tick < 500.0 {
                break;
            }
        }
        assert!(
            wrapped_tick >= 80.0 && wrapped_tick < 500.0,
            "期待回绕到 loop_start(100) 附近，实际 = {}",
            wrapped_tick,
        );

        manager.stop();
    }

    /// 测试 Bug 2 的完整路径：不同步循环状态时回绕不应触发
    ///
    /// 验证 fix 的必要性：如果没有 sync_loop_to_playback_state，
    /// 新创建的 PlaybackManager 不知道循环范围，seek 到循环终点后不会回绕
    #[test]
    fn test_loop_not_synced_no_wrap() {
        use crate::playback::PlaybackManager;
        use std::time::{Duration, Instant};

        let mut manager = PlaybackManager::new(480);
        manager.set_current_track_notes(Vec::new());
        // 故意不同步循环状态 - manager 不知道任何循环
        // set_looping(false) by default

        // 验证默认不循环时，seek 到 600 后继续前进（不应回绕）

        manager.play();
        let deadline = Instant::now() + Duration::from_millis(200);
        let mut tick_before = manager.current_tick();
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
            let tick_now = manager.current_tick();
            if tick_now > tick_before {
                break;
            }
            tick_before = tick_now;
        }

        // seek 到 600 → 不应回绕（因为没有设置循环）
        manager.seek(600.0);
        std::thread::sleep(Duration::from_millis(30));
        let tick_after = manager.current_tick();
        // 没有回绕，tick 应 > 500 (在 600 附近)
        assert!(
            tick_after > 500.0,
            "未同步循环状态时不应回绕，current_tick 应 > 500，实际 = {}",
            tick_after,
        );

        manager.stop();
    }
}

/// 可计数的 Mock MIDI 输出
#[allow(dead_code)]
struct CountingMockOutput {
    note_on_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
    note_off_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

impl lumino_midi_io::OutputConnection for CountingMockOutput {
    fn note_on(
        &mut self,
        _ch: u8,
        _key: u8,
        _vel: u8,
    ) -> std::result::Result<(), lumino_midi_io::Error> {
        self.note_on_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
    fn note_off(
        &mut self,
        _ch: u8,
        _key: u8,
        _vel: u8,
    ) -> std::result::Result<(), lumino_midi_io::Error> {
        self.note_off_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
    fn control_change(
        &mut self,
        _ch: u8,
        _controller: u8,
        _value: u8,
    ) -> std::result::Result<(), lumino_midi_io::Error> {
        Ok(())
    }
    fn program_change(
        &mut self,
        _ch: u8,
        _program: u8,
    ) -> std::result::Result<(), lumino_midi_io::Error> {
        Ok(())
    }
    fn pitch_bend(&mut self, _ch: u8, _value: f32) -> std::result::Result<(), lumino_midi_io::Error> {
        Ok(())
    }
    fn channel_pressure(
        &mut self,
        _ch: u8,
        _pressure: u8,
    ) -> std::result::Result<(), lumino_midi_io::Error> {
        Ok(())
    }
    fn poly_pressure(
        &mut self,
        _ch: u8,
        _key: u8,
        _pressure: u8,
    ) -> std::result::Result<(), lumino_midi_io::Error> {
        Ok(())
    }
    fn send_raw(&mut self, _data: [u8; 3]) -> std::result::Result<(), lumino_midi_io::Error> {
        Ok(())
    }
    fn close(self: Box<Self>) {}
}
