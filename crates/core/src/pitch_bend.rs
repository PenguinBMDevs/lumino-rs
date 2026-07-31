//! 弯音编辑器数据模型
//!
//! 独立于 `AutomationLane` 的紧凑弯音曲线存储，包含锚点、控制柄向量
//! 和绘制模式。退出编辑时展平为 `AutomationEvent` 序列写入轨道。

use crate::midi_types::PITCH_BEND_CENTER;
use serde::{Deserialize, Serialize};

/// 弯音范围常量：±2 semitones（硬编码）
pub const PITCH_BEND_RANGE_SEMITONES: i16 = 2;

/// 弯音值范围：-8192..+8191
pub const PITCH_BEND_MIN: i16 = -8192;
pub const PITCH_BEND_MAX: i16 = 8191;

/// 控制柄长度与曲线长度的比率（照搬 scratch-editor 的 HANDLE_RATIO）
pub const HANDLE_RATIO: f32 = 0.390_262_86;

/// 弯音绘制模式
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BendDrawMode {
    /// 曲线绘制（贝塞尔，默认）
    #[default]
    Curve,
    /// 直线绘制（折线）
    Line,
}

/// 弯音锚点（紧凑布局，含控制柄归一化偏移量）
///
/// 控制柄使用归一化参数化（与 `SegmentShape::Curve` 一致）：
/// - `handle_out_*`：出控制柄相对于"本锚点到下一锚点"线段的偏移
/// - `handle_in_*`：入控制柄相对于"前一锚点到本锚点"线段的偏移
/// - 归一化空间：X ∈ [0,1]（tick 进度），Y ∈ [-1,1]（弯音值偏移比例）
/// - 值为 (0, 0) 表示无控制柄（直线段）
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PitchBendAnchor {
    /// tick 位置（时间轴）
    pub tick: u32,
    /// 弯音值 (-8192..+8191, 0=中心)
    pub value: i16,
    /// 入控制柄 X 偏移（归一化 tick 进度）
    pub handle_in_x: f32,
    /// 入控制柄 Y 偏移（归一化弯音值比例）
    pub handle_in_y: f32,
    /// 出控制柄 X 偏移（归一化 tick 进度）
    pub handle_out_x: f32,
    /// 出控制柄 Y 偏移（归一化弯音值比例）
    pub handle_out_y: f32,
}

impl Default for PitchBendAnchor {
    fn default() -> Self {
        Self {
            tick: 0,
            value: 0,
            handle_in_x: 0.0,
            handle_in_y: 0.0,
            handle_out_x: 0.0,
            handle_out_y: 0.0,
        }
    }
}

impl PitchBendAnchor {
    /// 构造无控制柄的锚点（直线段）
    pub fn new(tick: u32, value: i16) -> Self {
        Self {
            tick,
            value,
            handle_in_x: 0.0,
            handle_in_y: 0.0,
            handle_out_x: 0.0,
            handle_out_y: 0.0,
        }
    }

    /// 是否有出控制柄（非零偏移）
    pub fn has_handle_out(&self) -> bool {
        self.handle_out_x.abs() > 1e-4 || self.handle_out_y.abs() > 1e-4
    }

    /// 是否有入控制柄（非零偏移）
    pub fn has_handle_in(&self) -> bool {
        self.handle_in_x.abs() > 1e-4 || self.handle_in_y.abs() > 1e-4
    }

    /// 两个控制柄是否共线（对称判定）
    pub fn is_colinear(&self) -> bool {
        let cross = self.handle_in_x * self.handle_out_y - self.handle_in_y * self.handle_out_x;
        let dot = self.handle_in_x * self.handle_out_x + self.handle_in_y * self.handle_out_y;
        cross.abs() < 1e-4 && dot < 0.0
    }

    /// 对称化：根据出控制柄计算入控制柄（方向反转、长度等比）
    pub fn symmetrize_in_from_out(&mut self) {
        self.handle_in_x = -self.handle_out_x;
        self.handle_in_y = -self.handle_out_y;
    }

