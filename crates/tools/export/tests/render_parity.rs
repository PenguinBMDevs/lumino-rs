//! 导出管线对拍：同一工程 CPU/GPU、内存/流式输出一致 + GPU 进度序列断言
//!
//! 仓库不含音色库素材，本测试需要外部 SF2：
//!
//! ```bash
//! $env:LUMINO_TEST_SF2 = "E:\source\Lumino\test-file\sf2\test.sf2"
//! cargo nextest run --test render_parity --run-ignored only
//! ```
//!
//! 覆盖：
//! - CPU：内存模式（`MidiDocument`）与流式模式（mmap 字节）输出**逐字节一致**；
//! - GPU（有 wgpu 适配器时）：内存/流式输出逐字节一致，且进度百分比序列符合
//!   两条入口各自的历史契约（内存有 0.15 序列化步骤、流式没有）；
//! - 输出非全零（解码 WAV 校验峰值，防止空音频假通过）。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use lumino_export::audio::config::{
    AudioBackendKind, AudioRenderConfig, ProgressCallback, ThreadMode,
};
use lumino_export::audio::gpu_backend::is_gpu_available;
use lumino_export::audio::{render_audio, render_audio_from_document};
use lumino_midi_loader::MidiDocument;

/// 进度日志（消息 + 百分比）
type ProgressLog = Arc<Mutex<Vec<(String, f64)>>>;

fn sf2_path() -> PathBuf {
    std::env::var("LUMINO_TEST_SF2")
        .map(PathBuf::from)
        .unwrap_or_else(|_| panic!("设置 LUMINO_TEST_SF2 指向 .sf2 文件后再运行本对拍"))
}

fn recorder() -> (ProgressLog, ProgressCallback) {
    let log: ProgressLog = Arc::new(Mutex::new(Vec::new()));
    let sink = log.clone();
    (
        log,
        Arc::new(move |msg: String, pct: f64| {
            sink.lock().expect("进度日志锁中毒").push((msg, pct));
        }),
    )
}

fn config(
    midi: &Path,
    sf2: &Path,
    out: &Path,
    backend: AudioBackendKind,
    progress: Option<ProgressCallback>,
) -> AudioRenderConfig {
    AudioRenderConfig {
        midi_path: midi.to_path_buf(),
        soundfonts: vec![sf2.to_path_buf()],
        output_path: out.to_path_buf(),
        backend,
        // 单线程确保对拍可复现（不改变 DSP，只固定调度顺序）
        channel_threading: ThreadMode::None,
        key_threading: ThreadMode::None,
        progress_callback: progress,
        ..Default::default()
    }
}

/// VLQ 变长整数（MIDI delta time）
fn vlq(mut n: u32) -> Vec<u8> {
    let mut bytes = vec![(n & 0x7F) as u8];
    n >>= 7;
    while n > 0 {
        bytes.push(((n & 0x7F) as u8) | 0x80);
        n >>= 7;
    }
    bytes.reverse();
    bytes
}

fn push_track(out: &mut Vec<u8>, events: &[u8]) {
    out.extend_from_slice(b"MTrk");
    out.extend_from_slice(&(events.len() as u32).to_be_bytes());
    out.extend_from_slice(events);
}

