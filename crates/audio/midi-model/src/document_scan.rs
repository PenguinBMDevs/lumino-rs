//! MIDI 文件头扫描与文本解码
//!
//! 从 `document.rs` 中提取的独立模块：轻量读取文件头（track_count / division）
//! 与解码 meta 文本（TrackName 等）。
//!
//! TrackName 的提取已改由 midly 快路径直接带出
//! （`TrackAllEvents.track_name`），本模块不再做第二次全量扫描。

use encoding_rs::*;

/// 如果数据有 RIFF wrapper，返回 MThd 起始位置之后的切片；否则返回原切片。
fn midi_data_after_riff(data: &[u8]) -> Option<&[u8]> {
    if data.len() < 4 {
        return None;
    }

    if &data[..4] == b"RIFF" {
        data.windows(4)
            .position(|w| w == b"MThd")
            .map(|pos| &data[pos..])
    } else if &data[..4] == b"MThd" {
        Some(data)
    } else {
        None
    }
}

/// 轻量扫描原始 MIDI 文件头，返回 (track_count, division)。
///
/// 支持普通 SMF 与 RIFF wrapper。
pub fn scan_header_info(data: &[u8]) -> Option<(u16, u16)> {
    let midi_data = midi_data_after_riff(data)?;

    if midi_data.len() < 14 {
        return None;
    }

    let header_len =
        u32::from_be_bytes([midi_data[4], midi_data[5], midi_data[6], midi_data[7]]) as usize;
    if header_len < 6 || 8 + header_len > midi_data.len() {
        return None;
    }

    let track_count = u16::from_be_bytes([midi_data[10], midi_data[11]]);
    let division = u16::from_be_bytes([midi_data[12], midi_data[13]]);
    Some((track_count, division))
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
    fn test_scan_header_info_smf_and_riff() {
        // 裸 SMF：MThd + len=6 + format 0 + 1 轨 + division 480。
        let header = [
            0x4D, 0x54, 0x68, 0x64, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x01, 0x01, 0xE0,
        ];
        assert_eq!(scan_header_info(&header), Some((1, 480)));

        // RIFF wrapper：MThd 位于 +12，必须被正确剥离。
        let riff_header = [
            0x52, 0x49, 0x46, 0x46, 0x00, 0x00, 0x00, 0x00, 0x52, 0x4D, 0x49, 0x44,
        ];
        let mut midi = Vec::new();
        midi.extend_from_slice(&riff_header);
        midi.extend_from_slice(&header);
        assert_eq!(scan_header_info(&midi), Some((1, 480)));

        // 非法 / 过短输入。
        assert_eq!(scan_header_info(&[]), None);
        assert_eq!(scan_header_info(b"NOTMIDI"), None);
    }
}
