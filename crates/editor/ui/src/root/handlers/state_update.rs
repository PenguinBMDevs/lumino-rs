//! 简单状态更新消息处理器
//!
//! 处理 `try_handle_simple_state` 中的剩余直接消息，
//! 包括滚动条、缩放、画布边界、动画帧、性能数据、MIDI 输入等。
//!
//! 分派表按业务对象拆到 `state_update/` 子模块；本文件只保留穷尽式分派表与
//! 动画帧处理。分派表不写 `_` 兜底：原兜底臂覆盖的变体显式列全并返回 false，
//! 新增 `Message` 变体必须在此归类，编译期即报错，避免被静默吞掉。

use crate::message::Message;
use crate::root::Root;
use std::time::Instant;

mod arrangement_state;
mod canvas_bounds;
mod ui_state;
mod view_state;

impl Root {
    /// 处理简单的状态更新消息
    pub(crate) fn try_handle_simple_state(&mut self, msg: &Message) -> bool {
        match msg {
            Message::Progress(progress) => self.handle_progress(progress),
            Message::ScrollbarScrolled(x) => self.scrollbar_x(*x),
            Message::ScrollbarScrolledY(y) => self.scrollbar_y(*y),
            Message::ArrangementScrollX(x) => self.handle_arrangement_scroll_x(*x),
            Message::ArrangementScrollY(y) => self.handle_arrangement_scroll_y(*y),
            Message::ArrangementZoomX { zoom, fixed_ratio } => {
                self.handle_arrangement_zoom_x(*zoom, *fixed_ratio)
            }
            Message::ArrangementZoomY { zoom, fixed_ratio } => {
                self.handle_arrangement_zoom_y(*zoom, *fixed_ratio)
            }
            Message::ZoomXChanged { zoom, fixed_ratio } => self.zoom_x(*zoom, *fixed_ratio),
            Message::ZoomYChanged { zoom, fixed_ratio } => self.zoom_y(*zoom, *fixed_ratio),
            Message::CanvasBoundsChanged { offset, size } => self.canvas_bounds(*offset, *size),
            Message::MenuStateChanged(is_open) => self.menu_state(*is_open),
            Message::CtrlKeyChanged(pressed) => self.ctrl_key(*pressed),
            Message::ShiftKeyChanged(pressed) => self.shift_key(*pressed),
            Message::ToggleSettings | Message::Null => true,
            Message::ModeToggled => self.handle_mode_toggle(),
            Message::AnimationTick => self.handle_animation_tick(),
            Message::VelocityPanelResize(height) => self.velocity_height(*height),
            Message::PerfUpdate(data) => self.perf_update(*data),
            Message::MidiInputEvent { data } => self.midi_input(data),
            Message::ArrangementCursorSet(tick) => self.arrange_cursor(*tick),
            Message::ArrangementSelectionChanged(rect) => self.arrange_selection(rect),
            Message::ArrangementSelectionCleared => self.arrange_selection_clear(),
            Message::ArrangementMoveNotes {
                delta_ticks,
                delta_tracks,
            } => self.arrange_move(*delta_ticks, *delta_tracks),
            Message::ArrangementErase {
                tick_start,
                tick_end,
                track_lo,
                track_hi,
            } => self.arrange_erase(*tick_start, *tick_end, *track_lo, *track_hi),
            Message::ArrangementRazor { tick, track } => self.arrange_razor(*tick, *track),
            Message::ArrangementAddNote {
                track,
                tick,
                duration,
                key,
                velocity,
            } => self.arrange_add_note(*track, *tick, *duration, *key, *velocity),
            Message::ArrangementGhostNotesUpdated(notes) => self.arrange_ghost_notes(notes),
            Message::ArrangementDragSelectionRect(rect) => self.arrange_drag_rect(rect),
            Message::ArrangementCopy => self.arrange_copy(),
            Message::ArrangementPaste => self.arrange_paste(),
            Message::ArrangementCut => self.arrange_cut(),
            Message::ArrangementDeleteSelection => self.arrange_delete(),
            Message::ArrangementSelectAll => self.arrange_select_all(),
            Message::VideoClip(action) => self.handle_video_clip_action(action.clone()),
            // 原 `_ => false` 的等价展开：下列变体在此处理器中不被消费，
            // 交由 MessageRouter / 其它处理器处理，返回值保持 false。
            Message::Core(_)
            | Message::Window(_)
            | Message::Sidebar(_)
            | Message::EditorAction(_)
            | Message::AudioAction(_)
            | Message::Settings(_)
            | Message::Toolbar(_)
            | Message::CustomPrecision(_)
            | Message::BrushSettings(_)
            | Message::DrawSettings(_)
            | Message::FillDivision(_)
            | Message::Help(_)
            | Message::ShapeDrawer(_)
            | Message::History(_)
            | Message::Collaboration(_)
            | Message::LoadConfirm(_)
            | Message::SaveConfirm(_)
            | Message::AutosaveRecover(_)
            | Message::ProjectSettings(_)
            | Message::PortSoundfonts(_)
            | Message::PortSoundfontsUnsupported(_)
            | Message::SettingsDialog(_)
            | Message::Velocity(_)
            | Message::LoopRange(_)
            | Message::AudioExport(_)
            | Message::SpeedChange(_)
            | Message::MaterialDelete(_)
            | Message::TempoMaxBpm(_)
            | Message::BatchEdit(_)
            | Message::RecoverTrack(_)
            | Message::VideoExport(_)
            | Message::Batch(_)
            | Message::PianoRollContextMenu(_)
            | Message::MaskPanel(_)
            | Message::RightSidebar(_)
            | Message::Cloud(_) => false,
        }
    }

