//! Yinhe 状态 — `YinheState { view_mode, layout: YinheLayout }` 与 `yinhe_layout.json` 持久化
//!
//! 约束（P1）：
//! - `YinheState` 聚合 `view_mode`（`crate::chrome::ViewMode`）与 `YinheLayout`
//!  （`arr_split / right_panel_width / show_pianoroll_in_arrange`，对应 yinhe 原
//!   `audio_settings.rs` 的 `ui_scale, layout.*`），
//!   **不污染** `lumino_core::storage::UiState`（5字段窗口几何）与 `UiConfig`（42字段全局配置），
//!   独立持久化到 `yinhe_layout.json`，复用 `src/storage/ui_state.rs` 的
//!   `Wrapper { inner, path, dirty, get/patch/save }` 存储模式。
//! - 字体/主题不经此持久化：字体走 `UiConfig.program_font_name/path`
//!  （`crates/editor/ui/src/host.rs:260 create_font_from_config`），主题走
//!   `lumino_ui_core::Theme`（`crate::theme::map_yinhe_base_to_lumino` 仅做数值迁移）。

pub mod layout;

pub use layout::{YINHE_LAYOUT_FILE, YinheLayout, YinheLayoutWrapper};

use std::path::{Path, PathBuf};
use std::{fs, io};

use serde::{Deserialize, Serialize};

use crate::chrome::ViewMode;

/// Yinhe 顶层状态 — 运行时由宿主持有，持久化到 `yinhe_layout.json`
///
/// ```text
/// YinheState {
///   view_mode: ViewMode,               // Arrange / Piano / Mix
///   layout: YinheLayout {
///     arr_split,                         // 0.05..0.95 走带/钢琴卷帘分割
///     right_panel_width,                 // 120..800 右侧面板宽
///     show_pianoroll_in_arrange,         // Arrange 叠加钢琴卷帘
///   }
/// }
/// ```
///
/// 与 `UiState` 隔离：`UiState` 仅存窗口几何（x/y/w/h/is_maximized），
/// `YinheState` 存 yinhe 副模式的视图与布局，文件为 `yinhe_layout.json`
///（与 `ui_state.json` / `config.json` 同级于 `ProjectDirs::preference_dir()`，
/// 但由调用方传入 `PathBuf`，本 crate 不直接依赖 `directories`，保持 `Cargo.toml`
/// 不引入新字体包之外的额外依赖与污染）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct YinheState {
    /// 当前 Yinhe 视图模式
    #[serde(default)]
    pub view_mode: ViewMode,
    /// 布局（走带分割/右侧面板/叠加开关）
    #[serde(default)]
    pub layout: YinheLayout,
}

impl Default for YinheState {
    fn default() -> Self {
        Self {
            view_mode: ViewMode::default(),
            layout: YinheLayout::default(),
        }
    }
}

impl YinheState {
    /// 兼容旧 `yinhe_layout.json` 的宽松反序列化（缺字段回落默认值并 clamp）
    pub fn from_json_slice(bytes: &[u8]) -> Self {
        serde_json::from_slice::<Self>(bytes).unwrap_or_default()
    }

    /// 将布局 clamp 到合法区间（代理到 `YinheLayout::clamp`）
    pub fn clamp(&mut self) {
        self.layout.clamp();
    }

    /// 归一化（clamp 后返回 self，便于链式）
    pub fn clamped(mut self) -> Self {
        self.clamp();
        self
    }
}

/// `YinheState` 持久化 Wrapper — 复用 `src/storage/ui_state.rs::UiStateWrapper` 模式
///
/// 文件固定为 `yinhe_layout.json`，与 `YinheLayoutWrapper` 同文件（JSON 结构为
/// `YinheState` 的完整序列化，含 `view_mode` 与 `layout`；若历史文件仅含
/// `YinheLayout` 字段，`#[serde(default)]` 会补齐 `view_mode`）。
#[derive(Debug)]
pub struct YinheStateWrapper {
    inner: YinheState,
    path: PathBuf,
    dirty: bool,
}

