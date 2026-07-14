//! 播放控制相关方法
//!
//! 包括 play/pause/stop/loop 以及录制功能。

use super::ToolbarHandler;
use crate::root::Root;

impl ToolbarHandler {
    pub(crate) fn handle_toolbar_playback(&self, root: &mut Root, event: &crate::toolbar::Event) {
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
        if root.playback.manager.is_none() {
            Self::init_playback_manager(root);
        }

        if let Some(manager) = &mut root.playback.manager {
            manager.play();
            root.toolbar.is_playing = true;
            tracing::info!("Root: 开始播放");
        }
    }

    /// 执行暂停逻辑
    fn do_pause(root: &mut Root) {
        if let Some(manager) = &mut root.playback.manager {
            manager.pause();
            root.toolbar.is_playing = false;
            tracing::info!("Root: 暂停播放");
        }
    }

    /// 执行停止逻辑
    fn do_stop(root: &mut Root) {
        if let Some(manager) = &mut root.playback.manager {
            manager.stop();
            root.toolbar.is_playing = false;
            root.editor.playback_position = 0.0;
            tracing::info!("Root: 停止播放");
        }
    }

    /// 执行循环切换逻辑
    fn do_toggle_loop(root: &mut Root) {
        crate::root::handlers::loop_range::LoopRangeHandler::handle_action(
            root,
            crate::message::LoopRangeAction::Toggle,
        );
    }

    /// 初始化播放管理器
    fn init_playback_manager(root: &mut Root) {
        use crate::playback::PlaybackManager;

        let division = root.editor.editor_state.view.ppq;
        let mut manager = PlaybackManager::new(division);

        // 先创建空的 manager，让 update_playback_notes 能工作
        manager.set_current_track_notes(Vec::new());
        root.playback.manager = Some(manager);

        // 通过 update_playback_notes 填充所有音轨的音符（含 document 懒加载）
        root.update_playback_notes();

        // 用缓存的 MIDI 输出连接
        if let Some(output) = root.playback.pending_midi_output.take()
            && let Some(manager) = &mut root.playback.manager
        {
            manager.set_midi_output(output);
        }

        // 应用缓存的 tempo 变化
        if let Some(changes) = root.playback.pending_tempo_changes.take()
            && let Some(manager) = &mut root.playback.manager
        {
            manager.set_tempo_changes(changes);
        }

        if let Some(_manager) = &root.playback.manager {
            tracing::info!(
                "Root: 播放管理器已初始化 (division={}, 过滤阈值={})",
                division,
                root.visual.velocity_filter_threshold,
            );
        }

        // 同步循环状态到播放引擎（用户可能在创建管理器前已开启循环）
        if let Some(loop_range) = &root.editor.loop_range
            && loop_range.enabled()
            && let Some(manager) = &mut root.playback.manager
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

    /// 处理录制
    pub(crate) fn handle_toolbar_recording(&self, root: &mut Root, event: &crate::toolbar::Event) {
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
}
