//! 工程归档单测。

use super::*;

#[test]
fn test_archive_header_roundtrip() {
    let header = ArchiveHeader {
        magic: *b"LMPJ",
        version: 1,
        compression_flags: 0x01,
        file_table_offset: 1024,
        file_table_compressed_size: 256,
        file_table_original_size: 512,
        created_at: 1716883200,
        _reserved: [0u8; 16],
    };
    let bytes = header.to_bytes();
    let decoded = ArchiveHeader::from_bytes(&bytes).expect("解码归档头部失败");
    assert_eq!(&decoded.magic, b"LMPJ");
    assert_eq!(decoded.version, 1);
    assert_eq!(decoded.file_table_offset, 1024);
}

#[test]
fn test_file_entry_roundtrip() {
    let entry = FileEntry {
        path: "data/project/tracks/000.lmtrack".into(),
        data_offset: 55,
        compressed_size: 128,
        original_size: 256,
        crc32: 0xDEADBEEF,
        is_compressed: true,
    };
    let encoded = entry.encode();
    let (decoded, consumed) = FileEntry::decode(&encoded).expect("解码文件条目失败");
    assert_eq!(consumed, encoded.len());
    assert_eq!(decoded.path, entry.path);
    assert_eq!(decoded.data_offset, entry.data_offset);
    assert_eq!(decoded.crc32, entry.crc32);
    assert!(decoded.is_compressed);
}

#[test]
fn test_build_and_read_archive() {
    let files = vec![
        ("metadata.toml".into(), b"name = \"Test\"".to_vec(), true),
        (
            "data/project/tracks/000.lmtrack".into(),
            vec![0x4C, 0x4D, 0x54, 0x52, 0x00, 0x01, 0x00, 0x00],
            true,
        ),
    ];

    let archive = build_archive(&files).expect("构建归档数据失败");
    assert!(!archive.is_empty());

    // 读取 metadata.toml
    let metadata =
        read_file_from_archive(&archive, "metadata.toml").expect("从归档读取metadata.toml失败");
    assert!(metadata.is_some());
    assert_eq!(
        metadata.expect("metadata.toml内容应为Some"),
        b"name = \"Test\""
    );

    // 读取不存在的文件
    let missing = read_file_from_archive(&archive, "notexist").expect("从归档读取不存在的文件失败");
    assert!(missing.is_none());
}

/// 手工构造单块数据区的归档（用于损坏样例测试）。
fn craft_archive(entries: Vec<FileEntry>, data: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0u8; ArchiveHeader::SIZE];
    bytes.extend_from_slice(data);

    let table = FileTable { entries };
    let encoded = table.encode();
    let compressed =
        zstd::stream::encode_all(std::io::Cursor::new(&encoded), 3).expect("压缩文件表失败");

    let header = ArchiveHeader {
        magic: *b"LMPJ",
        version: ARCHIVE_VERSION,
        compression_flags: ARCHIVE_COMPRESSION_ZSTD,
        file_table_offset: bytes.len() as u64,
        file_table_compressed_size: compressed.len() as u64,
        file_table_original_size: encoded.len() as u64,
        created_at: 0,
        _reserved: [0u8; 16],
    };
    bytes.extend_from_slice(&compressed);
    bytes[0..ArchiveHeader::SIZE].copy_from_slice(&header.to_bytes());
    bytes
}

/// DEBT-01 #118：头部/文件表/数据区的截断与越界必须返回错误而不是 panic。
#[test]
fn test_corrupt_archive_returns_errors() {
    // 空文件 / 截断头部
    assert!(read_file_from_archive(&[], "metadata.toml").is_err());
    let short = vec![0u8; 10];
    assert!(read_file_from_archive(&short, "metadata.toml").is_err());

    let mut header = ArchiveHeader {
        magic: *b"XXXX",
        version: ARCHIVE_VERSION,
        compression_flags: ARCHIVE_COMPRESSION_ZSTD,
        file_table_offset: ArchiveHeader::SIZE as u64,
        file_table_compressed_size: 0,
        file_table_original_size: 0,
        created_at: 0,
        _reserved: [0u8; 16],
    };
    let mut bytes = vec![0u8; ArchiveHeader::SIZE];
    bytes[0..ArchiveHeader::SIZE].copy_from_slice(&header.to_bytes());

    // 魔数错误
    let err = read_file_from_archive(&bytes, "x").expect_err("魔数错误必须报错");
    assert!(
        err.to_string().contains("工程文件"),
        "错误文案应指出不是有效工程: {err}"
    );

    // 版本不支持
    header.magic = *b"LMPJ";
    header.version = 2;
    bytes[0..ArchiveHeader::SIZE].copy_from_slice(&header.to_bytes());
    let err = read_file_from_archive(&bytes, "x").expect_err("未知版本必须报错");
    assert!(
        err.to_string().contains("版本"),
        "错误文案应包含版本信息: {err}"
    );

    // 文件表偏移 = u64::MAX：checked 溢出/越界，不 panic
    header.version = ARCHIVE_VERSION;
    header.file_table_offset = u64::MAX;
    header.file_table_compressed_size = 16;
    bytes[0..ArchiveHeader::SIZE].copy_from_slice(&header.to_bytes());
    assert!(read_file_from_archive(&bytes, "x").is_err());

    // 文件表压缩后大小越界
    header.file_table_offset = ArchiveHeader::SIZE as u64;
    header.file_table_compressed_size = u64::MAX;
    bytes[0..ArchiveHeader::SIZE].copy_from_slice(&header.to_bytes());
    assert!(read_file_from_archive(&bytes, "x").is_err());
}

/// DEBT-01 #118：巨量 count 不得触发巨量预分配（立即报错）。
#[test]
fn test_file_table_huge_count_fails_fast() {
    let raw = [0xFF, 0xFF, 0xFF, 0xFF]; // count = u32::MAX，但无条目字节
    assert!(FileTable::decode(&raw).is_err());
}

/// DEBT-01 #118：CRC 不匹配必须被检出（翻转数据区一个字节）。
#[test]
fn test_crc_mismatch_detected() {
    let files = vec![("a.bin".to_string(), b"hello-world".to_vec(), false)];
    let mut archive = build_archive(&files).expect("构建归档失败");
    // 数据区从文件头之后开始；翻转一个字节
    archive[ArchiveHeader::SIZE] ^= 0xFF;

    let err = read_file_from_archive(&archive, "a.bin").expect_err("CRC 不匹配必须报错");
    assert!(
        err.to_string().contains("校验失败"),
        "错误文案应包含校验失败: {err}"
    );
}

/// DEBT-01 #118：原始大小与文件头不一致必须被检出。
#[test]
fn test_original_size_mismatch_detected() {
    let data = b"hello";
    let entry = FileEntry {
        path: "a.bin".into(),
        data_offset: ArchiveHeader::SIZE as u64,
        compressed_size: data.len() as u64,
        original_size: 999, // 与真实值不符
        crc32: compute_crc32(data),
        is_compressed: false,
    };
    let bytes = craft_archive(vec![entry], data);
    assert!(ArchiveReader::new(&bytes).is_ok(), "构造样本应为合法归档");

    let err = read_file_from_archive(&bytes, "a.bin").expect_err("大小不一致必须报错");
    assert!(
        err.to_string().contains("大小"),
        "错误文案应指出大小不一致: {err}"
    );
}
