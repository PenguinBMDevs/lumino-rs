//! 图片转 MIDI 放置模式状态
//!
//! 转换完成后进入放置模式：用户用 Y 向选择工具框选生成区域，
//! 区域框常驻显示（除非按下空白处或切换工具），可整体 X 向移动、
//! 左右边框拉伸（变更总显示长度），预览音符实时显示
//! （当前音轨实色 + 其他音轨洋葱皮）。

/// 放置模式阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageToMidiMode {
    /// 未激活（默认）
    #[default]
    Inactive,
    /// 等待/正在框选生成区域
    Selecting,
    /// 区域已确定，预览显示，可移动/拉伸
    Placing,
}

/// 放置模式下的指针交互阶段
///
/// 独立于 `EditState`：图片转 MIDI 放置模式不耦合音符选择/选择框机制，
/// 框选复用 `EditState::Selecting` 仅用于绘制框选矩形，命中语义由本枚举驱动。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum I2mInteraction {
    /// 无交互
    #[default]
    None,
    /// 正在用 Y 选择工具框选生成区域（X 向 snap）
    Selecting,
    /// 正在 X 向整体移动生成区域
    Dragging,
    /// 正在拉伸区域左边界
    StretchLeft,
    /// 正在拉伸区域右边界
    StretchRight,
}

/// 预览音符（图片转换结果中的单个音符）
#[derive(Debug, Clone, Copy)]
pub struct PreviewNote {
    /// 原始 tick（相对预览起点，0 起）
    pub tick: f32,
    /// 音符长度（tick）
    pub length: f32,
    /// MIDI key
    pub key: u8,
}

/// 转换预览数据（每个调色板颜色一条音轨）
#[derive(Debug, Clone, Default)]
pub struct ImageToMidiPreview {
    /// 每轨音符列表；`tracks[i]` 对应第 i 个调色板颜色
    pub tracks: Vec<Vec<PreviewNote>>,
    /// 预览原始宽度（tick），用于 X 向拉伸等比缩放
    pub orig_width: f32,
}

/// 生成区域框（X 向 tick 范围 + Y 向 key 范围）
#[derive(Debug, Clone, Copy)]
pub struct RegionRect {
    /// 区域框左边界（tick）
    pub tick_start: f32,
    /// 区域框右边界（tick）
    pub tick_end: f32,
    /// 区域框下边界（key）
    pub key_lo: u8,
    /// 区域框上边界（key）
    pub key_hi: u8,
}

impl RegionRect {
    /// 创建区域框（自动归一化 tick/key 顺序）
    pub fn new(tick_a: f32, tick_b: f32, key_a: u8, key_b: u8) -> Self {
        let (tick_start, tick_end) = if tick_a <= tick_b {
            (tick_a, tick_b)
        } else {
            (tick_b, tick_a)
        };
        let (key_lo, key_hi) = if key_a <= key_b {
            (key_a, key_b)
        } else {
            (key_b, key_a)
        };
        Self {
            tick_start,
            tick_end: tick_end.max(tick_start + 1.0),
            key_lo,
            key_hi,
        }
    }

    /// 区域框宽度（tick）
    pub fn width(&self) -> f32 {
        (self.tick_end - self.tick_start).max(1.0)
    }

    /// 区域框 X 向整体平移
    pub fn offset_x(&mut self, delta_tick: f32) {
        self.tick_start += delta_tick;
        self.tick_end += delta_tick;
    }

    /// 调整左边界（拉伸左侧，钳制不超过右边界）
    pub fn set_left(&mut self, tick: f32) {
        self.tick_start = tick.min(self.tick_end - 1.0);
    }

    /// 调整右边界（拉伸右侧，钳制不低于左边界）
    pub fn set_right(&mut self, tick: f32) {
        self.tick_end = tick.max(self.tick_start + 1.0);
    }

    /// Y 向整体平移（素材放置移动音高）
    ///
    /// 允许 key 范围越界（0-127 之外），由 `note_screen_key` 渲染时 clamp，
    /// 保证素材在键盘上下两端都能自由移动。
    pub fn offset_keys(&mut self, delta: i32) {
        let d = delta.clamp(-128, 127) as i8;
        self.key_lo = self.key_lo.wrapping_add_signed(d);
        self.key_hi = self.key_hi.wrapping_add_signed(d);
    }
}

