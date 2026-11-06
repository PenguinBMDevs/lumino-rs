//! `ui-diff`：两张 PNG 的逐像素差异报告与红/蓝热图。
//!
//! 示例：
//! `ui-diff --a old.png --b new.png --out diff.png --color-tolerance 2 --geometry-tolerance 1`

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Parser;
use lumino_ui_parity::{CompareOptions, compare_images};
use serde_json::json;

/// 命令行参数。
#[derive(Debug, Parser)]
#[command(
    name = "ui-diff",
    about = "对比两张 UI 截图的逐像素差异，并输出红/蓝热图",
    version
)]
struct Args {
    /// 图片 A（旧基线）。
    #[arg(long)]
    a: PathBuf,
    /// 图片 B（新渲染结果）。
    #[arg(long)]
    b: PathBuf,
    /// 差异热图输出路径（PNG）。
    #[arg(long)]
    out: PathBuf,
    /// 单通道颜色容差（0..=255）。
    #[arg(long, default_value_t = 2)]
    color_tolerance: u8,
    /// 全局平移容差（0..=4）。
    #[arg(long, default_value_t = 1)]
    geometry_tolerance: u8,
    /// 存在超容差像素时以非零退出码失败（CI 用）。
    #[arg(long)]
    fail_on_diff: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let options = CompareOptions {
        color_tolerance: args.color_tolerance,
        geometry_tolerance: args.geometry_tolerance,
    };

    let result = match compare_images(&args.a, &args.b, &options) {
        Ok(result) => result,
        Err(error) => {
            let report = json!({
                "a": args.a.display().to_string(),
                "b": args.b.display().to_string(),
                "matched": false,
                "error": error.to_string(),
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Err(error);
        }
    };

    if let Some(parent) = args.out.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建差异图目录失败: {}", parent.display()))?;
    }
    result
        .diff
        .save_with_format(&args.out, image::ImageFormat::Png)
        .with_context(|| format!("写入差异图失败: {}", args.out.display()))?;

    let mut report = serde_json::to_value(&result.report)?;
    if let Some(object) = report.as_object_mut() {
        object.insert("a".into(), json!(args.a.display().to_string()));
        object.insert("b".into(), json!(args.b.display().to_string()));
        object.insert("out".into(), json!(args.out.display().to_string()));
    }
    println!("{}", serde_json::to_string_pretty(&report)?);

    if args.fail_on_diff && !result.report.matched {
        bail!(
            "存在 {} 个超容差像素（占比 {:.6}）",
            result.report.pixels_over_tolerance,
            result.report.pixels_over_tolerance_ratio
        );
    }

    Ok(())
}
