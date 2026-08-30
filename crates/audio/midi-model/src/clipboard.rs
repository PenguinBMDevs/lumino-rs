//! 剪贴板音符二进制载体（Lumino 程序本体间同步，紧凑 / 流式 / 内存友好）
//!
//! 设计目标：跨 Lumino 实例复制粘贴 10M 级音符时，剪贴板载荷内存 < 100MB、单程 < 500ms。
//! 文本 JSON 在该量级序列化字符串即 500MB+，故改用紧凑二进制：
//! - **MIDI 风格 delta 编码 tick**（LEB128 变长），密集黑键下每音符 tick 仅 ~1 字节
//! - 其余字段定长/变长打包：`key_offset` u8 / `length` varint / `velocity` u8 / `channel` u8 / `track` u16
//! - **流式编码**（不物化 `Vec<NoteEvent>`）；**分块解码**（不物化全量 `Vec`，按块回调）
//! - 载荷自带源 `division`，供粘贴端 PPQN 一致性重采样（多一次同步计算）
//!
//! 前置约定：输入的 `ClipRecord` 流须按 `tick_offset` **升序**（复制端按文档 tick 顺序遍历即天然满足）。

use crate::note_event::NoteEvent;

/// Domino（TAKABO SOFT）剪贴板互通：格式 `MidiPortalSequence` 的解析/编码。
pub mod domino;
pub use domino::{decode_domino_clipboard, encode_domino_clipboard, CLIPBOARD_FORMAT, PORTAL_MAGIC};

/// 剪贴板载荷魔数（"LUMC" = Lumino Clip）
pub const CLIP_MAGIC: [u8; 4] = *b"LUMC";
/// 当前格式版本
pub const CLIP_VERSION: u8 = 1;

/// 单条剪贴板音符记录（相对 origin 的偏移，紧凑）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipRecord {
    /// 相对 `origin_tick` 的 tick 偏移（编码时按 MIDI 风格做 delta 累积为绝对偏移）
    pub tick_offset: u32,
    /// 相对 `origin_key` 的键偏移（0-127，u8 足够）
    pub length: u32,
    pub key_offset: u8,
    pub velocity: u8,
    pub channel: u8,
    /// 目标音轨（绝对文档轨索引）
    pub track: u16,
}

impl ClipRecord {
    #[inline]
    pub fn new(
        tick_offset: u32,
        length: u32,
        key_offset: u8,
        velocity: u8,
        channel: u8,
        track: u16,
    ) -> Self {
        Self {
            tick_offset,
            length,
            key_offset,
            velocity,
            channel,
            track,
        }
    }
}

/// 剪贴板载荷头（解码后回传的元数据）
#[derive(Debug, Clone, Copy)]
pub struct ClipMeta {
    pub division: u16,
    pub origin_tick: u32,
    pub origin_key: u8,
    pub track_hint: u16,
    pub count: u32,
}

const HEADER_LEN: usize = 4 + 1 + 1 + 2 + 4 + 1 + 2 + 4; // 19

#[inline]
fn write_varint(out: &mut Vec<u8>, mut v: u32) {
    loop {
        let mut b = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            b |= 0x80;
        }
        out.push(b);
        if v == 0 {
            break;
        }
    }
}

#[inline]
fn read_varint(buf: &[u8], pos: &mut usize) -> Result<u32, String> {
    let mut v: u32 = 0;
    let mut shift = 0u32;
    loop {
        let b = *buf
            .get(*pos)
            .ok_or_else(|| "clipboard: varint 越界".to_string())?;
        *pos += 1;
        v |= ((b & 0x7f) as u32) << shift;
        if b & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift >= 32 {
            return Err("clipboard: varint 超长".to_string());
        }
    }
    Ok(v)
}

#[inline]
fn rd_u8(buf: &[u8], pos: &mut usize) -> Result<u8, String> {
    let b = *buf
        .get(*pos)
        .ok_or_else(|| "clipboard: 字节越界".to_string())?;
    *pos += 1;
    Ok(b)
}

#[inline]
fn rd_u16(buf: &[u8], pos: &mut usize) -> Result<u16, String> {
    let a = *buf
        .get(*pos)
        .ok_or_else(|| "clipboard: u16 越界".to_string())?;
    let b = *buf
        .get(*pos + 1)
        .ok_or_else(|| "clipboard: u16 越界".to_string())?;
    *pos += 2;
    Ok(u16::from_le_bytes([a, b]))
}

