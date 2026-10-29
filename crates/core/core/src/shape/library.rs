//! 形状库（DRAW-033A）
//!
//! 形状库与工程 `.lmshapes` 解耦：
//! - 内置三形在代码里生成；
//! - 用户图形保存为 `{shapes_dir}/*.lmshape` 的 JSON 文件；
//! - 坏文件跳过并回传 [`LibraryWarning`]，不阻断其余图形加载；
//! - 写入使用 `lumino_core::io::atomic`，中断不会留下半截目标文件。
//!
//! 库内坐标是单位框 0..=1；工程侧通过 `ShapeDrawing::to_model` 映射，
//! 由 `ShapeOp::Drawing` 展开为多笔画，不新增 `ShapeModel` 字段。
use std::path::{Path, PathBuf};

pub mod builtin;
mod drawing;
mod format;

pub use builtin::{BuiltinShape, builtin_drawings};
pub use drawing::ShapeDrawing;
pub use format::{
    EXTENSION, MAGIC, MAX_BYTES, VERSION, decode_drawing, delete_drawing, drawing_path,
    encode_drawing, save_drawing,
};

/// 加载某个图形文件失败时的可读提示
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryWarning {
    /// 出错/被跳过的路径
    pub path: PathBuf,
    /// 可读原因（不 panic）
    pub message: String,
}

/// 形状库：内置 + 用户图形
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeLibrary {
    /// 内置三形（Circle / Square / Triangle）
    pub builtin: Vec<ShapeDrawing>,
    /// 用户目录扫描结果（坏文件已被跳过）
    pub user: Vec<ShapeDrawing>,
}

impl ShapeLibrary {
    /// 只含内置图形
    pub fn with_builtins() -> Self {
        Self {
            builtin: builtin_drawings(),
            user: Vec::new(),
        }
    }

    /// 扫描用户目录并合并内置图形；目录不存在视为空库，不报错。
    pub fn load_dir(dir: &Path) -> (Self, Vec<LibraryWarning>) {
        let builtin = builtin_drawings();
        let (mut user, mut warnings) = format::load_user_dir(dir);
        // 用户图形 id 与内置 id 撞车时（例如把内置 Square 另存为 333），
        // 不能靠 `find` 的先后顺序猜；加载时就直接改成用户唯一 id。
        for index in 0..user.len() {
            let id = user[index].id.clone();
            if !builtin.iter().any(|builtin| builtin.id == id) {
                continue;
            }
            let mut suffix = 2u32;
            let unique = loop {
                let candidate = format!("{id}-{suffix}");
                let taken = builtin.iter().any(|shape| shape.id == candidate)
                    || user.iter().any(|shape| shape.id == candidate);
                if !taken {
                    break candidate;
                }
                suffix = suffix.saturating_add(1);
            };
            let old_path = format::drawing_path(dir, &id);
            warnings.push(LibraryWarning {
                path: old_path.clone(),
                message: format!("形状 id 与内置图形冲突，已重命名为 {unique}"),
            });
            user[index].id = unique;
            // 迁移旧文件；失败时保留内存 id，至少本次会话可正常选中。
            if format::save_drawing(dir, &user[index]).is_ok() {
                let _ = std::fs::remove_file(&old_path);
            }
        }
        (Self { builtin, user }, warnings)
    }

    /// 按 id 查找（先内置、后用户）
    pub fn find(&self, id: &str) -> Option<&ShapeDrawing> {
        self.builtin
            .iter()
            .chain(self.user.iter())
            .find(|drawing| drawing.id == id)
    }

    /// 全部图形（内置在前，用户在后的展示顺序）
    pub fn all(&self) -> impl Iterator<Item = &ShapeDrawing> {
        self.builtin.iter().chain(self.user.iter())
    }
}

/// 扫描目录的便捷入口（与 [`ShapeLibrary::load_dir`] 同义）
pub fn load_dir(dir: &Path) -> (ShapeLibrary, Vec<LibraryWarning>) {
    ShapeLibrary::load_dir(dir)
}

#[cfg(test)]
mod tests;
