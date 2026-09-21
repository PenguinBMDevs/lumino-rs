use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HeapItem {
    pub(super) sample: u32,
    pub(super) track_idx: usize,
    pub(super) packed: u32,
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

pub(super) struct TrackStream {
    reader: BufReader<File>,
    track_end: u64,
    // 已消费字节的逻辑位置（`BufReader` 会预读，底层 fd 位置不可信，小文件会误判结束）。
    pos: u64,
    tick: u64,
    running_status: Option<u8>,
}
impl TrackStream {
    pub(super) fn new(path: &Path, offset: u64, length: u32) -> Result<Self, SynthError> {
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
    pub(super) fn next_midi(&mut self) -> Result<Option<(u8, u32, u32)>, SynthError> {
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
    pub(super) fn next_with_tick(&mut self) -> Result<Option<(u64, u8, u32, u32)>, SynthError> {
        if let Some((ch, k, p)) = self.next_midi()? {
            Ok(Some((self.tick, ch, k, p)))
        } else {
            Ok(None)
        }
    }
}