    /// 对称化：根据入控制柄计算出控制柄
    pub fn symmetrize_out_from_in(&mut self) {
        self.handle_out_x = -self.handle_in_x;
        self.handle_out_y = -self.handle_in_y;
    }
}

/// 弯音曲线（一个轨道的完整弯音编辑数据）
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PitchBendCurve {
    /// 锚点列表（按 tick 升序排列）
    pub anchors: Vec<PitchBendAnchor>,
    /// 绘制模式
    pub mode: BendDrawMode,
    /// 基准音符的 MIDI key（用户选中音符的音高）
    pub base_key: u16,
    /// 轨道索引
    pub track: u16,
    /// MIDI 通道（按轨道设置，保留 u8）
    pub channel: u8,
    /// 当前选中的锚点索引（None=未选中）
    #[serde(skip)]
    pub selected_anchor: Option<usize>,
}

impl PitchBendCurve {
    /// 创建空的弯音曲线
    pub fn new(track: u16, channel: u8, base_key: u16) -> Self {
        Self {
            anchors: Vec::new(),
            mode: BendDrawMode::default(),
            base_key,
            track,
            channel,
            selected_anchor: None,
        }
    }

    /// 按 tick 二分查找锚点索引
    pub fn find_anchor_by_tick(&self, tick: u32) -> Option<usize> {
        self.anchors
            .partition_point(|a| a.tick < tick)
            .checked_sub(1)
            .filter(|&i| self.anchors.get(i).is_some_and(|a| a.tick == tick))
            .or_else(|| {
                let idx = self.anchors.partition_point(|a| a.tick < tick);
                if idx < self.anchors.len() && self.anchors[idx].tick == tick {
                    Some(idx)
                } else {
                    None
                }
            })
    }

    /// 插入锚点（保持 tick 升序）
    pub fn insert_anchor(&mut self, anchor: PitchBendAnchor) -> usize {
        let idx = self.anchors.partition_point(|a| a.tick < anchor.tick);
        // 同 tick 替换
        if idx < self.anchors.len() && self.anchors[idx].tick == anchor.tick {
            self.anchors[idx] = anchor;
            idx
        } else {
            self.anchors.insert(idx, anchor);
            idx
        }
    }

    /// 删除指定索引的锚点
    pub fn remove_anchor(&mut self, index: usize) -> Option<PitchBendAnchor> {
        if index < self.anchors.len() {
            Some(self.anchors.remove(index))
        } else {
            None
        }
    }

    /// 获取 tick 处的弯音值（采样）
    ///
    /// 曲线模式：三次贝塞尔插值
    /// 直线模式：线性插值
    /// 无锚点时返回 0（中心值）
    pub fn sample_value(&self, tick: u32) -> i16 {
        if self.anchors.is_empty() {
            return 0;
        }

        // 在第一个锚点之前
        if tick <= self.anchors[0].tick {
            return self.anchors[0].value;
        }

        // 在最后一个锚点之后（尾部延续）
        let last = self.anchors.len() - 1;
        if tick >= self.anchors[last].tick {
            return self.anchors[last].value;
        }

        // 二分查找所在区间
        let idx = self.anchors.partition_point(|a| a.tick < tick);
        let prev = &self.anchors[idx - 1];
        let next = &self.anchors[idx];

        let span = (next.tick - prev.tick) as f32;
        if span < 1.0 {
            return next.value;
        }
        let t = (tick - prev.tick) as f32 / span;

        match self.mode {
            BendDrawMode::Line => {
                // 线性插值
                let v = prev.value as f32 + (next.value - prev.value) as f32 * t;
                v.round()
                    .clamp(PITCH_BEND_MIN as f32, PITCH_BEND_MAX as f32) as i16
            }
            BendDrawMode::Curve => {
                // 三次贝塞尔插值（使用归一化控制柄偏移量）
                let p0y = prev.value as f32;
                let p3y = next.value as f32;
                let range = (PITCH_BEND_MAX as f32) - (PITCH_BEND_MIN as f32);

                // P1 = prev + handle_out * span (tick方向), handle_out_y * range (value方向)
                let p1y = p0y + prev.handle_out_y * range;
                let p2y = p3y + next.handle_in_y * range;

                // 从 x(u)=t 反解 u（Newton 迭代）
                let p1x = prev.handle_out_x; // 归一化
                let p2x = 1.0 + next.handle_in_x;
                let u = solve_bezier_u_for_x(t, p1x, p2x);

                let u1 = 1.0 - u;
                let v = 3.0 * u1 * u1 * u * p1y + 3.0 * u1 * u * u * p2y + u * u * u * p3y;

                v.round()
                    .clamp(PITCH_BEND_MIN as f32, PITCH_BEND_MAX as f32) as i16
            }
        }
    }

