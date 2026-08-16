//! 曲线工具贝塞尔路径绘制状态（支持多条路径批量绘制）
//!
//! 曲线工具在钢琴卷帘上通过点击拉出路径（初始为直线）：
//! - 前两次点击设置首尾端点（tick 按网格吸附、key 为整数格）；
//! - 点击线段中间可插入锚点（**不吸附网格**，自由精确定位）；
//! - 每段为三次贝塞尔曲线，锚点带 in/out 两个控制柄（首尾各显示一个），
//!   拖动控制柄弯曲曲线；自动柄（1/3 段长）保证未弯曲时为精确直线；
//! - 端点拖动保持吸附，中间锚点自由移动；双击中间锚点删除；
//! - **多条路径可同时存在**（空白处按下开始新路径，不清空已有），
//!   共享一组 √（批量确认）/ ×（批量取消）按钮；
//! - **路径编辑历史**：创建曲线（合并为一次）、插入/删除锚点、拖动锚点/
//!   控制柄/平移均为一次撤销操作（Ctrl+Z / Ctrl+Y），与 document 历史
//!   互不干扰（√ 确认后才写入 document 生成音符）。

/// 直线工具交互阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineToolInteraction {
    /// 无交互
    #[default]
    None,
    /// 拖动指定路径的锚点
    DraggingAnchor { path: usize, idx: usize },
    /// 整体平移指定路径（segment = 按下时命中的曲线段索引，
    /// 用于未拖动（视为点击插入锚点）时定位插入位置）
    DraggingLine { path: usize, segment: usize },
    /// 拖动控制柄
    DraggingHandle {
        path: usize,
        anchor_idx: usize,
        side: HandleSide,
    },
}

/// 控制柄方位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleSide {
    /// 入向控制柄（控制"来自上一锚点"的贝塞尔段）
    In,
    /// 出向控制柄（控制"到下一锚点"的贝塞尔段）
    Out,
}

/// 贝塞尔锚点
///
/// 位置与控制柄均为 (tick, key) 逻辑坐标；key 为 f32——
/// 中间锚点不吸附网格，可自由精确定位。
///
/// 控制柄初始为**自动维护**（`handles_auto`）：重算时取相邻段方向 1/3
/// 长度——三次贝塞尔在 cp1 = A + (B-A)/3、cp2 = B - (B-A)/3 时为**精确直线**，
/// 保证路径初始外观为直线，同时控制柄可见可交互（可随时拖动弯曲）。
/// 用户拖动控制柄后标记为自定义，不再被自动重算覆盖。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BezierAnchor {
    /// 锚点位置（tick, key）
    pub pos: (f32, f32),
    /// 出向控制柄偏移（相对 pos，控制"到下一锚点"的贝塞尔段）
    pub out_handle: (f32, f32),
    /// 入向控制柄偏移（相对 pos，控制"来自上一锚点"的贝塞尔段）
    pub in_handle: (f32, f32),
    /// 控制柄是否自动维护（未被用户自定义）：插入/删除/锚点移动时自动重算
    pub handles_auto: bool,
}

impl BezierAnchor {
    /// 构造锚点（控制柄自动维护，偏移为 0——由路径重算填充）
    pub fn new(pos: (f32, f32)) -> Self {
        Self {
            pos,
            out_handle: (0.0, 0.0),
            in_handle: (0.0, 0.0),
            handles_auto: true,
        }
    }

    /// 设置出向控制柄（标记为自定义，不再自动维护）
    pub fn set_out_handle(&mut self, offset: (f32, f32)) {
        self.out_handle = offset;
        self.handles_auto = false;
    }

    /// 设置入向控制柄（标记为自定义，不再自动维护）
    pub fn set_in_handle(&mut self, offset: (f32, f32)) {
        self.in_handle = offset;
        self.handles_auto = false;
    }

    /// 出向控制柄绝对坐标（逻辑坐标）
    pub fn out_handle_abs(&self) -> (f32, f32) {
        (
            self.pos.0 + self.out_handle.0,
            self.pos.1 + self.out_handle.1,
        )
    }

    /// 入向控制柄绝对坐标（逻辑坐标）
    pub fn in_handle_abs(&self) -> (f32, f32) {
        (self.pos.0 + self.in_handle.0, self.pos.1 + self.in_handle.1)
    }
}

/// 单条路径（有序锚点链）
pub type LinePath = Vec<BezierAnchor>;

