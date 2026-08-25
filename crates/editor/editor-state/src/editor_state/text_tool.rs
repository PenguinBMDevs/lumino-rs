//! 文字工具状态（文本框 + 输入文字 + 采样模式）
//!
//! 文字工具在钢琴卷帘上拉出一个文本框（Y 向精度 = key 范围，X 向 = 音符精度范围），
//! 用户输入文字后，按两种采样模式之一把字形占位转换为音符：
//! - `Normal`：与曲线工具一致，逐格点生成音符，长度 = 音符精度；
//! - `KeyRangeMerged`：每个 key 行内连续的墨水采样点合并为一个音符
//!   （颜料桶填充式实心区，任意空隙断开，不合并本应分开的笔画）。

/// 文字工具采样模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextToolMode {
    /// 正常采样：逐格点生成音符，长度 = 音符精度（与曲线工具一致）
    #[default]
    Normal,
    /// key 范围单文字单个音符采样：每个 key 行内连续墨水合并为一个音符
    KeyRangeMerged,
}

impl TextToolMode {
    /// 是否「按 key 行合并连续采样点」模式
    pub fn is_merged(&self) -> bool {
        matches!(self, TextToolMode::KeyRangeMerged)
    }
}

/// 文字工具状态
#[derive(Debug, Clone, Default)]
pub struct TextToolState {
    /// 框起点 tick（绘制时记录的精确值，确认时吸附音符精度）
    pub start_tick: f32,
    /// 框当前/终点 tick（绘制中实时更新）
    pub end_tick: f32,
    /// 框起点 key（绘制时记录的精确值，确认时吸附 key 线）
    pub start_key: u16,
    /// 框当前/终点 key
    pub end_key: u16,
    /// 是否已拉出框（进入编辑/预览态）
    pub active: bool,
    /// 是否正在文字输入（画布上显示 TextInput 覆盖层）
    pub editing: bool,
    /// 输入的文字
    pub text: String,
    /// 采样模式
    pub mode: TextToolMode,
    /// 字体家族名（默认跟随系统，需支持中文等多语言）
    pub font_family: String,
}

impl TextToolState {
    /// 创建带默认字体（微软雅黑，支持中文）的状态
    pub fn new() -> Self {
        Self {
            font_family: String::from("Microsoft YaHei"),
            ..Default::default()
        }
    }

    /// 清空全部状态（切换工具 / 取消时调用）
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// 从拖拽矩形设置框（精确坐标，确认时再吸附）
    pub fn set_drag(&mut self, start_tick: f32, end_tick: f32, start_key: u16, end_key: u16) {
        self.start_tick = start_tick;
        self.end_tick = end_tick;
        self.start_key = start_key;
        self.end_key = end_key;
    }

    /// 确认拉框后进入编辑态：吸附到网格并清空文字
    pub fn begin_editing(&mut self, snap: f32) {
        let (tick_lo, tick_hi) = self.normalized_ticks();
        self.start_tick = (tick_lo / snap).round() * snap;
        self.end_tick = (tick_hi / snap).round() * snap;
        if self.end_tick <= self.start_tick {
            self.end_tick = self.start_tick + snap;
        }
        let (key_lo, key_hi) = self.normalized_keys();
        self.start_key = key_lo;
        self.end_key = key_hi;
        self.active = true;
        self.editing = true;
        self.text.clear();
    }

    /// 规范化后的 (tick_min, tick_max)
    pub fn normalized_ticks(&self) -> (f32, f32) {
        (
            self.start_tick.min(self.end_tick),
            self.start_tick.max(self.end_tick),
        )
    }

    /// 规范化后的 (key_min, key_max)
    pub fn normalized_keys(&self) -> (u16, u16) {
        if self.start_key <= self.end_key {
            (self.start_key, self.end_key)
        } else {
            (self.end_key, self.start_key)
        }
    }

    /// 框宽对应的采样列数（X 向 = 音符精度）
    pub fn cols(&self, snap: f32) -> usize {
        let (lo, hi) = self.normalized_ticks();
        let snap = snap.max(1.0);
        ((hi - lo) / snap).round().max(1.0) as usize
    }

    /// 框高对应的 key 行数（Y 向 = key 范围）
    pub fn rows(&self) -> usize {
        let (lo, hi) = self.normalized_keys();
        (hi - lo + 1) as usize
    }

    /// 当前框是否含有可采样的内容
    pub fn has_content(&self) -> bool {
        self.active && !self.text.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mode_default_normal() {
        assert!(matches!(TextToolMode::default(), TextToolMode::Normal));
        assert!(!TextToolMode::Normal.is_merged());
        assert!(TextToolMode::KeyRangeMerged.is_merged());
    }

    #[test]
    fn test_state_default_font() {
        let s = TextToolState::new();
        assert_eq!(s.font_family, "Microsoft YaHei");
        assert!(!s.active);
    }

    #[test]
    fn test_normalized_and_dims() {
        let mut s = TextToolState::new();
        // 反向拖拽也应正确归一化
        s.set_drag(3840.0, 0.0, 64, 60);
        assert_eq!(s.normalized_ticks(), (0.0, 3840.0));
        assert_eq!(s.normalized_keys(), (60, 64));
        assert_eq!(s.rows(), 5);
        assert_eq!(s.cols(1920.0), 2);
    }

    #[test]
    fn test_begin_editing_snaps() {
        let mut s = TextToolState::new();
        // 精确坐标（非吸附），snap=1920
        s.set_drag(100.0, 4000.0, 60, 63);
        s.begin_editing(1920.0);
        // tick 吸附到 0 与 3840（>= 4000 方向 round）
        assert_eq!(s.start_tick, 0.0);
        assert_eq!(s.end_tick, 3840.0);
        assert!(s.active && s.editing);
        assert_eq!(s.rows(), 4);
    }
}
