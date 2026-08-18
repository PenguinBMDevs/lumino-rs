//! 每轨前进游标（从 `streaming.rs` 拆分）
//!
//! 维护一个零拷贝迭代器 + 预读的下一个事件，供 `StreamingMidiPlayer` 多轨互锁使用。

use midly::mmap::{MmapEventIter, MmapTrack};
use midly::{TrackEvent, TrackEventKind};

/// 每轨前进游标。
///
/// 维护一个零拷贝迭代器 + 预读的下一个事件。
pub(crate) struct TrackCursor<'a> {
    iter: MmapEventIter<'a>,
    current_tick: u64,
    peeked_delta: u32,
    peeked_event: Option<Result<TrackEvent<'a>, midly::Error>>,
    pub(crate) exhausted: bool,
}

impl<'a> TrackCursor<'a> {
    pub(crate) fn new(track: &MmapTrack<'a>) -> Self {
        Self {
            iter: track.iter(),
            current_tick: 0,
            peeked_delta: 0,
            peeked_event: None,
            exhausted: false,
        }
    }

    /// 确保 `peeked_event` 有值。轨道耗尽时设 `exhausted = true`。
    pub(crate) fn ensure_peeked(&mut self) {
        if self.exhausted || self.peeked_event.is_some() {
            return;
        }
        match self.iter.next() {
            Some(Ok(ev)) => {
                self.peeked_delta = u32::from(ev.delta);
                self.peeked_event = Some(Ok(ev));
            }
            Some(Err(e)) => self.peeked_event = Some(Err(e)),
            None => self.exhausted = true,
        }
    }

    /// 下一个事件的绝对 tick。耗尽时返回 `u64::MAX`。
    pub(crate) fn next_tick(&self) -> u64 {
        if self.exhausted || self.peeked_event.is_none() {
            u64::MAX
        } else {
            self.current_tick + self.peeked_delta as u64
        }
    }

    /// 消费当前 peek 事件并预读下一个。
    /// 返回 `(delta, TrackEventKind)`。
    pub(crate) fn consume(&mut self) -> Option<Result<(u32, TrackEventKind<'a>), midly::Error>> {
        let ev = self.peeked_event.take()?;
        match ev {
            Ok(e) => {
                let delta = u32::from(e.delta);
                let kind = e.kind;
                self.current_tick += delta as u64;
                self.ensure_peeked();
                Some(Ok((delta, kind)))
            }
            Err(err) => {
                self.ensure_peeked();
                Some(Err(err))
            }
        }
    }
}
