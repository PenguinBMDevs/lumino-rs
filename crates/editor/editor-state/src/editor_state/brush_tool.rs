//! 画刷工具矢量笔画状态（支持多条笔画批量绘制）
//!
//! 画刷 = 在卷帘上按下拖动出一笔**矢量笔画**（逻辑坐标折线：tick 自由浮点、
//! key 已吸附整数），拖动期间**不写 document**，松手后笔画进入「待确认」集合；
//! 画布上以圆头圆尾的粗线条实时预览（渲染在 UI 层）。
//!
//! - **多条笔画可同时存在**（空白处按下开始新笔画，不清空已有），
//!   共享一组 √（批量确认生成音符）/ ×（批量丢弃）按钮；
//! - 按住笔画实心区可整体拖动（X 向自由、Y 向按单个 key 吸附）；
//! - **笔画编辑历史**独立于 document 历史（参照 `LineToolState.path_history`）：
//!   创建/拖动笔画均为一次撤销操作（Ctrl+Z / Ctrl+Y），√ 确认时才写入
//!   document 并产生一条历史记录。
//!
//! 覆盖（断墨修复）：相邻采样点之间由 [`cov::cover_cells`] 做线段栅格化，
//! 输出**经过的全部网格单元**——采样频率再低也不会丢格，预览与生成共用同一函数。

pub mod cov;

use cov::CoveredCell;

/// 单条笔画：逻辑坐标折线（tick 自由浮点，key 已吸附整数）
///
/// 只存几何：粗细度与每层音轨分配在**预览与 √ 确认时**按当时 `BrushConfig`
/// 解释（见 `BrushConfig::track_for_level`），因此下拉改粗细度后预览立即跟随。
/// `base_track` 例外——它是**落笔时刻**的当前音轨，固定记录：默认层分配
/// （`tracks[level] == None`）以它为基准，避免用户在"画完 → 切轨 → √"之间
/// 让音符落到与预览不一致的音轨。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BrushStroke {
    /// 折线点序列（逻辑坐标 (tick, key)；至少 1 个点）
    pub points: Vec<(f32, f32)>,
    /// 落笔时刻的当前音轨（默认层分配的基准轨；`BrushConfig.tracks` 显式指定优先）
    pub base_track: usize,
}

impl BrushStroke {
    /// 以落笔点与基准轨创建一笔
    pub fn new(start: (f32, f32), base_track: usize) -> Self {
        Self {
            points: vec![start],
            base_track,
        }
    }

    /// 追加采样点；与末点完全相同则忽略（返回是否追加成功）
    ///
    /// 注意：这里只做**完全重合**去重，不做抽稀——抽稀属于渲染层，
    /// 覆盖正确性不允许依赖采样密度。
    pub fn push_point(&mut self, point: (f32, f32)) -> bool {
        if self.points.last() == Some(&point) {
            return false;
        }
        self.points.push(point);
        true
    }

    /// 是否为空笔画（无点）
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// 是否为单点笔画（点一下不拖）
    pub fn is_single_point(&self) -> bool {
        self.points.len() == 1
    }

    /// 末点
    pub fn last(&self) -> Option<(f32, f32)> {
        self.points.last().copied()
    }

    /// 整体平移（布尔运算：X 向自由、Y 向按单个 key 吸附由调用方取整后传入）
    ///
    /// tick 下限钳制到 0（避免负 tick），key 钳制到 0..=255（音符 key 上限）。
    pub fn translate(&mut self, delta_tick: f32, delta_key: f32) {
        for p in &mut self.points {
            p.0 = (p.0 + delta_tick).max(0.0);
            p.1 = (p.1 + delta_key).clamp(0.0, 255.0);
        }
    }

    /// 逻辑包围盒 (min_tick, max_tick, min_key, max_key)
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let mut it = self.points.iter().copied();
        let first = it.next()?;
        let mut b = (first.0, first.0, first.1, first.1);
        for (t, k) in it {
            b.0 = b.0.min(t);
            b.1 = b.1.max(t);
            b.2 = b.2.min(k);
            b.3 = b.3.max(k);
        }
        Some(b)
    }

    /// 本笔画的覆盖格（去重前）；`snap` = 网格精度（= 音符长度）
    pub fn covered_cells(&self, snap: f32) -> Vec<CoveredCell> {
        cov::cover_cells(&self.points, snap)
    }
}

