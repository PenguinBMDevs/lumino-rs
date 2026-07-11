//! Editor 核心方法
//!
//! 包含：构造函数、内存分析、远端光标、音频动作、撤销重做、框选动画、播放键色

use crate::editor::note::Note;
use crate::editor::velocity::VelocityPanel;
use crate::editor::{Editor, EditorMemory, SpatialIndexState, grid, onion_track_color};
use crate::message::AudioAction;
use iced_core::Point;
use iced_widget::canvas;
use std::cell::Cell;

impl Editor {
    /// 创建新的编辑器实例
    pub fn new() -> Self {
        Self {
            editor_state: crate::editor::editor_state::EditorState::new(),
            grid_cache: canvas::Cache::new(),
            keyboard_cache: canvas::Cache::new(),
            ruler_cache: canvas::Cache::new(),
            spatial: SpatialIndexState::default(),
            remote_cursors: std::collections::HashMap::new(),
            playback_position: 0.0,
            playback_key_colors: [0u8; 1024], // 256 keys × 4 bytes
            playback_key_colors_enabled: false,
            loop_range: Some(grid::LoopRange::new()),
            notes_changed: false,
            velocity_panel: VelocityPanel::new(),
            selection_box_anim: Cell::new(None),
        }
    }

    /// 收集编辑器各组件的内存占用快照
    pub fn memory_breakdown(&self) -> EditorMemory {
        let d = &self.editor_state.data;
        let note_size = std::mem::size_of::<Note>();

        // editor.notes
        let notes_len = d.notes.len();
        let notes_bytes = notes_len * note_size;

        // track_notes
        let track_notes_entries = d.track_notes.len();
        let mut track_notes_count = 0usize;
        let mut track_notes_bytes = 0usize;
        for notes in d.track_notes.values() {
            track_notes_count += notes.len();
            track_notes_bytes += notes.len() * note_size;
        }

        // document notes (NoteEvent=16B, (u32,f32)=8B)
        let doc_is_some = d.document.is_some();
        let doc_notes_cap: usize = d
            .document
            .as_ref()
            .map(|d| d.notes.iter().map(|v| v.capacity()).sum())
            .unwrap_or(0);
        let doc_events_bytes = d
            .document
            .as_ref()
            .map(|doc| {
                doc_notes_cap * std::mem::size_of::<lumino_midi_loader::NoteEvent>() // NoteEvent
                    + doc.tempo_changes.capacity() * 8 // (u32, f32)
            })
            .unwrap_or(0);

        tracing::info!(
            "[MEMORY_DEBUG] document={}, notes_cap={}, notes_len={}, track_notes_entries={}, track_notes_count={}",
            doc_is_some,
            doc_notes_cap,
            notes_len,
            track_notes_entries,
            track_notes_count,
        );

        EditorMemory {
            notes_bytes,
            track_notes_count,
            track_notes_bytes,
            track_notes_entries,
            document_events_bytes: doc_events_bytes,
        }
    }

    /// 更新远端鼠标位置
    pub fn update_remote_cursor(
        &mut self,
        user_id: std::sync::Arc<str>,
        x: f32,
        y: f32,
        color: std::sync::Arc<str>,
        username: std::sync::Arc<str>,
    ) {
        self.remote_cursors.insert(
            user_id.to_string(),
            (Point::new(x, y), color.to_string(), username.to_string()),
        );
    }

    /// 移除远端鼠标
    pub fn remove_remote_cursor(&mut self, user_id: &str) {
        self.remote_cursors.remove(user_id);
        self.grid_cache.clear();
    }

    /// 获取并清空待处理的音频动作
    pub fn take_audio_actions(&mut self) -> Vec<AudioAction> {
        let actions = self.editor_state.interaction.take_audio_actions();
        if !actions.is_empty() {
            tracing::debug!("Editor: 取出了 {} 个音频动作", actions.len());
        }
        actions
    }

    /// Push current state to history
    pub fn push_history(&mut self) {
        self.editor_state.data.push_history();
    }

    /// Undo the last action
    pub fn undo(&mut self) -> bool {
        if self.editor_state.data.undo() {
            self.grid_cache.clear();
            self.mark_notes_changed();
            true
        } else {
            false
        }
    }

