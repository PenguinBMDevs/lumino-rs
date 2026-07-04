//! Renderer 线程主循环 — 处理命令、渲染音频、推送到 ring buffer。
//!
//! 调度策略（借鉴 yinhe）：
//! - 有活干时持续渲染，不 sleep
//! - 无活时 sleep 1ms 等待命令
//! - cpal 回调只从 ring buffer 读取，永不阻塞

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crossbeam_channel::Receiver;

use crate::audio_ring::AudioRingProducer;
use crate::engine::{AudioEngine, PlayState};
use crate::spawn::AudioCommand;

const RENDER_CHUNK_FRAMES: usize = 256;
const STEREO_CHANNELS: usize = 2;
const RING_TARGET_FILL_RATIO: f32 = 0.125;

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
    shutdown_flag: Arc<AtomicBool>,
) {
    let mut scratch = vec![0.0f32; RENDER_CHUNK_FRAMES * STEREO_CHANNELS];

    while !shutdown_flag.load(Ordering::Relaxed) {
        let mut did_work = false;

        did_work |= process_commands(&engine, &cmd_rx, &mut ring_producer, &shutdown_flag);
        did_work |= render_if_needed(&engine, &mut scratch, &mut ring_producer);
        publish_state(&engine, &state_tx);

        if !did_work {
            thread::sleep(Duration::from_millis(1));
        }
    }

    // 优雅退出：关闭所有音符，清空 ring buffer
    let mut eng = engine.lock().unwrap();
    eng.all_notes_off();
    drop(eng);
    ring_producer.clear();
    tracing::info!("音频渲染线程已优雅退出");
}

fn process_commands(
    engine: &Arc<Mutex<AudioEngine>>,
    cmd_rx: &Receiver<AudioCommand>,
    ring: &mut AudioRingProducer,
    shutdown_flag: &AtomicBool,
) -> bool {
    let mut did_work = false;
    while let Ok(cmd) = cmd_rx.try_recv() {
        let mut eng = engine.lock().unwrap();
        match cmd {
            AudioCommand::Play => {
                tracing::debug!(
                    "[AUDIO] 收到 Play 命令, play_state={:?} -> Playing",
                    eng.play_state
                );
                // 清除 ring buffer 中积压的静音数据，消除播放启动延迟
                ring.clear();
                eng.play();
            }
            AudioCommand::Pause => {
                tracing::debug!("[AUDIO] 收到 Pause 命令");
                eng.pause();
            }
            AudioCommand::Stop => {
                tracing::debug!("[AUDIO] 收到 Stop 命令");
                eng.stop();
            }
            AudioCommand::SeekSample(s) => eng.seek_to_sample(s),
            AudioCommand::SeekTick(t) => eng.seek_to_tick(t),
            AudioCommand::NoteOn {
                channel,
                key,
                velocity,
            } => {
                tracing::debug!("[AUDIO] NoteOn ch={} key={} vel={}", channel, key, velocity);
                eng.preview_note_on(channel, key, velocity);
            }
            AudioCommand::NoteOff { channel, key } => {
                tracing::debug!("[AUDIO] NoteOff ch={} key={}", channel, key);
                eng.preview_note_off(channel, key);
            }
            AudioCommand::ControlChange {
                channel,
                controller,
                value,
            } => {
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
                shutdown_flag.store(true, Ordering::Relaxed);
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

    // 预分配静音 buffer，避免每次循环都分配内存
    let silence = vec![0.0f32; scratch.len()];

    while ring.len() < stereo_target {
        let mut eng = engine.lock().unwrap();

        // 非播放状态不渲染 MIDI 事件，填充静音
        if eng.play_state != PlayState::Playing {
            // 预览音符的 release 尾音需要继续渲染几个 block
            // 但如果没有活跃音符，直接填充静音
            if eng.active_notes.is_empty() {
                tracing::debug!(
                    "[AUDIO] 静音填充: play_state={:?}, active_notes={}, cursor={}, duration={}",
                    eng.play_state,
                    eng.active_notes.len(),
                    eng.cursor.position,
                    eng.duration_samples(),
                );
                drop(eng);
                let written = ring.push_slice(&silence);
                if written == 0 {
                    break;
                }
                did_work = true;
                continue;
            }
        }

        // 播放结束检查
        if eng.play_state == PlayState::Playing {
            let duration = eng.duration_samples();
            if duration > 0 && eng.cursor.position >= duration {
                tracing::debug!(
                    "[AUDIO] 播放结束: cursor={} >= duration={}",
                    eng.cursor.position,
                    duration,
                );
                eng.play_state = PlayState::Stopped;
                eng.all_notes_off();
                drop(eng);
                continue;
            }
        }

        let rendered = crate::engine_render::render_block(&mut eng, scratch);
        drop(eng);

        if rendered == 0 {
            // 没有更多数据可渲染，填充静音
            let written = ring.push_slice(&silence);
            if written == 0 {
                break;
            }
            did_work = true;
            continue;
        }

        // 只推送实际渲染的样本（render_block 可能截断了最后一帧）
        let actual_samples = rendered * STEREO_CHANNELS;
        let written = ring.push_slice(&scratch[..actual_samples]);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证 RING_TARGET_FILL_RATIO 在合理范围内
    /// 目标值：低于 0.5 意味着初始状况下 ring buffer 预填 < 1.5秒，
    /// 高于 0.05 意味着不会因为预填太少导致 underrun
    #[test]
    fn test_ring_target_fill_ratio_range() {
        assert!(
            RING_TARGET_FILL_RATIO > 0.05,
            "RING_TARGET_FILL_RATIO 太低会导致 underrun: {}",
            RING_TARGET_FILL_RATIO
        );
        assert!(
            RING_TARGET_FILL_RATIO < 0.5,
            "RING_TARGET_FILL_RATIO 太高会导致初始延迟过大: {}",
            RING_TARGET_FILL_RATIO
        );
    }

    /// 验证 RING_TARGET_FILL_RATIO 的具体值
    /// 修复后目标值 0.125（~0.37 秒缓冲）
    #[test]
    fn test_ring_target_fill_ratio_value() {
        // 0.125 = 1/8 缓冲。这个值确保初始延迟 < 0.5s
        assert!(
            (RING_TARGET_FILL_RATIO - 0.125).abs() < f32::EPSILON,
            "RING_TARGET_FILL_RATIO 应为 0.125，实际 {}",
            RING_TARGET_FILL_RATIO
        );
    }

    /// 验证 RENDER_CHUNK_FRAMES 与采样率的兼容性
    #[test]
    fn test_render_chunk_frames_valid() {
        // 必须 > 0 且为合理的块大小
        assert!(RENDER_CHUNK_FRAMES > 0);
        assert!(RENDER_CHUNK_FRAMES <= 4096);
        // 必须是偶数（用于立体声输出）
        assert!(RENDER_CHUNK_FRAMES % 2 == 0);
    }
}
