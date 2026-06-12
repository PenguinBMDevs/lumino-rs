//! MIDI 音轨名称扫描与文本解码
//!
//! 从 `document.rs` 中提取的独立模块，负责轻量扫描原始 MIDI 字节并提取音轨名称。

use encoding_rs::*;

/// 读取 VLQ（Variable Length Quantity）编码的值
fn read_vlq(data: &[u8], pos: &mut usize, end: usize) -> u32 {
    let mut value: u32 = 0;
    while *pos < end {
        let b = data[*pos];
        *pos += 1;
        value = (value << 7) | (b & 0x7F) as u32;
        if b & 0x80 == 0 {
            break;
        }
    }
    value
}

/// 在单个 MTrk chunk 中扫描 TrackName 事件
fn scan_track_name_in_chunk(data: &[u8], chunk_start: usize, chunk_end: usize) -> Option<String> {
    let mut pos = chunk_start;
    let mut last_status: u8 = 0;

    while pos < chunk_end {
        let _delta = read_vlq(data, &mut pos, chunk_end);
        if pos >= chunk_end {
            break;
        }

        let mut status = data[pos];
        if status >= 0x80 {
            pos += 1;
            if status < 0xF0 {
                last_status = status;
            }
        } else {
            status = last_status;
        }

        match status {
            0xFF => {
                if pos >= chunk_end {
                    break;
                }
                let meta_type = data[pos];
                pos += 1;
                let meta_len = read_vlq(data, &mut pos, chunk_end);
                let end = (pos + meta_len as usize).min(chunk_end);

                if meta_type == 0x03 {
                    let name_bytes = &data[pos..end];
                    let name = decode_midi_text(name_bytes);
                    if !name.is_empty() {
                        return Some(name);
                    }
                }
                pos = end;
            }
            0xF0 | 0xF7 => {
                let sysex_len = read_vlq(data, &mut pos, chunk_end);
                pos = (pos + sysex_len as usize).min(chunk_end);
            }
            0xF8..=0xFE => {}
            _ if status < 0xF0 => {
                let skip = match status & 0xF0 {
                    0xC0 | 0xD0 => 1,
                    0x80 | 0x90 | 0xA0 | 0xB0 | 0xE0 => 2,
                    _ => 0,
                };
                pos = (pos + skip).min(chunk_end);
            }
            _ => break,
        }
    }
    None
}

/// 轻量扫描原始 MIDI 字节，提取所有音轨的 TrackName 事件。
/// 使用 encoding_rs 自动检测编码（UTF-8 → Shift-JIS → GBK → Latin-1）。
pub fn scan_track_names(data: &[u8]) -> Vec<Option<String>> {
    if data.len() < 14 {
        return Vec::new();
    }

    let data = if &data[..4] == b"RIFF" {
        let mthd_pos = data.windows(4).position(|w| w == b"MThd");
        match mthd_pos {
            Some(pos) => &data[pos..],
            None => return Vec::new(),
        }
    } else if &data[..4] == b"MThd" {
        data
    } else {
        return Vec::new();
    };

    if data.len() < 14 {
        return Vec::new();
    }

    let header_len = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let track_count = u16::from_be_bytes([data[10], data[11]]) as usize;
    let header_total = 8 + header_len;
    if header_total > data.len() {
        return Vec::new();
    }

    let mut track_names = vec![None; track_count];
    let mut track_idx = 0;
    let mut offset = header_total;

    while track_idx < track_count && offset + 8 <= data.len() {
        if &data[offset..offset + 4] != b"MTrk" {
            let chunk_len =
                u32::from_be_bytes(data[offset + 4..offset + 8].try_into().unwrap_or([0; 4]))
                    as usize;
            offset += 8 + chunk_len;
            continue;
        }

        let chunk_len =
            u32::from_be_bytes(data[offset + 4..offset + 8].try_into().unwrap_or([0; 4])) as usize;
        offset += 8;
        let track_end = (offset + chunk_len).min(data.len());

        let name = scan_track_name_in_chunk(data, offset, track_end);
        if let Some(n) = name {
            track_names[track_idx] = Some(n);
        }

        track_idx += 1;
        offset = track_end;
    }

    track_names
}

