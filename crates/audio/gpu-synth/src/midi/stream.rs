//! True streaming MIDI — file is never fully loaded.
//!
//! Old `MidiStream` held `_raw: Vec<u8>` (800 MB) + `Smf` tracks (≈1 GB) +
//! `Vec<TimedEvent>` (1.6 GB) → >3 GB. This rewrite is `O(tracks + block)`:
//! header + track offsets are read via `memmap2` (zero-copy, dropped after
//! scan), each track is streamed via 8 KiB `BufReader<File>`; heap holds one
//! event per track.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use memmap2::Mmap;

use crate::SynthError;
use crate::midi::{TimedEvent, kind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HeapItem {
    sample: u32,
    track_idx: usize,
    packed: u32,
}
impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sample
            .cmp(&other.sample)
            .then_with(|| self.track_idx.cmp(&other.track_idx))
    }
}
impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn read_be_u16(b: &[u8]) -> u16 {
    u16::from_be_bytes([b[0], b[1]])
}
fn read_be_u32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

struct TrackStream {
    reader: BufReader<File>,
    track_end: u64,
    // 已消费字节的逻辑位置（`BufReader` 会预读，底层 fd 位置不可信，小文件会误判结束）。
    pos: u64,
    tick: u64,
    running_status: Option<u8>,
}
impl TrackStream {
    fn new(path: &Path, offset: u64, length: u32) -> Result<Self, SynthError> {
        let mut f = File::open(path).map_err(SynthError::Io)?;
        f.seek(SeekFrom::Start(offset)).map_err(SynthError::Io)?;
        Ok(Self {
            reader: BufReader::with_capacity(8 * 1024, f),
            track_end: offset + length as u64,
            pos: offset,
            tick: 0,
            running_status: None,
        })
    }
    fn is_finished(&self) -> bool {
        self.pos >= self.track_end
    }
    fn read_u8(&mut self) -> Result<u8, SynthError> {
        let mut b = [0u8; 1];
        self.reader.read_exact(&mut b).map_err(SynthError::Io)?;
        self.pos += 1;
        Ok(b[0])
    }
    fn read_vlq(&mut self) -> Result<u32, SynthError> {
        let mut v = 0u32;
        loop {
            let b = self.read_u8()?;
            v = (v << 7) | (b & 0x7F) as u32;
            if b & 0x80 == 0 {
                break;
            }
            if v > 0x0FFF_FFFF {
                return Err(SynthError::Midi("VLQ overflow".into()));
            }
        }
        Ok(v)
    }
    fn read_bytes(&mut self, n: usize) -> Result<Vec<u8>, SynthError> {
        let mut buf = vec![0u8; n];
        self.reader.read_exact(&mut buf).map_err(SynthError::Io)?;
        self.pos += n as u64;
        Ok(buf)
    }
    fn next_midi(&mut self) -> Result<Option<(u8, u32, u32)>, SynthError> {
        loop {
            if self.is_finished() {
                return Ok(None);
            }
            let delta = self.read_vlq()?;
            self.tick += delta as u64;
            let status = self.read_u8()?;
            let (ev_status, first_data) = if status < 0x80 {
                let rs = self
                    .running_status
                    .ok_or_else(|| SynthError::Midi("running status without prior".into()))?;
                (rs, Some(status))
            } else if status < 0xF0 {
                self.running_status = Some(status);
                (status, None)
            } else {
                (status, None)
            };
            match ev_status {
                0xFF => {
                    // 元事件类型字节仅用于推进流位置（各类元事件均跳过 `len` 字节）。
                    let _meta_type = if let Some(b) = first_data {
                        b
                    } else {
                        self.read_u8()?
                    };
                    let len = self.read_vlq()? as usize;
                    if len > 0 {
                        self.read_bytes(len)?;
                    }
                    continue;
                }
                0xF0 | 0xF7 => {
                    let len = if let Some(first) = first_data {
                        let mut v = (first & 0x7F) as u32;
                        if first & 0x80 != 0 {
                            loop {
                                let b = self.read_u8()?;
                                v = (v << 7) | (b & 0x7F) as u32;
                                if b & 0x80 == 0 {
                                    break;
                                }
                            }
                        }
                        v as usize
                    } else {
                        self.read_vlq()? as usize
                    };
                    if len > 0 {
                        let to_skip = if first_data.is_some() {
                            len.saturating_sub(1)
                        } else {
                            len
                        };
                        if to_skip > 0 {
                            self.read_bytes(to_skip)?;
                        }
                    }
                    continue;
                }
                _ if (0x80..0xF0).contains(&ev_status) => {
                    let ch = ev_status & 0x0F;
                    let nib = ev_status & 0xF0;
                    match nib {
                        0x80 => {
                            let key = if let Some(b) = first_data {
                                b
                            } else {
                                self.read_u8()?
                            };
                            self.read_u8()?;
                            return Ok(Some((ch, kind::NOTE_OFF, key as u32)));
                        }
                        0x90 => {
                            let key = if let Some(b) = first_data {
                                b
                            } else {
                                self.read_u8()?
                            };
                            let vel = self.read_u8()?;
                            if vel == 0 {
                                return Ok(Some((ch, kind::NOTE_OFF, key as u32)));
                            } else {
                                return Ok(Some((
                                    ch,
                                    kind::NOTE_ON,
                                    key as u32 | ((vel as u32) << 8),
                                )));
                            }
                        }
                        0xA0 => {
                            let _k = if let Some(b) = first_data {
                                b
                            } else {
                                self.read_u8()?
                            };
                            self.read_u8()?;
                            continue;
                        }
                        0xB0 => {
                            let ctrl = if let Some(b) = first_data {
                                b
                            } else {
                                self.read_u8()?
                            };
                            let val = self.read_u8()?;
                            return Ok(Some((
                                ch,
                                kind::CONTROL_CHANGE,
                                ctrl as u32 | ((val as u32) << 8),
                            )));
                        }
                        0xC0 => {
                            let prog = if let Some(b) = first_data {
                                b
                            } else {
                                self.read_u8()?
                            };
                            return Ok(Some((ch, kind::PROGRAM_CHANGE, prog as u32)));
                        }
                        0xD0 => {
                            let _v = if let Some(b) = first_data {
                                b
                            } else {
                                self.read_u8()?
                            };
                            continue;
                        }
                        0xE0 => {
                            let lsb = if let Some(b) = first_data {
                                b
                            } else {
                                self.read_u8()?
                            };
                            let msb = self.read_u8()?;
                            let bend = ((msb as u16) << 7) | (lsb as u16);
                            return Ok(Some((ch, kind::PITCH_BEND, bend as u32)));
                        }
                        _ => continue,
                    }
                }
                _ => continue,
            }
        }
    }
    fn next_with_tick(&mut self) -> Result<Option<(u64, u8, u32, u32)>, SynthError> {
        if let Some((ch, k, p)) = self.next_midi()? {
            Ok(Some((self.tick, ch, k, p)))
        } else {
            Ok(None)
        }
    }
}

