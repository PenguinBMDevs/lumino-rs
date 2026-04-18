//! 播放引擎调度器

use std::collections::BinaryHeap;

use crate::playback::PlaybackState;

use super::{EventType, MidiMessage, NoteEvent, ScheduledEvent};

/// 调度器（负责音符事件优先级队列管理）
pub struct Scheduler {
    /// 待播放的事件队列（优先队列，按tick排序）
    event_queue: BinaryHeap<ScheduledEvent>,
    /// 所有音符
    notes: Vec<NoteEvent>,
}

impl Scheduler {
    /// 创建新的调度器
    pub fn new() -> Self {
        Self {
            event_queue: BinaryHeap::new(),
            notes: Vec::new(),
        }
    }

    /// 设置音符列表
    pub fn set_notes(&mut self, notes: Vec<NoteEvent>) {
        self.notes = notes;
        self.rebuild_queue();
    }

    /// 重建事件队列
    pub fn rebuild_queue(&mut self) {
        self.event_queue.clear();

        for note in &self.notes {
            self.event_queue.push(ScheduledEvent {
                tick: note.tick,
                event_type: EventType::NoteOn {
                    channel: note.channel,
                    key: note.key,
                    velocity: note.velocity,
                },
            });

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
    pub fn update(&mut self, current_tick: f32, is_playing: bool) -> Vec<MidiMessage> {
        let mut messages = Vec::new();

        if !is_playing {
            return messages;
        }

        while let Some(event) = self.event_queue.peek() {
            if event.tick > current_tick {
                break;
            }

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

        messages
    }

    /// 获取事件数量（用于测试）
    #[expect(dead_code, reason = "用于测试")]
    pub fn len(&self) -> usize {
        self.event_queue.len()
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}
