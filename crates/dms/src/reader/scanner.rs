//! DMS 流式扫描器

use flate2::read::ZlibDecoder;
use std::io::Read;

use crate::constants::HEADER_SIZE;
use crate::error::{DmsError, Result};
use crate::node_type::DmsNodeType;
use crate::reader::{DmsScanResult, read_file_header};
use crate::utils;

/// 解析状态机
pub struct ScanState {
    buffer: Vec<u8>,
    valid_len: usize,
    decompressed_offset: usize,
    decompressed_length: usize,
    last_progress_report: f64,
    parent_stack: Vec<(u16, usize)>,
    cumulative_offset: usize,
}

impl ScanState {
    #[must_use]
    pub fn new(decompressed_length: usize) -> Self {
        // 缓冲区需要足够容纳解压后的数据
        // 使用解压长度 + 一些额外空间
        use crate::constants::SCAN_BUFFER_SIZE;
        let buffer_size = decompressed_length + SCAN_BUFFER_SIZE;

        Self {
            buffer: vec![0; buffer_size],
            valid_len: 0,
            decompressed_offset: 0,
            decompressed_length,
            last_progress_report: 0.0,
            parent_stack: Vec::with_capacity(32),
            cumulative_offset: 0,
        }
    }

    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.decompressed_offset >= self.decompressed_length
    }

    /// # Errors
    ///
    /// Returns an error if the operation fails.
    pub fn read_more_data<R: Read>(&mut self, decoder: &mut ZlibDecoder<R>) -> Result<bool> {
        const BUF_SIZE: usize = 4 * 1_048_576;

        // 总是尝试读取更多数据，直到解码器返回0
        let read_target = &mut self.buffer[self.valid_len..self.valid_len + BUF_SIZE];
        match decoder.read(read_target) {
            Ok(0) => {
                // 解码器返回0表示数据已读完
                // 标记为已完成
                self.decompressed_offset = self.decompressed_length;
                Ok(true) // 返回true表示数据已读完
            }
            Ok(n) => {
                self.valid_len += n;
                self.decompressed_offset += n;
                Ok(false) // 返回false表示还可以继续读取
            }
            Err(e) => Err(DmsError::Corrupted(format!("解压失败: {e}"))),
        }
    }

    /// # Errors
    ///
    /// Returns an error if the operation fails.
    pub fn parse_nodes(&mut self, result: &mut DmsScanResult) -> Result<()> {
        let mut parse_offset: usize = 0;

        // 预计算常量
        let track_base = DmsNodeType::TRACK.base_type();
        let note_event_base = DmsNodeType::NOTE_EVENT.base_type();
        let song_name_base = DmsNodeType::SONG_NAME.base_type();
        let song_copyright_base = DmsNodeType::SONG_COPYRIGHT.base_type();
        let song_comment_base = DmsNodeType::SONG_COMMENT.base_type();
        let song_ppqn_base = DmsNodeType::SONG_PPQN.base_type();
        let working_time_base = DmsNodeType::WORKING_TIME_SEC.base_type();
        let current_vars_base = DmsNodeType::CURRENT_VARS.base_type();
        let midi_out_cfg_base = DmsNodeType::MIDI_OUT_CFG.base_type();
        let key_palette_base = DmsNodeType::KEY_PALETTE.base_type();
        let port_cfg_base = DmsNodeType::PORT_CFG.base_type();

        while parse_offset + HEADER_SIZE <= self.valid_len {
            let type_id =
                u16::from_le_bytes([self.buffer[parse_offset], self.buffer[parse_offset + 1]]);
            let data_length = u32::from_le_bytes([
                self.buffer[parse_offset + 2],
                self.buffer[parse_offset + 3],
                self.buffer[parse_offset + 4],
                self.buffer[parse_offset + 5],
            ]) as usize;

            let data_start = parse_offset + HEADER_SIZE;
            let data_end = data_start + data_length;

            if data_end > self.valid_len {
                break;
            }

            let node_end_offset = self.cumulative_offset + data_end;

            // 弹出已结束的父节点
            while let Some((_, end_offset)) = self.parent_stack.last() {
                if node_end_offset > *end_offset {
                    self.parent_stack.pop();
                } else {
                    break;
                }
            }

            let current_parent_base = self.parent_stack.last().map(|(base, _)| *base);

            // 处理节点
            if current_parent_base.is_none() {
                match type_id {
                    t if t == song_name_base => {
                        result.song_name =
                            utils::decode_gb18030(&self.buffer[data_start..data_end]);
                    }
                    t if t == song_copyright_base => {
                        result.copyright =
                            utils::decode_gb18030(&self.buffer[data_start..data_end]);
                    }
                    t if t == song_comment_base => {
                        result.comment = utils::decode_gb18030(&self.buffer[data_start..data_end]);
                    }
                    t if t == song_ppqn_base => {
                        result.ppqn = utils::decode_u32_le(&self.buffer[data_start..data_end]);
                    }
                    t if t == working_time_base => {
                        result.working_time_sec =
                            utils::decode_u64_le(&self.buffer[data_start..data_end]);
                    }
                    t if t == track_base => {
                        result.track_count += 1;
                    }
                    _ => {}
                }
            }

            // 检查是否为音符事件
            if current_parent_base == Some(track_base) && type_id == note_event_base {
                result.total_notes += 1;
            }

            // 快速判断是否为复合节点
            let is_composite = type_id == track_base
                || type_id == current_vars_base
                || type_id == midi_out_cfg_base
                || type_id == key_palette_base
                || type_id == port_cfg_base
                || (current_parent_base == Some(track_base) && (2001..=2019).contains(&type_id));

            if is_composite {
                self.parent_stack.push((type_id, node_end_offset));
            }

            parse_offset = data_end;
        }

        self.cumulative_offset += parse_offset;

        let remaining = self.valid_len - parse_offset;
        if remaining > 0 && parse_offset > 0 {
            self.buffer.copy_within(parse_offset..self.valid_len, 0);
        }
        self.valid_len = remaining;

        for (_, end_offset) in &mut self.parent_stack {
            *end_offset -= parse_offset;
        }

        Ok(())
    }

    pub fn update_progress<F: Fn(f64)>(&mut self, progress_callback: &F) {
        let progress = (self.decompressed_offset as f64) / (self.decompressed_length as f64);
        if progress - self.last_progress_report >= 0.1
            || self.decompressed_offset >= self.decompressed_length
        {
            progress_callback(progress.min(1.0));
            self.last_progress_report = progress;
        }
    }
}

/// # Errors
///
/// Returns an error if the operation fails.
/// 流式扫描 DMS 文件（边解压边提取元数据，不保留完整解压数据）
pub fn scan_dms_streaming<R: Read>(stream: &mut R) -> Result<DmsScanResult> {
    scan_dms_streaming_with_progress(stream, |_| {})
}

/// 流式扫描 DMS 文件（带进度回调）
pub fn scan_dms_streaming_with_progress<R: Read, F: Fn(f64)>(
    stream: &mut R,
    progress_callback: F,
) -> Result<DmsScanResult> {
    let header = read_file_header(stream)?;
    let mut decoder = ZlibDecoder::new(stream);
    let mut result = DmsScanResult::default();
    let mut state = ScanState::new(header.decompressed_length);

    let mut eof_reached = false;
    while !state.is_finished() {
        if !eof_reached {
            eof_reached = state.read_more_data(&mut decoder)?;
        }
        state.parse_nodes(&mut result)?;
        state.update_progress(&progress_callback);
    }

    Ok(result)
}
