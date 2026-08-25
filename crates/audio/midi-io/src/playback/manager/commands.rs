//! 播放管理器命令处理
//!
//! 播放线程的命令枚举、命令分发与 MIDI 消息发送。
//!
//! 2026-08 拆分自 `manager.rs`：命令循环与 MIDI 输出逻辑独立成模块，
//! `manager.rs` 仅保留结构定义、线程生命周期与对外公共 API。

use std::sync::Arc;

use crate::OutputConnection;
use crate::playback::engine::{MidiMessage, MidiTrackEvent, PlaybackEngine};
use crate::playback::state::PlaybackState;
use crate::playback::{PlaybackAccessor, TempoChange};
use crossbeam_channel::Sender;
use parking_lot::Mutex;

use super::PlaybackFrame;

/// 播放线程命令
pub(crate) enum Command {
    SetMidiOutput(Box<dyn OutputConnection>),
    ClearMidiOutput,
    RebuildCurrentTrackQueue,
    SetDocument(Arc<lumino_midi_loader::MidiDocument>, u16),
    SetMidiEvents(Vec<MidiTrackEvent>),
    SetTempoChanges(Vec<TempoChange>),
    SetVelocityFilterThreshold(u8),
    /// 设置音轨静音/独奏状态（用于播放过滤：被静音或未被独奏的音轨不出声）
    SetTrackPlayStates(Vec<bool>, Vec<bool>),
    /// 设置某 MIDI 通道的音频域增益（线性，1.0 = 0 dB）
    SetChannelGain(u8, f32),
    /// 设置某 MIDI 通道的音频域声像（-1..1，0 = 居中）
    SetChannelPan(u8, f32),
    // 旧 SetCache/SetSkipTracksInCache 已移除（disk_cache future support）
    Play,
    Pause,
    Stop,
    Seek(f32),
    SetLooping(bool),
    SetLoopRange(f32, f32),
    ClearLoopRange,
    Quit,
}

/// 处理单个播放控制命令。
///
/// 从 `manager::PlaybackManager::new()` 的线程闭包中调用，按命令更新引擎状态和 MIDI 输出连接。
/// `frame_tx` / `last_frame` 用于在状态切换（Play/Pause/Stop）后主动推送一帧，
/// 保证 UI 能立即感知状态变化（无需等待下一播放循环迭代）。
pub(crate) fn handle_command(
    cmd: Command,
    engine: &mut PlaybackEngine,
    midi_output: &mut Option<Box<dyn OutputConnection>>,
    frame_tx: &Sender<PlaybackFrame>,
    last_frame: &Arc<Mutex<Option<PlaybackFrame>>>,
) {
    match cmd {
        Command::SetMidiOutput(output) => *midi_output = Some(output),
        Command::ClearMidiOutput => *midi_output = None,
        Command::RebuildCurrentTrackQueue => engine.rebuild_current_track_queue(),
        Command::SetDocument(doc, track) => engine.set_document(doc, track),
        Command::SetMidiEvents(events) => engine.set_midi_events(events),
        Command::SetTempoChanges(changes) => {
            let mut playback_guard = engine.playback().lock();
            playback_guard.set_tempo_changes(changes);
        }
        Command::SetVelocityFilterThreshold(threshold) => {
            engine.set_velocity_filter_threshold(threshold);
        }
        Command::SetTrackPlayStates(muted, soloed) => {
            // 当前轨发声状态变化时需要重建当前轨队列，使独奏/静音即时生效。
            let was_current_playable = engine.track_should_play(engine.current_track as usize);
            engine.set_track_play_states(muted, soloed);
            let is_current_playable = engine.track_should_play(engine.current_track as usize);
            if was_current_playable != is_current_playable {
                // 从当前播放位置重建当前轨队列（保留已发出事件之后的音符，
                // 丢弃过去已发出、其 NoteOff 不会再补发的悬挂音符）。
                let tick = engine.last_processed_tick.max(0.0);
                engine.rebuild_queue_from_current_track(Some(tick));
                // 当前轨由"发声"转为"静音"：立即静音清理，避免悬挂音符。
                if was_current_playable
                    && engine.is_playing()
                    && let Some(out) = midi_output
                {
                    let _ = out.all_notes_off();
                }
            }
        }
        Command::SetChannelGain(ch, gain) => {
            if let Some(out) = midi_output {
                let _ = out.set_channel_gain(ch, gain);
            }
        }
        Command::SetChannelPan(ch, pan) => {
            if let Some(out) = midi_output {
                let _ = out.set_channel_pan(ch, pan);
            }
        }
        Command::Play => {
            engine.play();
            push_state_frame(engine, frame_tx, last_frame, midi_output.as_deref());
        }
        Command::Pause => {
            engine.pause();
            if let Some(out) = midi_output {
                for ch in 0..16 {
                    let _ = out.control_change(ch, 64, 0);
                }
                let _ = out.all_notes_off();
            }
            push_state_frame(engine, frame_tx, last_frame, midi_output.as_deref());
        }
        Command::Stop => {
            engine.stop();
            if let Some(out) = midi_output {
                let _ = out.all_notes_off();
                let _ = out.reset_control();
            }
            push_state_frame(engine, frame_tx, last_frame, midi_output.as_deref());
        }
        Command::Seek(tick) => {
            // 仅播放中的 seek 可能存在「已 NoteOn 未收到 NoteOff」的悬挂音符，
            // 需要静音清理；停止/暂停状态本就无声（Stop/Pause 路径已清理过），
            // 无条件发送会在拖拽 scrub 等高频 seek 场景向输出设备灌冗余消息。
            if engine.state() == PlaybackState::Playing
                && let Some(out) = midi_output
            {
                let _ = out.all_notes_off();
                let _ = out.reset_control();
            }
            // seek 返回 chase 重放消息（跳转点之前生效的 CC/PC/PB/RPN），
            // 在清理之后发送，保证跳转后音色/弯音/踏板与目标位置一致。
            let chase_messages = engine.seek(tick);
            if !chase_messages.is_empty() {
                flush_midi_messages(&chase_messages, midi_output);
            }
            // 必须补推状态帧：暂停状态下 seek 后播放线程进入空闲分支不再周期推帧，
            // 若不主动推送，last_frame 停留旧 tick，UI 播放头不跳转。
            push_state_frame(engine, frame_tx, last_frame, midi_output.as_deref());
        }
        Command::SetLooping(looping) => engine.set_looping(looping),
        Command::SetLoopRange(start, end) => engine.set_loop_range(start, end),
        Command::ClearLoopRange => engine.clear_loop_range(),
        Command::Quit => {}
    }
}