/// 流式编码：输入 `ClipRecord` 迭代器（按 `tick_offset` 升序），输出紧凑二进制载荷。
///
/// 不在堆上物化 `Vec<NoteEvent>` 或 `Vec<ClipRecord>`，仅累积最终 `Vec<u8>`（即剪贴板载荷本身）。
///
/// `count` 为记录总数（调用方第一遍扫描已得知），用于精确预分配 `Vec<u8>` 容量。
/// `filter_map` / `flat_map` 等迭代器的 `size_hint` 下界为 0，若不显式传入 `count`，
/// 10MB 级载荷会反复 realloc 搬迁（~20 次深拷贝），这是钢琴卷帘「全选」复制的隐性悬崖。
pub fn encode_clipboard(
    records: impl Iterator<Item = ClipRecord>,
    count: usize,
    division: u16,
    origin_tick: u32,
    origin_key: u8,
    track_hint: u16,
) -> Vec<u8> {
    // 依据记录总数精确预分配，避免 66MB 载荷反复 realloc 搬迁
    let mut out = Vec::with_capacity(HEADER_LEN + count.saturating_mul(8));
    out.extend_from_slice(&CLIP_MAGIC);
    out.push(CLIP_VERSION);
    out.push(0); // flags 预留
    out.extend_from_slice(&division.to_le_bytes());
    out.extend_from_slice(&origin_tick.to_le_bytes());
    out.push(origin_key);
    out.extend_from_slice(&track_hint.to_le_bytes());
    let count_pos = out.len();
    out.extend_from_slice(&0u32.to_le_bytes()); // count 占位，尾部回填

    let mut count: u32 = 0;
    let mut prev_abs: u32 = 0; // 绝对 tick 偏移累加器（delta 基准）
    for r in records {
        let abs = r.tick_offset;
        let delta = abs.wrapping_sub(prev_abs); // 升序输入下为非负小值 → 变长极省
        write_varint(&mut out, delta);
        out.push(r.key_offset);
        write_varint(&mut out, r.length);
        out.push(r.velocity);
        out.push(r.channel);
        out.extend_from_slice(&r.track.to_le_bytes());
        prev_abs = abs;
        count += 1;
    }
    out[count_pos..count_pos + 4].copy_from_slice(&count.to_le_bytes());
    out
}

/// 解析头部，返回元数据（不含 body 偏移）。
pub fn parse_clipboard_header(bytes: &[u8]) -> Result<ClipMeta, String> {
    parse_header(bytes).map(|(m, _)| m)
}

/// 解析头部并返回元数据与 body 起始偏移。
fn parse_header(bytes: &[u8]) -> Result<(ClipMeta, usize), String> {
    if bytes.len() < HEADER_LEN {
        return Err("clipboard: 载荷过短".to_string());
    }
    if bytes[0..4] != CLIP_MAGIC {
        return Err("clipboard: 魔数不匹配".to_string());
    }
    let version = bytes[4];
    if version != CLIP_VERSION {
        return Err(format!("clipboard: 不支持的版本 {version}"));
    }
    let division = u16::from_le_bytes([bytes[6], bytes[7]]);
    let origin_tick = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    let origin_key = bytes[12];
    let track_hint = u16::from_le_bytes([bytes[13], bytes[14]]);
    let count = u32::from_le_bytes([bytes[15], bytes[16], bytes[17], bytes[18]]);
    Ok((
        ClipMeta {
            division,
            origin_tick,
            origin_key,
            track_hint,
            count,
        },
        HEADER_LEN,
    ))
}

/// 分块解码：不物化全量 `Vec<ClipRecord>`，每 `chunk_size` 条回调一次（内存有界）。
///
/// 回调收到的 `&[ClipRecord]` 内 `tick_offset` 已还原为**绝对偏移**（origin 已加回）。
pub fn decode_clipboard_chunks(
    bytes: &[u8],
    chunk_size: usize,
    mut f: impl FnMut(&[ClipRecord]),
) -> Result<ClipMeta, String> {
    let (meta, _) = parse_header(bytes)?;
    let mut buf: Vec<ClipRecord> = Vec::with_capacity(chunk_size.max(1));
    decode_clipboard_records(bytes, |tick_offset, length, key_offset, velocity, channel, track| {
        buf.push(ClipRecord::new(
            tick_offset, length, key_offset, velocity, channel, track,
        ));
        if buf.len() == chunk_size.max(1) {
            f(&buf);
            buf.clear();
        }
    })?;
    if !buf.is_empty() {
        f(&buf);
    }
    Ok(meta)
}

/// 流式解码：不物化任何中间 `Vec<ClipRecord>`，每解出一条即回调。
///
/// 比 `decode_clipboard_chunks` 省去每块的 `Vec<ClipRecord>` 分配与拷贝（1M 音符约省 24MB），
/// 是粘贴热路径的速度杠杆；回调直接拿到已还原为绝对偏移的字段，粘贴端可就地构造
/// `NoteEvent` 累加到按音轨分组的 `Vec`，**免去 `ClipRecord` 中间结构体二次构造**。
///
/// 回调参数：`(tick_offset, length, key_offset, velocity, channel, track)`，
/// 其中 `tick_offset` 已加回 `origin_tick`（绝对偏移）。
pub fn decode_clipboard_records(
    bytes: &[u8],
    mut f: impl FnMut(u32, u32, u8, u8, u8, u16),
) -> Result<ClipMeta, String> {
    let (meta, mut pos) = parse_header(bytes)?;
    let mut prev_abs: u32 = 0;
    for _ in 0..meta.count {
        let delta = read_varint(bytes, &mut pos)?;
        let abs = prev_abs.wrapping_add(delta);
        prev_abs = abs;
        let key_offset = rd_u8(bytes, &mut pos)?;
        let length = read_varint(bytes, &mut pos)?;
        let velocity = rd_u8(bytes, &mut pos)?;
        let channel = rd_u8(bytes, &mut pos)?;
        let track = rd_u16(bytes, &mut pos)?;
        f(abs, length, key_offset, velocity, channel, track);
    }
    Ok(meta)
}