/// 画刷交互阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrushInteraction {
    /// 无交互（含「已松手、笔画待确认」）
    #[default]
    None,
    /// 正在落笔绘制指定笔画
    Drawing {
        /// 目标笔画索引
        stroke: usize,
    },
    /// 正在整体拖动指定笔画
    Dragging {
        /// 目标笔画索引
        stroke: usize,
    },
}

impl BrushInteraction {
    /// 是否处于「鼠标按下」的交互中（用于 `is_editing` 判定）
    pub fn is_active(self) -> bool {
        !matches!(self, Self::None)
    }
}

/// 画刷矢量笔画状态
#[derive(Debug, Clone, PartialEq)]
pub struct BrushToolState {
    /// 全部笔画（每条 = 折线点序列）
    pub strokes: Vec<BrushStroke>,
    /// 当前交互阶段
    pub interaction: BrushInteraction,
    /// 拖动基准：按下时的原始逻辑坐标 (tick, key)——整体平移的增量基准
    pub drag_start_raw: (f32, f32),
    /// 拖动基准：被拖笔画的原始点列（增量始终相对它计算，避免累积漂移）
    pub drag_orig: BrushStroke,
    /// 笔画编辑历史（快照 = 操作后状态；`path_history_index` 指向当前状态）
    ///
    /// 栈始终含初始状态（`[空]`，index 0）；每次操作完成后 push 新状态，
    /// 拖动结束合并为一次撤销（`update_top_path_history`）。
    pub path_history: Vec<Vec<BrushStroke>>,
    /// 当前笔画历史状态索引（指向 `path_history` 中的当前状态）
    pub path_history_index: usize,
}

impl Default for BrushToolState {
    fn default() -> Self {
        Self {
            strokes: Vec::new(),
            interaction: BrushInteraction::None,
            drag_start_raw: (0.0, 0.0),
            drag_orig: BrushStroke::default(),
            // 历史栈初始含空状态（撤销基准）
            path_history: vec![Vec::new()],
            path_history_index: 0,
        }
    }
}

impl BrushToolState {
    /// 是否存在待确认的笔画（松手后未 √ / ×）
    pub fn has_pending(&self) -> bool {
        !self.strokes.is_empty()
    }

    /// 是否正在鼠标按下交互中（落笔绘制 / 整体拖动）
    pub fn is_active(&self) -> bool {
        self.interaction.is_active()
    }

    /// 是否正在落笔绘制
    pub fn is_drawing(&self) -> bool {
        matches!(self.interaction, BrushInteraction::Drawing { .. })
    }

    /// 是否正在整体拖动
    pub fn is_dragging(&self) -> bool {
        matches!(self.interaction, BrushInteraction::Dragging { .. })
    }

    /// 开始一笔新笔画，返回新笔画索引
    ///
    /// `base_track` = 落笔时刻的当前音轨（默认层分配基准，见 [`BrushStroke`]）。
    pub fn begin_stroke(&mut self, start: (f32, f32), base_track: usize) -> usize {
        self.strokes.push(BrushStroke::new(start, base_track));
        let index = self.strokes.len() - 1;
        self.interaction = BrushInteraction::Drawing { stroke: index };
        index
    }

    /// 向当前正在绘制的笔画追加采样点；返回是否追加成功
    pub fn push_point(&mut self, point: (f32, f32)) -> bool {
        let BrushInteraction::Drawing { stroke } = self.interaction else {
            return false;
        };
        let Some(s) = self.strokes.get_mut(stroke) else {
            return false;
        };
        s.push_point(point)
    }

    /// 落笔绘制结束（不改变笔画集合，仅退出交互态）
    pub fn finish_stroke(&mut self) {
        if self.is_drawing() {
            self.interaction = BrushInteraction::None;
        }
    }

    /// 开始整体拖动指定笔画（记录拖动基准与原始点列）
    pub fn begin_drag(&mut self, stroke: usize, raw_pos: (f32, f32)) -> bool {
        let Some(orig) = self.strokes.get(stroke).cloned() else {
            return false;
        };
        self.drag_start_raw = raw_pos;
        self.drag_orig = orig;
        self.interaction = BrushInteraction::Dragging { stroke };
        true
    }