fn read_header_and_tracks_mmap(mmap: &[u8]) -> Result<(u64, Vec<(u64, u32)>), SynthError> {
    if mmap.len() < 14 {
        return Err(SynthError::Midi("file too short".into()));
    }
    if &mmap[0..4] != b"MThd" {
        return Err(SynthError::Midi("not SMF".into()));
    }
    let hdr_len = read_be_u32(&mmap[4..8]);
    if hdr_len != 6 {
        return Err(SynthError::Midi("bad MThd length".into()));
    }
    let ntrks = read_be_u16(&mmap[10..12]);
    let division = read_be_u16(&mmap[12..14]);
    if division & 0x8000 != 0 {
        return Err(SynthError::Midi("SMPTE not supported".into()));
    }
    let tpb = (division & 0x7FFF) as u64;
    if tpb == 0 {
        return Err(SynthError::Midi("zero ticks per beat".into()));
    }
    let mut infos = Vec::with_capacity(ntrks as usize);
    let mut pos = 14usize;
    for _ in 0..ntrks {
        if pos + 8 > mmap.len() {
            return Err(SynthError::Midi("truncated MTrk header".into()));
        }
        if &mmap[pos..pos + 4] != b"MTrk" {
            return Err(SynthError::Midi("expected MTrk".into()));
        }
        let len = read_be_u32(&mmap[pos + 4..pos + 8]);
        let offset = (pos + 8) as u64;
        infos.push((offset, len));
        pos += 8 + len as usize;
        if pos > mmap.len() {
            return Err(SynthError::Midi("truncated track".into()));
        }
    }
    Ok((tpb, infos))
}

