//! 对话框 / 面板动作枚举合集。
//!
//! 这些类型原先各占一个 9–35 行的文件，彼此无依赖；合并后减少文件数，
//! 旧路径通过 `lib.rs` 里的内联别名模块继续可用。

use crate::DotType;
use crate::types::TupletType;
use lumino_core::BrushConfig;
use lumino_core::shape::MaskShow;
use std::path::PathBuf;

// ---- autosave_recover ----
// 自动保存恢复对话框动作（DEBT-12） // 启动时若检测到上次会话遗留的自动保存，走应用内对话框让用户选择： // 恢复为未保存工程，或放弃恢复（文件改名保留，下次不再询问）。

/// 自动保存恢复对话框动作
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutosaveRecoverAction {
    /// 恢复自动保存内容（按未保存工程载入）
    Restore,
    /// 不恢复（文件改名保留）
    Discard,
}

// ---- batch_edit ----
// 批量编辑动作

/// 批量编辑对话框中的目标字段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchEditField {
    /// 音符力度
    Velocity,
    /// 音符长度
    Gate,
    /// 音符 key 位置
    Key,
    /// 音符 tick 位置
    Tick,
}

/// 批量编辑动作
#[derive(Debug, Clone)]
pub enum BatchEditAction {
    /// 打开批量编辑对话框
    OpenDialog,
    /// 关闭批量编辑对话框
    CloseDialog,
    /// 确认批量编辑
    Confirm,
    /// 字段输入变更
    InputChanged(BatchEditField, String),
}

// ---- brush_settings ----
// 画刷「绘制行为」对话框动作 // 镜像 `custom_precision`：主窗侧 `OpenDialog` 触发 Runner 打开独立 OS 窗口， // 对话框内部编辑本地草稿（`RootState::brush_settings_draft`），`Save` 通过 // `DialogResult::BrushSettings` 回传主窗应用。

/// 画刷「绘制行为」对话框动作
#[derive(Debug, Clone)]
pub enum BrushSettingsAction {
    /// 打开对话框（主窗侧，携带当前画刷配置）
    OpenDialog(BrushConfig),
    /// 关闭对话框
    CloseDialog,
    /// 保存并应用配置
    Save,
    /// 取消
    Cancel,
    /// 设置某层音轨（level 0-based，None=默认自动分配）
    LevelTrackChanged(usize, Option<usize>),
    /// 在 level 之后插入新层
    AddLevel(usize),
    /// 删除 level 层（保留至少 1 层）
    RemoveLevel(usize),
}

// ---- custom_precision ----
// 自定义精度对话框动作

/// 自定义精度对话框动作
#[derive(Debug, Clone)]
pub enum CustomPrecisionAction {
    /// 打开对话框
    OpenDialog,
    /// 关闭对话框
    CloseDialog,
    /// 确认自定义精度
    Confirm,
    /// 三连音数量变更
    TupletCountChanged(String),
    /// 三连音类型变更
    TupletTypeChanged(TupletType),
    /// 附点类型变更
    DotTypeChanged(DotType),
    /// 分音符值变更
    NoteValueChanged(String),
    /// 除数变更
    DivisorChanged(String),
}

// ---- draw_settings ----
// 「音符画设置」对话框动作 // 与 `brush_settings` 同构：主窗侧 `OpenDialog` 触发 Runner 打开**独立 OS 窗口** // （各绘制工具的公共参数聚合入口，见 `view/draw_settings_dialog.rs`）。 // 之所以不复用 `brush_settings` 的通道：两者是**不同窗口类型** // （`DialogType::BrushSettings` / `DialogType::DrawSettings`）， // 共用动作枚举会让"画刷绘制行为"与"音符画设置"在事件层无法区分， // 沦为靠 payload 猜意图。

/// 「音符画设置」对话框动作
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawSettingsAction {
    /// 打开对话框（主窗侧，由悬浮条右端齿轮按钮触发）
    OpenDialog,
    /// 关闭对话框（对话框内「关闭」按钮）
    CloseDialog,
}

// ---- help ----
// 帮助系统动作（DRAW-038） // 只承载帮助窗口内的本地状态变更与两个全局请求（重置 tip / 关闭窗口）； // 话题正文与搜索在 `lumino-ui-core::help`，不进入消息层。

/// 帮助系统动作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelpAction {
    /// 搜索框内容变更
    QueryChanged(String),
    /// 选中搜索结果中的第 `index` 项
    TopicSelected(usize),
    /// 关闭当前首次使用提示
    DismissTip,
    /// 清除已看过的 tip，让它们下次重新出现
    ResetTips,
    /// 关闭 Help 窗口
    Close,
}

// ---- history ----
// 撤销历史面板动作（DRAW-035）

/// 历史面板动作
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryAction {
    /// 跳到历史游标（0 = 初始状态）
    Jump(usize),
    /// 关闭历史面板
    Close,
}

// ---- load_confirm ----
// 加载确认对话框动作

/// 加载确认对话框动作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadConfirmAction {
    /// 确认加载
    Confirm,
    /// 关闭对话框（取消加载）
    CloseDialog,
}

// ---- mask_panel ----
// 遮罩参数面板动作（DRAW-010） // 面板只承载「用户意图」，参数装配在 `lumino-ui` 的 `Root`： // pending 形状即时预览，已 Bake 走草稿 + Apply（一次撤销单元）。