    /// 拖到当前位置：X 向自由（原始 tick 差），Y 向按**单个 key** 吸附（四舍五入）
    ///
    /// 增量始终相对 `drag_orig` 计算（而非逐帧累加），保证与落笔吸附一致且无漂移。
    pub fn drag_to(&mut self, raw_pos: (f32, f32)) {
        let BrushInteraction::Dragging { stroke } = self.interaction else {
            return;
        };
        if self.drag_orig.is_empty() {
            return;
        }
        let delta_tick = raw_pos.0 - self.drag_start_raw.0;
        let delta_key = (raw_pos.1 - self.drag_start_raw.1).round();
        let orig = self.drag_orig.clone();
        if let Some(s) = self.strokes.get_mut(stroke) {
            *s = orig;
            s.translate(delta_tick, delta_key);
        }
    }

    /// 整体拖动结束（不改变笔画集合，仅退出交互态）
    pub fn end_drag(&mut self) {
        if self.is_dragging() {
            self.interaction = BrushInteraction::None;
        }
    }

    /// 当前拖动是否**真的移动了**笔画（相对 `drag_orig`）
    ///
    /// 用于拖动结束时判定是否记一步撤销：原地按一下（或拖回原位）不应产生
    /// "空撤销步"——否则用户按 Ctrl+Z 会出现"按了没反应"的一步。
    pub fn drag_moved(&self) -> bool {
        let BrushInteraction::Dragging { stroke } = self.interaction else {
            return false;
        };
        match self.strokes.get(stroke) {
            Some(current) => *current != self.drag_orig,
            None => false,
        }
    }

    /// 丢弃全部待确认笔画与笔画历史（**仅显式 ×**；切工具不再调用它）
    pub fn clear_pending(&mut self) {
        self.strokes.clear();
        self.interaction = BrushInteraction::None;
        self.drag_orig = BrushStroke::default();
        self.path_history = vec![Vec::new()];
        self.path_history_index = 0;
    }

    /// 仅收敛**未完成的交互手势**（落笔中 / 整体拖动中），保留全部笔画与笔画历史
    ///
    /// 供切换工具时调用：笔画是用户的绘制产物，不因切换工具被丢弃
    /// （见 `EditorState::set_tool`）；清空只发生在显式 × / √。
    pub fn cancel_interaction(&mut self) {
        self.interaction = BrushInteraction::None;
        self.drag_orig = BrushStroke::default();
        self.drag_start_raw = (0.0, 0.0);
    }

    // ── 笔画编辑历史（撤销/重做） ─────────────────────────

    /// 当前全部笔画快照
    pub fn snapshot(&self) -> Vec<BrushStroke> {
        self.strokes.clone()
    }

    /// 记录当前状态（操作完成后调用）：截断重做分支后入栈
    pub fn push_path_history(&mut self) {
        self.path_history.truncate(self.path_history_index + 1);
        self.path_history.push(self.snapshot());
        self.path_history_index = self.path_history.len() - 1;
    }

    /// 更新栈顶为当前状态（合并连续操作——落笔拖动、整体拖动）
    pub fn update_top_path_history(&mut self) {
        let snap = self.snapshot();
        if let Some(top) = self.path_history.last_mut() {
            *top = snap;
        }
    }

    /// 撤销一次笔画编辑；无可撤销返回 false
    pub fn undo_path(&mut self) -> bool {
        if self.path_history_index == 0 {
            return false;
        }
        self.path_history_index -= 1;
        self.strokes = self.path_history[self.path_history_index].clone();
        self.interaction = BrushInteraction::None;
        true
    }

    /// 重做一次笔画编辑；无可重做返回 false
    pub fn redo_path(&mut self) -> bool {
        if self.path_history_index + 1 >= self.path_history.len() {
            return false;
        }
        self.path_history_index += 1;
        self.strokes = self.path_history[self.path_history_index].clone();
        self.interaction = BrushInteraction::None;
        true
    }

    /// 是否有可撤销的笔画编辑
    pub fn can_undo_path(&self) -> bool {
        self.path_history_index > 0
    }

    /// 是否有可重做的笔画编辑
    pub fn can_redo_path(&self) -> bool {
        self.path_history_index + 1 < self.path_history.len()
    }

    /// 重置整个笔画状态（含历史）
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests;
