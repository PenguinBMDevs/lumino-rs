//! 生成 / 校验 egui 草稿基线 PNG。

use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Parser;
use lumino_ui_egui::capture::capture_scene;
use lumino_ui_egui::scene::{DPI_SCALES, SceneId, baseline_dir, baseline_file_name};

#[derive(Debug, Parser)]
#[command(name = "capture_egui", about = "生成或校验 egui 草稿视觉基线")]
struct Args {
    /// 输出目录。
    #[arg(long)]
    out_dir: Option<PathBuf>,
    /// 只渲染指定场景（main_chrome / settings_general / save_confirm_dialog / piano_roll_skeleton）。
    #[arg(long)]
    scene: Option<String>,
    /// 只渲染指定 DPI（1.0 / 1.25 / 1.5）。
    #[arg(long)]
    dpi: Option<f32>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let out_dir = args.out_dir.unwrap_or_else(baseline_dir);
    println!("egui 草稿输出目录: {}", out_dir.display());

    let mut created = 0_usize;
    let mut verified = 0_usize;
    let mut mismatched = Vec::new();

    for scene in SceneId::ALL {
        if let Some(filter) = &args.scene
            && filter != scene.slug()
        {
            continue;
        }
        for dpi in DPI_SCALES {
            if let Some(filter) = args.dpi
                && (filter - dpi).abs() > f32::EPSILON
            {
                continue;
            }
            let path = out_dir.join(baseline_file_name(scene, dpi));
            let captured = capture_scene(scene, dpi)?;
            let existed = path.exists();
            let matched = captured.matches_image(&path)?;
            if matched {
                if existed {
                    verified += 1;
                    println!(
                        "[ok]       {} ({}x{} @ {:.2}x)",
                        path.display(),
                        captured.width,
                        captured.height,
                        dpi
                    );
                } else {
                    created += 1;
                    println!(
                        "[created]  {} ({}x{} @ {:.2}x)",
                        path.display(),
                        captured.width,
                        captured.height,
                        dpi
                    );
                }
            } else {
                mismatched.push(path.clone());
                eprintln!(
                    "[mismatch] {} ({}x{} @ {:.2}x)",
                    path.display(),
                    captured.width,
                    captured.height,
                    dpi
                );
            }
        }
    }

    println!(
        "完成：新建 {created} 张，校验通过 {verified} 张，失败 {} 张",
        mismatched.len()
    );
    if !mismatched.is_empty() {
        bail!("{} 张 egui 草稿图与已有文件不一致", mismatched.len());
    }
    Ok(())
}