/// 双轨 SMF：轨 0 端口 0（tempo/拍号/两个音符），轨 1 端口 1（PC/CC/一个音符）。
///
/// 覆盖两条事件提取链都会处理的四类事件（NoteOn/NoteOff/CC/PC）与端口元事件，
/// 同时让 `max_port = 1` 走多端口通道映射。
fn build_midi() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"MThd");
    out.extend_from_slice(&6u32.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes()); // format 1
    out.extend_from_slice(&2u16.to_be_bytes()); // 2 tracks
    out.extend_from_slice(&480u16.to_be_bytes());

    let mut t0 = Vec::new();
    t0.extend_from_slice(&[0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]); // tempo 120 BPM
    t0.extend_from_slice(&[0x00, 0xFF, 0x58, 0x04, 0x04, 0x02, 0x18, 0x08]); // 4/4
    t0.extend_from_slice(&[0x00, 0x90, 0x3C, 0x64]); // C4 on
    t0.extend_from_slice(&vlq(240));
    t0.extend_from_slice(&[0x80, 0x3C, 0x40]); // C4 off
    t0.extend_from_slice(&[0x00, 0x90, 0x40, 0x64]); // E4 on
    t0.extend_from_slice(&vlq(240));
    t0.extend_from_slice(&[0x80, 0x40, 0x40]); // E4 off
    t0.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);
    push_track(&mut out, &t0);

    let mut t1 = Vec::new();
    t1.extend_from_slice(&[0x00, 0xFF, 0x21, 0x01, 0x01]); // port 1
    t1.extend_from_slice(&[0x00, 0xC1, 0x00]); // program 0 on ch1
    t1.extend_from_slice(&[0x00, 0xB1, 0x07, 0x64]); // CC7 = 100
    t1.extend_from_slice(&[0x00, 0x91, 0x3C, 0x64]); // C4 on ch1
    t1.extend_from_slice(&vlq(240));
    t1.extend_from_slice(&[0x81, 0x3C, 0x40]); // C4 off ch1
    t1.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);
    push_track(&mut out, &t1);

    out
}

/// 渲染一次并返回输出字节；`document = Some` 走内存模式，`None` 走流式模式。
fn render_once(
    midi: &Path,
    sf2: &Path,
    out: &Path,
    backend: AudioBackendKind,
    progress: Option<ProgressCallback>,
    document: Option<&MidiDocument>,
) -> Vec<u8> {
    let cfg = config(midi, sf2, out, backend, progress);
    match document {
        Some(doc) => render_audio_from_document(&cfg, doc).expect("内存模式渲染失败"),
        None => render_audio(&cfg).expect("流式模式渲染失败"),
    }
    std::fs::read(out).expect("读取渲染输出失败")
}

/// 解码 WAV 校验峰值（全零 = 没有真正渲染出声音）。
fn peak_amplitude(bytes: &[u8]) -> f32 {
    let cursor = std::io::Cursor::new(bytes);
    let reader = hound::WavReader::new(cursor).expect("输出应为合法 WAV");
    let spec = reader.spec();
    match spec.sample_format {
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .map(|s| s.expect("读取浮点样本失败").abs())
            .fold(0.0f32, f32::max),
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .into_samples::<i32>()
                .map(|s| (s.expect("读取整型样本失败") as f32 / max).abs())
                .fold(0.0f32, f32::max)
        }
    }
}

fn write_fixture(dir: &Path, sf2: &Path) -> (PathBuf, MidiDocument) {
    let midi_bytes = build_midi();
    let midi_path = dir.join("parity.mid");
    std::fs::write(&midi_path, &midi_bytes).expect("写入 MIDI 夹具失败");
    let (doc, _, _) = MidiDocument::from_notes_bytes(&midi_bytes, None).expect("MIDI 夹具应可解析");
    assert_eq!(doc.max_port(), 1, "夹具应含 FF 21 端口 1");
    assert!(!sf2.as_os_str().is_empty());
    (midi_path, doc)
}

/// CPU：内存模式与流式模式输出必须逐字节一致。
#[test]
#[ignore = "需要外部 SF2 素材：LUMINO_TEST_SF2"]
fn cpu_memory_and_streaming_output_are_identical() {
    let sf2 = sf2_path();
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let (midi_path, doc) = write_fixture(dir.path(), &sf2);

    let (mem_log, mem_cb) = recorder();
    let memory = render_once(
        &midi_path,
        &sf2,
        &dir.path().join("cpu_memory.wav"),
        AudioBackendKind::Cpu,
        Some(mem_cb),
        Some(&doc),
    );
    let (str_log, str_cb) = recorder();
    let streaming = render_once(
        &midi_path,
        &sf2,
        &dir.path().join("cpu_streaming.wav"),
        AudioBackendKind::Cpu,
        Some(str_cb),
        None,
    );

    assert_eq!(
        memory.len(),
        streaming.len(),
        "CPU 内存/流式输出长度不一致（{} vs {} 字节）",
        memory.len(),
        streaming.len()
    );
    assert!(
        memory == streaming,
        "CPU 内存/流式输出字节不一致（两套事件提取必须等价）"
    );
    let peak = peak_amplitude(&memory);
    assert!(peak > 1e-4, "输出峰值 {peak} 过低，疑似空音频假通过");

    // 两条 CPU 循环都必须在收尾上报 100%
    for (name, log) in [("内存", &mem_log), ("流式", &str_log)] {
        let pcts: Vec<f64> = log
            .lock()
            .expect("进度日志锁中毒")
            .iter()
            .map(|(_, p)| *p)
            .collect();
        assert_eq!(
            pcts.last().copied(),
            Some(1.0),
            "{name}模式最终进度应为 1.0"
        );
    }
}

