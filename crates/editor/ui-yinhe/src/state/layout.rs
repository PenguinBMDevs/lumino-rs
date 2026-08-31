//! Yinhe 布局持久化 — 独立于 lumino `UiState` 的 `yinhe_layout.json`
//!
//! 背景：
//! - `crates/core/core/src/storage/config.rs:172 UiConfig` 为 42 字段的全局配置
//!   （主题/语言/合成器/字体 `program_font_name/path` 等）；
//!   `crates/core/core/src/storage/ui_state.rs:5 UiState` 为 5 字段的窗口几何
//!   （`x/y/w/h/is_maximized`）。
//! - yinhe 原 `audio_settings.rs` 持有 `ui_scale, layout.arr_split/right_panel_width`
//!   等布局偏好；P1 要求 **不污染** `UiState` / `UiConfig`，改为独立文件
//!   `yinhe_layout.json`，复用 `src/storage/ui_state.rs` / `config.rs` 的
//!   `Wrapper { inner, path, dirty, get/patch/save }` 存储模式。
//! - 本文件定义 `YinheLayout`（`arr_split / right_panel_width / show_pianoroll_in_arrange`）
//!   与 `YinheLayoutWrapper`，供 `crate::state::YinheState` 聚合与宿主持久化。

use std::path::{Path, PathBuf};
use std::{fs, io};

use serde::{Deserialize, Serialize};

// ─── 默认值（与 yinhe 原 `audio_settings.rs` / `layout.rs` 对齐） ─────────

fn default_arr_split() -> f32 {
    0.5
}
fn default_right_panel_width() -> f32 {
    320.0
}
fn default_show_pianoroll_in_arrange() -> bool {
    false
}

/// Yinhe 布局 — 独立于 `lumino_core::storage::UiState`，持久化到 `yinhe_layout.json`
///
/// 对应 yinhe 原 `layout.arr_split / right_panel_width` 与 `show_pianoroll_in_arrange`。
/// `ui_scale` 已废弃：缩放由宿主 `UiConfig` 与窗口 `scale_factor` 统一管理，不在此持久化。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct YinheLayout {
    /// 走带/钢琴卷帘水平分割比例 `0.0..1.0`，对应 yinhe `layout.arr_split`
    ///
    /// `0.0` = 仅走带，`1.0` = 仅钢琴卷帘；运行时 clamp 到 `0.05..0.95`
    #[serde(default = "default_arr_split")]
    pub arr_split: f32,
    /// 右侧面板宽度（像素），对应 yinhe `layout.right_panel_width`
    ///
    /// 运行时 clamp 到 `120..800`
    #[serde(default = "default_right_panel_width")]
    pub right_panel_width: f32,
    /// Arrange 模式下是否叠加钢琴卷帘，对应 yinhe `show_pianoroll_in_arrange`
    ///
    /// 与 `crate::chrome::ChromeState.show_pianoroll_in_arrange` 运行时状态同步，
    /// 持久化于此，避免写入 `UiState`。
    #[serde(default = "default_show_pianoroll_in_arrange")]
    pub show_pianoroll_in_arrange: bool,
}

impl Default for YinheLayout {
    fn default() -> Self {
        Self {
            arr_split: default_arr_split(),
            right_panel_width: default_right_panel_width(),
            show_pianoroll_in_arrange: default_show_pianoroll_in_arrange(),
        }
    }
}

impl YinheLayout {
    /// 将字段 clamp 到合法区间（存储前/加载后调用）
    pub fn clamped(mut self) -> Self {
        self.clamp();
        self
    }

    /// 就地 clamp
    pub fn clamp(&mut self) {
        self.arr_split = self.arr_split.clamp(0.05, 0.95);
        self.right_panel_width = self.right_panel_width.clamp(120.0, 800.0);
    }

    /// 兼容旧 `yinhe_layout.json` 的宽松反序列化
    ///
    /// 字段缺失时回落默认值（`#[serde(default)]`），数值越界时 `clamp`。
    pub fn from_json_slice(bytes: &[u8]) -> Self {
        serde_json::from_slice::<Self>(bytes)
            .unwrap_or_default()
            .clamped()
    }
}

/// `yinhe_layout.json` 的文件名常量（与 `src/storage` 的 `ui_state.json` / `config.json` 并列）
pub const YINHE_LAYOUT_FILE: &str = "yinhe_layout.json";

