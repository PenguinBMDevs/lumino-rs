//! 侧边栏路由（从 sidebar/core.rs 迁入）与卷帘面板底部按钮

use lumino_extras::i18n::{Language, main_translations};

/// 侧边栏路由（页面标识）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// 文件路由
    File,
    /// 编排（钢琴卷帘）路由
    Arrangement,
    /// 自动化路由
    Automation,
    /// 视频渲染路由
    VideoExport,
    /// 音频渲染路由
    AudioExport,
}

impl Route {
    /// 获取路由提示文本（随语言切换）
    pub fn tooltip(&self, lang: Language) -> &'static str {
        let translations = main_translations(lang);
        match self {
            Route::File => translations.sidebar_file,
            Route::Arrangement => translations.sidebar_arrangement,
            Route::Automation => translations.sidebar_automation,
            Route::VideoExport => match lang {
                Language::ZhCn => "视频渲染",
                Language::EnUs => "Video Render",
            },
            Route::AudioExport => match lang {
                Language::ZhCn => "音频渲染",
                Language::EnUs => "Audio Render",
            },
        }
    }
}

/// 卷帘面板左侧栏底部按钮（横向三条杠 / 纵向三条杠）
///
/// 仅在钢琴卷帘面板显示时出现在左侧路由栏底部，纵向排布：
/// 最下方为纵向三条杠，其上方为横向三条杠。
///
/// 两个按钮的打开状态互斥，因此 `Sidebar` 用 `Option<RollBarButton>`
/// 单值表示当前激活项——互斥性由类型保证，而非依赖两个 bool 的运行时同步。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollBarButton {
    /// 横向三条杠（位于纵向按钮上方）
    Horizontal,
    /// 纵向三条杠（位于左侧栏最下方）
    Vertical,
}

impl RollBarButton {
    /// 获取按钮提示文本（随语言切换）
    ///
    /// 两个按钮代表卷帘内容的展开方向（横向卷帘 / 纵向卷帘）。
    pub fn tooltip(&self, lang: Language) -> &'static str {
        match self {
            RollBarButton::Horizontal => match lang {
                Language::ZhCn => "横向卷帘",
                Language::EnUs => "Horizontal Roll",
            },
            RollBarButton::Vertical => match lang {
                Language::ZhCn => "纵向卷帘",
                Language::EnUs => "Vertical Roll",
            },
        }
    }
}
