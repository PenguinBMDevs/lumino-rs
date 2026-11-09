//! Help 窗口模型。

/// Help 窗口：标题 / 搜索 / 分区话题列表 / 正文 / 底部按钮。
#[derive(Debug, Clone, Default)]
pub struct HelpModel {
    /// 顶部是否有 30px iced 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 窗口标题「帮助」。
    pub title: String,
    /// 搜索框占位（「搜索帮助...」）。
    pub search_hint: String,
    /// 当前查询文本。
    pub query: String,
    /// 过滤后的分区与话题（分区内保持生产顺序）。
    pub sections: Vec<HelpSectionModel>,
    /// 无匹配时的文案（列表与正文共用）。
    pub empty: String,
    /// 当前选中话题的正文。
    pub topic: Option<HelpTopicBodyModel>,
    /// 「全部重现」按钮。
    pub reset: String,
    /// 「关闭」按钮。
    pub close: String,
}

/// 一个分区（11px 标题 + 话题行）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HelpSectionModel {
    /// 分区标题（基础 / 编辑 / 绘制 / 参考）。
    pub title: String,
    /// 该分区下的话题。
    pub topics: Vec<HelpTopicRowModel>,
}

/// 列表中的一行话题。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpTopicRowModel {
    /// 在**过滤结果**中的下标（生产 `TopicSelected(index)`）。
    pub index: usize,
    /// 话题标题（当前语言）。
    pub label: String,
    /// 是否为当前选中项。
    pub active: bool,
}

/// 右侧正文。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HelpTopicBodyModel {
    /// 话题标题（22px）。
    pub title: String,
    /// 话题 tip（12px，工具类话题即工具显示名）。
    pub tip: String,
    /// 按空行切分的段落（14px）。
    pub paragraphs: Vec<String>,
}
