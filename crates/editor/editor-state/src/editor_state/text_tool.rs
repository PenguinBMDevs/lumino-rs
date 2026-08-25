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
    pub font_family: &'static str,
    /// 拖拽移动文本框时记录的抓取点（鼠标 tick / key 浮点），None 表示非移动中
    pub drag_grab: Option<(f32, f32)>,
    /// 拖拽移动开始时的原始框边界（start_tick, end_tick, start_key, end_key）
    pub drag_origin: Option<(f32, f32, u16, u16)>,
}

impl TextToolState {
    /// 创建带默认字体（微软雅黑，支持中文）的状态
    pub fn new() -> Self {
        Self {
            font_family: "Microsoft YaHei",
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

    /// 是否正在拖拽移动文本框
    pub fn is_dragging(&self) -> bool {
        self.drag_grab.is_some()
    }

    /// 开始拖拽移动：记录抓取点与当前框原始边界
    pub fn begin_move(&mut self, grab_tick: f32, grab_key: f32) {
        self.drag_origin = Some((self.start_tick, self.end_tick, self.start_key, self.end_key));
        self.drag_grab = Some((grab_tick, grab_key));
    }

    /// 结束拖拽移动（释放时调用，清除临时状态）
    pub fn end_move(&mut self) {
        self.drag_grab = None;
        self.drag_origin = None;
    }

    /// 按当前鼠标位置平移文本框：X 向按音符精度、Y 向按 key 行吸附，
    /// 框尺寸保持不变（整体平移）。tick 下限 0，key 限制在 0..=255。
    pub fn move_to(&mut self, cur_tick: f32, cur_key: f32, snap: f32) {
        let Some((g_tick, g_key)) = self.drag_grab else {
            return;
        };
        let Some((o_st, o_et, o_sk, o_ek)) = self.drag_origin else {
            return;
        };
        let snap = snap.max(1.0);
        let d_tick = ((cur_tick - g_tick) / snap).round() * snap;
        let d_key = (cur_key - g_key).round() as i32;
        let width = o_et - o_st;
        let new_start = (o_st + d_tick).max(0.0);
        let new_end = new_start + width;
        let height = (o_ek as i32) - (o_sk as i32);
        let new_sk = ((o_sk as i32) + d_key).clamp(0, 255 - height) as u16;
        let new_ek = (new_sk as i32 + height) as u16;
        self.start_tick = new_start;
        self.end_tick = new_end;
        self.start_key = new_sk;
        self.end_key = new_ek;
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

    #[test]
    fn test_move_box_translates_and_snaps() {
        let mut s = TextToolState::new();
        // 既有框：start(480,60) → end(960,64)，snap=480
        s.set_drag(480.0, 960.0, 60, 64);
        s.active = true;
        // 抓取点 (grab_tick=600, grab_key=62)
        s.begin_move(600.0, 62.0);
        // 拖到鼠标 (1100, 62)：X 位移 500 → 吸附到 480（一个精度单元）
        s.move_to(1100.0, 62.0, 480.0);
        assert_eq!(s.start_tick, 960.0, "整体右移一个精度单元");
        assert_eq!(s.end_tick, 1440.0, "宽度保持不变");
        assert_eq!(s.start_key, 60);
        assert_eq!(s.end_key, 64, "Y 未动时 key 行不变");

        // 再下移 2 个 key 行（grab_key 不变，cur_key=64）
        s.move_to(1100.0, 64.0, 480.0);
        assert_eq!(s.start_key, 62);
        assert_eq!(s.end_key, 66, "高度保持不变，整体下移 2 行");
        assert!(s.is_dragging());
        s.end_move();
        assert!(!s.is_dragging(), "释放后清除拖拽状态");
    }

    #[test]
    fn test_move_box_clamps_bounds() {
        let mut s = TextToolState::new();
        s.set_drag(100.0, 580.0, 250, 254); // 靠近底部
        s.active = true;
        s.begin_move(300.0, 252.0);
        // 试图大幅下移，应被限制在 key<=255 且保持高度（4 行）
        s.move_to(300.0, 300.0, 480.0);
        assert_eq!(s.start_key, 251, "下移被钳制在 255-高度(4)=251");
        assert_eq!(s.end_key, 255);
        // 尝试左移越过 0
        s.begin_move(100.0, 252.0);
        s.move_to(-5000.0, 252.0, 480.0);
        assert!(s.start_tick >= 0.0, "左移不能越过 tick=0");
    }
}
