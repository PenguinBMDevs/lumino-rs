/// 应用模式：编辑器/瀑布流/yinhe 副模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppMode {
    /// 钢琴卷帘编辑器模式（默认）
    #[default]
    Editor,
    /// 瀑布流视图模式
    Waterfall,
    /// yinhe 副模式（yinhe UI 套 lumino 底层，egui→iced）
    Yinhe,
}
