//! Build script: 自动检测 resources/palettes/ 目录下的调色板文件并嵌入
//!
//! 扫描 `CARGO_MANIFEST_DIR/../../resources/palettes/` 下的所有 .png 文件，
//! 将每个 PNG **复制到 $OUT_DIR**，并生成 `$OUT_DIR/palettes.rs`，
//! 其中以**相对文件名**（`include_bytes!("xxx.png")`）引用它们。
//!
//! 关键设计决策：
//! - **不硬编码绝对路径**：include_bytes! 相对路径相对于生成文件
//!   （$OUT_DIR/palettes.rs）解析，指向同目录下的 PNG —— 任何机器、
//!   任何平台编译都指向自己 checkout 里的资源，构建产物可移植。
//!   此前用 canonicalize() 绝对路径，Windows 会生成 `\\?\`/`//?/` verbatim
//!   前缀的编译机专属路径，跨平台/缓存复用场景必然失效（CI 挂过的坑）。
//! - cargo:rerun-if-changed 声明目录与文件，资源变化时自动重编译
//! - 不嵌入文件名列表：生成的代码包含文件名字符串常量

use std::fs;
use std::io;
use std::path::Path;

fn main() -> io::Result<()> {
    // ── 调色板目录 ─────────────────────────────────────────────────────────
    // CARGO_MANIFEST_DIR = crates/extras/
    // 相对于 workspace root = ../../resources/palettes/
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR 未设置");
    let palettes_dir = Path::new(&manifest_dir)
        .join("..")
        .join("..")
        .join("resources")
        .join("palettes");

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
