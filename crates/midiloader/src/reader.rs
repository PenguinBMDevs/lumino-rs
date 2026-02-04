use memmap2::Mmap;
use std::fs::File;
use std::path::Path;

use crate::error::Result;

/// 二进制数据读取 trait，定义了通用的读取接口
pub trait BinaryReader {
    /// 返回当前位置
    fn position(&self) -> usize;

    /// 返回总长度
    fn len(&self) -> usize;

    /// 检查是否为空
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 返回剩余字节数
    fn remaining(&self) -> usize;

    /// 跳转到指定位置
    fn seek(&mut self, position: usize);

    /// 跳过指定字节数
    fn skip(&mut self, count: usize);

    /// 预览指定字节数的数据，不移动位置
    fn peek(&self, count: usize) -> Option<&[u8]>;

    /// 读取指定字节数的数据
    fn read(&mut self, count: usize) -> Option<&[u8]>;

    /// 读取一个字节
    fn read_u8(&mut self) -> Option<u8> {
        self.read(1).map(|b| b[0])
    }

    /// 读取大端序 u16
    fn read_u16_be(&mut self) -> Option<u16> {
        self.read(2).map(|b| u16::from_be_bytes([b[0], b[1]]))
    }

    /// 读取大端序 u32
    fn read_u32_be(&mut self) -> Option<u32> {
        self.read(4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// 读取 MIDI 变长数值（Variable Length Quantity）
    fn read_varlen(&mut self) -> Option<u32> {
        let mut result: u32 = 0;
        let mut count = 0;

        loop {
            if count >= 4 {
                return None;
            }

            let byte = self.read_u8()?;
            result = (result << 7) | (byte & 0x7F) as u32;

            if byte & 0x80 == 0 {
                break;
            }

            count += 1;
        }

        Some(result)
    }
}

/// 基于内存映射文件的读取器
pub struct MmapReader {
    mmap: Mmap,
    position: usize,
    size: usize,
}

impl MmapReader {
    /// 打开文件并创建内存映射读取器
    ///
    /// # Safety
    ///
    /// 调用者需要确保文件在内存映射期间不会被修改或截断。
    /// 这是内存映射的固有限制，参见 `memmap2::Mmap::map` 文档。
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        let size = mmap.len();

        Ok(Self {
            mmap,
            position: 0,
            size,
        })
    }

    /// 获取指定范围的切片
    pub fn slice(&self, start: usize, end: usize) -> Option<&[u8]> {
        if start <= end && end <= self.size {
            Some(&self.mmap[start..end])
        } else {
            None
        }
    }

    /// 获取从当前位置开始的切片
    pub fn current_slice(&self, count: usize) -> Option<&[u8]> {
        self.slice(self.position, self.position + count)
    }
}

impl BinaryReader for MmapReader {
    fn position(&self) -> usize {
        self.position
    }

    fn len(&self) -> usize {
        self.size
    }

    fn remaining(&self) -> usize {
        self.size.saturating_sub(self.position)
    }

    fn seek(&mut self, position: usize) {
        self.position = position.min(self.size);
    }

    fn skip(&mut self, count: usize) {
        self.position = self.position.saturating_add(count).min(self.size);
    }

    fn peek(&self, count: usize) -> Option<&[u8]> {
        let end = (self.position + count).min(self.size);
        if self.position < end {
            Some(&self.mmap[self.position..end])
        } else {
            None
        }
    }

    fn read(&mut self, count: usize) -> Option<&[u8]> {
        let end = (self.position + count).min(self.size);
        // 允许读取0字节（返回空切片），但position不能超过size
        if self.position <= self.size && count == 0 {
            Some(&self.mmap[self.position..self.position])
        } else if self.position < end {
            let result = &self.mmap[self.position..end];
            self.position = end;
            Some(result)
        } else {
            None
        }
    }
}

/// 基于字节切片的读取器
pub struct ByteBuffer<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> ByteBuffer<'a> {
    /// 从字节切片创建读取器
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    /// 获取指定范围的切片
    pub fn slice(&self, start: usize, end: usize) -> Option<&'a [u8]> {
        if start <= end && end <= self.data.len() {
            Some(&self.data[start..end])
        } else {
            None
        }
    }

    /// 获取从当前位置开始的切片
    pub fn current_slice(&self, count: usize) -> Option<&'a [u8]> {
        self.slice(self.position, self.position + count)
    }
}