fn scan_tempos_mmap(
    mmap: &[u8],
    infos: &[(u64, u32)],
) -> Result<(Vec<(u64, u32)>, u64), SynthError> {
    let mut tempos: Vec<(u64, u32)> = Vec::new();
    let mut length_ticks = 0u64;
    for (off, len) in infos {
        let off = *off as usize;
        let len = *len as usize;
        let track = &mmap[off..off + len];
        let mut pos = 0usize;
        let mut tick = 0u64;
        let mut running: Option<u8> = None;
        while pos < track.len() {
            // VLQ delta
            let mut delta = 0u32;
            loop {
                if pos >= track.len() {
                    return Err(SynthError::Midi("truncated delta".into()));
                }
                let b = track[pos];
                pos += 1;
                delta = (delta << 7) | (b & 0x7F) as u32;
                if b & 0x80 == 0 {
                    break;
                }
            }
            tick += delta as u64;
            length_ticks = length_ticks.max(tick);
            if pos >= track.len() {
                break;
            }
            let status = track[pos];
            pos += 1;
            let (ev_status, first_data) = if status < 0x80 {
                let rs = running
                    .ok_or_else(|| SynthError::Midi("running status without prior".into()))?;
                (rs, Some(status))
            } else if status < 0xF0 {
                running = Some(status);
                (status, None)
            } else {
                (status, None)
            };
            match ev_status {
                0xFF => {
                    let meta_type = if let Some(b) = first_data {
                        b
                    } else {
                        if pos >= track.len() {
                            break;
                        }
                        let b = track[pos];
                        pos += 1;
                        b
                    };
                    // VLQ len
                    let mut meta_len = 0u32;
                    loop {
                        if pos >= track.len() {
                            break;
                        }
                        let b = track[pos];
                        pos += 1;
                        meta_len = (meta_len << 7) | (b & 0x7F) as u32;
                        if b & 0x80 == 0 {
                            break;
                        }
                    }
                    let meta_len = meta_len as usize;
                    if meta_type == 0x51 && meta_len == 3 {
                        if pos + 3 > track.len() {
                            break;
                        }
                        let tempo =
                            u32::from_be_bytes([0, track[pos], track[pos + 1], track[pos + 2]]);
                        tempos.push((tick, tempo));
                        pos += 3;
                    } else {
                        pos += meta_len;
                    }
                }
                0xF0 | 0xF7 => {
                    let len = if let Some(first) = first_data {
                        let mut v = (first & 0x7F) as u32;
                        if first & 0x80 != 0 {
                            loop {
                                if pos >= track.len() {
                                    break;
                                }
                                let b = track[pos];
                                pos += 1;
                                v = (v << 7) | (b & 0x7F) as u32;
                                if b & 0x80 == 0 {
                                    break;
                                }
                            }
                        }
                        v as usize
                    } else {
                        let mut v = 0u32;
                        loop {
                            if pos >= track.len() {
                                break;
                            }
                            let b = track[pos];
                            pos += 1;
                            v = (v << 7) | (b & 0x7F) as u32;
                            if b & 0x80 == 0 {
                                break;
                            }
                        }
                        v as usize
                    };
                    let to_skip = if first_data.is_some() {
                        len.saturating_sub(1)
                    } else {
                        len
                    };
                    pos += to_skip;
                }
                _ if (0x80..0xF0).contains(&ev_status) => {
                    let needed = match ev_status & 0xF0 {
                        0x80 | 0x90 | 0xA0 | 0xB0 | 0xE0 => 2,
                        0xC0 | 0xD0 => 1,
                        _ => 0,
                    };
                    let mut to_read = needed;
                    if first_data.is_some() {
                        to_read -= 1;
                    }
                    pos += to_read;
                }
                _ => {}
            }
        }
    }
    tempos.sort_by_key(|&(t, _)| t);
    Ok((tempos, length_ticks))
}

