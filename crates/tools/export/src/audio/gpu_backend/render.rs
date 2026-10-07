//! GPU 渲染入口与结果写盘

use super::*;

/// 检测 GPU 是否可用（尝试创建 wgpu 适配器）
pub fn is_gpu_available() -> bool {
    lumino_gpu_synth::gpu::create_gpu_context().is_ok()
}

/// 使用 GPU 后端从 MidiDocument 渲染到 Sink（内存模式）
pub fn render_audio_gpu_from_document(
    config: &AudioRenderConfig,
    doc: &MidiDocument,
) -> ExportResult<()> {
    use lumino_gpu_synth::GpuSynth;

    // REND-002 #87 防御：直接调用方（example/测试）可能未预填端口布局，
    // 合成通道空间必须以文档实际端口为准（外层 `render_audio_from_document`
    // 已填，此处 `max` 幂等）。
    let mut config = config.clone();
    config.midi_max_port = config.midi_max_port.max(doc.max_port());
    let config = &config;

    check_control(config)?;

    let report = |msg: &str, pct: f64| {
        if let Some(ref cb) = config.progress_callback {
            cb(msg.to_string(), pct);
        }
    };

    if config.soundfonts.is_empty() {
        return Err(ExportError::AudioWrite(
            "未指定音色库文件，请先在 音色库(SF2) 中选择 .sf2 文件".into(),
        ));
    }
    // 校验编码器参数
    if let Err(msg) = config
        .audio_codec
        .validate(config.sample_rate, config.audio_bitrate)
    {
        return Err(ExportError::AudioWrite(msg));
    }
    // 检查音色库文件存在与格式（SFZ 会直接 panic，需前置拦截）
    let sf_path = &config.soundfonts[0];
    if !sf_path.exists() {
        return Err(ExportError::AudioWrite(format!(
            "音色库文件不存在: {:?}，请检查路径或重新选择",
            sf_path
        )));
    }
    if let Some(ext) = sf_path.extension().and_then(|s| s.to_str())
        && !ext.eq_ignore_ascii_case("sf2")
        && !ext.eq_ignore_ascii_case("sfz")
    {
        return Err(ExportError::AudioWrite(format!(
            "不支持的音色库格式 {:?}：仅支持 .sf2/.sfz",
            sf_path
        )));
    }
    // 仅对 SF2 校验 RIFF 头，SFZ 为文本
    if let Some(ext) = sf_path.extension().and_then(|s| s.to_str())
        && ext.eq_ignore_ascii_case("sf2")
        && let Ok(mut f) = std::fs::File::open(sf_path)
    {
        use std::io::Read;
        let mut header = [0u8; 4];
        if f.read_exact(&mut header).is_ok() && header != *b"RIFF" {
            return Err(ExportError::AudioWrite(format!(
                "音色库不是合法的 SF2 {:?}：头应为 RIFF，实际 {:02X?}",
                sf_path, header
            )));
        }
    }

    report("GPU 初始化中...", 0.05);
    check_control(config)?;
    let synth_config = build_synth_config(config);
    let mut synth = GpuSynth::new(synth_config).map_err(|e| {
        ExportError::AudioWrite(format!(
            "GPU 初始化失败（可能无可用 Vulkan/Metal 适配器）: {e}"
        ))
    })?;

    report("GPU 加载音色库...", 0.10);
    check_control(config)?;
    synth
        .load_soundfont(sf_path, 0, 0)
        .map_err(|e| ExportError::AudioWrite(format!("GPU 音色库加载失败 {sf_path:?}: {e}")))?;

    report("GPU 序列化 MIDI（内存）...", 0.15);
    check_control(config)?;
    // 空文档直接报错，交由上层回退到文件模式
    let total_notes: usize = doc.notes.iter().map(|v| v.len()).sum();
    if total_notes == 0 && doc.control_events.is_empty() {
        return Err(ExportError::AudioWrite(
            "MIDI 文档中没有可渲染的事件（0 notes），请检查 MIDI 是否已加载".into(),
        ));
    }
    // EXP-002：SMF 序列化只发生在内存，直喂 GPU —— 全程无临时 MIDI 文件、无重复导出
    let export_data = build_export_data(doc, config);
    let midi_bytes = crate::midi::export_midi_to_bytes(&export_data)?;

    report("GPU 渲染中（可能耗时，黑 MIDI 请耐心）...", 0.20);
    check_control(config)?;
    // 渲染循环内的协作式暂停/中止检查点：GPU 渲染是长时间阻塞调用，
    // 没有它 UI 的"暂停/停止"要等整段渲染结束才生效。
    if let Some(ctrl) = config.control.clone() {
        synth.set_render_checkpoint(Some(std::sync::Arc::new(move || {
            ctrl.wait_if_paused();
            !ctrl.is_aborted()
        })));
    }
    attach_render_progress(&mut synth, config);
    let result = synth.render_midi_bytes(&midi_bytes).map_err(|e| match e {
        lumino_gpu_synth::SynthError::Cancelled => ExportError::Aborted,
        other => ExportError::AudioWrite(format!("GPU 渲染失败: {other}")),
    })?;
    check_control(config)?;

    report("GPU 写入输出...", 0.85);
    // 通过 Sink 写入目标文件（支持 WAV/MP3/FLAC 等）
    write_gpu_result_to_sink(config, &result.samples, result.sample_rate, result.channels)?;

    report("GPU 完成", 1.0);
    Ok(())
}

