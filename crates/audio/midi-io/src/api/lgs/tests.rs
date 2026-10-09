//! LGS (GPU) 后端单元测试。
//!
//! 自 `lgs.rs` 拆出（文件行数守卫 <400）；覆盖输出连接转发与
//! REND-016 #139 的全局复音上限透传。

use super::*;

/// 构造只含发送通道的连接（不启动 GPU/音频设备）。
fn conn() -> (
    LgsOutputConn,
    mpsc::Receiver<(u8, MidiEvent)>,
    mpsc::Receiver<PlaybackControl>,
) {
    let (event_tx, event_rx) = mpsc::channel();
    let (control_tx, control_rx) = mpsc::channel();
    (
        LgsOutputConn {
            event_tx: Arc::new(Mutex::new(Some(event_tx))),
            control_tx: Arc::new(Mutex::new(Some(control_tx))),
            velocity_filter: Arc::new(AtomicU8::new(0)),
        },
        event_rx,
        control_rx,
    )
}

/// REND-002 实时多端口：全局通道（16..=255）必须原样透传，不得 4bit 折叠。
#[test]
fn global_channel_passes_through_without_folding() {
    let (mut c, rx, _crx) = conn();
    c.note_on(16, 60, 100).expect("note_on");
    c.control_change(31, 7, 127).expect("cc");
    let (ch1, ev1) = rx.recv().expect("note_on 事件");
    assert_eq!(ch1, 16, "全局通道 16 不得折叠到 0");
    assert!(matches!(ev1, MidiEvent::NoteOn { key: 60, vel: 100 }));
    let (ch2, _) = rx.recv().expect("cc 事件");
    assert_eq!(ch2, 31);
}

/// 踏板清理走控制通道（引擎级 AllChannels 等价语义，不逐通道发事件）。
#[test]
fn release_all_dampers_goes_through_control_channel() {
    let (mut c, _rx, crx) = conn();
    c.release_all_dampers().expect("release");
    assert!(matches!(crx.recv(), Ok(PlaybackControl::ReleaseAllDampers)));
}

/// REND-016 #139：全局复音上限必须透传到 GPU `SynthConfig`（0 = 不限制保持旧行为）。
#[test]
fn synth_config_passes_global_voice_limit() {
    let options = LgsOptions {
        sample_rate: 48_000,
        block_size: 1024,
        max_voices_per_key: 4,
        max_voices: 16_384,
        use_sinc: false,
        velocity_filter_threshold: 0,
        audio_output_device: None,
        midi_max_port: 0,
    };
    let config = Lgs::synth_config_from(&options);
    assert_eq!(config.max_voices, 16_384, "全局上限必须透传");
    assert_eq!(config.max_voices_per_key, 4);
    assert_eq!(config.sample_rate, 48_000);
    assert_eq!(config.block_size, 1024);
    assert_eq!(config.midi_channels, 16, "max_port=0 → 单端口 16 通道");
}
