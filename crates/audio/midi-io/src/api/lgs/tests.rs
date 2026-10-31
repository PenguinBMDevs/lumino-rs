//! LGS (GPU) 后端单元测试。
//!
//! 自 `lgs.rs` 拆出（文件行数守卫 <400）；覆盖输出连接转发与
//! REND-016 #139 的全局复音上限透传。

use super::*;
use lumino_gpu_synth::audio::playback::StampedEvent;

/// 构造只含发送通道的连接（不启动 GPU/音频设备）。
fn conn() -> (
    LgsOutputConn,
    mpsc::Receiver<StampedEvent>,
    mpsc::Receiver<PlaybackControl>,
) {
    let (event_tx, event_rx) = mpsc::channel();
    let (control_tx, control_rx) = mpsc::channel();
    (
        LgsOutputConn {
            event_tx: Arc::new(SharedEventTx::new(Some(EventSender::new(event_tx)))),
            control_tx: Arc::new(Mutex::new(Some(control_tx))),
            velocity_filter: Arc::new(AtomicU8::new(0)),
            cached_sender: None,
            sender_epoch: u64::MAX,
            batch_scratch: Vec::new(),
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
    let (ch1, ev1, _at) = rx.recv().expect("note_on 事件");
    assert_eq!(ch1, 16, "全局通道 16 不得折叠到 0");
    assert!(matches!(ev1, MidiEvent::NoteOn { key: 60, vel: 100 }));
    let (ch2, _, _) = rx.recv().expect("cc 事件");
    assert_eq!(ch2, 31);
}

/// 踏板清理走控制通道（引擎级 AllChannels 等价语义，不逐通道发事件）。
#[test]
fn release_all_dampers_goes_through_control_channel() {
    let (mut c, _rx, crx) = conn();
    c.release_all_dampers().expect("release");
    assert!(matches!(crx.recv(), Ok(PlaybackControl::ReleaseAllDampers)));
}

/// 播放会话清零：ResetStats 经控制通道下发到渲染线程。
#[test]
fn reset_playback_stats_goes_through_control_channel() {
    let (mut c, _rx, crx) = conn();
    c.reset_playback_stats().expect("reset_playback_stats");
    assert!(matches!(crx.recv(), Ok(PlaybackControl::ResetStats)));
}

/// REND-016 #139：全局复音上限必须透传到 GPU `SynthConfig`（0 = 不限制保持旧行为）。
#[test]
fn synth_config_passes_global_voice_limit() {
    let options = LgsOptions {
        sample_rate: 48_000,
        block_size: 1024,
        max_voices_per_key: 4,
        max_voices: 16_384,
        soft_nps_gate: true,
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

/// REF-LGS-012：批量入口与逐条入口必须产出**完全相同**的事件序列
/// （同一映射、同一顺序、同样忽略不支持的消息）。
#[test]
fn batch_and_per_event_paths_emit_identical_events() {
    let (mut per, per_rx, _pc) = conn();
    let (mut batch, batch_rx, _bc) = conn();
    let messages: Vec<[u8; 3]> = vec![
        [0x90, 60, 100],
        [0xB0, 7, 127],
        [0x90, 61, 0], // vel=0：按 MIDI 约定等价 NoteOff，但事件类型保持 NoteOn
        [0xC0, 5, 0],
        [0xE0, 0x00, 0x40], // 中心 = 8192
        [0xD0, 1, 2],       // 通道后触：GPU 不支持 → 两条路径都忽略
        [0xA0, 60, 40],     // 复音后触：同上
        [0x80, 60, 0],
        [0x90, 62, 1],
    ];
    for m in &messages {
        per.send_raw(*m).expect("send_raw");
    }
    batch.send_raw_batch(&messages).expect("send_raw_batch");

    let a: Vec<(u8, MidiEvent)> = per_rx.try_iter().map(|(ch, ev, _)| (ch, ev)).collect();
    let b: Vec<(u8, MidiEvent)> = batch_rx.try_iter().map(|(ch, ev, _)| (ch, ev)).collect();
    assert_eq!(a, b, "批量与逐条必须逐事件一致");
    assert_eq!(a.len(), messages.len() - 2, "两条后触消息应被忽略");
    assert!(matches!(a[4].1, MidiEvent::PitchBend { value: 8192 }));
}

/// REF-LGS-012：发送端被热替换（管线重建）后，连接必须立刻用新发送端，
/// 不得继续往旧通道发（旧实现靠每事件加锁读 `Option`，这里验证纪元缓存）。
#[test]
fn sender_swap_is_picked_up_without_stale_cache() {
    let (mut c, old_rx, _cx) = conn();
    let shared = Arc::clone(&c.event_tx);
    c.send_raw([0x90, 60, 100]).expect("first");
    assert!(old_rx.try_recv().is_ok(), "旧通道应收到第一个事件");

    let (tx_new, new_rx) = mpsc::channel();
    shared.set(Some(EventSender::new(tx_new)));

    c.send_raw([0x90, 61, 100]).expect("second");
    assert!(
        old_rx.try_recv().is_err(),
        "替换后不得再往旧通道发事件（缓存失效）"
    );
    let (ch, ev, _) = new_rx.try_recv().expect("新通道应收到事件");
    assert_eq!(ch, 0);
    assert!(matches!(ev, MidiEvent::NoteOn { key: 61, .. }));

    // 发送端被关闭（None）：静默丢弃，不 panic。
    shared.set(None);
    c.send_raw([0x90, 62, 100]).expect("third");
    assert!(new_rx.try_recv().is_err());
}

/// REF-LGS-012：力度过滤在两条路径上行为一致（过轻 NoteOn 丢弃；
/// vel=0 视为释放，不过滤）。
#[test]
fn velocity_filter_applies_to_both_paths() {
    let (mut per, per_rx, _pc) = conn();
    let (mut batch, batch_rx, _bc) = conn();
    per.velocity_filter.store(64, Ordering::Relaxed);
    batch.velocity_filter.store(64, Ordering::Relaxed);
    let messages: Vec<[u8; 3]> = vec![[0x90, 60, 40], [0x90, 60, 65], [0x90, 61, 0]];
    for m in &messages {
        per.send_raw(*m).expect("send_raw");
    }
    batch.send_raw_batch(&messages).expect("send_raw_batch");
    assert_eq!(per_rx.try_iter().count(), 2, "40 应被过滤，65 与 0 放行");
    assert_eq!(batch_rx.try_iter().count(), 2);
}
