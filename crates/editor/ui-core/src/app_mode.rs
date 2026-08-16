/// 应用模式：编辑器/瀑布流
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppMode {
    #[default]
    Editor,
    Waterfall,
}
