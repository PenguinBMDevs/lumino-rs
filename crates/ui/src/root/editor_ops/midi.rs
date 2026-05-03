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
        if let Some(manager) = &mut self.playback_manager {
            manager.update_tempo_changes(tempo_change_list);
            tracing::debug!("Root::load_tempo_changes: tempo changes updated in playback manager");
        } else {
            self.pending_tempo_changes = Some(tempo_change_list);
            tracing::debug!(
                "Root::load_tempo_changes: playback manager not ready, cached tempo changes"
            );
        }
    }

    /// 设置 MIDI 输出连接
    pub fn set_midi_output(&mut self, output: Box<dyn lumino_midi::OutputConnection>) {
        if let Some(manager) = &mut self.playback_manager {
            manager.set_midi_output(output);
            tracing::info!("Root::set_midi_output: MIDI output connection set");
        } else {
            self.pending_midi_output = Some(output);
            tracing::debug!(
                "Root::set_midi_output: playback manager not ready, cached MIDI output"
            );
        }
    }

    /// 清除 MIDI 输出连接
    pub fn clear_midi_output(&mut self) {
        if let Some(manager) = &mut self.playback_manager {
            manager.clear_midi_output();
            tracing::info!("Root::clear_midi_output: MIDI output connection cleared");
        }
        self.pending_midi_output = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::note::Note;
    use crate::message::Message;
    use crate::playback::PlaybackState;
    use crate::toolbar;
    use lumino_core::storage::config::UiConfig;

    /// 模拟 MIDI 输出连接，用于测试 playback 流程
    struct MockOutput {
        _note_on_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
        _note_off_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
    }

    impl lumino_midi::OutputConnection for MockOutput {
        fn note_on(
            &mut self,
            _ch: u8,
            _key: u8,
            _vel: u8,
        ) -> std::result::Result<(), lumino_midi::Error> {
            self._note_on_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        }
        fn note_off(
            &mut self,
            _ch: u8,
            _key: u8,
            _vel: u8,
        ) -> std::result::Result<(), lumino_midi::Error> {
            self._note_off_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        }
        fn control_change(
            &mut self,
            _ch: u8,
            _controller: u8,
            _value: u8,
        ) -> std::result::Result<(), lumino_midi::Error> {
            Ok(())
        }
        fn program_change(
            &mut self,
            _ch: u8,
            _program: u8,
        ) -> std::result::Result<(), lumino_midi::Error> {
            Ok(())
        }
        fn pitch_bend(
            &mut self,
            _ch: u8,
            _value: f32,
        ) -> std::result::Result<(), lumino_midi::Error> {
            Ok(())
        }
        fn channel_pressure(
            &mut self,
            _ch: u8,
            _pressure: u8,
        ) -> std::result::Result<(), lumino_midi::Error> {
            Ok(())
        }
        fn poly_pressure(
            &mut self,
            _ch: u8,
            _key: u8,
            _pressure: u8,
        ) -> std::result::Result<(), lumino_midi::Error> {
            Ok(())
        }
        fn send_raw(&mut self, _data: [u8; 3]) -> std::result::Result<(), lumino_midi::Error> {
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
        assert!(root.playback_manager.is_none(), "初始无播放管理器");
        assert!(root.pending_midi_output.is_none(), "初始无挂起 MIDI 输出");

        // 添加测试音符
        root.editor.notes.push_back(Note::new(0.0, 60, 480.0));
        root.editor.notes.push_back(Note::new(480.0, 64, 480.0));

        // 设置 MIDI 输出 → 因无管理器应缓存
        root.set_midi_output(create_mock_output());
        assert!(
            root.pending_midi_output.is_some(),
            "无播放管理器时 set_midi_output 应缓存到 pending_midi_output"
        );
        assert!(
            root.playback_manager.is_none(),
            "pending 状态不应创建播放管理器"
        );

        // 发送 Play 消息 → 应消费缓存并创建管理器
        root.update(Message::Toolbar(toolbar::Event::Play));
        assert!(root.playback_manager.is_some(), "Play 消息应创建播放管理器");
        assert!(
            root.pending_midi_output.is_none(),
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
        root.editor.notes.push_back(Note::new(0.0, 60, 480.0));
        root.editor.notes.push_back(Note::new(480.0, 64, 480.0));

        // 先通过 Play 创建 playback_manager
        root.set_midi_output(create_mock_output());
        root.update(Message::Toolbar(toolbar::Event::Play));
        assert!(root.playback_manager.is_some());
        assert!(root.pending_midi_output.is_none());

        // 此时再调用 set_midi_output：应直接传递给 manager，不缓存
        root.set_midi_output(create_mock_output());
        assert!(
            root.pending_midi_output.is_none(),
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
        assert!(root.pending_midi_output.is_some());

        root.clear_midi_output();
        assert!(
            root.pending_midi_output.is_none(),
            "clear_midi_output 应清除 pending"
        );

        // 有管理器时
        root.editor.notes.push_back(Note::new(0.0, 60, 480.0));
        root.set_midi_output(create_mock_output());
        root.update(Message::Toolbar(toolbar::Event::Play));
        root.clear_midi_output();
        assert!(
            root.pending_midi_output.is_none(),
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
        root.editor.notes.push_back(Note::new(0.0, 60, 480.0));
        root.editor.notes.push_back(Note::new(480.0, 64, 240.0));
        root.editor.notes.push_back(Note::new(720.0, 67, 480.0));

        // 设置 MIDI 输出
        root.set_midi_output(create_mock_output());
        assert!(root.pending_midi_output.is_some());

        // 播放
        root.update(Message::Toolbar(toolbar::Event::Play));
        assert!(root.playback_manager.is_some(), "播放管理器应被创建");
        assert!(
            root.pending_midi_output.is_none(),
            "pending MIDI 输出应被消费"
        );
        assert!(root.toolbar.is_playing, "播放后工具栏应标记为 playing");

        // 停止
        root.update(Message::Toolbar(toolbar::Event::Stop));

        // 验证停止后状态（manager.state() 是异步的，但发送 Command::Stop 后
        // 引擎线程会处理并更新状态。由于 Manager::stop() 是同步发送消息，
        // 引擎线程在下一个 1ms 睡眠周期处理它。此处给线程一点时间处理。）
        if let Some(ref manager) = root.playback_manager {
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
        assert!(root.playback_manager.is_some());

        // 添加音符并标记变更
        root.editor.notes.push_back(Note::new(0.0, 60, 480.0));
        root.editor.mark_notes_changed();

        // 触发音符更新（模拟 handle_editor_action 中的流程）
        if root.editor.notes_changed() {
            root.update_playback_notes();
            root.editor.clear_notes_changed();
        }

        // 管理器仍存在，pending 无缓存
        assert!(root.playback_manager.is_some());
        assert!(root.pending_midi_output.is_none());

        root.update(Message::Toolbar(toolbar::Event::Stop));
    }

    /// 测试 load_tempo_changes 在无管理器时缓存
    #[test]
    fn test_tempo_changes_cached_when_no_manager() {
        let mut root = create_root();

        assert!(root.pending_tempo_changes.is_none());

        root.load_tempo_changes(vec![(0, 500000)]); // 120 BPM
        assert!(
            root.pending_tempo_changes.is_some(),
            "无管理器时 tempo changes 应缓存"
        );

        // 播放时应消费缓存的 tempo changes
        root.editor.notes.push_back(Note::new(0.0, 60, 480.0));
        root.set_midi_output(create_mock_output());
        root.update(Message::Toolbar(toolbar::Event::Play));

        assert!(
            root.pending_tempo_changes.is_none(),
            "播放后 tempo changes 应被消费"
        );
        assert!(root.playback_manager.is_some());

        root.update(Message::Toolbar(toolbar::Event::Stop));
    }

    /// 测试 set_midi_output 和 set_playback_midi_output (Host 层) 的一致性
    #[test]
    fn test_host_set_playback_midi_output_flow() {
        let mut root = create_root();
        root.editor.notes.push_back(Note::new(0.0, 60, 480.0));

        // Host::set_playback_midi_output → Root::set_midi_output
        root.set_midi_output(create_mock_output());
        assert!(root.pending_midi_output.is_some());

        root.update(Message::Toolbar(toolbar::Event::Play));
        assert!(root.pending_midi_output.is_none());
        assert!(root.playback_manager.is_some());

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
        root.editor.notes.push_back(Note::new(0.0, 60, 480.0));

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
        assert!(root.pending_midi_output.is_some());

        // 发送 Play 消息
        root.update(Message::Toolbar(toolbar::Event::Play));
        assert!(root.playback_manager.is_some(), "播放管理器应被创建");
        assert!(
            root.pending_midi_output.is_none(),
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
            root.pending_midi_output.is_some(),
            "启动后 pending_midi_output 应有值"
        );

        // 步骤2：用户画一个音符（模拟鼠标操作）
        // 这会在 finish_drawing 中调用 mark_notes_changed
        // 然后在 handle_editor_action 中调用 update_playback_notes
        // 但此时 playback_manager 还不存在，所以 update_playback_notes 什么都不做
        root.editor.notes.push_back(Note::new(0.0, 60, 480.0));
        root.editor.mark_notes_changed();

        // 模拟 handle_editor_action 的处理
        if root.editor.notes_changed() {
            root.update_playback_notes();
            root.editor.clear_notes_changed();
        }

        // 此时 playback_manager 应该还不存在
        assert!(
            root.playback_manager.is_none(),
            "画音符后不应创建播放管理器"
        );

        // 步骤3：用户点击播放按钮
        root.update(Message::Toolbar(toolbar::Event::Play));

        // 播放管理器应该被创建
        assert!(
            root.playback_manager.is_some(),
            "点击播放后应创建播放管理器"
        );
        assert!(
            root.pending_midi_output.is_none(),
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
        root.editor.notes.push_back(Note::new(0.0, 60, 480.0));
        root.update(Message::Toolbar(toolbar::Event::Play));
        assert!(root.playback_manager.is_some(), "第一次播放应创建管理器");

        // 停止
        root.update(Message::Toolbar(toolbar::Event::Stop));
        thread::sleep(Duration::from_millis(50));

        // 步骤2：再画一个音符（模拟用户操作）
        root.editor.notes.push_back(Note::new(480.0, 64, 480.0));
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
}

/// 可计数的 Mock MIDI 输出
#[allow(dead_code)]
struct CountingMockOutput {
    note_on_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
    note_off_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

impl lumino_midi::OutputConnection for CountingMockOutput {
    fn note_on(
        &mut self,
        _ch: u8,
        _key: u8,
        _vel: u8,
    ) -> std::result::Result<(), lumino_midi::Error> {
        self.note_on_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
    fn note_off(
        &mut self,
        _ch: u8,
        _key: u8,
        _vel: u8,
    ) -> std::result::Result<(), lumino_midi::Error> {
        self.note_off_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
    fn control_change(
        &mut self,
        _ch: u8,
        _controller: u8,
        _value: u8,
    ) -> std::result::Result<(), lumino_midi::Error> {
        Ok(())
    }
    fn program_change(
        &mut self,
        _ch: u8,
        _program: u8,
    ) -> std::result::Result<(), lumino_midi::Error> {
        Ok(())
    }
    fn pitch_bend(&mut self, _ch: u8, _value: f32) -> std::result::Result<(), lumino_midi::Error> {
        Ok(())
    }
    fn channel_pressure(
        &mut self,
        _ch: u8,
        _pressure: u8,
    ) -> std::result::Result<(), lumino_midi::Error> {
        Ok(())
    }
    fn poly_pressure(
        &mut self,
        _ch: u8,
        _key: u8,
        _pressure: u8,
    ) -> std::result::Result<(), lumino_midi::Error> {
        Ok(())
    }
    fn send_raw(&mut self, _data: [u8; 3]) -> std::result::Result<(), lumino_midi::Error> {
        Ok(())
    }
    fn close(self: Box<Self>) {}
}
