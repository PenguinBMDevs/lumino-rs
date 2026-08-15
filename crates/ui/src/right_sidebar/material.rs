//! 素材库数据模型与加载逻辑
//!
//! 素材（.lmmaterial）= 带 `[material]` 元数据标记的单文件 LMPJ 归档。
//! 素材来源：
//! - 内置：编译期嵌入（`lumino_extras::embedded_materials`）；
//! - 用户：配置文件目录下的 `Materials/` 文件夹（导入时复制保存）。

use std::path::{Path, PathBuf};

use lumino_editor_state::{ImageToMidiPreview, PreviewNote};

/// 素材来源
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialSource {
    /// 编译期嵌入的内置素材
    BuiltIn,
    /// 用户配置目录中的本地素材
    User,
}

/// 素材库列表条目
#[derive(Debug, Clone)]
pub struct MaterialEntry {
    /// 素材名称（metadata.project.name，无则用文件名）
    pub name: String,
    /// 作者（metadata.project.author；素材导出时跟随工程设置面板填写署名）
    pub author: String,
    /// 素材来源（内置 / 本地）
    pub source: MaterialSource,
    /// 用户素材的磁盘路径（内置素材为 None）
    pub path: Option<PathBuf>,
    /// 内置素材的原始字节（用户素材为 None）
    pub data: Option<&'static [u8]>,
    /// 是否多轨素材
    pub multi_track: bool,
    /// 音轨数量（单轨 = 1，解析失败 = 0）
    pub track_count: usize,
    /// 解析是否有效（无效素材置灰显示）
    pub valid: bool,
    /// 放置预览缓存（扫描时预解析；拖出时同步使用，零延迟）
    pub preview: Option<ImageToMidiPreview>,
}

impl MaterialEntry {
    /// 内置素材条目（懒解析：音轨数在扫描时填充）
    pub fn built_in(embedded: &lumino_extras::EmbeddedMaterial) -> Self {
        Self {
            name: embedded.name.to_string(),
            author: String::new(),
            source: MaterialSource::BuiltIn,
            path: None,
            data: Some(embedded.data),
            multi_track: false,
            track_count: 0,
            valid: true,
            preview: None,
        }
    }

    /// 用户素材条目（按路径）
    pub fn user(path: PathBuf) -> Self {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "未知素材".into());
        Self {
            name,
            author: String::new(),
            source: MaterialSource::User,
            path: Some(path),
            data: None,
            multi_track: false,
            track_count: 0,
            valid: true,
            preview: None,
        }
    }

    /// 从原始字节解析元数据并填充音轨信息（内置素材用）
    pub fn resolve_built_in(&mut self) {
        let Some(data) = self.data else {
            return;
        };
        let project = match lumino_project::project::load::load_project_from_bytes(data) {
            Ok(project) => project,
            Err(e) => {
                tracing::warn!("内置素材 {} 解析失败: {e}", self.name);
                self.valid = false;
                self.author.clear();
                self.multi_track = false;
                self.track_count = 0;
                self.preview = None;
                return;
            }
        };
        self.apply_project_meta(&project);
    }

    /// 从磁盘路径解析元数据并填充音轨信息（用户素材用）
    pub fn resolve_user(&mut self) {
        let Some(path) = &self.path else {
            return;
        };
        let project = match lumino_export::load_project(path) {
            Ok(project) => project,
            Err(e) => {
                tracing::warn!("本地素材 {} 解析失败: {e}", path.display());
                self.valid = false;
                self.author.clear();
                self.multi_track = false;
                self.track_count = 0;
                self.preview = None;
                return;
            }
        };
        self.apply_project_meta(&project);
    }

    /// 应用工程元数据（名称 / 作者 / 多轨标记 / 音轨数）并预解析放置预览
    fn apply_project_meta(&mut self, project: &lumino_project::LuminoProject) {
        let meta = &project.metadata;
        // 作者：跟随工程设置面板填写（素材导出时已写入 metadata.project.author）
        self.author = meta.project.author.clone();
        // 名称优先使用素材 metadata 中的名字
        if meta.is_material_file() {
            self.name = meta.project.name.clone();
            self.multi_track = meta
                .material
                .as_ref()
                .map(|m| m.multi_track)
                .unwrap_or(false);
            self.track_count = meta
                .material_track_count()
                .max(project.loaded_track_count());
        } else {
            self.multi_track = project.loaded_track_count() > 1;
            self.track_count = project.loaded_track_count();
        }
        self.valid = true;
        // 预解析放置预览：拖出时同步使用，避免异步加载导致的拖放时序缺陷
        self.preview = Some(project_to_material_preview(project));
    }
}

/// 素材库状态
#[derive(Debug, Clone, Default)]
pub struct MaterialLibrary {
    /// 素材列表（内置在前，本地在后）
    pub entries: Vec<MaterialEntry>,
    /// "添加素材"下拉菜单是否展开
    pub add_menu_open: bool,
    /// 是否正在扫描/解析素材
    pub scanning: bool,
    /// 右键菜单打开的素材列表索引（None = 菜单关闭）
    pub context_menu_target: Option<usize>,
    /// 右键菜单弹出位置（窗口逻辑坐标，由 Host 在消息处理时注入）
    pub context_menu_pos: Option<(f32, f32)>,
    /// 正在重命名的素材（列表索引 + 当前输入值）
    pub renaming_material: Option<(usize, String)>,
    /// 等待删除确认的素材列表索引
    pub pending_delete: Option<usize>,
}