fn build_tempo_segs(tempos: &[(u64, u32)], tpb: u64) -> Vec<(u64, f64, f64)> {
    let mut segs = Vec::with_capacity(tempos.len() + 1);
    let mut prev_tick = 0u64;
    let mut prev_tempo = 500_000.0;
    let mut cum = 0.0;
    for &(tick, us) in tempos {
        segs.push((prev_tick, cum, prev_tempo));
        cum += (tick - prev_tick) as f64 * prev_tempo / 1_000_000.0 / tpb as f64;
        prev_tick = tick;
        prev_tempo = us as f64;
    }
    segs.push((prev_tick, cum, prev_tempo));
    segs
}
fn ticks_to_sample(tick: u64, segs: &[(u64, f64, f64)], tpb: u64, sr: u32) -> u32 {
    let i = segs
        .partition_point(|&(s, _, _)| s <= tick)
        .saturating_sub(1);
    let (st, cum, us) = segs[i];
    let sec = cum + (tick - st) as f64 * us / 1_000_000.0 / tpb as f64;
    (sec * sr as f64).round() as u32
}

pub struct MidiStream {
    sample_rate: u32,
    ticks_per_beat: u64,
    tempo_segs: Vec<(u64, f64, f64)>,
    end_sample: u64,
    length_ticks: u64,
    path: PathBuf,
    track_infos: Vec<(u64, u32)>,
    streams: Vec<TrackStream>,
    heap: BinaryHeap<Reverse<HeapItem>>,
}
impl MidiStream {
    pub fn open(path: impl AsRef<Path>, sample_rate: u32) -> Result<Self, SynthError> {
        let path = path.as_ref().to_path_buf();
        let file = File::open(&path).map_err(SynthError::Io)?;
        let mmap =
            unsafe { Mmap::map(&file).map_err(|e| SynthError::Io(std::io::Error::other(e)))? };
        let (tpb, infos) = read_header_and_tracks_mmap(&mmap)?;
        let (tempos, length_ticks) = scan_tempos_mmap(&mmap, &infos)?;
        let tempo_segs = build_tempo_segs(&tempos, tpb);
        let end_sample = ticks_to_sample(length_ticks, &tempo_segs, tpb, sample_rate) as u64;
        drop(mmap);
        let mut streams = Vec::with_capacity(infos.len());
        for (off, len) in &infos {
            streams.push(TrackStream::new(&path, *off, *len)?);
        }
        let mut heap = BinaryHeap::new();
        for (idx, st) in streams.iter_mut().enumerate() {
            if let Some((tick, ch, k, p)) = st.next_with_tick()? {
                let sample = ticks_to_sample(tick, &tempo_segs, tpb, sample_rate);
                let packed = ((ch as u32) << 28) | ((k & 0xF) << 24) | (p & 0x00FF_FFFF);
                heap.push(Reverse(HeapItem {
                    sample,
                    track_idx: idx,
                    packed,
                }));
            }
        }
        Ok(Self {
            sample_rate,
            ticks_per_beat: tpb,
            tempo_segs,
            end_sample,
            length_ticks,
            path,
            track_infos: infos,
            streams,
            heap,
        })
    }
    pub fn parse(raw: &[u8], sample_rate: u32) -> Result<Self, SynthError> {
        let tmp =
            std::env::temp_dir().join(format!("lumino_parse_{}_{}.mid", sample_rate, raw.len()));
        std::fs::write(&tmp, raw).map_err(SynthError::Io)?;
        let s = Self::open(&tmp, sample_rate);
        let _ = std::fs::remove_file(&tmp);
        s
    }
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    pub fn end_sample(&self) -> u64 {
        self.end_sample
    }
    pub fn length_ticks(&self) -> u64 {
        self.length_ticks
    }
    pub fn duration_secs(&self) -> f64 {
        self.end_sample as f64 / self.sample_rate as f64
    }
    pub fn is_exhausted(&self) -> bool {
        self.heap.is_empty()
    }
    pub fn rewind(&mut self) -> Result<(), SynthError> {
        self.streams.clear();
        for (off, len) in &self.track_infos {
            self.streams.push(TrackStream::new(&self.path, *off, *len)?);
        }
        self.heap.clear();
        let segs = self.tempo_segs.clone();
        let tpb = self.ticks_per_beat;
        let sr = self.sample_rate;
        for (idx, st) in self.streams.iter_mut().enumerate() {
            if let Some((tick, ch, k, p)) = st.next_with_tick()? {
                let sample = ticks_to_sample(tick, &segs, tpb, sr);
                let packed = ((ch as u32) << 28) | ((k & 0xF) << 24) | (p & 0x00FF_FFFF);
                self.heap.push(Reverse(HeapItem {
                    sample,
                    track_idx: idx,
                    packed,
                }));
            }
        }
        Ok(())
    }
    pub fn peek(&self) -> Option<TimedEvent> {
        self.heap.peek().map(|Reverse(it)| TimedEvent {
            sample: it.sample,
            packed: it.packed,
        })
    }
    pub fn next_event(&mut self) -> Option<TimedEvent> {
        let Reverse(item) = self.heap.pop()?;
        let ev = TimedEvent {
            sample: item.sample,
            packed: item.packed,
        };
        let (segs, tpb, sr) = (
            self.tempo_segs.clone(),
            self.ticks_per_beat,
            self.sample_rate,
        );
        if let Some(st) = self.streams.get_mut(item.track_idx)
            && let Ok(Some((tick, ch, k, p))) = st.next_with_tick()
        {
            let sample = ticks_to_sample(tick, &segs, tpb, sr);
            let packed = ((ch as u32) << 28) | ((k & 0xF) << 24) | (p & 0x00FF_FFFF);
            self.heap.push(Reverse(HeapItem {
                sample,
                track_idx: item.track_idx,
                packed,
            }));
        }
        Some(ev)
    }
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<TimedEvent> {
        self.next_event()
    }
    pub fn for_each_note_on<F>(&self, mut f: F)
    where
        F: FnMut(u8, u8),
    {
        for (off, len) in &self.track_infos {
            if let Ok(mut st) = TrackStream::new(&self.path, *off, *len) {
                while let Ok(Some((_ch, k, p))) = st.next_midi() {
                    if k == kind::NOTE_ON {
                        let key = (p & 0xFF) as u8;
                        let vel = ((p >> 8) & 0xFF) as u8;
                        if vel > 1 {
                            f(key, vel);
                        }
                    }
                }
            }
        }
    }
    pub fn collect_note_ons(&self) -> Vec<(u8, u8)> {
        let mut o = Vec::new();
        self.for_each_note_on(|k, v| o.push((k, v)));
        o
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    /// 构造最小合法 SMF（1 轨：tempo + note on/off + end），避免依赖 gitignored 大文件。
    fn minimal_smf() -> Vec<u8> {
        let mut v = Vec::new();
        // MThd: format 0, 1 track, division 480
        v.extend_from_slice(b"MThd");
        v.extend_from_slice(&[0, 0, 0, 6, 0, 0, 0, 1, 0x01, 0xE0]);
        // Track data
        let mut track = Vec::new();
        // delta 0, tempo 500000 (07 A1 20)
        track.extend_from_slice(&[0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]);
        // delta 0, note-on ch0 key60 vel100
        track.extend_from_slice(&[0x00, 0x90, 0x3C, 0x64]);
        // delta 480 (83 60), note-off ch0 key60 vel64
        track.extend_from_slice(&[0x83, 0x60, 0x80, 0x3C, 0x40]);
        // delta 0, end of track
        track.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);
        v.extend_from_slice(b"MTrk");
        v.extend_from_slice(&(track.len() as u32).to_be_bytes());
        v.extend_from_slice(&track);
        v
    }
    #[test]
    fn stream_roundtrip_small() {
        let raw = minimal_smf();
        let midi = MidiStream::parse(&raw, 64_000).expect("最小 SMF 应可解析");
        assert!(midi.end_sample() > 0);
        assert!(!midi.is_exhausted());
        let mut s = midi;
        let mut last = 0u32;
        let mut c = 0;
        while let Some(ev) = s.next_event() {
            assert!(ev.sample >= last);
            last = ev.sample;
            c += 1;
        }
        assert!(c > 0);
        assert!(s.is_exhausted());
    }
}
