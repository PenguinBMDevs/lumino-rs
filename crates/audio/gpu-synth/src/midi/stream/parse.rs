use super::*;

fn read_be_u16(b: &[u8]) -> u16 {
    u16::from_be_bytes([b[0], b[1]])
}
fn read_be_u32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

pub(super) fn read_header_and_tracks_mmap(
    mmap: &[u8],
) -> Result<(u64, Vec<(u64, u32)>), SynthError> {
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

pub(super) fn scan_tempos_mmap(
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
