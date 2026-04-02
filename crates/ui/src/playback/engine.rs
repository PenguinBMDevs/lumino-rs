//! 播放引擎模块
//!
//! 负责音符调度和MIDI输出

use super::{Playback, PlaybackAccessor, PlaybackState};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::{Arc, Mutex};

/// 音符事件（用于播放调度）
#[derive(Debug, Clone)]
pub struct NoteEvent {
    /// 事件时刻（tick）
    pub tick: f32,
    /// MIDI通道
    pub channel: u8,
    /// 音高
    pub key: u8,
    /// 力度
    pub velocity: u8,
    /// 音符长度（tick）
    pub length: f32,
}

/// 调度的音符事件（内部使用）
#[derive(Debug, Clone)]
struct ScheduledEvent {
    tick: f32,
    event_type: EventType,
}

#[derive(Debug, Clone)]
enum EventType {
    NoteOn { channel: u8, key: u8, velocity: u8 },
    NoteOff { channel: u8, key: u8 },
}

impl PartialEq for ScheduledEvent {
    fn eq(&self, other: &Self) -> bool {
        self.tick == other.tick
    }
}

impl Eq for ScheduledEvent {}

impl PartialOrd for ScheduledEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // 反向排序，让小的tick先出队（BinaryHeap 是最大堆）
        Some(other.tick.total_cmp(&self.tick))
    }
}

impl Ord for ScheduledEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        // 直接使用 total_cmp 避免 unwrap
        other.tick.total_cmp(&self.tick)
    }
}

/// 播放引擎
pub struct PlaybackEngine {
    /// 播放器
    playback: Arc<Mutex<Playback>>,
    /// 待播放的事件队列（优先队列，按tick排序）
    event_queue: BinaryHeap<ScheduledEvent>,
    /// 所有音符
    notes: Vec<NoteEvent>,
    /// 循环播放
    looping: bool,
    /// 循环范围（开始tick，结束tick）
    loop_range: Option<(f32, f32)>,
}

impl PlaybackEngine {
    /// 创建新的播放引擎
    pub fn new(playback: Arc<Mutex<Playback>>) -> Self {
        Self {
            playback,
            event_queue: BinaryHeap::new(),
            notes: Vec::new(),
            looping: false,
            loop_range: None,
        }
    }

    /// 设置音符列表
    pub fn set_notes(&mut self, notes: Vec<NoteEvent>) {
        self.notes = notes;
        self.rebuild_queue();
    }

    /// 设置循环
    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    /// 设置循环范围
    pub fn set_loop_range(&mut self, start: f32, end: f32) {
        self.loop_range = Some((start, end));
    }

    /// 清除循环范围
    pub fn clear_loop_range(&mut self) {
        self.loop_range = None;
    }

    /// 重建事件队列
    fn rebuild_queue(&mut self) {
        self.event_queue.clear();

        for note in &self.notes {
            // NoteOn事件
            self.event_queue.push(ScheduledEvent {
                tick: note.tick,
                event_type: EventType::NoteOn {
                    channel: note.channel,
                    key: note.key,
                    velocity: note.velocity,
                },
            });

            // NoteOff事件
            self.event_queue.push(ScheduledEvent {
                tick: note.tick + note.length,
                event_type: EventType::NoteOff {
                    channel: note.channel,
                    key: note.key,
                },
            });
        }
    }

    /// 处理播放更新
    ///
    /// 返回：需要发送的MIDI消息列表
    pub fn update(&mut self) -> Vec<MidiMessage> {
        let mut messages = Vec::new();

        // 获取当前播放状态和tick，然后立即释放锁
        let (current_tick, is_playing) = {
            let Some(playback) = self.lock_playback() else {
                return messages;
            };
            (
                playback.current_tick(),
                playback.state() == PlaybackState::Playing,
            )
        };

        if !is_playing {
            return messages;
        }

        // 处理所有到期的事件
        while let Some(event) = self.event_queue.peek() {
            if event.tick > current_tick {
                break;
            }

            // SAFETY: peek() 返回 Some 时 pop() 一定返回 Some，因为这是单线程操作
            let event = if let Some(e) = self.event_queue.pop() {
                e
            } else {
                tracing::error!("播放引擎: 事件队列状态不一致，peek 有值但 pop 失败");
                break;
            };

            match event.event_type {
                EventType::NoteOn {
                    channel,
                    key,
                    velocity,
                } => {
                    messages.push(MidiMessage::NoteOn {
                        channel,
                        key,
                        velocity,
                    });
                }
                EventType::NoteOff { channel, key } => {
                    messages.push(MidiMessage::NoteOff { channel, key });
                }
            }
        }

        // 检查循环（需要在独立的作用域中处理锁）
        if self.looping
            && let Some((loop_start, loop_end)) = self.loop_range
            && current_tick >= loop_end
        {
            // 先 seek 再 rebuild_queue，避免借用冲突
            self.seek_playback(loop_start);
            self.rebuild_queue();
        }

        messages
    }

    /// 安全地跳转播放位置（内部辅助方法）
    fn seek_playback(&self, tick: f32) {
        if let Some(mut playback) = self.lock_playback() {
            playback.seek(tick);
            // playback 在此处 drop
        }
    }

    /// 播放
    pub fn play(&mut self) {
        // 获取状态后立即释放锁
        let state = self
            .lock_playback()
            .map_or(PlaybackState::Stopped, |p| p.state());

        if state == PlaybackState::Stopped {
            self.rebuild_queue();
        }

        // 重新获取锁来执行 play
        if let Some(mut playback) = self.lock_playback() {
            playback.play();
        }
    }

    /// 暂停
    pub fn pause(&mut self) {
        if let Some(mut playback) = self.lock_playback() {
            playback.pause();
        }
    }

    /// 停止
    pub fn stop(&mut self) {
        // 先停止播放
        if let Some(mut playback) = self.lock_playback() {
            playback.stop();
            // playback 在此处 drop
        }
        // 然后重建队列
        self.rebuild_queue();
    }

    /// 跳转
    pub fn seek(&mut self, tick: f32) {
        // 先跳转
        self.seek_playback(tick);
        // 然后重建队列
        self.rebuild_queue();
    }

    /// 获取播放状态
    pub fn state(&self) -> PlaybackState {
        self.lock_playback()
            .map_or(PlaybackState::Stopped, |p| p.state())
    }

    /// 获取当前tick
    pub fn current_tick(&self) -> f32 {
        self.lock_playback().map_or(0.0, |p| p.current_tick())
    }
}

impl PlaybackAccessor for PlaybackEngine {
    fn playback(&self) -> &Arc<Mutex<Playback>> {
        &self.playback
    }
}

/// MIDI消息（简化版）
#[derive(Debug, Clone)]
pub enum MidiMessage {
    NoteOn { channel: u8, key: u8, velocity: u8 },
    NoteOff { channel: u8, key: u8 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::Playback;

    #[test]
    fn test_event_scheduling() {
        let playback = Arc::new(Mutex::new(Playback::new(480)));
        let mut engine = PlaybackEngine::new(playback);

        // 添加一些测试音符
        engine.set_notes(vec![
            NoteEvent {
                tick: 0.0,
                channel: 0,
                key: 60,
                velocity: 100,
                length: 480.0,
            },
            NoteEvent {
                tick: 480.0,
                channel: 0,
                key: 64,
                velocity: 100,
                length: 480.0,
            },
        ]);

        // 应该有4个事件（2个NoteOn + 2个NoteOff）
        assert_eq!(engine.event_queue.len(), 4);
    }
}