impl YinheStateWrapper {
    /// 从 `path` 加载 `yinhe_layout.json`，不存在或解析失败回落 `Default`
    pub fn new(path: PathBuf) -> Self {
        let inner = match fs::read(&path) {
            Ok(bytes) => YinheState::from_json_slice(&bytes).clamped(),
            Err(_) => YinheState::default(),
        };
        Self {
            inner,
            path,
            dirty: false,
        }
    }

    /// 一次性加载（不持有 path）
    pub fn load_from(path: impl AsRef<Path>) -> YinheState {
        match fs::read(path.as_ref()) {
            Ok(bytes) => YinheState::from_json_slice(&bytes).clamped(),
            Err(_) => YinheState::default(),
        }
    }

    /// 只读访问
    pub fn get(&self) -> &YinheState {
        &self.inner
    }

    /// 可变修改并标记 dirty（自动 clamp 布局）
    pub fn patch<F>(&mut self, f: F)
    where
        F: FnOnce(&mut YinheState),
    {
        f(&mut self.inner);
        self.inner.clamp();
        self.dirty = true;
    }

    /// 直接替换并标记 dirty
    pub fn set(&mut self, state: YinheState) {
        self.inner = state.clamped();
        self.dirty = true;
    }

    /// 保存到 `yinhe_layout.json`（`dirty == false` 时跳过）
    pub fn save(&mut self) -> io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = fs::File::create(&self.path)?;
        let writer = io::BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &self.inner)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        self.dirty = false;
        Ok(())
    }

    /// 是否有未保存的修改
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// 持久化路径
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chrome::ViewMode;
    use std::fs;

    #[test]
    fn default_state() {
        let s = YinheState::default();
        assert_eq!(s.view_mode, ViewMode::Arrange);
        assert_eq!(s.layout, YinheLayout::default());
    }

    #[test]
    fn serde_roundtrip() {
        let s = YinheState {
            view_mode: ViewMode::Piano,
            layout: YinheLayout {
                arr_split: 0.3,
                right_panel_width: 280.0,
                show_pianoroll_in_arrange: true,
            },
        };
        let json = serde_json::to_string(&s).expect("serialize");
        assert!(json.contains("view_mode"));
        assert!(json.contains("layout"));
        assert!(json.contains("arr_split"));
        let back: YinheState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, s);
    }

    #[test]
    fn wrapper_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "lumino_yinhe_state_test_{}_{}",
            std::process::id(),
            99
        ));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join(YINHE_LAYOUT_FILE);
        let _ = fs::remove_file(&path);

        let mut w = YinheStateWrapper::new(path.clone());
        w.patch(|s| {
            s.view_mode = ViewMode::Mix;
            s.layout.arr_split = 0.8;
            s.layout.show_pianoroll_in_arrange = true;
        });
        w.save().expect("save");

        let w2 = YinheStateWrapper::new(path.clone());
        assert_eq!(w2.get().view_mode, ViewMode::Mix);
        assert!((w2.get().layout.arr_split - 0.8).abs() < 1e-6);
        assert!(w2.get().layout.show_pianoroll_in_arrange);

        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn legacy_layout_only_json_compat() {
        // 历史文件可能仅含 YinheLayout 字段（无 view_mode），应回落默认值
        let json = br#"{"layout": {"arr_split": 0.6, "right_panel_width": 300.0, "show_pianoroll_in_arrange": false}}"#;
        let s = YinheState::from_json_slice(json);
        assert_eq!(s.view_mode, ViewMode::Arrange);
        assert!((s.layout.arr_split - 0.6).abs() < 1e-6);
    }

    #[test]
    fn not_polluting_uistate() {
        let s = YinheState::default();
        let json = serde_json::to_string(&s).expect("serialize");
        // 不应出现 UiState 的 5 字段
        assert!(!json.contains("is_maximized"));
        assert!(!json.contains("\"x\""));
        assert!(!json.contains("\"y\""));
        assert!(!json.contains("\"w\""));
        assert!(!json.contains("\"h\""));
        // 应出现 yinhe 独立字段
        assert!(json.contains("arr_split"));
        assert!(json.contains("right_panel_width"));
        assert!(json.contains("show_pianoroll_in_arrange"));
        assert!(json.contains("view_mode"));
    }
}
