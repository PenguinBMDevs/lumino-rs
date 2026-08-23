//! XSynth 输出连接实现
//!
//! `XSynthOutputConn` 是 XSynth 合成器后端对外暴露的 MIDI 输出连接。
//! 它通过共享事件发送器（`Arc<Mutex<RealtimeEventSender>>`）向渲染线程发送事件；
//! 音频设备变化触发合成管线重建时，XSynth 会替换共享发送器，
//! 所有已创建的连接自动跟随新管线，无需重新打开连接。

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use crate::realtime::{ChannelMixHandle, RealtimeEventSender, SynthEvent};
use xsynth_core::channel::{ChannelAudioEvent, ChannelEvent, ControlEvent};

use crate::constants::*;
use crate::{Error, OutputConnection};

/// XSynth MIDI 输出连接
pub(crate) struct XSynthOutputConn {
    /// 共享事件发送器（与 XSynth 共用 Arc，重建管线后自动跟随新发送器）
    pub(crate) sender: Arc<Mutex<RealtimeEventSender>>,
    /// 混音参数共享句柄（与 XSynth 共用 Arc，重建管线后自动跟随新句柄）。
    /// 用于音频域每通道增益/声像设置，与 MIDI CC 解耦。
    pub(crate) mixer: ChannelMixHandle,
}

impl XSynthOutputConn {
    /// 发送事件到渲染线程 — 通过 xsynth-realtime 的 RealtimeEventSender。
    fn send_event(&self, event: SynthEvent) {
        self.sender
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .send_event(event);
    }
}

impl OutputConnection for XSynthOutputConn {
    fn note_on(&mut self, ch: u8, key: u8, vel: u8) -> Result<(), Error> {
        let channel = (ch & MIDI_CHANNEL_MASK) as u32;
        let velocity = if vel == 0 { 1 } else { vel };
        self.send_event(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(ChannelAudioEvent::NoteOn {
                key: key & MIDI_VALUE_MASK,
                vel: velocity & MIDI_VALUE_MASK,
            }),
        ));
        Ok(())
    }

    fn note_off(&mut self, ch: u8, key: u8, _vel: u8) -> Result<(), Error> {
        let channel = (ch & MIDI_CHANNEL_MASK) as u32;
        self.send_event(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(ChannelAudioEvent::NoteOff {
                key: key & MIDI_VALUE_MASK,
            }),
        ));
        Ok(())
    }

    fn control_change(&mut self, ch: u8, controller: u8, value: u8) -> Result<(), Error> {
        let channel = (ch & MIDI_CHANNEL_MASK) as u32;
        self.send_event(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::Raw(
                controller, value,
            ))),
        ));
        Ok(())
    }

    fn program_change(&mut self, ch: u8, program: u8) -> Result<(), Error> {
        let channel = (ch & MIDI_CHANNEL_MASK) as u32;
        self.send_event(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(ChannelAudioEvent::ProgramChange(program)),
        ));
        Ok(())
    }

    fn pitch_bend(&mut self, ch: u8, value: f32) -> Result<(), Error> {
        let channel = (ch & MIDI_CHANNEL_MASK) as u32;
        self.send_event(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::PitchBendValue(
                value,
            ))),
        ));
        Ok(())
    }

    fn send_raw(&mut self, data: [u8; 3]) -> Result<(), Error> {
        let status = data[0] & 0xF0;
        let channel = (data[0] & 0x0F) as u32;
        let b1 = data[1];
        let b2 = data[2];

        match status {
            0x80 => self.send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::NoteOff {
                    key: b1 & MIDI_VALUE_MASK,
                }),
            )),
            0x90 => self.send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::NoteOn {
                    key: b1 & MIDI_VALUE_MASK,
                    vel: b2 & MIDI_VALUE_MASK,
                }),
            )),
            0xB0 => self.send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::Raw(b1, b2))),
            )),
            0xC0 => self.send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::ProgramChange(b1)),
            )),
            0xD0 => self.send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::Raw(0, b1))),
            )),
            0xE0 => {
                let bend = ((b1 as u16) | ((b2 as u16) << 7)) as f32;
                self.send_event(SynthEvent::Channel(
                    channel,
                    ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::PitchBendValue(
                        bend,
                    ))),
                ));
            }
            _ => {
                return Err(Error::SendFailed(format!(
                    "xsynth 不支持的消息类型: 0x{:02X}",
                    status
                )));
            }
        };
        Ok(())
    }

    fn all_notes_off(&mut self) -> Result<(), Error> {
        self.send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
            ChannelAudioEvent::AllNotesOff,
        )));
        Ok(())
    }

    fn reset_control(&mut self) -> Result<(), Error> {
        self.send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
            ChannelAudioEvent::ResetControl,
        )));
        Ok(())
    }

    fn set_channel_gain(&mut self, ch: u8, gain: f32) -> Result<(), Error> {
        let mix = self.mixer.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cm) = mix.get(ch as usize) {
            cm.gain.store(gain.max(0.0).to_bits(), Ordering::Relaxed);
        }
        Ok(())
    }

    fn set_channel_pan(&mut self, ch: u8, pan: f32) -> Result<(), Error> {
        let mix = self.mixer.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cm) = mix.get(ch as usize) {
            cm.pan
                .store(pan.clamp(-1.0, 1.0).to_bits(), Ordering::Relaxed);
        }
        Ok(())
    }

    fn close(self: Box<Self>) {
        tracing::debug!("XSynthOutputConn::close: 关闭连接");
    }
}
