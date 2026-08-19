//! ffmpeg 命令行参数构建
//!
//! 组装原始视频帧（stdin rawvideo）+ 各硬件后端专属编码参数的完整命令，
//! 从 `ffmpeg.rs` 拆分而来。

use crate::video::config::{EncoderBackend, QualityPreset, VideoCodec, VideoExportConfig};

/// 组装 ffmpeg 命令行参数（纯视频流，无音频）
pub(crate) fn build_ffmpeg_args(config: &VideoExportConfig, input_pix_fmt: &str) -> Vec<String> {
    let mut args = Vec::new();

    // ── 视频输入：stdin raw BGRA/RGBA ──
    // ffmpeg 内部完成 BGRA/RGBA→YUV 转换
    args.push("-f".to_string());
    args.push("rawvideo".to_string());
    args.push("-pix_fmt".to_string());
    args.push(input_pix_fmt.to_string());
    args.push("-s".to_string());
    args.push(format!("{}x{}", config.width, config.height));
    args.push("-r".to_string());
    args.push(format!("{:.3}", config.fps));
    // 限制 ffmpeg 内部队列，防止编码速度跟不上时堆积数 GB 内存
    args.push("-thread_queue_size".to_string());
    args.push("8".to_string());
    args.push("-i".to_string());
    args.push("-".to_string());

    // ── 多线程 ──
    args.push("-threads".to_string());
    args.push("0".to_string());

    // ── 视频编码器 ──
    args.push("-c:v".to_string());
    args.push(config.ffmpeg_encoder_name());

    // ── 色彩范围：强制全范围 PC，防止暗化 ──
    // ffmpeg 默认 YUV 输出为 limited range (16-235)，会压缩亮度导致偏色
    args.push("-color_range".to_string());
    args.push("pc".to_string());

    // ── 后端专属质量参数 ──
    match &config.backend {
        EncoderBackend::Software => build_software_args(&mut args, &config.codec, &config.quality),
        EncoderBackend::VideoToolbox => {
            build_videotoolbox_args(&mut args, &config.codec, &config.quality)
        }
        EncoderBackend::Nvenc => build_nvenc_args(&mut args, &config.quality),
        EncoderBackend::Amf => build_amf_args(&mut args, &config.quality),
        EncoderBackend::Qsv => build_qsv_args(&mut args, &config.quality),
        EncoderBackend::MediaFoundation => build_mf_args(&mut args, &config.quality),
        EncoderBackend::Vaapi => build_vaapi_args(&mut args, &config.quality),
    }

    // ── 输出像素格式（全部后端统一，ProRes 为 yuv422p，其余 yuv420p） ──
    args.push("-pix_fmt".to_string());
    args.push(config.codec.ffmpeg_pix_fmt().to_string());

    // ── 封装格式 ──
    args.push("-f".to_string());
    args.push(config.container.ffmpeg_muxer().to_string());

    // muxing 队列：防止编码落后于封装时 OOM
    args.push("-max_muxing_queue_size".to_string());
    args.push("64".to_string());

    // 覆盖输出
    args.push("-y".to_string());

    // 输出路径
    args.push(config.output_path.to_string_lossy().to_string());

    args
}

// ---------------------------------------------------------------------------
// 后端专属质量参数
// ---------------------------------------------------------------------------

/// 软件编码器：libx264 / libx265 / prores_ks / libvpx-vp9 / libsvtav1
///
/// 像素格式由调用方统一追加（`build_ffmpeg_args` 末尾）。
fn build_software_args(args: &mut Vec<String>, codec: &VideoCodec, quality: &QualityPreset) {
    match codec {
        VideoCodec::H264 | VideoCodec::H265 => {
            args.push("-crf".to_string());
            args.push(quality.crf().to_string());
            args.push("-preset".to_string());
            args.push(quality.preset().to_string());
        }
        VideoCodec::Vp9 => {
            args.push("-crf".to_string());
            args.push(quality.crf().to_string());
            args.push("-b:v".to_string());
            args.push("0".to_string());
            // VP9 多线程
            args.push("-row-mt".to_string());
            args.push("1".to_string());
            args.push("-tile-columns".to_string());
            args.push("2".to_string());
        }
        VideoCodec::Av1 => {
            args.push("-crf".to_string());
            args.push(quality.crf().to_string());
            // SVT-AV1 多线程
            args.push("-svtav1-params".to_string());
            args.push(format!("lp={}", num_cpus()));
        }
        VideoCodec::ProRes => {
            args.push("-profile:v".to_string());
            args.push("3".to_string());
            args.push("-qscale:v".to_string());
            args.push("9".to_string());
        }
    }
}

