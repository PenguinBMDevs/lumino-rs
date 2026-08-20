//! 侧边栏分组 ID（从 sidebar/core.rs 迁入）
//!
//! 定义左侧路由栏的分组标识及其灯条颜色与提示文案。

use iced_core::Color;
use lumino_extras::i18n::Language;

/// 侧边栏分组 ID
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupId {
    /// 钢琴卷帘组（红色）
    PianoRoll,
    /// 工程走带组（绿色）
    Project,
    /// 播放器组（黄色）
    Waterfall,
    /// 渲染组（蓝色）
    Renderer,
}

impl GroupId {
    /// 父按钮灯条颜色（硬编码）
    pub fn parent_color(&self) -> Color {
        match self {
            GroupId::PianoRoll => Color::from_rgb(0.85, 0.15, 0.15),
            GroupId::Project => Color::from_rgb(0.15, 0.75, 0.35),
            GroupId::Waterfall => Color::from_rgb(0.85, 0.75, 0.10),
            GroupId::Renderer => Color::from_rgb(0.15, 0.45, 0.85),
        }
    }

    /// 子按钮灯条颜色（比父按钮浅）
    pub fn child_color(&self) -> Color {
        match self {
            GroupId::PianoRoll => Color::from_rgb(0.65, 0.35, 0.35),
            GroupId::Project => Color::from_rgb(0.35, 0.65, 0.45),
            GroupId::Waterfall => Color::from_rgb(0.65, 0.58, 0.30),
            GroupId::Renderer => Color::from_rgb(0.35, 0.55, 0.65),
        }
    }

    /// 获取分组提示文本（随语言切换）
    pub fn tooltip(&self, lang: Language) -> &'static str {
        match self {
            GroupId::PianoRoll => match lang {
                Language::ZhCn => "钢琴卷帘组",
                Language::EnUs => "Piano Roll",
            },
            GroupId::Project => match lang {
                Language::ZhCn => "工程走带",
                Language::EnUs => "Project",
            },
            GroupId::Waterfall => match lang {
                Language::ZhCn => "播放器",
                Language::EnUs => "Player",
            },
            GroupId::Renderer => match lang {
                Language::ZhCn => "渲染器",
                Language::EnUs => "Renderer",
            },
        }
    }
}
