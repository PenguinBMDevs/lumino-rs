//! Build script: 自动检测资源目录下的调色板与素材文件并嵌入
//!
//! 1. 扫描 `resources/palettes/` 目录下的所有 .png 文件 → 生成 `$OUT_DIR/palettes.rs`
//! 2. 扫描 `resources/Materials/` 目录下的所有 .lmmaterial 文件 → 生成 `$OUT_DIR/materials.rs`
//!
//! 嵌入方式与设计决策见下方各节注释（include_bytes! 相对路径，
//! cargo:rerun-if-changed 声明目录与文件）。

use std::fs;
use std::io;
use std::path::Path;

fn main() -> io::Result<()> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR 未设置");
    let resources_dir = Path::new(&manifest_dir)
        .join("..")
        .join("..")
        .join("..")
        .join("resources");

    build_palettes(&resources_dir)?;
    build_materials(&resources_dir)?;
    Ok(())
}

// ── 调色板 ─────────────────────────────────────────────────────────────────

fn build_palettes(resources_dir: &Path) -> io::Result<()> {
    let palettes_dir = resources_dir.join("palettes");

    println!("cargo:rerun-if-changed={}", palettes_dir.display());

    // 如果目录不存在，创建它
    if !palettes_dir.exists() {
        println!(
            "cargo:warning=调色板目录不存在，创建: {}",
            palettes_dir.display()
        );
        fs::create_dir_all(&palettes_dir)?;
        write_empty_palettes()?;
        return Ok(());
    }

    // ── 收集所有 PNG 文件 ──────────────────────────────────────────────────
    let mut entries: Vec<_> = fs::read_dir(&palettes_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("png"))
                .unwrap_or(false)
        })
        .collect();

    // 稳定排序：按文件名排序（确保编译结果确定性）
    entries.sort_by_key(|e| e.file_name());

    // ── 复制 PNG 到 OUT_DIR 并生成 Rust 代码 ─────────────────────────────
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR 未设置");
    let out_path = Path::new(&out_dir);
    let output_path = out_path.join("palettes.rs");

    let mut output = String::new();
    output.push_str("// !!! 自动生成 - 请勿手动修改 !!!\n");
    output.push_str("// 由 build.rs 在编译时根据 resources/palettes/ 目录生成\n\n");
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

        output.push_str("    EmbeddedPalette {\n");
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
    println!("cargo:info=palettes: 检测到 {} 个调色板文件", entries.len());
    for entry in &entries {
        let name = entry.file_name();
        println!("cargo:info=  - {}", name.to_string_lossy());
    }

    Ok(())
}

// ── 素材 ───────────────────────────────────────────────────────────────────

/// 扫描 `resources/Materials/` 目录下的 .lmmaterial 文件并生成 `$OUT_DIR/materials.rs`
///
/// 与调色板相同的嵌入策略：文件复制到 OUT_DIR，生成代码用相对路径
/// `include_bytes!("xxx.lmmaterial")` 引用；`name` 为文件名（不含扩展名）。
fn build_materials(resources_dir: &Path) -> io::Result<()> {
    let materials_dir = resources_dir.join("Materials");

    println!("cargo:rerun-if-changed={}", materials_dir.display());

    // 目录不存在时创建空目录（不生成错误）
    if !materials_dir.exists() {
        fs::create_dir_all(&materials_dir)?;
        write_empty_materials()?;
        return Ok(());
    }

    let mut entries: Vec<_> = fs::read_dir(&materials_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("lmmaterial"))
                .unwrap_or(false)
        })
        .collect();

    // 稳定排序：按文件名排序（确保编译结果确定性）
    entries.sort_by_key(|e| e.file_name());

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR 未设置");
    let out_path = Path::new(&out_dir);
    let output_path = out_path.join("materials.rs");

    let mut output = String::new();
    output.push_str("// !!! 自动生成 - 请勿手动修改 !!!\n");
    output.push_str("// 由 build.rs 在编译时根据 resources/Materials/ 目录生成\n\n");
    output.push_str("[\n");

    for entry in &entries {
        let path = entry.path();
        let file_name = entry.file_name();
        let name_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        fs::copy(&path, out_path.join(&file_name))?;
        println!("cargo:rerun-if-changed={}", path.display());

        output.push_str("    EmbeddedMaterial {\n");
        output.push_str(&format!("        name: \"{}\",\n", escape_str(&name_stem)));
        output.push_str(&format!(
            "        data: include_bytes!(\"{}\"),\n",
            escape_str(&file_name.to_string_lossy())
        ));
        output.push_str("    },\n");
    }

    output.push_str("]\n");

    fs::write(&output_path, output)?;

    println!("cargo:info=materials: 检测到 {} 个素材文件", entries.len());
    for entry in &entries {
        let name = entry.file_name();
        println!("cargo:info=  - {}", name.to_string_lossy());
    }

    Ok(())
}

/// 当素材目录不存在或为空时，生成空的素材列表
fn write_empty_materials() -> io::Result<()> {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR 未设置");
    let output_path = std::path::Path::new(&out_dir).join("materials.rs");

    let output = "// !!! 自动生成 - 请勿手动修改 !!!\n// 由 build.rs 生成（空目录）\n\n#[allow(unused_imports)]\nuse super::EmbeddedMaterial;\n\n&[]\n";

    fs::write(&output_path, output)?;
    println!("cargo:info=materials: 目录为空，无素材文件");
    Ok(())
}

/// 转义字符串字面量中的特殊字符，防止文件名含 `"` / `\` 时生成非法代码
fn escape_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// 当目录不存在或为空时，生成空的调色板列表
fn write_empty_palettes() -> io::Result<()> {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR 未设置");
    let output_path = std::path::Path::new(&out_dir).join("palettes.rs");

    let output = "// !!! 自动生成 - 请勿手动修改 !!!\n// 由 build.rs 生成（空目录）\n\n#[allow(unused_imports)]\nuse super::EmbeddedPalette;\n\n&[]\n";

    fs::write(&output_path, output)?;
    println!("cargo:info=palettes: 目录为空，无调色板文件");
    Ok(())
}