/// Yinhe 布局持久化 Wrapper — 复用 `src/storage/ui_state.rs: UiStateWrapper` 模式
///
/// - `new(path)`：从 `path` 读取 `yinhe_layout.json`，解析失败回落 `Default`；
///
/// - `get()` / `patch(|l| ...)`：访问与标记 dirty；
///
/// - `save()`：`dirty == false` 时跳过写盘，否则 `serde_json::to_writer` + `fs::create_dir_all`。
///
/// 不持有 `UiState`，不写入 `UiConfig`，独立文件避免污染全局状态。
#[derive(Debug)]
pub struct YinheLayoutWrapper {
    inner: YinheLayout,
    path: PathBuf,
    dirty: bool,
}

impl YinheLayoutWrapper {
    /// 从 `path` 加载 `yinhe_layout.json`，不存在或解析失败则回落默认值
    pub fn new(path: PathBuf) -> Self {
        let inner = match fs::read(&path) {
            Ok(bytes) => YinheLayout::from_json_slice(&bytes),
            Err(_) => YinheLayout::default(),
        };
        Self {
            inner,
            path,
            dirty: false,
        }
    }

    /// 便捷：从任意 `Path` 加载（不持有 path，适用于一次性读取/迁移）
    pub fn load_from(path: impl AsRef<Path>) -> YinheLayout {
        match fs::read(path.as_ref()) {
            Ok(bytes) => YinheLayout::from_json_slice(&bytes),
            Err(_) => YinheLayout::default(),
        }
    }

    /// 获取当前布局只读引用
    pub fn get(&self) -> &YinheLayout {
        &self.inner
    }

    /// 可变修改并标记 dirty（复用 `UiStateWrapper::patch` 语义）
    pub fn patch<F>(&mut self, f: F)
    where
        F: FnOnce(&mut YinheLayout),
    {
        f(&mut self.inner);
        self.inner.clamp();
        self.dirty = true;
    }

    /// 直接替换整个布局（内部 clamp 并标记 dirty）
    pub fn set(&mut self, layout: YinheLayout) {
        self.inner = layout.clamped();
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

    /// 当前持久化路径
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 强制标记 dirty（供外部在批量 patch 后统一 save）
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn default_clamped() {
        let l = YinheLayout::default().clamped();
        assert!((0.05..=0.95).contains(&l.arr_split));
        assert!((120.0..=800.0).contains(&l.right_panel_width));
    }

    #[test]
    fn clamp_out_of_range() {
        let mut l = YinheLayout {
            arr_split: -1.0,
            right_panel_width: 5000.0,
            show_pianoroll_in_arrange: true,
        };
        l.clamp();
        assert_eq!(l.arr_split, 0.05);
        assert_eq!(l.right_panel_width, 800.0);
    }

    #[test]
    fn serde_missing_fields_fallback() {
        let json = br#"{"arr_split": 0.3}"#;
        let l = YinheLayout::from_json_slice(json);
        assert!((l.arr_split - 0.3).abs() < 1e-6);
        assert_eq!(l.right_panel_width, default_right_panel_width());
        assert!(!l.show_pianoroll_in_arrange);
    }

    #[test]
    fn wrapper_patch_and_save_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "lumino_yinhe_layout_test_{}_{}",
            std::process::id(),
            42
        ));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join(YINHE_LAYOUT_FILE);
        let _ = fs::remove_file(&path);

        let mut w = YinheLayoutWrapper::new(path.clone());
        w.patch(|l| {
            l.arr_split = 0.7;
            l.right_panel_width = 400.0;
            l.show_pianoroll_in_arrange = true;
        });
        assert!(w.is_dirty());
        w.save().expect("save should succeed");

        let w2 = YinheLayoutWrapper::new(path.clone());
        assert!((w2.get().arr_split - 0.7).abs() < 1e-6);
        assert_eq!(w2.get().right_panel_width, 400.0);
        assert!(w2.get().show_pianoroll_in_arrange);

        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
        let _ = PathBuf::from("unused");
    }

    #[test]
    fn not_polluting_uistate() {
        // 仅断言 YinheLayout 不包含 UiState 的 5 字段（x/y/w/h/is_maximized），
        // 且文件名为 yinhe_layout.json 而非 ui_state.json
        assert_eq!(YINHE_LAYOUT_FILE, "yinhe_layout.json");
        let l = YinheLayout::default();
        let s = serde_json::to_string(&l).expect("serialize");
        assert!(!s.contains("is_maximized"));
        assert!(s.contains("arr_split"));
        assert!(s.contains("right_panel_width"));
        assert!(s.contains("show_pianoroll_in_arrange"));
    }
}