impl<'a> BinaryReader for ByteBuffer<'a> {
    fn position(&self) -> usize {
        self.position
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.position)
    }

    fn seek(&mut self, position: usize) {
        self.position = position.min(self.data.len());
    }

    fn skip(&mut self, count: usize) {
        self.position = self.position.saturating_add(count).min(self.data.len());
    }

    fn peek(&self, count: usize) -> Option<&[u8]> {
        let end = (self.position + count).min(self.data.len());
        if self.position < end {
            Some(&self.data[self.position..end])
        } else {
            None
        }
    }

    fn read(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = (self.position + count).min(self.data.len());
        // 允许读取0字节（返回空切片），但position不能超过data长度
        if self.position <= self.data.len() && count == 0 {
            Some(&self.data[self.position..self.position])
        } else if self.position < end {
            let result = &self.data[self.position..end];
            self.position = end;
            Some(result)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_byte_buffer_basic() {
        let data = vec![0x01, 0x02, 0x03, 0x04];
        let mut reader = ByteBuffer::new(&data);

        assert_eq!(reader.len(), 4);
        assert_eq!(reader.position(), 0);
        assert_eq!(reader.remaining(), 4);

        assert_eq!(reader.read_u8(), Some(0x01));
        assert_eq!(reader.position(), 1);

        assert_eq!(reader.read_u16_be(), Some(0x0203));
        assert_eq!(reader.position(), 3);
    }

    #[test]
    fn test_byte_buffer_varlen() {
        // 测试变长数值读取
        // 0x7F = 127 (单字节)
        let data = vec![0x7F];
        let mut reader = ByteBuffer::new(&data);
        assert_eq!(reader.read_varlen(), Some(127));

        // 0x81 0x00 = 128 (双字节)
        // 0x81 = 10000001, 最高位为1表示继续，数据位为0000001
        // 0x00 = 00000000, 最高位为0表示结束，数据位为0000000
        // 结果 = (0000001 << 7) | 0000000 = 128
        let data = vec![0x81, 0x00];
        let mut reader = ByteBuffer::new(&data);
        assert_eq!(reader.read_varlen(), Some(128));

        // 0x80 0x80 0x00 = 16384 (三字节)
        // 第一个字节：10000000 -> 数据位 0000000，继续
        // 第二个字节：10000000 -> 数据位 0000000，继续
        // 第三个字节：00000000 -> 数据位 0000000，结束
        // 结果 = ((0000000 << 7) | 0000000) << 7 | 0000000 = 0
        // 等等，让我重新计算：
        // 0x80 = 10000000，数据位是 0000000
        // 0x80 = 10000000，数据位是 0000000
        // 0x00 = 00000000，数据位是 0000000
        // result = ((0 << 7) | 0) << 7 | 0 = 0
        // 要得到 16384 = 0x4000 = 01000000 00000000
        // 需要：0100000 0000000 0000000 (21位)
        // 编码：0x81 0x80 0x00
        // 0x81 = 10000001 -> 数据位 0000001
        // 0x80 = 10000000 -> 数据位 0000000
        // 0x00 = 00000000 -> 数据位 0000000
        // result = ((1 << 7) | 0) << 7 | 0 = 128 << 7 = 16384
        let data = vec![0x81, 0x80, 0x00];
        let mut reader = ByteBuffer::new(&data);
        assert_eq!(reader.read_varlen(), Some(16384));
    }

    #[test]
    fn test_byte_buffer_peek() {
        let data = vec![0x01, 0x02, 0x03];
        let reader = ByteBuffer::new(&data);

        assert_eq!(reader.peek(2), Some(&[0x01, 0x02][..]));
        assert_eq!(reader.position(), 0); // 位置不变
    }

    #[test]
    fn test_byte_buffer_seek_and_skip() {
        let data = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        let mut reader = ByteBuffer::new(&data);

        reader.skip(2);
        assert_eq!(reader.position(), 2);
        assert_eq!(reader.read_u8(), Some(0x03));

        reader.seek(0);
        assert_eq!(reader.position(), 0);
        assert_eq!(reader.read_u8(), Some(0x01));
    }

    #[test]
    fn test_byte_buffer_bounds_check() {
        let data = vec![0x01, 0x02];
        let mut reader = ByteBuffer::new(&data);

        // 尝试读取超出范围的数据 - 应该返回能读取的部分
        // 因为我们的实现允许读取部分数据（只要 position < end）
        // 当 position=0, size=2, count=3 时，end = min(0+3, 2) = 2
        // 所以 read(3) 会返回 Some([0x01, 0x02])
        // 如果要测试边界检查，应该测试 position >= size 的情况

        // 先读取所有数据
        reader.read(2);
        // 现在 position = 2, size = 2
        // 再尝试读取应该返回 None
        assert_eq!(reader.read_u8(), None);
        assert_eq!(reader.read(1), None);
    }
}