/// 路径编辑历史快照（全部路径 + 颜料桶已填充格点）
///
/// 填充格点与路径同属"待确认编辑内容"：√ 确认时合并生成音符，
/// × 清空，Ctrl+Z 一并撤销。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PathSnapshot {
    /// 全部路径（每条 = 锚点链）
    pub paths: Vec<LinePath>,
    /// 已填充格点（逻辑坐标：tick = snap 倍数、key 整数格）
    pub fill: Vec<(f32, u16)>,
}

/// 曲线工具贝塞尔路径状态
#[derive(Debug, Clone, PartialEq)]
pub struct LineToolState {
    /// 全部路径（每条 = 锚点链，>= 2 个锚点为完整路径）
    pub paths: Vec<LinePath>,
    /// 颜料桶填充标记（逻辑坐标：tick = snap 倍数、key 整数；每次点击一格）
    ///
    /// 标记不直接生成音符：√ 确认时按封闭图形覆盖范围计算全部格点
    /// （`confirm_fill_cells`），× 清空，Ctrl+Z 可撤销。
    pub fill: Vec<(f32, u16)>,
    /// 当前交互阶段
    pub interaction: LineToolInteraction,
    /// 拖拽基准：按下时的吸附（tick, key）——端点锚点/整条平移的增量基准
    pub drag_start_snap: (f32, f32),
    /// 拖拽基准：按下时的原始（tick, key）——中间锚点/控制柄的增量基准
    pub drag_start_raw: (f32, f32),
    /// 拖拽基准：按下时被拖锚点的原始值
    pub drag_anchor_orig: BezierAnchor,
    /// 拖拽基准：平移时被拖路径的原始值
    pub drag_line_orig: LinePath,
    /// 拖拽基准：按下时被拖控制柄的原始偏移
    pub drag_handle_orig: (f32, f32),
    /// 按下待定标志：曲线段按下后移动超过阈值才确认拖动；
    /// 未确认松开视为点击（插入锚点）
    pub drag_confirmed: bool,
    /// 上次追加锚点的路径索引（连续创建同一路径时合并历史用）；
    /// None = 无创建中连续追加
    pub last_push_path: Option<usize>,
    /// 颜料桶填充模式（启用式开关）：开启后曲线工具点击画布 =
    /// 填充封闭区域，不再绘制锚点；仅曲线工具激活时有效
    pub fill_enabled: bool,
    /// 路径编辑历史（快照 = 操作后状态；`path_history_index` 指向当前状态）
    ///
    /// 栈始终含初始状态（`[空]`，index 0）；每次操作完成后 push 新状态，
    /// 连续追加锚点（创建同一路径）时更新栈顶合并为一次撤销。
    pub path_history: Vec<PathSnapshot>,
    pub path_history_index: usize,
}

impl Default for LineToolState {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            fill: Vec::new(),
            interaction: LineToolInteraction::None,
            drag_start_snap: (0.0, 0.0),
            drag_start_raw: (0.0, 0.0),
            drag_anchor_orig: BezierAnchor::default(),
            drag_line_orig: Vec::new(),
            drag_handle_orig: (0.0, 0.0),
            drag_confirmed: false,
            last_push_path: None,
            fill_enabled: false,
            // 历史栈初始含空状态（撤销基准）
            path_history: vec![PathSnapshot::default()],
            path_history_index: 0,
        }
    }
}

impl LineToolState {
    /// 是否已有至少一个锚点
    pub fn has_anchor(&self) -> bool {
        self.paths.iter().any(|p| !p.is_empty())
    }

    /// 是否存在完整路径（>= 2 个锚点）
    pub fn is_complete(&self) -> bool {
        self.paths.iter().any(|p| p.len() >= 2)
    }

    /// 当前创建中的路径索引：最后一条未完整（< 2 锚点）的路径
    pub fn creating_path(&self) -> Option<usize> {
        self.paths.iter().rposition(|p| p.len() < 2)
    }

    /// 追加锚点到指定路径（未完整时设置端点用）
    pub fn push_anchor(&mut self, path_idx: usize, pos: (f32, f32)) {
        let Some(path) = self.paths.get_mut(path_idx) else {
            return;
        };
        path.push(BezierAnchor::new(pos));
        self.recompute_auto_handles();
    }

    /// 在指定路径的段 [index-1, index] 之间插入锚点（index ∈ 1..=len），
    /// 位置为点击处（不吸附网格）。越界返回 false。
    pub fn insert_anchor_at(&mut self, path_idx: usize, index: usize, pos: (f32, f32)) -> bool {
        let Some(path) = self.paths.get_mut(path_idx) else {
            return false;
        };
        if index == 0 || index > path.len() {
            return false;
        }
        path.insert(index, BezierAnchor::new(pos));
        self.recompute_auto_handles();
        true
    }