    /// 按 1 tick 粒度采样生成弯音事件序列（报告 5.1：曲线模式 1 tick 采样）
    ///
    /// - `_ticks_per_measure`：保留参数以兼容旧接口，实际固定 1 tick 步长
    /// - `start_tick` / `end_tick`：采样范围
    /// - 跳过相邻相同值（节流）
    pub fn sample_to_events(
        &self,
        _ticks_per_measure: u32,
        start_tick: u32,
        end_tick: u32,
    ) -> Vec<PitchBendSample> {
        if self.anchors.is_empty() {
            return Vec::new();
        }

        // 1 tick 粒度采样，精确还原弯音曲线（含突变点）
        let step = 1u32;

        let mut result = Vec::new();
        let mut last_value: Option<i16> = None;

        let mut tick = start_tick;
        while tick <= end_tick {
            let value = self.sample_value(tick);
            // 跳过相邻相同值
            if last_value != Some(value) {
                result.push(PitchBendSample {
                    tick,
                    value: (value + PITCH_BEND_CENTER) as u16, // 转为 0..16383
                });
                last_value = Some(value);
            }
            tick = tick.saturating_add(step);
        }

        // 确保最后一个锚点的值被写入
        if let Some(last_anchor) = self.anchors.last() {
            let final_value = last_anchor.value;
            if last_value != Some(final_value) {
                result.push(PitchBendSample {
                    tick: last_anchor.tick,
                    value: (final_value + PITCH_BEND_CENTER) as u16,
                });
            }
        }

        result
    }
}

/// 弯音采样点（用于写入 AutomationEvent）
#[derive(Clone, Copy, Debug)]
pub struct PitchBendSample {
    /// tick 位置
    pub tick: u32,
    /// 弯音原始值 (0..16383, 中心 8192)
    pub value: u16,
}

