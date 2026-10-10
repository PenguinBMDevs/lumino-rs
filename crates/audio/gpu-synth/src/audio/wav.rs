//! WAV file reading and writing (32-bit IEEE float and 16-bit PCM).
//!
//! The reference audio from the acceptance test is a 32-bit float stereo
//! WAV at 64 kHz, which this module reads and writes losslessly.

use std::io::{Read, Write};
use std::path::Path;

use crate::error::SynthError;

/// A decoded WAV file.
#[derive(Debug, Clone)]
pub struct WavData {
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Number of channels (1 = mono, 2 = stereo).
    pub channels: u16,
    /// Interleaved f32 samples in `[-1.0, 1.0]`.
    pub samples: Vec<f32>,
}

/// Reads a WAV file into float samples. Supports 8/16/24/32-bit integer PCM
/// and 32-bit IEEE float.
///
/// # Errors
///
/// Returns [`SynthError::Io`] on read failures and
/// [`SynthError::Config`] for unsupported formats.
pub fn read_wav(path: impl AsRef<Path>) -> Result<WavData, SynthError> {
    let mut f = std::fs::File::open(path)?;
    read_wav_from(&mut f)
}

/// data 块硬上限（2 GiB）：块长度来自文件头（不可信），禁止直接巨量分配
/// （DEBT-02 #119）。
const MAX_WAV_DATA_BYTES: usize = 1 << 31;

/// Reads a WAV from any reader.
pub fn read_wav_from(r: &mut impl Read) -> Result<WavData, SynthError> {
    let mut header = [0u8; 12];
    r.read_exact(&mut header)?;
    if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" {
        return Err(SynthError::Config("not a RIFF/WAVE file".into()));
    }

    let mut fmt: Option<(u16, u16, u32, u16)> = None; // tag, ch, rate, bits
    let mut data: Option<Vec<u8>> = None;

    loop {
        let mut chunk_hdr = [0u8; 8];
        match r.read_exact(&mut chunk_hdr) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        }
        let id = &chunk_hdr[0..4];
        let size =
            u32::from_le_bytes([chunk_hdr[4], chunk_hdr[5], chunk_hdr[6], chunk_hdr[7]]) as usize;
        match id {
            b"fmt " => {
                // fmt 块至少 16 字节；短块直接报错。
                // DEBT-02 #119：旧实现按 `size.min(40)` 分配后固定读 buf[14..16]，
                // size < 16 时越界 panic。
                if size < 16 {
                    return Err(SynthError::Config(format!("fmt chunk too short: {size}")));
                }
                let fmt_len = size.min(40);
                let mut buf = vec![0u8; fmt_len];
                r.read_exact(&mut buf)?;
                let tag = u16::from_le_bytes([buf[0], buf[1]]);
                let ch = u16::from_le_bytes([buf[2], buf[3]]);
                let rate = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
                let bits = u16::from_le_bytes([buf[14], buf[15]]);
                fmt = Some((tag, ch, rate, bits));
                if size > fmt_len {
                    skip(r, size - fmt_len)?;
                }
            }
            b"data" => {
                // DEBT-02 #119：data 长度不可信，超过硬上限直接拒绝（防巨量分配）
                if size > MAX_WAV_DATA_BYTES {
                    return Err(SynthError::Config(format!(
                        "data chunk too large: {size} bytes"
                    )));
                }
                let mut buf = vec![0u8; size];
                r.read_exact(&mut buf)?;
                data = Some(buf);
            }
            _ => {
                skip(r, size)?;
            }
        }
        if size % 2 == 1 {
            // pad byte
            let mut pad = [0u8; 1];
            let _ = r.read_exact(&mut pad);
        }
    }

    let (tag, channels, sample_rate, bits) =
        fmt.ok_or_else(|| SynthError::Config("missing fmt chunk".into()))?;
    let raw = data.ok_or_else(|| SynthError::Config("missing data chunk".into()))?;
    if channels == 0 || channels > 2 {
        return Err(SynthError::Config(format!(
            "unsupported channel count {channels}"
        )));
    }

    let samples = match (tag, bits) {
        (1, 8) => raw.iter().map(|b| (*b as f32 / 128.0) - 1.0).collect(),
        (1, 16) => raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
            .collect(),
        (1, 24) => raw
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| {
                // 24-bit 小端 → i32：按最高位做符号扩展，再按 2^23 归一化。
                // DEBT-02 #119：旧实现 `[c0,c1,c2,0] >> 8 / 32768` 未做符号扩展，
                // 负样本会变成正的大值（幅度/符号双错）。
                let top = if c[2] & 0x80 != 0 { 0xFF } else { 0x00 };
                let v = i32::from_le_bytes([c[0], c[1], c[2], top]);
                v as f32 / 8_388_608.0
            })
            .collect(),
        (1, 32) => raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f32 / 2147483648.0)
            .collect(),
        (3, 32) | (65534, 32) => raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
        _ => {
            return Err(SynthError::Config(format!(
                "unsupported WAV format tag={tag} bits={bits}"
            )));
        }
    };

    Ok(WavData {
        sample_rate,
        channels,
        samples,
    })
}