/// 使用 GPU 后端从磁盘 MIDI 文件渲染（流式模式，无 MidiDocument）
pub fn render_audio_gpu_streaming(config: &AudioRenderConfig) -> ExportResult<()> {
    use lumino_gpu_synth::GpuSynth;

    check_control(config)?;

    let report = |msg: &str, pct: f64| {
        if let Some(ref cb) = config.progress_callback {
            cb(msg.to_string(), pct);
        }
    };

    if config.soundfonts.is_empty() {
        return Err(ExportError::AudioWrite(
            "未指定音色库文件，请先在 音色库(SF2) 中选择 .sf2 文件".into(),
        ));
    }
    if let Err(msg) = config
        .audio_codec
        .validate(config.sample_rate, config.audio_bitrate)
    {
        return Err(ExportError::AudioWrite(msg));
    }
    let sf_path = &config.soundfonts[0];
    if !sf_path.exists() {
        return Err(ExportError::AudioWrite(format!(
            "音色库文件不存在: {:?}",
            sf_path
        )));
    }
    if let Some(ext) = sf_path.extension().and_then(|s| s.to_str())
        && !ext.eq_ignore_ascii_case("sf2")
        && !ext.eq_ignore_ascii_case("sfz")
    {
        return Err(ExportError::AudioWrite(format!(
            "不支持的音色库格式 {:?}：仅支持 .sf2/.sfz",
            sf_path
        )));
    }
    if let Some(ext) = sf_path.extension().and_then(|s| s.to_str())
        && ext.eq_ignore_ascii_case("sf2")
        && let Ok(mut f) = std::fs::File::open(sf_path)
    {
        use std::io::Read;
        let mut header = [0u8; 4];
        if f.read_exact(&mut header).is_ok() && header != *b"RIFF" {
            return Err(ExportError::AudioWrite(format!(
                "音色库不是合法的 SF2 {:?}：头应为 RIFF，实际 {:02X?}",
                sf_path, header
            )));
        }
    }
    if !config.midi_path.exists() {
        return Err(ExportError::AudioWrite(format!(
            "MIDI 文件不存在: {:?}",
            config.midi_path
        )));
    }

    report("GPU 初始化中...", 0.05);
    check_control(config)?;
    let synth_config = build_synth_config(config);
    let mut synth = GpuSynth::new(synth_config)
        .map_err(|e| ExportError::AudioWrite(format!("GPU 初始化失败: {e}")))?;

    report("GPU 加载音色库...", 0.10);
    check_control(config)?;
    synth
        .load_soundfont(sf_path, 0, 0)
        .map_err(|e| ExportError::AudioWrite(format!("GPU 音色库加载失败 {sf_path:?}: {e}")))?;

    report("GPU 渲染中...", 0.20);
    check_control(config)?;
    // 渲染循环内的协作式暂停/中止检查点（同内存模式）。
    if let Some(ctrl) = config.control.clone() {
        synth.set_render_checkpoint(Some(std::sync::Arc::new(move || {
            ctrl.wait_if_paused();
            !ctrl.is_aborted()
        })));
    }
    attach_render_progress(&mut synth, config);
    let result = synth
        .render_midi_file(&config.midi_path)
        .map_err(|e| match e {
            lumino_gpu_synth::SynthError::Cancelled => ExportError::Aborted,
            other => ExportError::AudioWrite(format!("GPU 渲染失败: {other}")),
        })?;
    check_control(config)?;

    report("GPU 写入输出...", 0.85);
    write_gpu_result_to_sink(config, &result.samples, result.sample_rate, result.channels)?;
    report("GPU 完成", 1.0);
    Ok(())
}

