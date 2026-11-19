//! 播放控制相关方法
//!
//! 包括 play/pause/stop/loop、走带小节步进 / 全曲边界定位以及录制功能。

use super::ToolbarHandler;
use super::seek::{global_eot_tick, next_measure_tick, previous_measure_tick};
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
            crate::toolbar::Event::SkipBackward => {
                Self::do_skip_measure(root, false);
            }
            crate::toolbar::Event::SkipForward => {
                Self::do_skip_measure(root, true);
            }
            crate::toolbar::Event::SkipBackwardToStart => {
                Self::do_skip_to_start(root);
            }
            crate::toolbar::Event::SkipForwardToEnd => {
                Self::do_skip_to_end(root);
            }
            crate::toolbar::Event::ToggleLoop => {
                Self::do_toggle_loop(root);
            }
            _ => {}
        }
    }

    /// 执行播放逻辑
    ///
    /// **自动提交编辑**：开始播放前调用 `commit_current_edit()`，
    /// 保证播放的是用户当前正在编辑的音符（ghost 方案：松手即提交）。
    fn do_play(root: &mut Root) {
        // 自动提交当前编辑（ghost 方案：松手即提交）
        if root.editor.commit_current_edit() {
            tracing::debug!("播放前自动提交编辑");
        }

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
            // 停止后清空键盘演奏颜色，恢复无颜色状态
            root.editor.clear_playback_key_colors();
            tracing::info!("Root: 停止播放");
        }
    }

    /// 走带倒带 / 快进：单点步进一个小节。
    ///
    /// - 快进：严格跳到下一条小节线；
    /// - 快退：本小节前 1/2 退到上一个小节，后 1/2（含正中）回本小节开头。
    fn do_skip_measure(root: &mut Root, forward: bool) {
        let tick = root.editor.playback_position;
        let ppq = root.editor.editor_state.view.ppq;
        let target = {
            let signatures = &root.editor.editor_state.data.time_signatures;
            if forward {
                next_measure_tick(tick, ppq, signatures)
            } else {
                previous_measure_tick(tick, ppq, signatures)
            }
        };
        Self::do_seek(root, target);
    }

    /// 走带倒带 + Ctrl：回到全曲开头（播放中继续从开头播放）。
    fn do_skip_to_start(root: &mut Root) {
        Self::do_seek(root, 0.0);
    }

    /// 走带快进 + Ctrl：定位到全局 EOT（全曲末尾）。
    ///
    /// 播放中必须先停再 seek：播放引擎越过轨尾会自动 `stop()` 并把进度复位为 0，
    /// 若保持播放态 seek 到末尾，下一帧指示线会被自动停止拉回开头。
    fn do_skip_to_end(root: &mut Root) {
        let target = global_eot_tick(root.editor.editor_state.data.document.as_ref());
        if root.toolbar.is_playing {
            if let Some(manager) = &mut root.playback.manager {
                manager.stop();
            }
            root.toolbar.is_playing = false;
            root.editor.clear_playback_key_colors();
        }
        Self::do_seek(root, target);
    }

    /// 统一的走带定位入口：先写编辑态位置，再同步播放引擎。
    ///
    /// 管理器尚未创建时只写 `playback_position`——首次播放会经
    /// `init_playback_manager` 的 REND-018 路径 seek 到该位置。
    fn do_seek(root: &mut Root, tick: f32) {
        let tick = tick.max(0.0);
        root.editor.playback_position = tick;
        if let Some(manager) = &mut root.playback.manager {
            manager.seek(tick);
        }
        tracing::debug!("Root: 走带定位到 {tick:.2} tick");
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
        manager.rebuild_current_track_queue();
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

        // REND-002：文档装载的强制重建会复位合成器通道混音（`clone_channel_mix`），
        // 新 manager 接线完成后按面板值重新下发一次，避免"面板显示增益 vs
        // 合成器实际增益"不一致（此前只在用户动滑块时才同步）。
        root.update_playback_track_mix();

        // REND-018：惰性创建时同步演奏指示线位置——加载工程后（管理器尚未创建）
        // 拖动指示线再按播放，应从拖动位置起播。此前只同步了音符/输出/tempo/
        // 循环/混音，manager 内部位置仍为 0，`play()` 会从头开始，而拖动产生的
        // seek 因 `manager.is_none()` 被静默丢弃。放在这里可一次性覆盖所有
        // "重建 manager 后内部位置归零"的路径。
        let start_tick = root.editor.playback_position;
        if start_tick > f32::EPSILON
            && let Some(manager) = &mut root.playback.manager
        {
            manager.seek(start_tick);
            tracing::debug!("Root: 播放管理器初始 seek 到 {start_tick:.2}");
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
