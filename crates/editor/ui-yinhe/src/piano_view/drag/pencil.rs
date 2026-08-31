//! 铅笔工具 — 对应 `yinhe piano_view/pencil.rs:899`
//!
//! 提供 `PencilDrag` 状态机（Create / Move / ResizeLeft / ResizeRight）
//! 的 iced 侧桩。完整提交逻辑（新建音符 `NoteEvent` / `PencilNoteDrag` 移动）
//! 保持与 yinhe 一致，仅将 `egui::Ui::data().get_persisted` 替换为
//! `PianoDragState` / `PencilState` 的 Program State。

use iced_core::Point;
use lumino_core::ViewState;

/// 铅笔工具拖动模式（对齐 yinhe `PencilDrag`，tick 用 f32 复用 `ViewState` 精度）
#[derive(Debug, Clone)]
pub enum PencilDrag {
    /// 新建音符：起始 (tick, key)
    Create { tick: f32, key: u8 },
    /// 移动已有音符：track 索引 + 起始/结束 + 按下 snap 位置 + last_dk
    Move {
        track: u16,
        start_tick: u32,
        key: u8,
        end_tick: u32,
        press_tick: f32,
        last_dk: i32,
    },
    /// 右边缘拉伸
    ResizeRight {
        track: u16,
        start: u32,
        end: u32,
        key: u8,
    },
    /// 左边缘拉伸
    ResizeLeft {
        track: u16,
        start: u32,
        end: u32,
        key: u8,
    },
}

/// 命中音符信息（用于 pencil hit test）
#[derive(Debug, Clone)]
pub struct HitNote {
    pub track: u16,
    pub start_tick: u32,
    pub end_tick: u32,
    pub key: u8,
    /// 命中模式（Move / ResizeLeft / ResizeRight）
    pub mode: PencilHitMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PencilHitMode {
    Move,
    ResizeLeft,
    ResizeRight,
}

/// 铅笔工具按下的有效性判定（对齐 yinhe `valid_pencil_track`）
///
/// `write_track` 为布局计算的写入目标轨（主音轨或回退轨）；
///
/// - `None` 时不允许任何写入（未选中音轨一律不操作）
/// - `Some(conductor)` 时禁止写入 Conductor
/// - 不可见轨禁止写入
#[must_use]
pub fn valid_pencil_track(
    write_track: Option<u16>,
    track_visible: &[bool],
    conductor_idx: Option<u16>,
) -> Option<u16> {
    let t = write_track?;
    if Some(t) == conductor_idx {
        return None;
    }
    if !track_visible.get(t as usize).copied().unwrap_or(false) {
        return None;
    }
    Some(t)
}

/// 铅笔工具状态（Program State 侧持久化，对齐 `ui.id().with("pencil_drag")`）
#[derive(Debug, Default, Clone)]
pub struct PencilState {
    /// 当前铅笔拖动（跨帧持久化）
    pub drag: Option<PencilDrag>,
    /// ghost 预览（start, end, key, track）— 颜色由 shader 存储缓冲取
    pub ghost_notes: Vec<(u32, u32, u8, u16)>,
    /// 被拖动中需隐藏的原音符（track, start, key）
    pub hidden_notes: Vec<(u16, u32, u8)>,
}

impl PencilState {
    /// 清空拖动与预览（松手/取消时调用）
    pub fn clear(&mut self) {
        self.drag = None;
        self.ghost_notes.clear();
        self.hidden_notes.clear();
    }

