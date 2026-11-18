//! Maestro 输出连接：把 Lumino 的全局通道事件翻译成 Maestro 短消息。

use std::sync::Arc;

use maestro_api_rs::{ErrorKind, Realtime, TIME_NOW, short_message};

use crate::constants::*;
use crate::{Error, OutputConnection, PlaybackOutput};

use super::{MaestroShared, channel_parts, pitch_bend_value};

/// Maestro 输出连接。
///
/// 所有连接共享同一个引擎；引擎在文档端口布局变化时热替换，连接通过
/// `generation` 感知并重新读取，不需要重建播放管线。
pub struct MaestroOutputConn {
    shared: Arc<MaestroShared>,
    cached: Option<Arc<Realtime>>,
    cached_generation: u64,
}

impl MaestroOutputConn {
    pub(super) fn new(shared: Arc<MaestroShared>) -> Self {
        Self {
            shared,
            cached: None,
            cached_generation: u64::MAX,
        }
    }

    /// 取当前引擎；代数变化时重新读取一次（稳态零锁）。
    fn current(&mut self) -> Option<Arc<Realtime>> {
        let generation = self.shared.generation();
        if self.cached.is_none() || self.cached_generation != generation {
            self.cached = self.shared.current_engine();
            self.cached_generation = generation;
        }
        self.cached.clone()
    }

    fn send_short(&mut self, port: u8, msg: u32) -> Result<(), Error> {
        let Some(realtime) = self.current() else {
            // 引擎重建窗口：丢弃事件，不阻塞播放线程。
            return Ok(());
        };
        match realtime.send_short(port, msg, TIME_NOW) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotRunning => {
                tracing::debug!("Maestro: 引擎重建中，丢弃事件");
                Ok(())
            }
            Err(error) => Err(Error::SendFailed(format!("Maestro send_short: {error}"))),
        }
    }

    /// 全局通道事件 → 短消息；`status` 为不含通道位的基值。
    fn send_channel(&mut self, ch: u16, status: u8, d1: u8, d2: u8) -> Result<(), Error> {
        let (port, channel) = channel_parts(ch);
        self.send_short(port, short_message(status | channel, d1, d2))
    }

    fn notes_off(&mut self) -> Result<(), Error> {
        let Some(realtime) = self.current() else {
            return Ok(());
        };
        match realtime.notes_off() {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotRunning => Ok(()),
            Err(error) => Err(Error::SendFailed(format!("Maestro notes_off: {error}"))),
        }
    }

    fn reset(&mut self) -> Result<(), Error> {
        let Some(realtime) = self.current() else {
            return Ok(());
        };
        match realtime.reset() {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotRunning => Ok(()),
            Err(error) => Err(Error::SendFailed(format!("Maestro reset: {error}"))),
        }
    }
}

impl OutputConnection for MaestroOutputConn {
    fn note_on(&mut self, ch: u16, key: u8, vel: u8) -> Result<(), Error> {
        // 与既有后端一致：vel=0 在默认实现里被抬到 1，不转 NoteOff。
        let velocity = if vel == 0 { 1 } else { vel };
        self.send_channel(
            ch,
            STATUS_NOTE_ON,
            key & MIDI_VALUE_MASK,
            velocity & MIDI_VALUE_MASK,
        )
    }

    fn note_off(&mut self, ch: u16, key: u8, vel: u8) -> Result<(), Error> {
        self.send_channel(
            ch,
            STATUS_NOTE_OFF,
            key & MIDI_VALUE_MASK,
            vel & MIDI_VALUE_MASK,
        )
    }

    fn control_change(&mut self, ch: u16, controller: u8, value: u8) -> Result<(), Error> {
        self.send_channel(
            ch,
            STATUS_CONTROL_CHANGE,
            controller & MIDI_VALUE_MASK,
            value & MIDI_VALUE_MASK,
        )
    }

    fn program_change(&mut self, ch: u16, program: u8) -> Result<(), Error> {
        self.send_channel(ch, STATUS_PROGRAM_CHANGE, program & MIDI_VALUE_MASK, 0)
    }

    fn pitch_bend(&mut self, ch: u16, value: f32) -> Result<(), Error> {
        let (lsb, msb) = pitch_bend_value(value);
        self.send_channel(ch, STATUS_PITCH_BEND, lsb, msb)
    }

    fn channel_pressure(&mut self, ch: u16, pressure: u8) -> Result<(), Error> {
        self.send_channel(ch, STATUS_CHANNEL_PRESSURE, pressure & MIDI_VALUE_MASK, 0)
    }

    fn poly_pressure(&mut self, ch: u16, key: u8, pressure: u8) -> Result<(), Error> {
        self.send_channel(
            ch,
            STATUS_POLY_PRESSURE,
            key & MIDI_VALUE_MASK,
            pressure & MIDI_VALUE_MASK,
        )
    }

    /// 原始 3 字节消息没有全局端口信息；按 port 0 兜底（多端口播放走高层方法）。
    fn send_raw(&mut self, data: [u8; 3]) -> Result<(), Error> {
        self.send_short(
            0,
            short_message(
                data[0],
                data[1] & MIDI_VALUE_MASK,
                data[2] & MIDI_VALUE_MASK,
            ),
        )
    }

    fn all_notes_off(&mut self) -> Result<(), Error> {
        self.notes_off()
    }

    fn reset_control(&mut self) -> Result<(), Error> {
        // Maestro 的 reset 同时清队列/音符/控制器；停止与 seek 需要这个语义。
        self.reset()
    }

    fn close(self: Box<Self>) {
        tracing::debug!("Maestro: 输出连接已关闭");
    }
}

impl PlaybackOutput for MaestroOutputConn {
    /// 暂停清理：`notes_off` 覆盖全部端口；Maestro 会保留控制器/程序与队列。
    fn release_all_dampers(&mut self) -> Result<(), Error> {
        self.notes_off()
    }

    /// Maestro 的 nps/eps 是瞬时速率，不是累计计数；无需清零。
    fn reset_playback_stats(&mut self) -> Result<(), Error> {
        Ok(())
    }
}