    /// 处理动画 tick（切换动画 + 平滑滚动 + 弹簧物理 + 关于页 logo 彩蛋）
    ///
    /// 模式切换动画与平滑滚动两个子步骤在 [`super::animation_tick`]（逐行等价搬迁，
    /// 目的：`state_update.rs` 回到 600 行内，且把「翻转 `AppMode` 的状态机」与
    /// CPU 后端的过渡守卫放在同一模块）。
    pub(crate) fn handle_animation_tick(&mut self) -> bool {
        // 每次 tick 只取一次时钟，避免同一帧内多个动画读到不同时间基准
        let now = Instant::now();

        // 模式切换动画 + `AppMode` 归属同步（含 CPU 后端下的过渡取消；
        // 状态机与守卫同在 `animation_tick` 模块，防止「只看 current_mode」的竞态）
        self.tick_mode_animation();

        // 平滑滚动推进 + 越界钳制（REND-019 重绘风暴的收敛判定在该方法内）
        self.tick_smooth_scroll();

        self.editor.update_selection_box_animation(None);

        // 音轨拖拽排序长按计时：候选按下后超过阈值自动激活拖拽
        self.sidebar.update_track_reorder_timer(now);

        // 轮询异步 MoveOp 提交结果（每帧一次，将后台线程结果应用到 data 并 push history）
        if self.editor.poll_async_commit().is_some() {
            self.update_playback_notes();
            self.editor.clear_notes_changed();
        }

        // 补放本地编辑临界区期间延迟的远端音符操作（每帧一次；临界区结束后应用）
        self.drain_deferred_remote_ops();

        // 清理过期 Toast（每帧调用，低成本 O(N) retain）
        self.toast.cleanup_expired(now);

        // 关于页 logo 彩蛋（UI-007）：推进晃动 / 坠落消失序列。
        // 相位推进必须在 update 路径完成——`Root::view` 只读 `&self`，无法推进状态。
        self.state.about_egg.update(now);

        // 关于页回声洞彩蛋（UI-006）：推进打字机逐字揭示 / 闪烁退出。
        // 同上，推进只在 update 路径；`view` 只读 `text()` 与 `opacity()`。
        self.state.echo_cave.update(now);

        // 落地瞬间播放内置「钢管落地」音效：信号是消费式的，保证整段序列只响一次。
        // 播放走专属工作线程（立即返回），音频输出设备跟随设置面板的选择。
        if self.state.about_egg.take_impact_signal() {
            lumino_midi_io::ui_sfx::play_pipe_impact(
                self.settings.synth.selected_audio_output_device.as_deref(),
            );
        }

        true
    }
}