/// macOS VideoToolbox：h264_videotoolbox / hevc_videotoolbox / prores_videotoolbox
///
/// 使用目标码率与质量等级（1=最佳, 4=最快），而非 CRF。
fn build_videotoolbox_args(args: &mut Vec<String>, codec: &VideoCodec, quality: &QualityPreset) {
    match codec {
        VideoCodec::H264 | VideoCodec::H265 => {
            let (bitrate, vt_q) = match quality {
                QualityPreset::High => ("50M", "1"),
                QualityPreset::Medium => ("20M", "2"),
                QualityPreset::Low => ("10M", "4"),
            };
            args.push("-b:v".to_string());
            args.push(bitrate.to_string());
            args.push("-quality".to_string());
            args.push(vt_q.to_string());
        }
        VideoCodec::ProRes => {
            let bitrate = match quality {
                QualityPreset::High => "100M",
                QualityPreset::Medium => "50M",
                QualityPreset::Low => "20M",
            };
            args.push("-b:v".to_string());
            args.push(bitrate.to_string());
        }
        _ => {
            // 不支持的 编码器/后端 组合 —— ffmpeg 会报错，但仍尝试
            args.push("-b:v".to_string());
            args.push("20M".to_string());
        }
    }
}

/// 恒定质量档位（NVENC -cq / QSV -global_quality / VAAPI -qp 共用）
fn quality_cq(quality: &QualityPreset) -> &'static str {
    match quality {
        QualityPreset::High => "18",
        QualityPreset::Medium => "23",
        QualityPreset::Low => "28",
    }
}

/// NVIDIA NVENC：h264_nvenc / hevc_nvenc / av1_nvenc（Windows & Linux）
///
/// 使用 -cq 恒定质量 + VBR 码率控制 + preset (p1-p7)。
fn build_nvenc_args(args: &mut Vec<String>, quality: &QualityPreset) {
    let preset = match quality {
        // p1=最快, p7=最慢
        QualityPreset::High => "p5",
        QualityPreset::Medium => "p4",
        QualityPreset::Low => "p2",
    };

    args.push("-cq".to_string());
    args.push(quality_cq(quality).to_string());
    args.push("-rc".to_string());
    args.push("vbr".to_string());
    args.push("-preset".to_string());
    args.push(preset.to_string());
}

/// AMD AMF：h264_amf / hevc_amf / av1_amf（Windows）
fn build_amf_args(args: &mut Vec<String>, quality: &QualityPreset) {
    let (bitrate, amf_q) = match quality {
        QualityPreset::High => ("15M", "quality"),
        QualityPreset::Medium => ("8M", "balanced"),
        QualityPreset::Low => ("4M", "speed"),
    };
    args.push("-b:v".to_string());
    args.push(bitrate.to_string());
    args.push("-quality".to_string());
    args.push(amf_q.to_string());
}

/// Intel QuickSync：h264_qsv / hevc_qsv / av1_qsv / vp9_qsv（Windows & Linux）
fn build_qsv_args(args: &mut Vec<String>, quality: &QualityPreset) {
    let preset = match quality {
        QualityPreset::High => "medium",
        QualityPreset::Medium => "fast",
        QualityPreset::Low => "veryfast",
    };

    args.push("-global_quality".to_string());
    args.push(quality_cq(quality).to_string());
    args.push("-preset".to_string());
    args.push(preset.to_string());
}

/// Windows MediaFoundation：h264_mf / hevc_mf（Windows DXVA/D3D11）
fn build_mf_args(args: &mut Vec<String>, quality: &QualityPreset) {
    let bitrate = match quality {
        QualityPreset::High => "30M",
        QualityPreset::Medium => "15M",
        QualityPreset::Low => "5M",
    };
    args.push("-b:v".to_string());
    args.push(bitrate.to_string());
}

/// VAAPI：h264_vaapi / hevc_vaapi / av1_vaapi / vp9_vaapi（Linux）
fn build_vaapi_args(args: &mut Vec<String>, quality: &QualityPreset) {
    args.push("-qp".to_string());
    args.push(quality_cq(quality).to_string());
}

/// 返回可用 CPU 核心数
fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}
