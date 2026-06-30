//! Renderer 线程主循环 — 处理命令、渲染音频、推送到 ring buffer。
//!
//! 调度策略（借鉴 yinhe）：
//! - 有活干时持续渲染，不 sleep
//! - 无活时 sleep 1ms 等待命令
//! - cpal 回调只从 ring buffer 读取，永不阻塞

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crossbeam_channel::Receiver;

use crate::audio_ring::AudioRingProducer;
use crate::engine::{AudioEngine, PlayState};
use crate::spawn::AudioCommand;

const RENDER_CHUNK_FRAMES: usize = 256;
const STEREO_CHANNELS: usize = 2;
const RING_TARGET_FILL_RATIO: f32 = 0.5;

/// 引擎状态快照 — 供 UI 查询播放进度。
#[derive(Clone, Copy, Debug)]
pub struct EngineStateSnapshot {
    pub play_state: PlayState,
    pub position_samples: u64,
    pub position_tick: f64,
    pub duration_samples: u64,
    pub has_model: bool,
}

/// 启动 renderer 线程。
pub(crate) fn run_audio_renderer(
    engine: Arc<Mutex<AudioEngine>>,
    mut ring_producer: AudioRingProducer,
    cmd_rx: Receiver<AudioCommand>,
    state_tx: crossbeam_channel::Sender<EngineStateSnapshot>,
) {
    let mut scratch =
        vec![0.0f32; RENDER_CHUNK_FRAMES * STEREO_CHANNELS];

    loop {
        let mut did_work = false;

        did_work |= process_commands(&engine, &cmd_rx);
        did_work |= render_if_needed(&engine, &mut scratch, &mut ring_producer);
        publish_state(&engine, &state_tx);

        if !did_work {
            thread::sleep(Duration::from_millis(1));
        }
    }
}

fn process_commands(
    engine: &Arc<Mutex<AudioEngine>>,
    cmd_rx: &Receiver<AudioCommand>,
) -> bool {
    let mut did_work = false;
    while let Ok(cmd) = cmd_rx.try_recv() {
        let mut eng = engine.lock().unwrap();
        match cmd {
            AudioCommand::Play => eng.play(),
            AudioCommand::Pause => eng.pause(),
            AudioCommand::Stop => eng.stop(),
            AudioCommand::SeekSample(s) => eng.seek_to_sample(s),
            AudioCommand::SeekTick(t) => eng.seek_to_tick(t),
            AudioCommand::NoteOn { channel, key, velocity } => {
                eng.preview_note_on(channel, key, velocity);
            }
            AudioCommand::NoteOff { channel, key } => {
                eng.preview_note_off(channel, key);
            }
            AudioCommand::ControlChange { channel, controller, value } => {
                eng.preview_cc(channel, controller, value);
            }
            AudioCommand::ProgramChange { channel, program } => {
                eng.preview_program_change(channel, program);
            }
            AudioCommand::PitchBend { channel, value } => {
                eng.preview_pitch_bend(channel, value);
            }
            AudioCommand::AllNotesOff => eng.all_notes_off(),
            AudioCommand::ResetAll => eng.reset_all(),
            AudioCommand::Shutdown => {
                std::process::exit(0);
            }
        }
        did_work = true;
    }
    did_work
}

fn render_if_needed(
    engine: &Arc<Mutex<AudioEngine>>,
    scratch: &mut [f32],
    ring: &mut AudioRingProducer,
) -> bool {
    let mut did_work = false;

    let target_fill = (ring.capacity() as f32 * RING_TARGET_FILL_RATIO) as usize;
    let stereo_target = target_fill - (target_fill % STEREO_CHANNELS);

    while ring.len() < stereo_target {
        let mut eng = engine.lock().unwrap();

        // 播放结束检查
        if eng.play_state == PlayState::Playing {
            let duration = eng.duration_samples();
            if duration > 0 && eng.cursor.position >= duration {
                eng.play_state = PlayState::Stopped;
                eng.all_notes_off();
                break;
            }
        }

        // 无论是否在播放，都渲染一块音频：
        // - 播放时：渲染 MIDI 事件
        // - 非播放时：渲染预览音符的 release 尾声
        crate::engine_render::render_block(&mut eng, scratch);
        drop(eng);

        let written = ring.push_slice(scratch);
        if written == 0 {
            break;
        }
        did_work = true;
    }

    did_work
}

fn publish_state(
    engine: &Arc<Mutex<AudioEngine>>,
    state_tx: &crossbeam_channel::Sender<EngineStateSnapshot>,
) {
    let eng = engine.lock().unwrap();
    let snapshot = EngineStateSnapshot {
        play_state: eng.play_state,
        position_samples: eng.position_samples(),
        position_tick: eng.position_tick(),
        duration_samples: eng.duration_samples(),
        has_model: eng.state.has_model(),
    };
    let _ = state_tx.try_send(snapshot);
}
