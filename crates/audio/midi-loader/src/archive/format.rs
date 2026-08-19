//! 压缩格式检测与 MIDI 扩展名判断

use std::path::Path;

/// 支持的压缩包格式枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    /// ZIP 格式（含 .zip / .zipx）。
    Zip,
    /// RAR 格式（.rar）。
    Rar,
    /// 7-Zip 格式（.7z）。
    SevenZ,
    /// 纯 TAR（未压缩）
    Tar,
    /// GZ 压缩的 TAR（.tar.gz / .tgz）
    TarGz,
    /// XZ 压缩的 TAR（.tar.xz / .txz）
    TarXz,
    /// 纯 GZ 压缩文件（单文件压缩，不是 TAR）
    Gz,
    /// 纯 XZ 压缩文件（单文件压缩，不是 TAR）
    Xz,
    /// LZH / LHA 格式（.lzh / .lha）。
    Lzh,
    /// ISO 光盘镜像格式（.iso）。
    Iso,
}

impl ArchiveFormat {
    /// 返回该格式对应的常见扩展名列表（不含点号）
    pub fn extensions(&self) -> &[&str] {
        match self {
            ArchiveFormat::Zip => &["zip", "zipx"],
            ArchiveFormat::Rar => &["rar"],
            ArchiveFormat::SevenZ => &["7z"],
            ArchiveFormat::Tar => &["tar"],
            ArchiveFormat::TarGz => &["tgz"],
            ArchiveFormat::TarXz => &["txz"],
            ArchiveFormat::Gz => &["gz"],
            ArchiveFormat::Xz => &["xz"],
            ArchiveFormat::Lzh => &["lzh", "lha"],
            ArchiveFormat::Iso => &["iso"],
        }
    }
}

/// 根据文件扩展名检测压缩包格式
///
/// 返回 `Some(format)` 如果扩展名匹配已知的压缩格式，否则返回 `None`。
pub fn detect_format(path: &Path) -> Option<ArchiveFormat> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())?
        .to_ascii_lowercase();

    // 处理复合扩展名 .tar.gz / .tar.xz
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let lower_name = file_name.to_ascii_lowercase();

    // 优先检测复合扩展名（长扩展名在前）
    if lower_name.ends_with(".tar.gz") {
        return Some(ArchiveFormat::TarGz);
    }
    if lower_name.ends_with(".tar.xz") {
        return Some(ArchiveFormat::TarXz);
    }
    if lower_name.ends_with(".tar.bz2")
        || lower_name.ends_with(".tbz2")
        || lower_name.ends_with(".tbz")
    {
        return Some(ArchiveFormat::Gz); // unarc-rs handles bz2
    }

    // 单扩展名检测
    match ext.as_str() {
        "zip" | "zipx" => Some(ArchiveFormat::Zip),
        "rar" => Some(ArchiveFormat::Rar),
        "7z" => Some(ArchiveFormat::SevenZ),
        "tar" => Some(ArchiveFormat::Tar),
        "tgz" => Some(ArchiveFormat::TarGz),
        "txz" => Some(ArchiveFormat::TarXz),
        "gz" => Some(ArchiveFormat::Gz),
        "xz" => Some(ArchiveFormat::Xz),
        "lzh" | "lha" => Some(ArchiveFormat::Lzh),
        "iso" => Some(ArchiveFormat::Iso),
        _ => None,
    }
}

/// 判断文件是否为已知的压缩包格式
pub fn is_archive(path: &Path) -> bool {
    detect_format(path).is_some()
}

/// 判断文件名是否为有效的 MIDI 扩展名
///
/// 允许的扩展名: .mid, .midi, .lmpj
pub fn is_midi_extension(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.ends_with(".mid") || name.ends_with(".midi") || name.ends_with(".lmpj")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_detect_zip() {
        assert_eq!(
            detect_format(&PathBuf::from("test.zip")),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(
            detect_format(&PathBuf::from("test.zipx")),
            Some(ArchiveFormat::Zip)
        );
    }

    #[test]
    fn test_detect_rar() {
        assert_eq!(
            detect_format(&PathBuf::from("test.rar")),
            Some(ArchiveFormat::Rar)
        );
    }

    #[test]
    fn test_detect_7z() {
        assert_eq!(
            detect_format(&PathBuf::from("test.7z")),
            Some(ArchiveFormat::SevenZ)
        );
    }

    #[test]
    fn test_detect_tar() {
        assert_eq!(
            detect_format(&PathBuf::from("test.tar")),
            Some(ArchiveFormat::Tar)
        );
    }

    #[test]
    fn test_detect_gz() {
        assert_eq!(
            detect_format(&PathBuf::from("test.gz")),
            Some(ArchiveFormat::Gz)
        );
    }

    #[test]
    fn test_detect_tgz() {
        // .tgz 现在是 TarGz（GZ 压缩的 TAR）
        assert_eq!(
            detect_format(&PathBuf::from("test.tgz")),
            Some(ArchiveFormat::TarGz)
        );
    }

    #[test]
    fn test_detect_tar_gz() {
        assert_eq!(
            detect_format(&PathBuf::from("test.tar.gz")),
            Some(ArchiveFormat::TarGz)
        );
    }

    #[test]
    fn test_detect_xz() {
        assert_eq!(
            detect_format(&PathBuf::from("test.xz")),
            Some(ArchiveFormat::Xz)
        );
    }

    #[test]
    fn test_detect_txz() {
        assert_eq!(
            detect_format(&PathBuf::from("test.txz")),
            Some(ArchiveFormat::TarXz)
        );
    }

    #[test]
    fn test_detect_tar_xz() {
        assert_eq!(
            detect_format(&PathBuf::from("test.tar.xz")),
            Some(ArchiveFormat::TarXz)
        );
    }

    #[test]
    fn test_detect_lzh() {
        assert_eq!(
            detect_format(&PathBuf::from("test.lzh")),
            Some(ArchiveFormat::Lzh)
        );
        assert_eq!(
            detect_format(&PathBuf::from("test.lha")),
            Some(ArchiveFormat::Lzh)
        );
    }

    #[test]
    fn test_detect_iso() {
        assert_eq!(
            detect_format(&PathBuf::from("test.iso")),
            Some(ArchiveFormat::Iso)
        );
    }

    #[test]
    fn test_is_midi_extension() {
        assert!(is_midi_extension("song.mid"));
        assert!(is_midi_extension("song.MID"));
        assert!(is_midi_extension("song.midi"));
        assert!(is_midi_extension("song.lmpj"));
        assert!(!is_midi_extension("song.txt"));
        assert!(!is_midi_extension("song.zip"));
        assert!(!is_midi_extension("song"));
    }

    #[test]
    fn test_is_archive() {
        assert!(is_archive(&PathBuf::from("test.zip")));
        assert!(is_archive(&PathBuf::from("test.rar")));
        assert!(is_archive(&PathBuf::from("test.7z")));
        assert!(is_archive(&PathBuf::from("test.iso")));
        assert!(!is_archive(&PathBuf::from("test.mid")));
        assert!(!is_archive(&PathBuf::from("test.lmpj")));
    }

    #[test]
    fn test_is_midi_file() {
        let path = PathBuf::from("test.mid");
        assert!(super::super::is_midi_file(&path));
        let path = PathBuf::from("test.txt");
        assert!(!super::super::is_midi_file(&path));
    }
}
