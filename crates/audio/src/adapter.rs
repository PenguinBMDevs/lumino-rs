//! OutputConnection 适配器 — 让 CpalAudioHandle 兼容旧的 PlaybackManager。
//!
//! 将 `OutputConnection` trait 的 `send_raw([u8; 3])` 调用解析为 MIDI 消息，
//! 转换为 `AudioCommand` 发送到 `CpalAudioHandle`。
//!
//! 这样 PlaybackManager 的 1ms poll 线程和 handle_audio_action 都无需改动，
//! 只是底层从 xsynth-realtime 换成了新的 ring-buffer 引擎。

use crossbeam_channel::Sender;
use lumino_midi_io::{Error, OutputConnection};

use crate::spawn::AudioCommand;

/// MIDI 状态字节常量
const STATUS_NOTE_OFF: u8 = 0x80;
const STATUS_NOTE_ON: u8 = 0x90;
const STATUS_CONTROL_CHANGE: u8 = 0xB0;
const STATUS_PROGRAM_CHANGE: u8 = 0xC0;
const STATUS_CHANNEL_PRESSURE: u8 = 0xD0;
const STATUS_PITCH_BEND: u8 = 0xE0;
const STATUS_POLY_PRESSURE: u8 = 0xA0;

const MIDI_CHANNEL_MASK: u8 = 0x0F;
const CC_ALL_NOTES_OFF: u8 = 123;
const CC_RESET_ALL_CONTROLLERS: u8 = 121;

/// 适配器 — 包装 AudioCommand Sender，实现 OutputConnection trait。
pub struct AudioCommandAdapter {
    cmd_tx: Sender<AudioCommand>,
}

impl AudioCommandAdapter {
    pub fn new(cmd_tx: Sender<AudioCommand>) -> Self {
        Self { cmd_tx }
    }
}

impl OutputConnection for AudioCommandAdapter {
    fn note_on(&mut self, ch: u8, key: u8, vel: u8) -> Result<(), Error> {
        let channel = ch & MIDI_CHANNEL_MASK;
        let velocity = if vel == 0 { 1 } else { vel };
        let _ = self.cmd_tx.send(AudioCommand::NoteOn {
            channel,
            key,
            velocity,
        });
        Ok(())
    }

    fn note_off(&mut self, ch: u8, key: u8, _vel: u8) -> Result<(), Error> {
        let channel = ch & MIDI_CHANNEL_MASK;
        let _ = self.cmd_tx.send(AudioCommand::NoteOff { channel, key });
        Ok(())
    }

    fn control_change(&mut self, ch: u8, controller: u8, value: u8) -> Result<(), Error> {
        let channel = ch & MIDI_CHANNEL_MASK;
        let _ = self.cmd_tx.send(AudioCommand::ControlChange {
            channel,
            controller,
            value,
        });
        Ok(())
    }

    fn program_change(&mut self, ch: u8, program: u8) -> Result<(), Error> {
        let channel = ch & MIDI_CHANNEL_MASK;
        let _ = self
            .cmd_tx
            .send(AudioCommand::ProgramChange { channel, program });
        Ok(())
    }

    fn pitch_bend(&mut self, ch: u8, value: f32) -> Result<(), Error> {
        let channel = ch & MIDI_CHANNEL_MASK;
        let _ = self.cmd_tx.send(AudioCommand::PitchBend { channel, value });
        Ok(())
    }

    fn send_raw(&mut self, data: [u8; 3]) -> Result<(), Error> {
        let status = data[0];
        let channel = status & MIDI_CHANNEL_MASK;
        match status & 0xF0 {
            STATUS_NOTE_ON if data[2] > 0 => {
                let _ = self.cmd_tx.send(AudioCommand::NoteOn {
                    channel,
                    key: data[1],
                    velocity: data[2],
                });
            }
            STATUS_NOTE_ON | STATUS_NOTE_OFF => {
                let _ = self.cmd_tx.send(AudioCommand::NoteOff {
                    channel,
                    key: data[1],
                });
            }
            STATUS_CONTROL_CHANGE => {
                let _ = self.cmd_tx.send(AudioCommand::ControlChange {
                    channel,
                    controller: data[1],
                    value: data[2],
                });
            }
            STATUS_PROGRAM_CHANGE => {
                let _ = self.cmd_tx.send(AudioCommand::ProgramChange {
                    channel,
                    program: data[1],
                });
            }
            STATUS_PITCH_BEND => {
                let raw = (u16::from(data[2]) << 7) | u16::from(data[1]);
                let value = (raw as f32 / 8192.0) - 1.0;
                let _ = self.cmd_tx.send(AudioCommand::PitchBend { channel, value });
            }
            STATUS_CHANNEL_PRESSURE | STATUS_POLY_PRESSURE => {
                // Channel/Poly pressure 暂不支持，静默忽略
            }
            _ => {}
        }
        Ok(())
    }

    fn all_notes_off(&mut self) -> Result<(), Error> {
        let _ = self.cmd_tx.send(AudioCommand::AllNotesOff);
        Ok(())
    }

    fn reset_control(&mut self) -> Result<(), Error> {
        let _ = self.cmd_tx.send(AudioCommand::ResetAll);
        Ok(())
    }

    fn close(self: Box<Self>) {
        // 通道在 Drop 时自动关闭
    }
}
