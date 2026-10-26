//! 帮助系统动作（DRAW-038）
//!
//! 只承载帮助窗口内的本地状态变更与两个全局请求（重置 tip / 关闭窗口）；
//! 话题正文与搜索在 `lumino-ui-core::help`，不进入消息层。

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
