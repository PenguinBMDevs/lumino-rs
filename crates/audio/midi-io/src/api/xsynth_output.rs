//! XSynth 输出连接实现
//!
//! `XSynthOutputConn` 是 XSynth 合成器后端对外暴露的 MIDI 输出连接。
//! 它通过共享事件发送器（`Arc<Mutex<RealtimeEventSender>>`）向渲染线程发送事件；
//! 音频设备变化触发合成管线重建时，XSynth 会替换共享发送器，
//! 所有已创建的连接自动跟随新管线，无需重新打开连接。

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use crate::realtime::{ChannelMixHandle, RealtimeEventSender, SynthEvent};
use xsynth_core::channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ControlEvent};

use crate::constants::*;
use crate::{Error, OutputConnection, PlaybackOutput};

/// XSynth MIDI 输出连接
pub(crate) struct XSynthOutputConn {
    /// 共享事件发送器（与 XSynth 共用 Arc，重建管线后自动跟随新发送器）
    pub(crate) sender: Arc<Mutex<RealtimeEventSender>>,
    /// 混音参数共享句柄（与 XSynth 共用 Arc，重建管线后自动跟随新句柄）。
    /// 用于音频域每通道增益/声像设置，与 MIDI CC 解耦。
    pub(crate) mixer: ChannelMixHandle,
    /// 主输出实时响度峰值共享句柄（重建管线后自动跟随）。
    pub(crate) master: Arc<AtomicU32>,
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

/// raw 3 字节 MIDI 消息 → `(通道, xsynth 音频事件)`；`None` = 该类型 xsynth 不支持。
///
/// 抽成纯函数的理由：这是「消息类型 → 合成事件」的唯一映射表，必须可单测——
/// 曾经的缺陷正出在这里。
///
/// # `0xD0` / `0xA0` 必须返回 `None`
///
/// `xsynth_core::channel::ChannelAudioEvent` **没有**后触变体（只有 NoteOn/NoteOff/
/// AllNotesOff/AllNotesKilled/ResetControl/Control/ProgramChange/SystemReset）。
/// 旧实现把 `0xD0`（通道后触）映射为 `ControlEvent::Raw(0, b1)`——而 **CC0 就是
/// Bank Select MSB**！后果有三：
/// 1. 通道后触会**改掉该通道的音色库**（听感为音色跳变）；
/// 2. 默认实现把全局通道折叠到低 4 位 ⇒ 多端口下改的是**别的端口的同号通道**；
/// 3. `PercussionTracker` 把 CC0 当 Bank Select 证据 ⇒ 误判打击乐模态。
///
/// 无声丢弃（不产生任何事件）远优于产生错误的 Bank Select。`0xA0` 同理，
/// 与 `lgs.rs` 既有决策一致。
///
/// # `0xE0` 契约
///
/// `PitchBendValue` 为归一化 -1.0..1.0（raw 中心 8192 → 0）；raw 14-bit 直传会把
/// 中心值当作灵敏度倍数（潜在跑调/爆音）。
fn map_raw_message(data: [u8; 3]) -> Option<(u32, ChannelAudioEvent)> {
    let status = data[0] & 0xF0;
    let channel = u32::from(data[0] & 0x0F);
    let (b1, b2) = (data[1], data[2]);

    let audio = match status {
        0x80 => ChannelAudioEvent::NoteOff {
            key: b1 & MIDI_VALUE_MASK,
        },
        0x90 => ChannelAudioEvent::NoteOn {
            key: b1 & MIDI_VALUE_MASK,
            vel: b2 & MIDI_VALUE_MASK,
        },
        0xB0 => ChannelAudioEvent::Control(ControlEvent::Raw(b1, b2)),
        0xC0 => ChannelAudioEvent::ProgramChange(b1),
        0xE0 => {
            let raw = u16::from(b1) | (u16::from(b2) << 7);
            let bend = (f32::from(raw) - 8192.0) / 8192.0;
            ChannelAudioEvent::Control(ControlEvent::PitchBendValue(bend))
        }
        // 0xA0 / 0xD0（后触两种）与其余类型：xsynth 无对应事件，显式丢弃。
        _ => return None,
    };
    Some((channel, audio))
}

impl OutputConnection for XSynthOutputConn {
    fn note_on(&mut self, ch: u16, key: u8, vel: u8) -> Result<(), Error> {
        // REND-002：直接消费全局通道（port*16+channel），由合成层按 Custom
        // 通道数寻址；合成器对越界通道静默忽略，不再做 4bit 折叠。
        let channel = u32::from(ch);
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

    fn note_off(&mut self, ch: u16, key: u8, _vel: u8) -> Result<(), Error> {
        let channel = u32::from(ch);
        self.send_event(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(ChannelAudioEvent::NoteOff {
                key: key & MIDI_VALUE_MASK,
            }),
        ));
        Ok(())
    }

