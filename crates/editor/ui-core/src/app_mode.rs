/// 应用模式：编辑器/瀑布流
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppMode {
    /// 钢琴卷帘编辑器模式（默认）
    #[default]
    Editor,
    /// 瀑布流视图模式
    Waterfall,
}
