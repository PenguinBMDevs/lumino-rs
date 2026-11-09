//! `capture_iced`：生成 / 校验 iced 视觉基线 PNG。
//!
//! 无参数运行即输出 4 个场景 × 3 档 DPI（共 12 张）到
//! `tests/ui-baseline/iced/`。文件已存在时逐字节比对，不一致则打印路径并以
//! 非零退出码失败（用于 CI / 重构验收）。

use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Parser;
use lumino_ui_parity::{DPI_SCALES, Scene, SceneId, baseline_dir, capture::capture_scene};

/// 命令行参数。
#[derive(Debug, Parser)]
#[command(
    name = "capture_iced",
    about = "生成或校验 Lumino 的 iced 视觉基线 PNG",
    version
)]
struct Args {
    /// 输出目录（默认仓库内 tests/ui-baseline/iced）。
    #[arg(long)]
    out_dir: Option<PathBuf>,
}

fn main() -> Result<()> {
    // iced 基线必须在**纯 iced 通道**下渲染：叠加层默认开启后，iced 视图会返回
    // 同尺寸占位（内容由 egui 绘制），直接渲染会与基线不符。
    // SAFETY：进程刚启动、尚无其他线程读取环境变量。
    unsafe {
        std::env::set_var("LUMINO_EGUI_OVERLAY", "0");
    }

    let args = Args::parse();
    let out_dir = args.out_dir.unwrap_or_else(baseline_dir);

    println!("iced 视觉基线输出目录: {}", out_dir.display());

    let mut created = 0_usize;
    let mut verified = 0_usize;
    let mut mismatched: Vec<PathBuf> = Vec::new();

    for scene_id in SceneId::ALL {
        let mut scene = Scene::build(scene_id);

        for scale in DPI_SCALES {
            let file_name = scene_id.baseline_file_name(scale);
            let path = out_dir.join(&file_name);
            let captured = capture_scene(&mut scene, scale)?;
            let existed = path.exists();
            let matched = captured.matches_image(&path)?;

            if !matched {
                mismatched.push(path.clone());
                eprintln!(
                    "[mismatch] {} ({}x{} @ {:.2}x)",
                    path.display(),
                    captured.width,
                    captured.height,
                    scale
                );
            } else if existed {
                verified += 1;
                println!(
                    "[ok]       {} ({}x{} @ {:.2}x)",
                    path.display(),
                    captured.width,
                    captured.height,
                    scale
                );
            } else {
                created += 1;
                println!(
                    "[created]  {} ({}x{} @ {:.2}x)",
                    path.display(),
                    captured.width,
                    captured.height,
                    scale
                );
            }
        }
    }

    println!(
        "完成：新建 {created} 张，校验通过 {verified} 张，失败 {} 张",
        mismatched.len()
    );

    if !mismatched.is_empty() {
        for path in &mismatched {
            eprintln!("基线不一致: {}", path.display());
        }
        bail!(
            "{} 张基线图与当前渲染结果不一致（如为预期改动，请人工确认后删除旧 PNG 重新生成）",
            mismatched.len()
        );
    }

    Ok(())
}
