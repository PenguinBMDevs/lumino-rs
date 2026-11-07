//! 对比 iced 基线与 egui 草稿图，输出统计 JSON 与差异热图。

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Parser;
use image::{Rgba, RgbaImage};
use lumino_ui_egui::scene::{DPI_SCALES, SceneId, baseline_file_name};

#[derive(Debug, Parser)]
#[command(name = "compare_egui", about = "对比 iced 与 egui 草稿基线")]
struct Args {
    /// iced 基线目录。
    #[arg(long, default_value = "tests/ui-baseline/iced")]
    iced_dir: PathBuf,
    /// egui 草稿目录。
    #[arg(long, default_value = "tests/ui-baseline/egui")]
    egui_dir: PathBuf,
    /// 报告输出目录。
    #[arg(long, default_value = "target/ui-parity-reports")]
    out_dir: PathBuf,
    /// 单通道颜色容差。
    #[arg(long, default_value_t = 2)]
    color_tolerance: u8,
    /// 存在超容差像素时返回非零。
    #[arg(long)]
    fail: bool,
}

#[derive(Debug, serde::Serialize)]
struct Report {
    scene: String,
    dpi: f32,
    width: u32,
    height: u32,
    different_pixels: u64,
    over_tolerance: u64,
    total_pixels: u64,
    ratio: f64,
    max_channel_delta: u8,
    mean_channel_delta: f64,
    matched: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    std::fs::create_dir_all(&args.out_dir)
        .with_context(|| format!("创建报告目录失败: {}", args.out_dir.display()))?;

    let mut reports = Vec::new();
    let mut missing = Vec::new();

    for scene in SceneId::ALL {
        for dpi in DPI_SCALES {
            let name = baseline_file_name(scene, dpi);
            let iced_path = args.iced_dir.join(&name);
            let egui_path = args.egui_dir.join(&name);
            if !iced_path.exists() || !egui_path.exists() {
                missing.push(name);
                continue;
            }

            let iced = image::open(&iced_path)
                .with_context(|| format!("读取 iced 基线失败: {}", iced_path.display()))?
                .to_rgba8();
            let egui = image::open(&egui_path)
                .with_context(|| format!("读取 egui 草稿失败: {}", egui_path.display()))?
                .to_rgba8();

            if iced.dimensions() != egui.dimensions() {
                bail!(
                    "{} 尺寸不一致: iced {:?}, egui {:?}",
                    name,
                    iced.dimensions(),
                    egui.dimensions()
                );
            }

            let (report, diff) = compare(&scene, dpi, &iced, &egui, args.color_tolerance);
            let diff_path = args.out_dir.join(format!("{name}.diff.png"));
            image::save_buffer(
                &diff_path,
                diff.as_raw(),
                diff.width(),
                diff.height(),
                image::ColorType::Rgba8,
            )
            .with_context(|| format!("写入差异图失败: {}", diff_path.display()))?;

            let json_path = args.out_dir.join(format!("{name}.json"));
            std::fs::write(&json_path, serde_json::to_vec_pretty(&report)?)
                .with_context(|| format!("写入报告失败: {}", json_path.display()))?;
            reports.push(report);
        }
    }

    println!(
        "{:<24} {:>5} {:>12} {:>12} {:>10} {:>8} {:>8}",
        "scene", "dpi", "different", "over_tol", "ratio", "max", "mean"
    );
    for report in &reports {
        println!(
            "{:<24} {:>5.2} {:>12} {:>12} {:>10.4} {:>8} {:>8.2}",
            report.scene,
            report.dpi,
            report.different_pixels,
            report.over_tolerance,
            report.ratio,
            report.max_channel_delta,
            report.mean_channel_delta
        );
    }
    if !missing.is_empty() {
        println!("缺少图片 {} 张: {:?}", missing.len(), missing);
    }

    if args.fail && reports.iter().any(|report| !report.matched) {
        bail!("存在超容差像素的对比项");
    }
    Ok(())
}

fn compare(
    scene: &SceneId,
    dpi: f32,
    iced: &RgbaImage,
    egui: &RgbaImage,
    tolerance: u8,
) -> (Report, RgbaImage) {
    let (width, height) = iced.dimensions();
    let total = u64::from(width) * u64::from(height);
    let mut different = 0_u64;
    let mut over = 0_u64;
    let mut max_delta = 0_u8;
    let mut sum_delta = 0_u64;
    let mut diff = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 255]));

    for (x, y, pixel_iced) in iced.enumerate_pixels() {
        let pixel_egui = egui.get_pixel(x, y);
        let mut pixel_delta = 0_u8;
        for channel in 0..3 {
            let a = pixel_iced[channel];
            let b = pixel_egui[channel];
            let delta = a.abs_diff(b);
            pixel_delta = pixel_delta.max(delta);
            sum_delta += u64::from(delta);
        }
        max_delta = max_delta.max(pixel_delta);
        if pixel_delta > 0 {
            different += 1;
        }
        if pixel_delta > tolerance {
            over += 1;
        }

        let out = if pixel_delta == 0 {
            Rgba([0, 0, 0, 255])
        } else if pixel_iced[0] as u16 + pixel_iced[1] as u16 + pixel_iced[2] as u16
            >= pixel_egui[0] as u16 + pixel_egui[1] as u16 + pixel_egui[2] as u16
        {
            let strength = (pixel_delta.saturating_mul(4)).max(32);
            Rgba([strength, 0, 0, 255])
        } else {
            let strength = (pixel_delta.saturating_mul(4)).max(32);
            Rgba([0, 0, strength, 255])
        };
        diff.put_pixel(x, y, out);
    }

    let report = Report {
        scene: scene.slug().to_owned(),
        dpi,
        width,
        height,
        different_pixels: different,
        over_tolerance: over,
        total_pixels: total,
        ratio: over as f64 / total.max(1) as f64,
        max_channel_delta: max_delta,
        mean_channel_delta: sum_delta as f64 / (total.max(1) * 3) as f64,
        matched: over == 0,
    };
    (report, diff)
}