    fn control_change(&mut self, ch: u16, controller: u8, value: u8) -> Result<(), Error> {
        let channel = u32::from(ch);
        self.send_event(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::Raw(
                controller, value,
            ))),
        ));
        Ok(())
    }

    fn program_change(&mut self, ch: u16, program: u8) -> Result<(), Error> {
        let channel = u32::from(ch);
        self.send_event(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(ChannelAudioEvent::ProgramChange(program)),
        ));
        Ok(())
    }

    fn pitch_bend(&mut self, ch: u16, value: f32) -> Result<(), Error> {
        let channel = u32::from(ch);
        self.send_event(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::PitchBendValue(
                value,
            ))),
        ));
        Ok(())
    }

    /// 通道后触：xsynth **无**对应事件类型，显式安静丢弃。
    ///
    /// 不能走默认实现——默认实现会把全局通道 `& 0x0F` 折叠后调
    /// `send_raw([0xD0|ch, …])`：多端口下会落到**别的端口的同号通道**。
    /// 与 `lgs.rs` 既有决策一致（「GPU 合成器不支持，忽略以避免噪声报错」）。
    fn channel_pressure(&mut self, _ch: u16, _pressure: u8) -> Result<(), Error> {
        Ok(())
    }

    /// 复音后触：同上（xsynth 无对应事件）。
    fn poly_pressure(&mut self, _ch: u16, _key: u8, _pressure: u8) -> Result<(), Error> {
        Ok(())
    }

    fn send_raw(&mut self, data: [u8; 3]) -> Result<(), Error> {
        match map_raw_message(data) {
            Some((channel, audio)) => {
                self.send_event(SynthEvent::Channel(channel, ChannelEvent::Audio(audio)));
                Ok(())
            }
            None => Err(Error::SendFailed(format!(
                "xsynth 不支持的消息类型: 0x{:02X}",
                data[0] & 0xF0
            ))),
        }
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

    fn get_channel_levels(&self) -> [f32; 16] {
        let mut levels = [0.0f32; 16];
        if let Ok(mix) = self.mixer.lock() {
            for (i, cm) in mix.iter().enumerate().take(16) {
                levels[i] = f32::from_bits(cm.peak.load(Ordering::Relaxed));
            }
        }
        levels
    }

    fn get_master_level(&self) -> f32 {
        f32::from_bits(self.master.load(Ordering::Relaxed))
    }

    fn close(self: Box<Self>) {
        tracing::debug!("XSynthOutputConn::close: 关闭连接");
    }
}

/// REND-002 方案 B：软件合成后端的播放能力扩展覆写。
impl PlaybackOutput for XSynthOutputConn {
    /// 运行时打击乐模态切换（Bank Select 推导，播放侧下发）。
    fn set_percussion_mode(&mut self, ch: u16, on: bool) -> Result<(), Error> {
        self.send_event(SynthEvent::Channel(
            u32::from(ch),
            ChannelEvent::Config(ChannelConfigEvent::SetPercussionMode(on)),
        ));
        Ok(())
    }

    /// 暂停清理：释放全部全局通道的延音踏板（多端口下覆盖所有端口通道）。
    fn release_all_dampers(&mut self) -> Result<(), Error> {
        self.send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
            ChannelAudioEvent::Control(ControlEvent::Raw(64, 0)),
        )));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **回归守卫**：后触（0xD0 / 0xA0）绝不能被映射成 CC0（Bank Select MSB）。
    ///
    /// 旧实现把 `0xD0` 映射为 `ControlEvent::Raw(0, b1)` ⇒ 通道后触会改掉音色库，
    /// 多端口下还会因通道折叠改错通道、并污染 `PercussionTracker` 的 Bank Select 判定。
    #[test]
    fn aftertouch_is_never_mapped_to_bank_select() {
        assert!(
            map_raw_message([0xD0, 64, 0]).is_none(),
            "通道后触必须显式不支持——映射成 CC0 会改掉 Bank Select"
        );
        assert!(
            map_raw_message([0xA0, 60, 64]).is_none(),
            "复音后触同样必须显式不支持"
        );
        // 反证：合法的 CC0（Bank Select MSB）必须仍然可映射，别把功能一起关掉
        assert!(
            matches!(
                map_raw_message([0xB0, 0, 120]),
                Some((_, ChannelAudioEvent::Control(ControlEvent::Raw(0, 120))))
            ),
            "真正的 CC0 必须继续透传"
        );
    }

    /// 映射表逐字节核对（通道取低 4 位，数据字节按 7bit 掩码）。
    #[test]
    fn raw_messages_map_to_expected_events() {
        let (ch, ev) = map_raw_message([0x90 | 5, 60, 100]).expect("NoteOn 应支持");
        assert_eq!(ch, 5);
        assert!(matches!(
            ev,
            ChannelAudioEvent::NoteOn { key: 60, vel: 100 }
        ));

        let (ch, ev) = map_raw_message([0x80 | 9, 60, 0]).expect("NoteOff 应支持");
        assert_eq!(ch, 9);
        assert!(matches!(ev, ChannelAudioEvent::NoteOff { key: 60 }));

        let (ch, ev) = map_raw_message([0xB0 | 3, 7, 100]).expect("CC 应支持");
        assert_eq!(ch, 3);
        assert!(matches!(
            ev,
            ChannelAudioEvent::Control(ControlEvent::Raw(7, 100))
        ));

        let (_, ev) = map_raw_message([0xC0 | 2, 42, 0]).expect("PC 应支持");
        assert!(matches!(ev, ChannelAudioEvent::ProgramChange(42)));
    }

    /// 弯音中心（raw 0x2000：lsb 0 / msb 64）必须归一化为 0.0。
    #[test]
    fn pitch_bend_center_is_zero() {
        let (_, ev) = map_raw_message([0xE0, 0, 64]).expect("PB 应支持");
        match ev {
            ChannelAudioEvent::Control(ControlEvent::PitchBendValue(v)) => {
                assert!(v.abs() < 1e-6, "中心弯音必须为 0，实际 {v}");
            }
            _ => panic!("中心弯音应映射为 PitchBendValue"),
        }
    }
}