    /// 命中测试：判断本地坐标是否在已有音符上（仅 track 维度过滤，简化桩）
    #[must_use]
    pub fn hit_test(
        &self,
        view: &ViewState,
        local_pos: Point,
        music_rect: iced_core::Rectangle,
        notes: &[(u16, u32, u32, u8)],
        edge_px: f32,
    ) -> Option<HitNote> {
        if !music_rect.contains(local_pos) {
            return None;
        }
        let rel_x = local_pos.x - music_rect.x;
        let rel_y = local_pos.y - music_rect.y;
        let key = view.y_to_key(rel_y) as u8;
        for (track, s, e, k) in notes {
            if *k != key {
                continue;
            }
            let a = view.tick_to_x(*s as f32) - music_rect.x;
            let b = view.tick_to_x(*e as f32) - music_rect.x;
            if rel_x < a.min(b) || rel_x > a.max(b) {
                continue;
            }
            let cross = view.key_to_y(*k as u16) - music_rect.y;
            if rel_y < cross || rel_y > cross + view.zoom_y {
                continue;
            }
            let dist_l = (rel_x - a).abs();
            let dist_r = (rel_x - b).abs();
            let mode = if dist_l < edge_px {
                PencilHitMode::ResizeLeft
            } else if dist_r < edge_px {
                PencilHitMode::ResizeRight
            } else {
                PencilHitMode::Move
            };
            return Some(HitNote {
                track: *track,
                start_tick: *s,
                end_tick: *e,
                key: *k,
                mode,
            });
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    /// 开始拖动（按下时调用，写入 `drag`）
    pub fn press(
        &mut self,
        view: &ViewState,
        local_pos: Point,
        music_rect: iced_core::Rectangle,
        notes: &[(u16, u32, u32, u8)],
        write_track: Option<u16>,
        track_visible: &[bool],
        conductor_idx: Option<u16>,
        snap_tick: impl Fn(f32) -> f32,
    ) {
        let Some(_track) = valid_pencil_track(write_track, track_visible, conductor_idx) else {
            return;
        };
        if let Some(hit) = self.hit_test(view, local_pos, music_rect, notes, 6.0) {
            self.drag = Some(match hit.mode {
                PencilHitMode::ResizeLeft => PencilDrag::ResizeLeft {
                    track: hit.track,
                    start: hit.start_tick,
                    end: hit.end_tick,
                    key: hit.key,
                },
                PencilHitMode::ResizeRight => PencilDrag::ResizeRight {
                    track: hit.track,
                    start: hit.start_tick,
                    end: hit.end_tick,
                    key: hit.key,
                },
                PencilHitMode::Move => {
                    let raw = view.x_to_tick(local_pos.x);
                    PencilDrag::Move {
                        track: hit.track,
                        start_tick: hit.start_tick,
                        key: hit.key,
                        end_tick: hit.end_tick,
                        press_tick: snap_tick(raw),
                        last_dk: 0,
                    }
                }
            });
            return;
        }
        // 非命中音符：创建新音符
        if music_rect.contains(local_pos) {
            let raw = view.x_to_tick(local_pos.x);
            let tick = snap_tick(raw).max(0.0);
            let key = view.y_to_key(local_pos.y - music_rect.y + view.ruler_height) as u8;
            self.drag = Some(PencilDrag::Create { tick, key });
        }
    }

    /// 拖动中计算 ghost / hidden（移动时调用，松手前预览）
    pub fn drag_update(
        &mut self,
        view: &ViewState,
        local_pos: Point,
        music_rect: iced_core::Rectangle,
        snap_tick: impl Fn(f32) -> f32,
        default_gate: u32,
    ) {
        let Some(drag) = self.drag.clone() else {
            return;
        };
        self.ghost_notes.clear();
        self.hidden_notes.clear();
        let clamped = Point::new(
            local_pos
                .x
                .clamp(music_rect.x, music_rect.x + music_rect.width),
            local_pos
                .y
                .clamp(music_rect.y, music_rect.y + music_rect.height),
        );
        let tick = snap_tick(view.x_to_tick(clamped.x)).max(0.0);
        let key = view.y_to_key(clamped.y - music_rect.y + view.ruler_height) as u8;
        match drag {
            PencilDrag::Create { tick: s, key: sk } => {
                let cur_end = tick.max(s + default_gate as f32);
                // 一量化内 key 跟随鼠标（与 yinhe 一致）
                let eff_key = if cur_end - s <= default_gate as f32 {
                    key
                } else {
                    sk
                };
                self.ghost_notes
                    .push((s as u32, cur_end as u32, eff_key, 0));
            }
            PencilDrag::Move {
                track,
                start_tick,
                key: ok,
                end_tick,
                press_tick,
                ..
            } => {
                let dt = (tick as i64 - press_tick as i64) as i32;
                let dk = key as i32 - ok as i32;
                let ns = (start_tick as i64 + dt as i64).max(0) as u32;
                let len = end_tick - start_tick;
                self.ghost_notes
                    .push((ns, ns + len, (ok as i32 + dk) as u8, track));
                self.hidden_notes.push((track, start_tick, ok));
            }
            PencilDrag::ResizeRight {
                track,
                start,
                key: k,
                ..
            } => {
                let snapped = snap_tick(tick) as u32;
                let ne = snapped.max(start + 1);
                self.ghost_notes.push((start, ne, k, track));
                self.hidden_notes.push((track, start, k));
            }
            PencilDrag::ResizeLeft {
                track, end, key: k, ..
            } => {
                let snapped = snap_tick(tick) as u32;
                let ns = snapped.min(end - 1);
                self.ghost_notes.push((ns, end, k, track));
                self.hidden_notes.push((track, end.saturating_sub(1), k));
            }
        }
    }
}
