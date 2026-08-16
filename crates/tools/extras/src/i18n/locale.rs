//! 区域设置检测 — 按语言获取对应的翻译数据

use super::translations::{ENUS_MAIN, MainTranslations, ZHCN_MAIN};
use lumino_core::types::Language;

/// 获取主界面翻译
pub fn get(lang: Language) -> &'static MainTranslations {
    match lang {
        Language::ZhCn => &ZHCN_MAIN,
        Language::EnUs => &ENUS_MAIN,
    }
}