impl MaterialLibrary {
    /// 是否已初始化（首次打开素材库面板时惰性扫描）
    pub fn is_initialized(&self) -> bool {
        !self.entries.is_empty() || self.scanning
    }

    /// 设置右键菜单弹出位置（Host 捕获鼠标坐标后注入）
    pub fn set_context_menu_pos(&mut self, x: f32, y: f32) {
        self.context_menu_pos = Some((x, y));
    }
}

/// 扫描全部素材（内置 + 用户配置目录），返回列表
///
/// 内置素材来自编译期嵌入；用户素材来自 `config_dir()/Materials/*.lmmaterial`。
/// 解析失败的条目保留名称并标记 `valid = false`（面板置灰显示）。
pub fn scan_materials(user_dir: &Path) -> Vec<MaterialEntry> {
    let mut entries: Vec<MaterialEntry> = Vec::new();

    // 内置素材（编译期嵌入）
    for embedded in lumino_extras::embedded_materials() {
        let mut entry = MaterialEntry::built_in(embedded);
        entry.resolve_built_in();
        entries.push(entry);
    }

    // 用户素材（配置目录 Materials 文件夹）
    if user_dir.is_dir() {
        let mut user_paths: Vec<PathBuf> = std::fs::read_dir(user_dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension()
                            .and_then(|ext| ext.to_str())
                            .map(|ext| ext.eq_ignore_ascii_case("lmmaterial"))
                            .unwrap_or(false)
                    })
                    .collect()
            })
            .unwrap_or_default();
        user_paths.sort();
        for path in user_paths {
            let mut entry = MaterialEntry::user(path);
            entry.resolve_user();
            entries.push(entry);
        }
    }

    entries
}

/// 将工程转换为素材放置预览（每轨音符列表 + 总宽度）
///
/// 通过 `to_midi_document` 提取各轨音符（自动化等工程数据不参与放置预览，
/// 但素材文件本身完整保留）。
pub fn project_to_material_preview(project: &lumino_project::LuminoProject) -> ImageToMidiPreview {
    let Ok(doc) = project.to_midi_document() else {
        return ImageToMidiPreview::default();
    };
    let mut tracks: Vec<Vec<PreviewNote>> = Vec::new();
    let mut orig_width: f32 = 0.0;
    for track_idx in 0..doc.track_count() {
        let notes = doc.track_notes(track_idx);
        if notes.is_empty() {
            continue;
        }
        let preview: Vec<PreviewNote> = notes
            .iter()
            .map(|n| PreviewNote {
                tick: n.start_tick as f32,
                length: (n.end_tick - n.start_tick) as f32,
                key: n.key,
            })
            .collect();
        for note in &preview {
            orig_width = orig_width.max(note.tick + note.length);
        }
        tracks.push(preview);
    }
    ImageToMidiPreview {
        tracks,
        orig_width: orig_width.max(1.0),
    }
}

/// 获取用户素材目录（应用程序配置文件目录下的 Materials 文件夹）
///
/// 与 `src/storage.rs::config_dir()`（directories::ProjectDirs）保持一致：
/// - Windows: `%APPDATA%\PenguinBMDevs\lumino\Materials`
/// - macOS: `~/Library/Application Support/com.PenguinBMDevs.lumino/Materials`
/// - Linux: `~/.config/PenguinBMDevs/lumino/Materials`
pub fn user_materials_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| {
                if cfg!(target_os = "macos") {
                    PathBuf::from(h).join("Library").join("Application Support")
                } else {
                    PathBuf::from(h).join(".config")
                }
            })
        })
        .unwrap_or_else(|| PathBuf::from("."));
    let project_path = if cfg!(target_os = "macos") {
        PathBuf::from("com.PenguinBMDevs.lumino")
    } else {
        PathBuf::from("PenguinBMDevs").join("lumino")
    };
    base.join(project_path).join("Materials")
}

/// 将素材归档复制到用户素材目录（导入后保存）
pub fn copy_material_to_user_dir(source: &Path, user_dir: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(user_dir)?;
    let file_name = source
        .file_name()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "无效的文件名"))?;
    let dest = user_dir.join(file_name);
    std::fs::copy(source, &dest)?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_materials_returns_list() {
        // 使用不存在的目录：仅返回内置素材（编译期嵌入列表，允许为空）
        let entries = scan_materials(Path::new("__nonexistent_dir__"));
        for entry in &entries {
            if entry.valid {
                // 有效内置素材应具有非空字节
                assert!(entry.data.is_some());
            }
        }
        // 内置素材必然位于列表最前
        if let Some(first) = entries.first() {
            assert_eq!(first.source, MaterialSource::BuiltIn);
        }
    }

    #[test]
    fn test_user_entry_name_from_stem() {
        let entry = MaterialEntry::user(PathBuf::from("/tmp/foo.lmmaterial"));
        assert_eq!(entry.name, "foo");
        // 作者初始为空，解析元数据后填充（跟随工程设置面板署名）
        assert_eq!(entry.author, "");
        assert_eq!(entry.source, MaterialSource::User);
        assert_eq!(
            entry.path.as_deref(),
            Some(Path::new("/tmp/foo.lmmaterial"))
        );
    }
}