/// GPU：内存/流式输出一致 + 两条入口的进度序列各自保留。
#[test]
#[ignore = "需要外部 SF2 素材：LUMINO_TEST_SF2"]
fn gpu_memory_and_streaming_output_are_identical() {
    if !is_gpu_available() {
        eprintln!("跳过 GPU 对拍：当前环境无可用 wgpu 适配器");
        return;
    }

    let sf2 = sf2_path();
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let (midi_path, doc) = write_fixture(dir.path(), &sf2);

    let (mem_log, mem_cb) = recorder();
    let memory = render_once(
        &midi_path,
        &sf2,
        &dir.path().join("gpu_memory.wav"),
        AudioBackendKind::Gpu,
        Some(mem_cb),
        Some(&doc),
    );
    let (str_log, str_cb) = recorder();
    let streaming = render_once(
        &midi_path,
        &sf2,
        &dir.path().join("gpu_streaming.wav"),
        AudioBackendKind::Gpu,
        Some(str_cb),
        None,
    );

    assert_eq!(
        memory.len(),
        streaming.len(),
        "GPU 内存/流式输出长度不一致（{} vs {} 字节）",
        memory.len(),
        streaming.len()
    );
    assert!(memory == streaming, "GPU 内存/流式输出字节不一致");
    let peak = peak_amplitude(&memory);
    assert!(peak > 1e-4, "GPU 输出峰值 {peak} 过低，疑似空音频假通过");

    let mem_pcts: Vec<f64> = mem_log
        .lock()
        .expect("进度日志锁中毒")
        .iter()
        .map(|(_, p)| *p)
        .collect();
    let str_pcts: Vec<f64> = str_log
        .lock()
        .expect("进度日志锁中毒")
        .iter()
        .map(|(_, p)| *p)
        .collect();

    let has = |pcts: &[f64], want: f64| pcts.iter().any(|p| (*p - want).abs() < 1e-9);

    assert!(has(&mem_pcts, 0.05), "内存模式应上报 0.05 初始化进度");
    assert!(has(&mem_pcts, 0.10), "内存模式应上报 0.10 音色库进度");
    assert!(
        has(&mem_pcts, 0.15),
        "内存模式必须有 0.15 序列化步骤（两条入口的差异契约）"
    );
    assert!(has(&mem_pcts, 0.20), "内存模式应上报 0.20 渲染进度");
    assert!(has(&mem_pcts, 0.85), "内存模式应上报 0.85 写盘进度");
    assert_eq!(
        mem_pcts.last().copied(),
        Some(1.0),
        "内存模式最终进度应为 1.0"
    );

    assert!(has(&str_pcts, 0.05), "流式模式应上报 0.05 初始化进度");
    assert!(has(&str_pcts, 0.10), "流式模式应上报 0.10 音色库进度");
    assert!(
        !has(&str_pcts, 0.15),
        "流式模式复用已读入字节，不应出现 0.15 序列化步骤"
    );
    assert!(has(&str_pcts, 0.20), "流式模式应上报 0.20 渲染进度");
    assert!(has(&str_pcts, 0.85), "流式模式应上报 0.85 写盘进度");
    assert_eq!(
        str_pcts.last().copied(),
        Some(1.0),
        "流式模式最终进度应为 1.0"
    );
}