/// Writes interleaved float samples as a 32-bit IEEE float WAV file.
///
/// # Errors
///
/// Returns [`SynthError::Io`] on write failures.
pub fn write_f32_wav(
    path: impl AsRef<Path>,
    samples: &[f32],
    sample_rate: u32,
) -> Result<(), SynthError> {
    write_f32_wav_channels(path, samples, sample_rate, 2)
}

/// Writes interleaved float samples as a 32-bit IEEE float WAV file with an
/// explicit channel count.
///
/// # Errors
///
/// Returns [`SynthError::Io`] on write failures.
pub fn write_f32_wav_channels(
    path: impl AsRef<Path>,
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
) -> Result<(), SynthError> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    let data_len = (samples.len() * 4) as u32;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVE")?;
    f.write_all(b"fmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&3u16.to_le_bytes())?; // IEEE float
    f.write_all(&channels.to_le_bytes())?;
    f.write_all(&sample_rate.to_le_bytes())?;
    f.write_all(&(sample_rate * channels as u32 * 4).to_le_bytes())?;
    f.write_all(&(channels * 4).to_le_bytes())?;
    f.write_all(&32u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    for s in samples {
        f.write_all(&s.to_le_bytes())?;
    }
    f.flush()?;
    Ok(())
}

/// Incremental WAV writer — streams blocks to disk without holding the
/// whole render in memory. Writes a placeholder header on `create` and patches
/// it on `finalize`.
///
/// # Example
/// ```no_run
/// use lumino_gpu_synth::audio::wav::WavStreamWriter;
/// let mut w = WavStreamWriter::create("out.wav", 64_000, 2).unwrap();
/// w.write_samples(&[0.0; 1024]).unwrap();
/// w.finalize().unwrap();
/// ```
pub struct WavStreamWriter {
    file: std::io::BufWriter<std::fs::File>,
    sample_rate: u32,
    channels: u16,
    frames_written: u64,
}

impl WavStreamWriter {
    /// Creates `path` and writes a placeholder 32-bit float WAV header.
    pub fn create(
        path: impl AsRef<std::path::Path>,
        sample_rate: u32,
        channels: u16,
    ) -> Result<Self, crate::SynthError> {
        let file = std::fs::File::create(path.as_ref())?;
        let mut w = std::io::BufWriter::new(file);
        // RIFF header with placeholder sizes (patched in finalize)
        w.write_all(b"RIFF")?;
        w.write_all(&0u32.to_le_bytes())?; // file size - 8
        w.write_all(b"WAVE")?;
        w.write_all(b"fmt ")?;
        w.write_all(&16u32.to_le_bytes())?;
        w.write_all(&3u16.to_le_bytes())?; // IEEE float
        w.write_all(&channels.to_le_bytes())?;
        w.write_all(&sample_rate.to_le_bytes())?;
        w.write_all(&(sample_rate * channels as u32 * 4).to_le_bytes())?;
        w.write_all(&(channels * 4).to_le_bytes())?;
        w.write_all(&32u16.to_le_bytes())?;
        w.write_all(b"data")?;
        w.write_all(&0u32.to_le_bytes())?; // data size
        w.flush()?;
        Ok(Self {
            file: w,
            sample_rate,
            channels,
            frames_written: 0,
        })
    }

    /// Appends interleaved `samples` (length must be a multiple of `channels`).
    pub fn write_samples(&mut self, samples: &[f32]) -> Result<(), crate::SynthError> {
        debug_assert!(samples.len().is_multiple_of(self.channels as usize));
        for s in samples {
            self.file.write_all(&s.to_le_bytes())?;
        }
        self.frames_written += (samples.len() as u64) / self.channels as u64;
        Ok(())
    }

    /// Number of frames written so far.
    pub fn frames_written(&self) -> u64 {
        self.frames_written
    }

    /// Finalizes the file by patching the RIFF/data sizes.
    pub fn finalize(mut self) -> Result<(), crate::SynthError> {
        self.file.flush()?;
        let data_bytes = self.frames_written * self.channels as u64 * 4;
        let file_bytes = 36 + data_bytes;
        let mut file = self
            .file
            .into_inner()
            .map_err(|e| crate::SynthError::Io(e.into_error()))?;
        use std::io::{Seek, SeekFrom};
        file.seek(SeekFrom::Start(4))?;
        file.write_all(&(file_bytes as u32).to_le_bytes())?;
        file.seek(SeekFrom::Start(40))?;
        file.write_all(&(data_bytes as u32).to_le_bytes())?;
        file.flush()?;
        Ok(())
    }

    /// Sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    /// Channels.
    pub fn channels(&self) -> u16 {
        self.channels
    }
}

/// Writes interleaved float samples as 16-bit PCM WAV.
///
/// # Errors
///
/// Returns [`SynthError::Io`] on write failures.
pub fn write_i16_wav(
    path: impl AsRef<Path>,
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
) -> Result<(), SynthError> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    let data_len = (samples.len() * 2) as u32;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVE")?;
    f.write_all(b"fmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?; // PCM
    f.write_all(&channels.to_le_bytes())?;
    f.write_all(&sample_rate.to_le_bytes())?;
    f.write_all(&(sample_rate * channels as u32 * 2).to_le_bytes())?;
    f.write_all(&(channels * 2).to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        f.write_all(&v.to_le_bytes())?;
    }
    f.flush()?;
    Ok(())
}

fn skip(r: &mut impl Read, mut n: usize) -> Result<(), SynthError> {
    let mut buf = [0u8; 4096];
    while n > 0 {
        let chunk = n.min(4096);
        r.read_exact(&mut buf[..chunk])?;
        n -= chunk;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav_with_chunks(fmt_tag: u16, channels: u16, rate: u32, bits: u16, data: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&fmt_tag.to_le_bytes());
        v.extend_from_slice(&channels.to_le_bytes());
        v.extend_from_slice(&rate.to_le_bytes());
        let block_align = channels * (bits / 8);
        v.extend_from_slice(&(rate * block_align as u32).to_le_bytes());
        v.extend_from_slice(&block_align.to_le_bytes());
        v.extend_from_slice(&bits.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&(data.len() as u32).to_le_bytes());
        v.extend_from_slice(data);
        v
    }

    /// DEBT-02 #119：24-bit 负样本必须按符号扩展解码（旧实现负变正）。
    #[test]
    fn decodes_24bit_pcm_with_sign() {
        let mut data = Vec::new();
        data.extend_from_slice(&[0x00, 0x00, 0xC0]); // -0.5
        data.extend_from_slice(&[0x00, 0x00, 0x40]); // +0.5
        let bytes = wav_with_chunks(1, 1, 48_000, 24, &data);
        let wav = read_wav_from(&mut bytes.as_slice()).expect("24-bit WAV 应可读");
        assert!(
            (wav.samples[0] + 0.5).abs() < 1e-4,
            "负半幅应为 -0.5，实际 {}",
            wav.samples[0]
        );
        assert!(
            (wav.samples[1] - 0.5).abs() < 1e-4,
            "正半幅应为 +0.5，实际 {}",
            wav.samples[1]
        );
    }

    /// DEBT-02 #119：短 fmt 块必须报错而不是越界 panic。
    #[test]
    fn short_fmt_chunk_is_rejected_without_panic() {
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&4u32.to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&12u32.to_le_bytes());
        v.extend_from_slice(&[0u8; 12]);
        assert!(read_wav_from(&mut v.as_slice()).is_err());
    }

    /// DEBT-02 #119：data 长度超过硬上限必须拒绝，不做巨量分配。
    #[test]
    fn huge_data_size_is_rejected_without_allocation() {
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes()); // PCM
        v.extend_from_slice(&1u16.to_le_bytes()); // mono
        v.extend_from_slice(&48_000u32.to_le_bytes());
        v.extend_from_slice(&96_000u32.to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_wav_from(&mut v.as_slice()).is_err());
    }
}