/// 主动推送一帧状态快照（用于 Play/Pause/Stop 等状态切换后）。
///
/// 状态切换后播放线程可能进入空闲分支（不再每 1ms 推帧），
/// 主动 try_send 一帧保证 UI 立即感知状态变化，绝不阻塞。
fn push_state_frame(
    engine: &PlaybackEngine,
    frame_tx: &Sender<PlaybackFrame>,
    last_frame: &Arc<Mutex<Option<PlaybackFrame>>>,
    midi_output: Option<&dyn OutputConnection>,
) {
    let bpm = engine.lock_playback().map_or(120.0, |p| p.current_bpm());
    let (channel_levels, master_level) = midi_output
        .map(|out| (out.get_channel_levels(), out.get_master_level()))
        .unwrap_or(([0.0; 16], 0.0));
    let frame = PlaybackFrame {
        tick: engine.current_tick(),
        state: engine.state(),
        bpm,
        channel_levels,
        master_level,
    };
    let _ = frame_tx.try_send(frame);
    *last_frame.lock() = Some(frame);
}

/// 将引擎输出的 MIDI 消息发送到 MIDI 输出设备。
pub(crate) fn flush_midi_messages(
    messages: &[MidiMessage],
    midi_output: &mut Option<Box<dyn OutputConnection>>,
) {
    let Some(out) = midi_output else { return };
    let msg_count = messages.len();

    for msg in messages {
        match msg {
            MidiMessage::NoteOn {
                channel,
                key,
                velocity,
            } => {
                let _ = out.note_on(*channel, *key, *velocity);
            }
            MidiMessage::NoteOff { channel, key } => {
                let _ = out.note_off(*channel, *key, 0);
            }
            MidiMessage::ControlChange {
                channel,
                controller,
                value,
            } => {
                tracing::debug!(
                    "PlaybackManager: 发送 CC ch={} cc={} val={}",
                    channel,
                    controller,
                    value,
                );
                let _ = out.control_change(*channel, *controller, *value);
            }
            MidiMessage::ProgramChange { channel, program } => {
                let _ = out.program_change(*channel, *program);
            }
            MidiMessage::PitchBend { channel, value } => {
                let _ = out.pitch_bend(*channel, *value);
            }
            MidiMessage::ChannelPressure { channel, pressure } => {
                let _ = out.channel_pressure(*channel, *pressure);
            }
            MidiMessage::PolyPressure {
                channel,
                key,
                pressure,
            } => {
                let _ = out.poly_pressure(*channel, *key, *pressure);
            }
        }
    }
    if msg_count > 0 {
        tracing::trace!("PlaybackManager: sent {} MIDI events", msg_count);
    }
}
