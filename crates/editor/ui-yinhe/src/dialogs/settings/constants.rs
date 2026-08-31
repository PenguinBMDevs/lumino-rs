//! 设置常量 — yinhe `dialogs/settings/constants.rs:219` 的 iced 迁移桩

/// 设置分类 i18n key（顺序即 `settings_tab` 索引）
pub const CATEGORY_KEYS: [&str; 6] = [
    "settings.cat.theme",
    "settings.cat.language",
    "settings.cat.audio",
    "settings.cat.render",
    "settings.cat.shortcuts",
    "settings.cat.general",
];

/// 设置项注册表（供搜索），保留 `zh / en / ja / ko` 四语
#[derive(Debug, Clone)]
pub struct SettingItem {
    pub cat: usize,
    pub zh: &'static str,
    pub en: &'static str,
    pub ja: &'static str,
    pub ko: &'static str,
}

pub const SETTING_ITEMS: &[SettingItem] = &[
    SettingItem {
        cat: 0,
        zh: "主题预设",
        en: "Theme preset",
        ja: "テーマプリセット",
        ko: "테마 프리셋",
    },
    SettingItem {
        cat: 0,
        zh: "背景",
        en: "Background color",
        ja: "背景色",
        ko: "배경색",
    },
    SettingItem {
        cat: 0,
        zh: "主文字",
        en: "Text color",
        ja: "テキスト色",
        ko: "텍스트 색",
    },
    SettingItem {
        cat: 0,
        zh: "强调色",
        en: "Accent color",
        ja: "アクセント色",
        ko: "강조색",
    },
    SettingItem {
        cat: 1,
        zh: "语言",
        en: "Language",
        ja: "言語",
        ko: "언어",
    },
    SettingItem {
        cat: 2,
        zh: "输出设备",
        en: "Output device",
        ja: "出力デバイス",
        ko: "출력 장치",
    },
    SettingItem {
        cat: 2,
        zh: "采样率",
        en: "Sample rate",
        ja: "サンプルレート",
        ko: "샘플 레이트",
    },
    SettingItem {
        cat: 2,
        zh: "缓冲区大小",
        en: "Buffer size",
        ja: "バッファサイズ",
        ko: "버퍼 크기",
    },
    SettingItem {
        cat: 5,
        zh: "允许重叠",
        en: "Allow overlapping notes",
        ja: "ノート重なり許可",
        ko: "노트 겹침 허용",
    },
];
