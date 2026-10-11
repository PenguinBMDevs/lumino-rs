//! 下拉框/标签的本地化包装（数据值 ≠ 显示文案）
//!
//! ## 为什么需要它
//!
//! 视频导出对话框里的一批下拉选项，其取值同时是界面状态里存的**规范数据值**
//! （例如 `VideoExportDialogState::render_mode == "计数器"`、`quality == "高"`）。
//! 这些值还会被 `match` / `==` 当 key 用（见 `render_settings.rs` 的模式分支、
//! `handlers/dialog/video_export/helpers.rs` 的 `counter_font_mode` 分派），
//! **一旦改成英文，逻辑会静默失效**。
//!
//! [`LocalizedChoice`] 把「数据值」与「显示文案」两层分开：
//!
//! - 内部（`.0`）**始终是原始中文数据值**，参与 `PartialEq` 选中比较，
//!   并经选中回调原样回传消息 → 状态里存的仍是中文 key；
//! - `Display` 走运行时文案目录取词（[`tr_dyn`]），渲染出来的文案随语言切换。
//!
//! ```ignore
//! let options: Vec<LocalizedChoice> =
//!     ["高", "中", "低"].into_iter().map(LocalizedChoice::new).collect();
//! pick_list(options, Some(LocalizedChoice::new(state.quality.clone())), |v: LocalizedChoice| {
//!     Message::VideoExport(VideoExportAction::QualityChanged(v.0))
//! })
//! ```

use lumino_core::i18n::tr_dyn;

/// 下拉/标签用包装：内部仍是规范数据值，`Display` 走文案目录取词。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalizedChoice(pub String);

impl LocalizedChoice {
    /// 由原始数据值构造（`&str` / `String` 均可）。
    pub(crate) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl std::fmt::Display for LocalizedChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&tr_dyn(&self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 数据值必须原样保留，且选中比较只看数据值、不看显示文案。
    #[test]
    fn test_data_value_preserved_and_compared_by_value() {
        let choice = LocalizedChoice::new("左上");
        assert_eq!(choice.0, "左上");
        assert_eq!(choice, LocalizedChoice::new("左上".to_string()));
        // 显示文案（可能的英文）绝不能成为比较键，否则选中态会失配。
        assert_ne!(choice, LocalizedChoice::new("Top Left"));
    }

    /// `Display` 必须走 `tr_dyn` 取词；**不切换进程级语言**——
    /// 同 crate 的 `helpers.rs` 测试依赖中文输出，切换语言会让它们随机失败。
    #[test]
    fn test_display_routes_through_tr_dyn() {
        assert_eq!(
            LocalizedChoice::new("左上").to_string(),
            lumino_core::i18n::tr_dyn("左上")
        );
        // 未登记的数据值（如 MIDITrail，本就是拉丁文）回退原文，不能变成空串。
        let fallback = LocalizedChoice::new("MIDITrail");
        assert_eq!(fallback.to_string(), "MIDITrail");
        assert_eq!(fallback.0, "MIDITrail");
    }
}
