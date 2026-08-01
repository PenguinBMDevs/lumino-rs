use std::{io, ops::RangeInclusive, sync::Arc};

use crossbeam_channel::Sender;
use xsynth_core::channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent};

use crate::util::ReadWriteAtomicU64;

use super::nps::{should_send_for_vel_and_nps, RoughNpsTracker};

pub(super) struct EventSender {
    sender: Sender<ChannelEvent>,
    nps: RoughNpsTracker,
    max_nps: Arc<ReadWriteAtomicU64>,
    skipped_notes: [u64; 128],
    ignore_range: RangeInclusive<u8>,
}

impl EventSender {
    pub(super) fn new(
        max_nps: Arc<ReadWriteAtomicU64>,
        sender: Sender<ChannelEvent>,
        ignore_range: RangeInclusive<u8>,
    ) -> Result<Self, io::Error> {
        Ok(EventSender {
            sender,
            nps: RoughNpsTracker::new()?,
            max_nps,
            skipped_notes: [0; 128],
            ignore_range,
        })
    }

    pub(super) fn send_audio(&mut self, event: ChannelAudioEvent) {
        match &event {
            ChannelAudioEvent::NoteOn { vel, key } => {
                if *key > 127 {
                    return;
                }

                let nps = self.nps.calculate_nps();
                if should_send_for_vel_and_nps(*vel, nps, self.max_nps.read())
                    && !self.ignore_range.contains(vel)
                {
                    self.sender.send(ChannelEvent::Audio(event)).ok();
                    self.nps.add_note();
                } else {
                    self.skipped_notes[*key as usize] += 1;
                }
            }
            ChannelAudioEvent::NoteOff { key } => {
                if *key > 127 {
                    return;
                }

                if self.skipped_notes[*key as usize] > 0 {
                    self.skipped_notes[*key as usize] -= 1;
                } else {
                    self.sender.send(ChannelEvent::Audio(event)).ok();
                }
            }
            _ => {
                self.sender.send(ChannelEvent::Audio(event)).ok();
            }
        }
    }

    pub(super) fn send_config(&mut self, event: ChannelConfigEvent) {
        self.sender.send(ChannelEvent::Config(event)).ok();
    }

    pub(super) fn reset_skipped_notes(&mut self) {
        self.skipped_notes = [0; 128];
    }

    pub(super) fn set_ignore_range(&mut self, ignore_range: RangeInclusive<u8>) {
        self.ignore_range = ignore_range;
    }
}

impl Clone for EventSender {
    fn clone(&self) -> Self {
        EventSender {
            sender: self.sender.clone(),
            max_nps: self.max_nps.clone(),
            nps: RoughNpsTracker::new().unwrap_or_else(|_| RoughNpsTracker::disabled()),
            skipped_notes: [0; 128],
            ignore_range: self.ignore_range.clone(),
        }
    }
}