/// 解三次贝塞尔方程 B_x(u) = t 求 u（Newton 迭代）
#[inline]
fn solve_bezier_u_for_x(t: f32, p1x: f32, p2x: f32) -> f32 {
    let mut u = t.clamp(0.0, 1.0);
    for _ in 0..6 {
        let u1 = 1.0 - u;
        let f = 3.0 * u1 * u1 * u * p1x + 3.0 * u1 * u * u * p2x + u * u * u - t;
        let df = 3.0 * u1 * u1 * p1x + 6.0 * u1 * u * (p2x - p1x) + 3.0 * u * u * (1.0 - p2x);
        if df.abs() < 1e-6 {
            break;
        }
        u -= f / df;
        u = u.clamp(0.0, 1.0);
    }
    u
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anchor_default() {
        let a = PitchBendAnchor::default();
        assert_eq!(a.tick, 0);
        assert_eq!(a.value, 0);
        assert!(!a.has_handle_in());
        assert!(!a.has_handle_out());
    }

    #[test]
    fn test_anchor_new_no_handles() {
        let a = PitchBendAnchor::new(480, 1000);
        assert_eq!(a.tick, 480);
        assert_eq!(a.value, 1000);
        assert!(!a.has_handle_in());
        assert!(!a.has_handle_out());
    }

    #[test]
    fn test_colinear_detection() {
        let mut a = PitchBendAnchor::new(0, 0);
        a.handle_out_x = 0.3;
        a.handle_out_y = 0.1;
        a.symmetrize_in_from_out();
        assert!(a.is_colinear());

        // 打破共线
        a.handle_in_y = 0.5;
        assert!(!a.is_colinear());
    }

    #[test]
    fn test_symmetrize() {
        let mut a = PitchBendAnchor::new(0, 0);
        a.handle_out_x = 0.3;
        a.handle_out_y = 0.2;
        a.symmetrize_in_from_out();
        assert_eq!(a.handle_in_x, -0.3);
        assert_eq!(a.handle_in_y, -0.2);
    }

    #[test]
    fn test_curve_insert_and_find() {
        let mut curve = PitchBendCurve::new(0, 0, 60);
        curve.insert_anchor(PitchBendAnchor::new(0, 0));
        curve.insert_anchor(PitchBendAnchor::new(480, 1000));
        curve.insert_anchor(PitchBendAnchor::new(960, -500));

        assert_eq!(curve.anchors.len(), 3);
        assert!(curve.find_anchor_by_tick(480).is_some());
        assert!(curve.find_anchor_by_tick(100).is_none());
    }

    #[test]
    fn test_curve_insert_replaces_same_tick() {
        let mut curve = PitchBendCurve::new(0, 0, 60);
        curve.insert_anchor(PitchBendAnchor::new(480, 100));
        curve.insert_anchor(PitchBendAnchor::new(480, 200));
        assert_eq!(curve.anchors.len(), 1);
        assert_eq!(curve.anchors[0].value, 200);
    }

    #[test]
    fn test_sample_value_line_mode() {
        let mut curve = PitchBendCurve::new(0, 0, 60);
        curve.mode = BendDrawMode::Line;
        curve.insert_anchor(PitchBendAnchor::new(0, 0));
        curve.insert_anchor(PitchBendAnchor::new(480, 8191));

        // 中点应约为 4096
        let mid = curve.sample_value(240);
        assert!((mid - 4096).abs() < 2, "expected ~4096, got {mid}");
    }

    #[test]
    fn test_sample_value_before_first_anchor() {
        let mut curve = PitchBendCurve::new(0, 0, 60);
        curve.insert_anchor(PitchBendAnchor::new(480, 1000));
        assert_eq!(curve.sample_value(0), 1000);
    }

    #[test]
    fn test_sample_value_after_last_anchor() {
        let mut curve = PitchBendCurve::new(0, 0, 60);
        curve.insert_anchor(PitchBendAnchor::new(480, 1000));
        assert_eq!(curve.sample_value(960), 1000);
    }

    #[test]
    fn test_sample_to_events_skips_same_values() {
        let mut curve = PitchBendCurve::new(0, 0, 60);
        curve.mode = BendDrawMode::Line;
        curve.insert_anchor(PitchBendAnchor::new(0, 0));
        curve.insert_anchor(PitchBendAnchor::new(1920, 0)); // 两锚点值相同

        // 采样范围内所有值都是 0，应该只有第一个事件
        let events = curve.sample_to_events(480, 0, 1920);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].value, PITCH_BEND_CENTER as u16);
    }

    #[test]
    fn test_sample_to_events_changing_values() {
        let mut curve = PitchBendCurve::new(0, 0, 60);
        curve.mode = BendDrawMode::Line;
        curve.insert_anchor(PitchBendAnchor::new(0, 0));
        curve.insert_anchor(PitchBendAnchor::new(480, 8191));

        let events = curve.sample_to_events(480, 0, 480);
        // 至少有起始和结束事件
        assert!(events.len() >= 2);
        assert_eq!(events[0].value, PITCH_BEND_CENTER as u16);
        assert_eq!(
            events.last().unwrap().value,
            (8191 + PITCH_BEND_CENTER) as u16
        );
    }

    #[test]
    fn test_empty_curve_sample() {
        let curve = PitchBendCurve::new(0, 0, 60);
        assert_eq!(curve.sample_value(100), 0);
        assert!(curve.sample_to_events(480, 0, 480).is_empty());
    }
}