/// 全量解码（小数据 / 单测用；大数据请用 `decode_clipboard_chunks` 以免 O(N) 中间 `Vec`）。
pub fn decode_clipboard(bytes: &[u8]) -> Result<(ClipMeta, Vec<ClipRecord>), String> {
    let (meta, _) = parse_header(bytes)?;
    let mut out = Vec::with_capacity(meta.count as usize);
    decode_clipboard_chunks(bytes, meta.count as usize, |chunk| out.extend_from_slice(chunk))?;
    Ok((meta, out))
}

/// 把一条 `ClipRecord` 还原为文档 `NoteEvent`（已加回 origin 与 PPQN 重采样 ratio）。
///
/// `ratio == 1.0`（同 PPQN，最常见）走**纯整数**快路径，不做任何 `f64` 转换与舍入，
/// 这是 10M 级粘贴的速度关键。
#[inline]
pub fn record_to_note_event(r: &ClipRecord, meta: &ClipMeta, ratio: f64) -> NoteEvent {
    let (start, end) = if ratio == 1.0 {
        let s = meta.origin_tick.saturating_add(r.tick_offset);
        let e = s.saturating_add(r.length);
        (s, e)
    } else {
        let tick_offset = (r.tick_offset as f64 * ratio).round();
        let length = (r.length as f64 * ratio).round();
        let s = (meta.origin_tick as f64 + tick_offset).max(0.0) as u32;
        let e = (s as f64 + length).max(s as f64) as u32;
        (s, e)
    };
    let key = (meta.origin_key as u32 + r.key_offset as u32).min(127) as u8;
    NoteEvent::new(start, end, key, r.velocity, r.channel)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted_records(n: u32) -> Vec<ClipRecord> {
        (0..n)
            .map(|i| {
                ClipRecord::new(
                    i,         // tick_offset 升序，delta=1
                    i % 4 + 1, // length 1..4（varint 1 字节）
                    (i % 100) as u8,
                    100,
                    0,
                    0,
                )
            })
            .collect()
    }

    #[test]
    fn test_roundtrip_preserves_records() {
        let recs = sorted_records(2000);
        let bytes = encode_clipboard(recs.iter().copied(), recs.len(), 480, 0, 0, 0);
        let (meta, out) = decode_clipboard(&bytes).unwrap();
        assert_eq!(meta.division, 480);
        assert_eq!(meta.count as usize, recs.len());
        assert_eq!(out.len(), recs.len());
        for (a, b) in recs.iter().zip(out.iter()) {
            assert_eq!(a, b, "往返后记录应逐字节一致");
        }
    }

    #[test]
    fn test_payload_compactness_small() {
        let recs = sorted_records(2000);
        let bytes = encode_clipboard(recs.iter().copied(), recs.len(), 480, 0, 0, 0);
        assert!(bytes.len() < 2000 * 12, "二进制应远小于定长编码");
    }

    #[test]
    fn test_ppqn_resample_doubles_length() {
        let recs = vec![ClipRecord::new(100, 200, 5, 100, 0, 0)];
        let n = recs.len();
        let bytes = encode_clipboard(recs.into_iter(), n, 480, 50, 60, 0);
        let (meta, out) = decode_clipboard(&bytes).unwrap();
        let ratio = 960.0 / 480.0;
        let n = record_to_note_event(&out[0], &meta, ratio);
        assert_eq!(n.start_tick, 250);
        assert_eq!(n.length(), 400);
        assert_eq!(n.key, 65);
        assert_eq!(n.velocity, 100);
        assert_eq!(n.channel, 0);
    }

    #[test]
    fn test_ppqn_no_resample_when_equal() {
        let recs = vec![ClipRecord::new(100, 50, 5, 100, 0, 0)];
        let n = recs.len();
        let bytes = encode_clipboard(recs.into_iter(), n, 480, 50, 60, 0);
        let (meta, out) = decode_clipboard(&bytes).unwrap();
        let n = record_to_note_event(&out[0], &meta, 1.0);
        assert_eq!(n.start_tick, 150);
        assert_eq!(n.length(), 50);
    }

    #[test]
    fn test_decode_chunks_bounds_memory() {
        let recs = sorted_records(100_000);
        let bytes = encode_clipboard(recs.iter().copied(), recs.len(), 480, 0, 0, 0);
        let mut seen = 0usize;
        let mut max_chunk = 0usize;
        decode_clipboard_chunks(&bytes, 10_000, |chunk| {
            seen += chunk.len();
            max_chunk = max_chunk.max(chunk.len());
        })
        .unwrap();
        assert_eq!(seen, 100_000);
        assert!(max_chunk <= 10_000);
    }
}