    /// Redo the last undone action
    pub fn redo(&mut self) -> bool {
        if self.editor_state.data.redo() {
            self.grid_cache.clear();
            self.mark_notes_changed();
            tracing::info!("重做操作成功");
            true
        } else {
            tracing::info!("没有可重做的操作");
            false
        }
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        self.editor_state.data.history.can_undo()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        self.editor_state.data.history.can_redo()
    }

    /// 更新框选框的弹簧物理动画
    ///
    /// 使用弹簧物理模拟让选择框边界产生 Q 弹的弹性效果。
    /// 以 snap_precision 为精度单位"跳跃"，在跳跃之间使用弹簧动画过渡：
    /// - 鼠标移动时，先计算吸附到网格的目标位置
    /// - 只有当吸附位置发生变化时，才更新弹簧目标
    /// - 弹簧以弹性方式从上一个吸附位置过渡到新的吸附位置
    /// - 弹簧收敛后标记 converged，供 frame.rs 停止 AnimationTick 轮询
    ///
    /// `mouse_pos`:
    /// - `Some(pos)`: 鼠标移动中，重新计算吸附目标
    /// - `None`: 持续推进弹簧物理向现有目标收敛（用于 AnimationTick）
    pub(crate) fn update_selection_box_animation(&self, mouse_pos: Option<Point>) {
        use crate::editor::EditState;
        use crate::editor::SelectionBoxAnimState;
        use lumino_core::storage::config::SelectionBoxMode;

        // 直接跟随模式：不需要弹簧动画，直接返回
        if self.editor_state.view.selection_box_mode == SelectionBoxMode::Direct {
            // 清除任何残留的动画状态
            self.selection_box_anim.set(None);
            return;
        }

        let interaction = &self.editor_state.interaction;

        match interaction.edit_state {
            EditState::Selecting {
                start_tick,
                start_key,
                current_tick,
                current_key,
                ..
            } => {
                // 计算起点的屏幕坐标（固定锚点）
                let start_x = self.tick_to_x(start_tick);
                let start_y = self.key_to_y(start_key);
                let start_pos = Point::new(start_x, start_y);

                // 计算吸附后的目标位置
                let snapped_tick = if let Some(pos) = mouse_pos {
                    let tick = self.x_to_tick(pos.x);
                    self.snap_tick(tick)
                } else {
                    current_tick
                };
                let snapped_key = if let Some(pos) = mouse_pos {
                    self.y_to_key(pos.y)
                } else {
                    current_key
                };

                // 获取或初始化动画状态
                let anim = self.selection_box_anim.get();

                let (display_current, mut velocity, last_snapped_tick, last_snapped_key) =
                    if let Some(state) = anim {
                        (
                            state.current_pos,
                            state.velocity,
                            state.snapped_tick,
                            state.snapped_key,
                        )
                    } else {
                        // 初始状态：显示位置等于第一个吸附位置
                        let init_x = self.tick_to_x(snapped_tick);
                        let init_y = self.key_to_y(snapped_key);
                        (
                            Point::new(init_x, init_y),
                            Point::new(0.0, 0.0),
                            snapped_tick,
                            snapped_key,
                        )
                    };

                // 判断吸附位置是否发生变化
                let snapped_changed =
                    snapped_tick != last_snapped_tick || snapped_key != last_snapped_key;

                // 计算弹簧目标位置：吸附位置变化时更新目标，否则保持上一次的目标
                let spring_target = if snapped_changed {
                    let target_x = self.tick_to_x(snapped_tick);
                    let target_y = self.key_to_y(snapped_key);
                    Point::new(target_x, target_y)
                } else {
                    let target_x = self.tick_to_x(last_snapped_tick);
                    let target_y = self.key_to_y(last_snapped_key);
                    Point::new(target_x, target_y)
                };

                // 弹簧物理参数（Q弹效果）
                const STIFFNESS: f32 = 400.0; // 弹簧刚度（越大回弹越快）
                const DAMPING: f32 = 15.0; // 阻尼系数（越小越弹）
                const MASS: f32 = 1.0; // 质量
                const DT: f32 = 1.0 / 60.0; // 固定时间步长（假设60fps）
                const SUB_STEPS: i32 = 4; // 每帧子步数，提高稳定性

                let mut current = display_current;

                // 半隐式欧拉积分，多子步提高稳定性
                for _ in 0..SUB_STEPS {
                    let dt = DT / SUB_STEPS as f32;

                    // 计算弹簧力（胡克定律）
                    let displacement_x = spring_target.x - current.x;
                    let displacement_y = spring_target.y - current.y;
                    let spring_force_x = STIFFNESS * displacement_x;
                    let spring_force_y = STIFFNESS * displacement_y;

                    // 计算阻尼力
                    let damping_force_x = DAMPING * velocity.x;
                    let damping_force_y = DAMPING * velocity.y;

                    // 计算加速度（F = ma => a = F/m）
                    let accel_x = (spring_force_x - damping_force_x) / MASS;
                    let accel_y = (spring_force_y - damping_force_y) / MASS;

                    // 更新速度和位置
                    velocity.x += accel_x * dt;
                    velocity.y += accel_y * dt;
                    current.x += velocity.x * dt;
                    current.y += velocity.y * dt;
                }

                // 弹簧收敛判断：位置和速度都足够接近目标时标记收敛
                let dx = current.x - spring_target.x;
                let dy = current.y - spring_target.y;
                let dist_sq = dx * dx + dy * dy;
                let speed_sq = velocity.x * velocity.x + velocity.y * velocity.y;
                const POS_THRESHOLD_SQ: f32 = 0.25; // 0.5 像素的平方
                const VEL_THRESHOLD_SQ: f32 = 0.01; // 0.1 像素/帧的平方

                let converged = dist_sq < POS_THRESHOLD_SQ && speed_sq < VEL_THRESHOLD_SQ;

                self.selection_box_anim.set(Some(SelectionBoxAnimState {
                    start_pos,
                    current_pos: current,
                    velocity,
                    snapped_tick,
                    snapped_key,
                    converged,
                }));
            }
            _ => {
                // 非选择状态，清除动画状态
                self.selection_box_anim.set(None);
            }
        }
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    /// 根据当前播放位置，计算每个 key 上被洋葱皮音符覆盖的颜色
    ///
    /// 直接从 `MidiDocument.track_notes()` 读取，数据已在 MIDI 导入时按 track 分组
    /// 并按 `start_tick` 升序排列。使用 `partition_point` 二分查找当前 tick 的活动音符，
    /// 零额外内存开销。
    ///
    /// 播放停止时（`playback_position == 0.0`）清空颜色立即返回。
    /// 当 `playback_key_colors_enabled == false` 时直接返回。
    pub fn update_playback_key_colors(&mut self) {
        puffin::profile_function!();
        if !self.playback_key_colors_enabled {
            return;
        }

        if (self.playback_position - 0.0).abs() < f32::EPSILON {
            if self.playback_key_colors != [0u8; 1024] {
                self.playback_key_colors = [0u8; 1024];
            }
            return;
        }

        let Some(doc) = self.editor_state.data.document.as_ref() else {
            return;
        };

        let tick = self.playback_position;
        let tick_u32 = tick as u32;
        let mut new_colors = [0u8; 1024];

        // 遍历所有音轨，直接从 MidiDocument 读
        for track_idx in 0..doc.track_count() {
            let notes = doc.track_notes(track_idx);
            if notes.is_empty() {
                continue;
            }

            let color = onion_track_color(track_idx);

            // 二分查找：找到所有 start_tick <= current_tick 的音符
            // notes 每轨内按 start_tick 升序排列，partition_point 返回第一个 > tick 的索引
            let end = notes.partition_point(|n| n.start_tick <= tick_u32);
            if end == 0 {
                continue;
            }

            // 遍历候选音符，过滤出 end_tick > tick 的活动音符
            for n in &notes[..end] {
                if n.end_tick() > tick_u32 {
                    let offset = (n.key as usize) * 4;
                    new_colors[offset..offset + 4].copy_from_slice(&color);
                }
            }
        }

        // 直接更新颜色，不清空 keyboard_cache！
        // overlay 层会独立绘制这些颜色
        self.playback_key_colors = new_colors;
    }
}
