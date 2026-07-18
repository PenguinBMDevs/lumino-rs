//! Editor 核心方法
//!
//! 包含：构造函数、内存分析、远端光标、音频动作、撤销重做、框选动画、播放键色

use crate::note::Note;
use crate::velocity::VelocityPanel;
use crate::{Editor, EditorMemory, SpatialIndexState, grid, onion_track_color};
use iced_core::Point;
use iced_widget::canvas;
use lumino_ui_core::message::AudioAction;
use std::cell::Cell;
use std::sync::Arc;

/// 播放键色增量扫描状态
///
/// 用于避免 `update_playback_key_colors` 每帧 O(N) 全量扫描，
/// 其中 N = `start_tick <= 当前 tick` 的音符数（随播放时间线性增长）。
///
/// 通过缓存上次扫描位置和当前活跃音符集合，把每帧扫描量降到
/// O(新进入活跃的音符) + O(活跃音符 retain)。
///
/// 触发全量重建的条件：
/// - 首次调用（`doc_addr == None`）
/// - MIDI 文档切换（`doc_addr` 变化）
/// - tick 回退（循环播放、用户拖动进度条）
/// - tick 大幅前跳（用户 seek）
#[derive(Default)]
pub(crate) struct PlaybackScanState {
    /// 上次扫描到的 tick（用于判断 tick 方向）
    pub last_tick: f32,
    /// 每条音轨上次扫描到的索引（partition_point 结果缓存）
    pub scan_idx: Vec<usize>,
    /// 当前活跃音符缓存：(end_tick, key_offset_bytes, color)
    /// 每帧 retain 清理已结束音符，涂色时直接遍历此 Vec
    pub active_notes: Vec<(u32, usize, [u8; 4])>,
    /// 上次扫描时的 `Arc<MidiDocument>` 地址（用于检测文档切换）
    pub doc_addr: Option<usize>,
}

/// 判定 seek 阈值（单位：tick）
///
/// 超过此阈值视为用户 seek，需要全量重建。
/// 保守取 5 秒等价 tick（480 PPQ @ 120BPM ≈ 960 tick/秒 → 4800 tick）
const SEEK_THRESHOLD_TICKS: f32 = 5000.0;

impl Editor {
    /// 创建新的编辑器实例
    pub fn new() -> Self {
        // 使用 UI 内存标签包裹编辑器初始化，便于内存监控归因
        lumino_memtrace::with_tag(lumino_memtrace::AllocTag::Ui, || {
            Self {
                editor_state: crate::editor_state::EditorState::new(),
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
                context_menu: crate::context_menu::PianoRollContextMenuState::default(),
                playback_scan_state: PlaybackScanState::default(),
            }
        })
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
    pub fn update_selection_box_animation(&self, mouse_pos: Option<Point>) {
        use crate::EditState;
        use crate::SelectionBoxAnimState;
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
    /// 并按 `start_tick` 升序排列。使用 `partition_point` 二分查找当前 tick 的活动音符。
    ///
    /// 播放停止时（`playback_position == 0.0`）清空颜色立即返回。
    /// 当 `playback_key_colors_enabled == false` 时直接返回。
    ///
    /// # 性能策略（增量扫描）
    ///
    /// 朴素实现遍历 `[0, end)` 区间所有音符，复杂度 O(end)，其中 `end` 随播放时间
    /// 线性增长——百万级音符的 MIDI 播放 6 分钟后，每帧扫描量可达千万级。
    ///
    /// 本方法维护 [`PlaybackScanState`] 缓存上次扫描位置和当前活跃音符集合：
    /// - 正常播放：增量扫描新进入的音符 + retain 清理已结束音符，每帧 O(活跃音符数)
    /// - seek / 循环回绕 / 文档切换：触发全量重建
    pub fn update_playback_key_colors(&mut self) {
        puffin::profile_function!();
        if !self.playback_key_colors_enabled {
            return;
        }

        if (self.playback_position - 0.0).abs() < f32::EPSILON {
            if self.playback_key_colors != [0u8; 1024] {
                self.playback_key_colors = [0u8; 1024];
            }
            // 停止时重置扫描状态，下次播放从头开始
            self.playback_scan_state = PlaybackScanState::default();
            return;
        }

        let Some(doc) = self.editor_state.data.document.as_ref() else {
            return;
        };

        let tick = self.playback_position;
        let tick_u32 = tick as u32;
        let track_count = doc.track_count();

        // 检测 MIDI 文档切换：缓存地址变化即视为新文档
        let current_doc_addr = Arc::as_ptr(doc) as *const () as usize;
        let doc_changed = self.playback_scan_state.doc_addr != Some(current_doc_addr);

        // 检测 tick 跳跃：回退或大幅前跳都触发全量重建
        let last_tick = self.playback_scan_state.last_tick;
        let need_full_rebuild =
            doc_changed || tick < last_tick || (tick - last_tick) > SEEK_THRESHOLD_TICKS;

        if need_full_rebuild {
            // 全量重建：scan_idx 清零，active_notes 清空，从 0 开始扫描到 end
            self.playback_scan_state = PlaybackScanState {
                last_tick: tick,
                scan_idx: vec![0; track_count],
                active_notes: Vec::new(),
                doc_addr: Some(current_doc_addr),
            };

            for track_idx in 0..track_count {
                let notes = doc.track_notes(track_idx);
                if notes.is_empty() {
                    continue;
                }
                let color = onion_track_color(track_idx);
                let end = notes.partition_point(|n| n.start_tick <= tick_u32);
                self.playback_scan_state.scan_idx[track_idx] = end;
                for n in &notes[..end] {
                    if n.end_tick() > tick_u32 {
                        let offset = (n.key as usize) * 4;
                        self.playback_scan_state
                            .active_notes
                            .push((n.end_tick(), offset, color));
                    }
                }
            }
        } else {
            // 增量扫描：从上次位置继续，把新进入活跃的音符 push 进 active_notes
            // 注意：scan_idx 长度可能 < track_count（首次扫描前未初始化），用 max 兜底
            if self.playback_scan_state.scan_idx.len() < track_count {
                self.playback_scan_state.scan_idx.resize(track_count, 0);
            }

            for track_idx in 0..track_count {
                let notes = doc.track_notes(track_idx);
                if notes.is_empty() {
                    continue;
                }
                let color = onion_track_color(track_idx);
                let start = self.playback_scan_state.scan_idx[track_idx];
                let end = notes.partition_point(|n| n.start_tick <= tick_u32);
                self.playback_scan_state.scan_idx[track_idx] = end;
                // 仅扫描 [start, end) 区间——新进入活跃的音符
                for n in &notes[start..end] {
                    if n.end_tick() > tick_u32 {
                        let offset = (n.key as usize) * 4;
                        self.playback_scan_state
                            .active_notes
                            .push((n.end_tick(), offset, color));
                    }
                }
            }

            // 清理已结束的音符（活跃音符数通常 < 几百，retain O(几百)）
            self.playback_scan_state
                .active_notes
                .retain(|(end_tick, _, _)| *end_tick > tick_u32);
        }

        self.playback_scan_state.last_tick = tick;

        // 用活跃音符集合涂色——遍历量 O(活跃音符数)，与已播放音符总数无关
        let mut new_colors = [0u8; 1024];
        for (_, offset, color) in &self.playback_scan_state.active_notes {
            let offset = *offset;
            new_colors[offset..offset + 4].copy_from_slice(color);
        }
        self.playback_key_colors = new_colors;
    }
}