    /// 删除指定路径的锚点；仅中间锚点可删（端点不可删，保留至少 2 个锚点）。
    pub fn delete_anchor(&mut self, path_idx: usize, index: usize) -> bool {
        let Some(path) = self.paths.get_mut(path_idx) else {
            return false;
        };
        if index == 0 || index + 1 >= path.len() {
            return false;
        }
        path.remove(index);
        self.recompute_auto_handles();
        true
    }

    /// 指定路径锚点可见的控制柄（首锚点只显示 out、尾锚点只显示 in、
    /// 中间锚点显示 in + out 两个）
    pub fn visible_handle_sides(&self, path_idx: usize, index: usize) -> Vec<HandleSide> {
        let Some(path) = self.paths.get(path_idx) else {
            return Vec::new();
        };
        if index == 0 {
            vec![HandleSide::Out]
        } else if index + 1 >= path.len() {
            vec![HandleSide::In]
        } else {
            vec![HandleSide::In, HandleSide::Out]
        }
    }

    /// 重算全部路径的自动控制柄：相邻锚点间的柄取段方向 1/3 长度
    /// （三次贝塞尔直线条件，保证路径初始外观为直线）。
    ///
    /// 仅重算 `handles_auto`（未被用户自定义）的柄；
    /// 用户拖动过的柄保持原值不被覆盖。
    pub fn recompute_auto_handles(&mut self) {
        for path in &mut self.paths {
            for i in 0..path.len().saturating_sub(1) {
                let a = path[i];
                let b = path[i + 1];
                if a.handles_auto {
                    path[i].out_handle = ((b.pos.0 - a.pos.0) / 3.0, (b.pos.1 - a.pos.1) / 3.0);
                }
                if b.handles_auto {
                    path[i + 1].in_handle = ((a.pos.0 - b.pos.0) / 3.0, (a.pos.1 - b.pos.1) / 3.0);
                }
            }
        }
    }

    // ── 路径编辑历史（撤销/重做） ─────────────────────────

    /// 当前全部路径 + 填充格点快照
    pub fn snapshot(&self) -> PathSnapshot {
        PathSnapshot {
            paths: self.paths.clone(),
            fill: self.fill.clone(),
        }
    }

    /// 记录当前状态（操作完成后调用）：截断重做分支后入栈
    pub fn push_path_history(&mut self) {
        self.path_history.truncate(self.path_history_index + 1);
        self.path_history.push(self.snapshot());
        self.path_history_index = self.path_history.len() - 1;
    }

    /// 更新栈顶为当前状态（合并连续操作——创建同一路径的锚点追加）
    pub fn update_top_path_history(&mut self) {
        let snap = self.snapshot();
        if let Some(top) = self.path_history.last_mut() {
            *top = snap;
        }
    }

    /// 撤销一次路径编辑；无可撤销返回 false
    pub fn undo_path(&mut self) -> bool {
        if self.path_history_index == 0 {
            return false;
        }
        self.path_history_index -= 1;
        let snap = &self.path_history[self.path_history_index];
        self.paths = snap.paths.clone();
        self.fill = snap.fill.clone();
        true
    }

    /// 重做一次路径编辑；无可重做返回 false
    pub fn redo_path(&mut self) -> bool {
        if self.path_history_index + 1 >= self.path_history.len() {
            return false;
        }
        self.path_history_index += 1;
        let snap = &self.path_history[self.path_history_index];
        self.paths = snap.paths.clone();
        self.fill = snap.fill.clone();
        true
    }

    /// 是否有可撤销的路径编辑
    pub fn can_undo_path(&self) -> bool {
        self.path_history_index > 0
    }

    /// 是否有可重做的路径编辑
    pub fn can_redo_path(&self) -> bool {
        self.path_history_index + 1 < self.path_history.len()
    }

    /// 重置整个路径状态（含历史）
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    // ── 颜料桶填充 ─────────────────────────

    /// 是否已有填充标记
    pub fn has_fill(&self) -> bool {
        !self.fill.is_empty()
    }

    /// 添加填充标记（去重）；返回新增数量
    pub fn add_fill_marks(&mut self, marks: &[(f32, u16)]) -> usize {
        let mut added = 0;
        for &mark in marks {
            if !self.fill.contains(&mark) {
                self.fill.push(mark);
                added += 1;
            }
        }
        added
    }

    /// 清除全部填充标记；返回是否清除了内容
    pub fn clear_fill(&mut self) -> bool {
        let had = self.has_fill();
        self.fill.clear();
        had
    }
}

#[cfg(test)]
mod tests;