/// 图片转 MIDI 放置状态
#[derive(Debug, Clone, Default)]
pub struct ImageToMidiState {
    /// 放置模式阶段
    pub mode: ImageToMidiMode,
    /// 转换预览数据（后台线程转换完成后填充）
    pub preview: Option<ImageToMidiPreview>,
    /// 生成区域框
    pub region: Option<RegionRect>,
    /// 是否正在后台转换中
    pub converting: bool,
    /// 放置模式指针交互阶段
    pub interaction: I2mInteraction,
    /// 框选/移动/拉伸的起点 tick（X 向操作基准）
    pub drag_start_tick: f32,
    /// 拖拽起点 key（Y 向操作基准；素材放置整体上下移动用）
    pub drag_start_key: f32,
    /// 素材拖出时的跟随区域（素材预览跟随鼠标移动，松手前生效；
    /// 与 `region` 互斥——`region` 确认后 `drag_follow` 清空）
    pub drag_follow: Option<RegionRect>,
    /// 是否允许 Y 向整体移动（素材放置 = true；i2m 区域框保持原语义 = false）
    pub allow_y_drag: bool,
    /// 预览渲染代际：区域确认/移动/拉伸/取消时递增，
    /// 渲染线程据此判断是否需要重建预览音符实例
    pub preview_generation: u64,
}

impl ImageToMidiState {
    /// 放置模式是否激活（Inactive 之外均视为激活）
    pub fn is_active(&self) -> bool {
        self.mode != ImageToMidiMode::Inactive
    }

    /// 当前生效的区域（region 优先；素材拖出跟随阶段用 drag_follow）
    pub fn active_region(&self) -> Option<RegionRect> {
        self.region.or(self.drag_follow)
    }

    /// 标记开始后台转换
    pub fn begin_converting(&mut self) {
        self.preview = None;
        self.reset_placement_fields();
        self.converting = true;
        self.mode = ImageToMidiMode::Selecting;
    }

    /// 设置转换结果（进入等待框选阶段）
    pub fn set_preview(&mut self, preview: ImageToMidiPreview) {
        self.preview = Some(preview);
        self.reset_placement_fields();
        self.mode = ImageToMidiMode::Selecting;
    }

    /// 重置放置过程字段（预览保留）
    fn reset_placement_fields(&mut self) {
        self.converting = false;
        self.region = None;
        self.interaction = I2mInteraction::None;
        self.drag_start_tick = 0.0;
        self.drag_start_key = 0.0;
        self.drag_follow = None;
        self.allow_y_drag = false;
        self.preview_generation = 0;
    }

    /// 开始框选生成区域（X 向 snap，起点 tick 由调用方提供）
    pub fn begin_selecting(&mut self, start_tick: f32) {
        self.mode = ImageToMidiMode::Selecting;
        self.interaction = I2mInteraction::Selecting;
        self.drag_start_tick = start_tick;
    }

    /// 确认生成区域（进入放置阶段，显示预览）
    pub fn confirm_region(&mut self, region: RegionRect) {
        self.region = Some(region);
        self.mode = ImageToMidiMode::Placing;
        self.interaction = I2mInteraction::None;
    }

    /// 清空当前区域（回到等待框选阶段，预览隐藏）
    pub fn clear_region(&mut self) {
        self.region = None;
        self.drag_follow = None;
        self.interaction = I2mInteraction::None;
        if self.preview.is_some() {
            self.mode = ImageToMidiMode::Selecting;
        } else {
            self.mode = ImageToMidiMode::Inactive;
        }
    }

    /// 标记预览渲染变化（区域确认/移动/拉伸/取消时由交互层调用）
    pub fn bump_preview_generation(&mut self) {
        self.preview_generation = self.preview_generation.wrapping_add(1);
    }

    /// 取消整个放置流程并还原（× 按钮 / 切换工具）
    pub fn cancel(&mut self) {
        *self = Self::default();
    }