/// 遮罩参数面板动作
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MaskPanelAction {
    /// 关闭面板
    Close,
    /// 开关遮罩（false = 移除标记）
    SetEnabled(bool),
    /// 带宽（keys，0..=32）
    SetSize(f32),
    /// 取反：图案只显示在遮罩之外（挖洞）
    SetInvert(bool),
    /// 自身音符发声（false = 纯切割，只留虚线边框）
    SetOwn(bool),
    /// 区域来源：内部 / 沿线
    SetShow(MaskShow),
    /// 已 Bake：把草稿写入（一次 `edit_shape` 撤销单元）
    Apply,
    /// 已 Bake：把遮罩透出的可见片段实体化为新形状（原形状/遮罩不变）
    ApplyAsShape,
}

// ---- material_delete ----
// 素材删除确认对话框动作（DRAW-045，独立 OS 窗口）。 // 对话框只回传用户决策；文件删除、列表刷新与 Toast 仍由主窗口的 // `RightSidebarAction::MaterialDeleteConfirmed` 语义完成。

/// 素材删除确认对话框动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialDeleteDialogAction {
    /// 确认删除（携带待删除素材下标）。
    Confirm(usize),
    /// 取消（按钮、遮罩外点击或窗口 X）。
    Cancel,
}

// ---- project_settings ----
// 工程设置对话框动作

/// 工程设置对话框动作
#[derive(Debug, Clone)]
pub enum ProjectSettingsAction {
    /// 打开对话框
    OpenDialog,
    /// 关闭对话框
    CloseDialog,
    /// 确认工程设置
    Confirm,
    /// 项目名称变更
    TitleChanged(String),
    /// BPM 速度变更
    TempoChanged(String),
    /// 版权信息变更
    CopyrightChanged(String),
    /// 作者变更
    AuthorChanged(String),
    /// 拍号分子变更
    TimeSignatureNumeratorChanged(String),
    /// 拍号分母变更
    TimeSignatureDenominatorChanged(String),
    /// 启用通道 10（DRAW-032）：区域着色色号上限 15 → 16
    Use10Changed(bool),
}

// ---- recover_track ----
// 找回删除音轨对话框动作 // 用于"找回删除音轨"对话框内的 UI 交互。实际的磁盘 I/O（读取缓存文件、 // 写入 sidebar.tracks、释放 reserved_track_ids）由 Runner 在收到 // `DialogResult::RecoverTrack*` 后完成。

/// 找回删除音轨对话框动作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoverTrackAction {
    /// 关闭对话框（取消 / 关闭按钮）
    CloseDialog,
    /// 选中条目变化
    SelectionChanged(usize),
    /// 恢复选中条目到原位置
    ///
    /// 由对话框视图在用户点击"恢复"按钮时发出。Root 接收后转换为
    /// `DialogResult::RecoverTrackRestore` 并关闭对话框。
    Restore {
        /// 缓存文件路径
        path: PathBuf,
        /// 删除时记录的原始 sidebar.tracks 索引
        original_index: usize,
    },
    /// 永久销毁选中条目
    ///
    /// 由对话框视图在用户点击"永久删除"按钮时发出。Root 接收后转换为
    /// `DialogResult::RecoverTrackPermanentlyDelete` 转交 Runner；对话框**保持开启**，
    /// Runner 销毁缓存文件后重新扫描缓存目录刷新条目列表，支持连续操作。
    PermanentlyDelete {
        /// 缓存文件路径
        path: PathBuf,
        /// 删除时记录的音轨 ID（用于释放 reserved_track_ids 占用）
        track_id: u16,
    },
}

// ---- save_confirm ----
// 保存确认对话框动作 // 关闭工程 / 打开另一个工程 / 退出软件时，若当前工程存在未保存的更改， // 弹出此对话框让用户决定：保存当前工程、放弃更改直接关闭、或取消关闭操作。

/// 保存确认对话框动作
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveConfirmAction {
    /// 保存当前工程后继续（关闭/打开/退出）
    Save,
    /// 放弃未保存的更改，直接关闭（不保存）
    Discard,
    /// 取消关闭操作，继续编辑
    Cancel,
}

// ---- settings_dialog ----
// 设置对话框动作

/// 设置对话框动作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsDialogAction {
    /// 打开设置对话框
    OpenDialog,
    /// 关闭设置对话框
    CloseDialog,
}

// ---- speed_change ----
// 音符变速动作

/// 音符变速动作
#[derive(Debug, Clone)]
pub enum SpeedChangeAction {
    /// 打开音符变速对话框
    OpenDialog,
    /// 关闭音符变速对话框
    CloseDialog,
    /// 确认音符变速
    Confirm,
    /// 速度倍率输入变更
    FactorChanged(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_speed_change_action_variants() {
        let action = SpeedChangeAction::OpenDialog;
        assert!(matches!(action, SpeedChangeAction::OpenDialog));

        let action = SpeedChangeAction::FactorChanged("0.5".to_string());
        assert!(matches!(action, SpeedChangeAction::FactorChanged(_)));
    }
}

// ---- tempo_max_bpm ----
// 自定义 Tempo BPM 上限对话框动作（DRAW-045，独立 OS 窗口）。 // 输入串与校验结果留在对话框自己的状态里；本枚举只承载用户操作， // 校验通过后的 `bpm` 由生产侧 `DialogResult` 回传。

/// 「自定义 BPM 上限」对话框动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TempoMaxBpmDialogAction {
    /// 输入框文本变化（原样回传，校验在确认时进行）。
    Input(String),
    /// 确认：生产侧解析并应用。
    Confirm,
    /// 取消（按钮或窗口 X）。
    Cancel,
}