/// 将 GPU 渲染结果写入 Sink（处理声道数与采样率）
fn write_gpu_result_to_sink(
    config: &AudioRenderConfig,
    samples: &[f32],
    sample_rate: u32,
    channels: u32,
) -> ExportResult<()> {
    // GPU 输出采样率应与 config 一致（SynthConfig 已按 config 构造），若不一致则告警
    if sample_rate != config.sample_rate {
        tracing::warn!(
            "GPU 渲染采样率 {} 与配置 {} 不一致，以渲染结果为准",
            sample_rate,
            config.sample_rate
        );
    }
    // 通道数校验（DEBT-04 #121：Mono 请求改为写前真降混，不再静默交错写出）
    let expected_ch = config.channels.channel_count() as u32;
    let needs_downmix = expected_ch == 1 && channels == 2;
    if channels != expected_ch && !needs_downmix {
        tracing::warn!("GPU 渲染声道 {} 与配置 {} 不一致", channels, expected_ch);
    }

    let mut sink = create_output_sink(config)?;

    // 分块写入，避免单次过大；进度从 0.85 映射到 1.0
    const CHUNK_FRAMES: usize = 4096;
    let ch = channels as usize;
    let mut offset = 0;
    while offset < samples.len() {
        check_control(config)?;
        let end = (offset + CHUNK_FRAMES * ch).min(samples.len());
        if needs_downmix {
            // 真降混：mono = 0.5 * (L + R)（分块，避免整曲临时副本）
            let mut mono = Vec::with_capacity((end - offset) / 2);
            downmix_stereo_to_mono(&samples[offset..end], &mut mono);
            sink.write_samples(&mono)?;
        } else {
            sink.write_samples(&samples[offset..end])?;
        }
        offset = end;
        // 进度回调（按样本进度估算 0.85→1.0）
        if let Some(ref cb) = config.progress_callback {
            let inner = offset as f64 / samples.len().max(1) as f64;
            let pct = 0.85 + inner * 0.15;
            cb(format!("GPU 写入 {:.1}%", inner * 100.0), pct);
        }
    }
    sink.finalize()?;
    Ok(())
}

/// 立体声交错 → 单声道真降混：`mono = 0.5 * (L + R)`（DEBT-04 #121）。
///
/// 供写 sink 前调用；分块调用避免整曲临时副本。
pub(crate) fn downmix_stereo_to_mono(stereo: &[f32], out: &mut Vec<f32>) {
    out.clear();
    out.reserve(stereo.len() / 2);
    for pair in stereo.as_chunks::<2>().0 {
        out.push(0.5 * (pair[0] + pair[1]));
    }
}

/// 供调用方查询 GPU 后端是否可用
pub fn gpu_backend_available() -> bool {
    is_gpu_available()
}

#[cfg(test)]
mod tests {
    use super::downmix_stereo_to_mono;

    /// DEBT-04 #121：Mono 导出必须是真降混（0.5*(L+R)），不是丢掉右声道。
    #[test]
    fn downmix_averages_both_channels() {
        let stereo = [1.0f32, -1.0, 0.5, 0.25, 0.0, 1.0];
        let mut mono = Vec::new();
        downmix_stereo_to_mono(&stereo, &mut mono);
        assert_eq!(mono.len(), 3);
        assert!((mono[0] - 0.0).abs() < 1e-6, "L+R 反相应抵消");
        assert!((mono[1] - 0.375).abs() < 1e-6);
        assert!((mono[2] - 0.5).abs() < 1e-6);
    }

    /// 小切片（分块边界）仍正确。
    #[test]
    fn downmix_handles_small_slices() {
        let mut mono = Vec::new();
        downmix_stereo_to_mono(&[0.25, 0.75], &mut mono);
        assert_eq!(mono, vec![0.5]);
    }
}