/// 解码 MIDI 文本（尝试 UTF-8 → Shift-JIS → GBK → EUC-JP → Latin-1）
pub fn decode_midi_text(bytes: &[u8]) -> String {
    // 1. 先检查纯 ASCII（ASCII 是有效的 UTF-8，可直接转换）
    if bytes.is_ascii() {
        return String::from_utf8(bytes.to_vec())
            .unwrap_or_else(|_| String::from_utf8_lossy(bytes).into_owned());
    }

    // 2. 尝试 UTF-8
    if let Ok(s) = String::from_utf8(bytes.to_vec()) {
        return s;
    }

    // 3. 尝试常见日语编码 Shift-JIS
    let (cow, _) = SHIFT_JIS.decode_without_bom_handling(bytes);
    if !cow.contains('\u{FFFD}') {
        return cow.into_owned();
    }

    // 4. 尝试 GBK（简体中文）
    let (cow, _) = GBK.decode_without_bom_handling(bytes);
    if !cow.contains('\u{FFFD}') {
        return cow.into_owned();
    }

    // 5. 尝试 EUC-JP
    let (cow, _) = EUC_JP.decode_without_bom_handling(bytes);
    if !cow.contains('\u{FFFD}') {
        return cow.into_owned();
    }

    // 6. 回退到 Latin-1（逐字节映射）
    bytes.iter().map(|&b| b as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_midi_text() {
        // ASCII
        assert_eq!(decode_midi_text(b"Piano"), "Piano");

        // UTF-8 Chinese
        let utf8 = "钢琴".as_bytes();
        assert_eq!(decode_midi_text(utf8), "钢琴");

        // Shift-JIS (Japanese for "piano")
        let sjis = [0x83, 0x70, 0x83, 0x41, 0x83, 0x6E]; // "ピアノ" in Shift-JIS
        let decoded = decode_midi_text(&sjis);
        assert!(!decoded.is_empty(), "Shift-JIS should decode to something");
    }

    #[test]
    fn test_scan_track_names_empty() {
        let names = scan_track_names(&[]);
        assert!(names.is_empty());
    }

    #[test]
    fn test_scan_track_names_invalid() {
        let names = scan_track_names(b"NOTMIDI");
        assert!(names.is_empty());
    }

    #[test]
    fn test_scan_track_names_single_track() {
        let header = [
            0x4D, 0x54, 0x68, 0x64, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x01, 0x01, 0xE0,
        ];
        let track = [
            0x4D, 0x54, 0x72, 0x6B, 0x00, 0x00, 0x00, 0x0F, 0x00, 0xFF, 0x03, 0x05, 0x50, 0x69,
            0x61, 0x6E, 0x6F, 0x00, 0xFF, 0x2F, 0x00,
        ];
        let mut midi = Vec::new();
        midi.extend_from_slice(&header);
        midi.extend_from_slice(&track);

        let names = scan_track_names(&midi);
        assert_eq!(names.len(), 1);
        assert_eq!(names[0], Some("Piano".to_string()));
    }

    #[test]
    fn test_scan_track_names_no_track_name() {
        let header = [
            0x4D, 0x54, 0x68, 0x64, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x01, 0x01, 0xE0,
        ];
        let track = [
            0x4D, 0x54, 0x72, 0x6B, 0x00, 0x00, 0x00, 0x04, 0x00, 0xFF, 0x2F, 0x00,
        ];
        let mut midi = Vec::new();
        midi.extend_from_slice(&header);
        midi.extend_from_slice(&track);

        let names = scan_track_names(&midi);
        assert_eq!(names.len(), 1);
        assert_eq!(names[0], None);
    }

    #[test]
    fn test_scan_track_names_riff_wrapper() {
        let riff_header = [
            0x52, 0x49, 0x46, 0x46, 0x00, 0x00, 0x00, 0x00, 0x52, 0x4D, 0x49, 0x44,
        ];
        let mthd = [
            0x4D, 0x54, 0x68, 0x64, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x01, 0x01, 0xE0,
        ];
        let track = [
            0x4D, 0x54, 0x72, 0x6B, 0x00, 0x00, 0x00, 0x0F, 0x00, 0xFF, 0x03, 0x05, 0x50, 0x69,
            0x61, 0x6E, 0x6F, 0x00, 0xFF, 0x2F, 0x00,
        ];
        let mut midi = Vec::new();
        midi.extend_from_slice(&riff_header);
        midi.extend_from_slice(&mthd);
        midi.extend_from_slice(&track);

        let names = scan_track_names(&midi);
        assert_eq!(names.len(), 1);
        assert_eq!(names[0], Some("Piano".to_string()));
    }
}