    /// X 向缩放比例（区域框宽度 / 预览原始宽度）
    pub fn scale_x(&self) -> f32 {
        let Some(region) = self.active_region() else {
            return 1.0;
        };
        let Some(preview) = &self.preview else {
            return 1.0;
        };
        if preview.orig_width <= 0.0 {
            return 1.0;
        }
        region.width() / preview.orig_width
    }

    /// 预览音符在区域内的显示起点 tick（X 向等比映射）
    pub fn note_screen_tick(&self, orig_tick: f32) -> f32 {
        let Some(region) = self.active_region() else {
            return orig_tick;
        };
        let Some(preview) = &self.preview else {
            return orig_tick;
        };
        if preview.orig_width <= 0.0 {
            return region.tick_start;
        }
        region.tick_start + (orig_tick / preview.orig_width) * region.width()
    }

    /// 预览音符在区域内的显示长度（X 向等比缩放）
    pub fn note_screen_length(&self, orig_length: f32) -> f32 {
        (orig_length * self.scale_x()).max(1.0)
    }

    /// 收集某一轨道的预览音符（应用区域映射），返回 (tick, key, length) 列表
    pub fn track_screen_notes(&self, track_idx: usize) -> Vec<(f32, u8, f32)> {
        let Some(preview) = &self.preview else {
            return Vec::new();
        };
        let Some(track) = preview.tracks.get(track_idx) else {
            return Vec::new();
        };
        track
            .iter()
            .map(|n| {
                (
                    self.note_screen_tick(n.tick),
                    self.note_screen_key(n.key),
                    self.note_screen_length(n.length),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_inactive() {
        let state = ImageToMidiState::default();
        assert_eq!(state.mode, ImageToMidiMode::Inactive);
        assert!(!state.is_active());
    }

    #[test]
    fn test_set_preview_enters_selecting() {
        let mut state = ImageToMidiState::default();
        state.begin_converting();
        assert!(state.converting);
        let preview = ImageToMidiPreview {
            tracks: vec![vec![PreviewNote {
                tick: 0.0,
                length: 10.0,
                key: 60,
            }]],
            orig_width: 100.0,
        };
        state.set_preview(preview);
        assert!(!state.converting);
        assert_eq!(state.mode, ImageToMidiMode::Selecting);
    }

    #[test]
    fn test_region_mapping_scales_notes() {
        let mut state = ImageToMidiState {
            preview: Some(ImageToMidiPreview {
                tracks: vec![vec![PreviewNote {
                    tick: 50.0,
                    length: 10.0,
                    key: 60,
                }]],
                orig_width: 100.0,
            }),
            ..Default::default()
        };
        state.confirm_region(RegionRect::new(100.0, 300.0, 0, 127));
        // orig 50 / 100 * 200 = 100 → tick_start + 100 = 200
        assert_eq!(state.note_screen_tick(50.0), 200.0);
        // length 10 * 2 = 20
        assert_eq!(state.note_screen_length(10.0), 20.0);
        assert_eq!(state.mode, ImageToMidiMode::Placing);
    }

    #[test]
    fn test_region_ops() {
        let mut region = RegionRect::new(200.0, 100.0, 10, 5);
        assert_eq!(region.tick_start, 100.0);
        assert_eq!(region.tick_end, 200.0);
        assert_eq!(region.key_lo, 5);
        assert_eq!(region.key_hi, 10);
        region.offset_x(50.0);
        assert_eq!(region.tick_start, 150.0);
        assert_eq!(region.tick_end, 250.0);
        region.set_right(100.0);
        assert_eq!(region.tick_end, 151.0);
        region.set_left(300.0);
        assert_eq!(region.tick_start, 150.0);
    }

    #[test]
    fn test_cancel_resets() {
        let mut state = ImageToMidiState::default();
        state.begin_converting();
        state.set_preview(ImageToMidiPreview {
            tracks: Vec::new(),
            orig_width: 10.0,
        });
        state.confirm_region(RegionRect::new(0.0, 10.0, 0, 10));
        assert!(state.is_active());
        state.cancel();
        assert_eq!(state.mode, ImageToMidiMode::Inactive);
        assert!(state.preview.is_none());
        assert!(state.region.is_none());
    }
}
