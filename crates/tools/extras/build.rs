//! Build script: 自动检测资源目录下的调色板与素材文件并嵌入
//!
//! 1. 扫描 `resources/palettes/` 目录下的所有 .png 文件 → 生成 `$OUT_DIR/palettes.rs`
//! 2. 扫描 `resources/Materials/` 目录下的所有 .lmmaterial 文件 → 生成 `$OUT_DIR/materials.rs`
//!
//! 两类资源共用同一套「扫描 → 稳定排序 → 复制到 OUT_DIR → 生成 include_bytes! 清单」
//! 实现（[`build_embedded`]），差异见 [`ResourceKind`]。
//!
//! 嵌入方式与设计决策见下方各节注释（include_bytes! 相对路径，
//! cargo:rerun-if-changed 声明目录与文件）。

use std::fs;
use std::io;
use std::path::Path;

/// 读取构建环境变量；缺失时返回 io::Error（供 `?` 传播，构建失败由 cargo 报告）。
fn env_or_err(key: &str) -> io::Result<String> {
    std::env::var(key).map_err(|_| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("构建环境变量 {key} 未设置（cargo 应始终注入）"),
        )
    })
}

/// 一类嵌入资源（调色板 / 素材）的清单描述。
struct ResourceKind {
    /// `resources/` 下的子目录名
    dir: &'static str,
    /// 只收集该扩展名（大小写不敏感）
    extension: &'static str,
    /// 生成代码引用的元素类型名
    type_name: &'static str,
    /// 生成文件名
    output: &'static str,
    /// `cargo:info` 前缀
    label: &'static str,
    /// 状态文案里的条目称呼
    noun: &'static str,
    /// 目录缺失时打印的告警前缀（`None` = 静默创建）
    missing_warning: Option<&'static str>,
}

const PALETTES: ResourceKind = ResourceKind {
    dir: "palettes",
    extension: "png",
    type_name: "EmbeddedPalette",
    output: "palettes.rs",
    label: "palettes",
    noun: "调色板文件",
    missing_warning: Some("调色板目录不存在，创建"),
};

const MATERIALS: ResourceKind = ResourceKind {
    dir: "Materials",
    extension: "lmmaterial",
    type_name: "EmbeddedMaterial",
    output: "materials.rs",
    label: "materials",
    noun: "素材文件",
    missing_warning: None,
};

fn main() -> io::Result<()> {
    let manifest_dir = env_or_err("CARGO_MANIFEST_DIR")?;
    let resources_dir = Path::new(&manifest_dir)
        .join("..")
        .join("..")
        .join("..")
        .join("resources");

    build_embedded(&resources_dir, &PALETTES)?;
    build_embedded(&resources_dir, &MATERIALS)?;
    Ok(())
}

// ── 共用生成器 ─────────────────────────────────────────────────────────────

/// 扫描 `resources/{kind.dir}/` 下指定扩展名的文件，复制到 `OUT_DIR` 并生成
/// 嵌入清单 `{kind.output}`。
///
/// 生成代码的形状（以调色板为例）：
///
/// ```text
/// [
///     EmbeddedPalette {
///         name: "<文件名（去扩展名）>",
///         data: include_bytes!("<文件名>"),
///     },
/// ]
/// ```
///
/// 文件复制到 OUT_DIR 后用相对路径引用，与任何绝对路径解耦；条目按文件名
/// 稳定排序，确保编译结果确定性。
fn build_embedded(resources_dir: &Path, kind: &ResourceKind) -> io::Result<()> {
    let dir = resources_dir.join(kind.dir);

    println!("cargo:rerun-if-changed={}", dir.display());

    // 目录不存在时创建（调色板历史上有告警，素材静默）
    if !dir.exists() {
        if let Some(warning) = kind.missing_warning {
            println!("cargo:warning={warning}: {}", dir.display());
        }
        fs::create_dir_all(&dir)?;
        return write_empty_embedded(kind);
    }

    // ── 收集指定扩展名的文件 ──────────────────────────────────────────────
    let mut entries: Vec<_> = fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case(kind.extension))
                .unwrap_or(false)
        })
        .collect();

    // 稳定排序：按文件名排序（确保编译结果确定性）
    entries.sort_by_key(|e| e.file_name());

    // ── 复制文件到 OUT_DIR 并生成 Rust 代码 ───────────────────────────────
    let out_dir = env_or_err("OUT_DIR")?;
    let out_path = Path::new(&out_dir);
    let output_path = out_path.join(kind.output);

    let mut output = String::new();
    output.push_str("// !!! 自动生成 - 请勿手动修改 !!!\n");
    output.push_str(&format!(
        "// 由 build.rs 在编译时根据 resources/{}/ 目录生成\n\n",
        kind.dir
    ));
    output.push_str("[\n");

    for entry in &entries {
        let path = entry.path();
        let file_name = entry.file_name();
        let name_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        // 复制到 OUT_DIR：include_bytes! 相对路径（相对于本生成文件）
        // 直接指向 OUT_DIR 根下的同名文件，与任何绝对路径解耦
        fs::copy(&path, out_path.join(&file_name))?;
        println!("cargo:rerun-if-changed={}", path.display());

        output.push_str(&format!("    {} {{\n", kind.type_name));
        output.push_str(&format!("        name: \"{}\",\n", escape_str(&name_stem)));
        output.push_str(&format!(
            "        data: include_bytes!(\"{}\"),\n",
            escape_str(&file_name.to_string_lossy())
        ));
        output.push_str("    },\n");
    }

    output.push_str("]\n");

    fs::write(&output_path, output)?;

    // ── 打印状态 ───────────────────────────────────────────────────────────
    println!(
        "cargo:info={}: 检测到 {} 个{}",
        kind.label,
        entries.len(),
        kind.noun
    );
    for entry in &entries {
        let name = entry.file_name();
        println!("cargo:info=  - {}", name.to_string_lossy());
    }

    Ok(())
}

/// 当资源目录不存在或为空时，生成空的嵌入列表
fn write_empty_embedded(kind: &ResourceKind) -> io::Result<()> {
    let out_dir = env_or_err("OUT_DIR")?;
    let output_path = std::path::Path::new(&out_dir).join(kind.output);

    let output = format!(
        "// !!! 自动生成 - 请勿手动修改 !!!\n// 由 build.rs 生成（空目录）\n\n#[allow(unused_imports)]\nuse super::{};\n\n&[]\n",
        kind.type_name
    );

    fs::write(&output_path, output)?;
    println!("cargo:info={}: 目录为空，无{}", kind.label, kind.noun);
    Ok(())
}

/// 转义字符串字面量中的特殊字符，防止文件名含 `"` / `\` 时生成非法代码
fn escape_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
