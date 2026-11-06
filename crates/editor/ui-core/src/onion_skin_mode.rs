//! 洋葱皮显示范围模式（工具栏洋葱皮按钮右侧下拉三角的选项）
//!
//! 用户需求（2026-11-05）：下拉提供「显示全部音轨 / 上一个音轨 / 下一个音轨 /
//! 指定音轨」四种范围；**指定音轨**本轮只留入口，功能待接入。
//!
//! # 为什么单独一个模块而不是塞进 `Toolbar`
//!
//! 这个值有两个消费者，必须共用**同一份**字符串口径：
//! 1. UI 工具栏（菜单勾选态 + `Host::onion_skin_mode()`）；
//! 2. 工程文件持久化（`SettingsMetadata::onion_skin_mode`，见 project crate）。
//!
//! 工程范围记忆 = 写进 `.lmpj` 的 `metadata.toml` ⇒ 必须能从字符串还原，
//! 且**未知字符串不得报错**（旧版客户端打开新版工程、或工程被手工编辑）——
//! 落回默认 `AllTracks` 而不是让整个工程加载失败。

/// 洋葱皮显示范围模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OnionSkinMode {
    /// 显示全部音轨（默认；静音轨仍按既有契约不显示）
    #[default]
    AllTracks,
    /// 只显示当前音轨的**上一个**音轨（首轨时无洋葱皮）
    PreviousTrack,
    /// 只显示当前音轨的**下一个**音轨（末轨时无洋葱皮）
    NextTrack,
    /// 只显示**指定音轨**（本轮仅入口，功能待接入）
    SpecificTrack,
}

impl OnionSkinMode {
    /// 全部模式（顺序 = 下拉菜单显示顺序）
    #[must_use]
    pub const fn all() -> [Self; 4] {
        [
            Self::AllTracks,
            Self::PreviousTrack,
            Self::NextTrack,
            Self::SpecificTrack,
        ]
    }

    /// 持久化字符串（写进工程文件 `metadata.toml` 的 `[settings]` 段）
    ///
    /// 小写下划线口径：与工程文件里其它枚举字段（如 `auto_scroll_mode`）一致。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AllTracks => "all_tracks",
            Self::PreviousTrack => "previous_track",
            Self::NextTrack => "next_track",
            Self::SpecificTrack => "specific_track",
        }
    }

    /// 从持久化字符串还原（**未知值落回默认 `AllTracks`**，绝不失败）
    ///
    /// 宽松解析的理由：工程文件是用户可读可编辑的 TOML，且旧/新版本互相打开时
    /// 可能出现本版本不认识的取值——为一个显示模式让整个工程加载失败不可接受。
    #[must_use]
    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim() {
            "previous_track" => Self::PreviousTrack,
            "next_track" => Self::NextTrack,
            "specific_track" => Self::SpecificTrack,
            _ => Self::AllTracks,
        }
    }

    /// 该模式是否已实现（`SpecificTrack` 本轮仅入口）
    ///
    /// 未实现的模式**不得写入状态**：否则工具栏会显示一个什么都没做的勾选态。
    #[must_use]
    pub const fn is_implemented(self) -> bool {
        !matches!(self, Self::SpecificTrack)
    }
}

#[cfg(test)]
mod tests {
    use super::OnionSkinMode;

    /// 持久化字符串必须双向稳定：写出去的能原样读回来。
    #[test]
    fn test_mode_string_roundtrip() {
        for mode in OnionSkinMode::all() {
            assert_eq!(
                OnionSkinMode::from_str_lossy(mode.as_str()),
                mode,
                "{} 往返失败",
                mode.as_str()
            );
        }
    }

    /// 未知/空/畸形字符串一律落回默认，不得 panic（工程可被手工编辑）。
    #[test]
    fn test_unknown_string_falls_back_to_default() {
        for raw in ["", "  ", "bogus", "ALL_TRACKS", "previous", "指定音轨"] {
            assert_eq!(
                OnionSkinMode::from_str_lossy(raw),
                OnionSkinMode::AllTracks,
                "{raw:?} 应落回默认 AllTracks"
            );
        }
    }

    /// 默认必须是「全部音轨」：删掉此档会让老工程打开后洋葱皮行为变化。
    #[test]
    fn test_default_is_all_tracks() {
        assert_eq!(OnionSkinMode::default(), OnionSkinMode::AllTracks);
        assert!(OnionSkinMode::default().is_implemented());
    }

    /// 指定音轨本轮未实现；其余三档必须可用（否则菜单等于三个死项）。
    #[test]
    fn test_only_specific_track_is_unimplemented() {
        assert!(!OnionSkinMode::SpecificTrack.is_implemented());
        for mode in [
            OnionSkinMode::AllTracks,
            OnionSkinMode::PreviousTrack,
            OnionSkinMode::NextTrack,
        ] {
            assert!(mode.is_implemented(), "{mode:?} 应已实现");
        }
    }

    /// 菜单顺序固定为：全部 → 上一个 → 下一个 → 指定（勿按字母序重排）。
    #[test]
    fn test_menu_order_is_stable() {
        assert_eq!(
            OnionSkinMode::all(),
            [
                OnionSkinMode::AllTracks,
                OnionSkinMode::PreviousTrack,
                OnionSkinMode::NextTrack,
                OnionSkinMode::SpecificTrack,
            ]
        );
    }
}
