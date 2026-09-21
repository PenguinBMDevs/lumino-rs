use super::parse::{read_header_and_tracks_mmap, scan_tempos_mmap};
use super::tempo::{build_tempo_segs, ticks_to_sample};
use super::track::{HeapItem, TrackStream};
use super::*;

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
