//! 图片转 MIDI 转换结果缓存（DRAW-030）
//!
//! 一次转换的成本 = 解码 + 调色板聚类 + 抖动；实测 debug 构建下 1920×1080
//! 图片约 0.6s（无抖动）/ 1.2s（误差扩散），2400 边长的大图更久。
//!
//! 放置流程里 `×` 取消与 `√` 写入都会结束放置（清空预览），同一张图再贴
//! 一份或重新框选时若重新解码/聚类，纯属重复劳动。缓存键 = **图片路径 +
//! 文件 mtime/大小 + 全部面板参数**，因此：
//! - 换图片、改任一参数 → 自动失效，不会给出过期结果；
//! - `×` / `√` 之后再点「转换为 MIDI」→ 命中，立即出预览。
//!
//! 只保留最近一次的条目（内存上限 = 一份预览音符），不做过期淘汰。

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use lumino_core::I2mSettings;
use lumino_editor_state::ImageToMidiPreview;

/// 缓存键：同一张图 + 同一组参数才算命中
///
/// `mtime` 与文件大小一起使用：用户替换图片时两者通常都会变，键自动失配。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct I2mCacheKey {
    /// 图片路径
    path: PathBuf,
    /// 文件修改时间（读不到时为 `None`，仍参与比较）
    modified: Option<SystemTime>,
    /// 文件字节数
    len: u64,
    /// 转换参数（稳定 id 口径）
    settings: I2mSettings,
}

impl I2mCacheKey {
    /// 读取文件元信息生成缓存键；文件不存在/无权限时返回 `None`（不缓存）
    pub fn new(path: &Path, settings: I2mSettings) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        Some(Self {
            path: path.to_path_buf(),
            modified: meta.modified().ok(),
            len: meta.len(),
            settings,
        })
    }
}

/// 缓存条目：键 + 转换结果
#[derive(Debug, Clone)]
pub struct I2mCacheEntry {
    /// 生成该预览的键
    pub key: I2mCacheKey,
    /// 转换结果（每色一轨的预览音符）
    pub preview: ImageToMidiPreview,
}

impl I2mCacheEntry {
    /// 命中则返回预览副本（渲染需要独立所有权；音符规模远小于图片本身）
    pub fn lookup(
        cache: &Option<I2mCacheEntry>,
        key: &Option<I2mCacheKey>,
    ) -> Option<ImageToMidiPreview> {
        let key = key.as_ref()?;
        let entry = cache.as_ref()?;
        if entry.key == *key {
            Some(entry.preview.clone())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_editor_state::PreviewNote;

    fn write_file(name: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, bytes).expect("写临时文件失败");
        path
    }

    fn preview(tick: f32) -> ImageToMidiPreview {
        ImageToMidiPreview {
            tracks: vec![vec![PreviewNote {
                tick,
                length: 4.0,
                key: 60,
            }]],
            orig_width: 10.0,
        }
    }

    #[test]
    fn test_key_hits_on_same_file_and_settings() {
        let path = write_file("lumino_i2m_cache_hit.png", b"image-v1");
        let settings = I2mSettings::default();
        let first = I2mCacheKey::new(&path, settings.clone()).expect("应生成键");
        let second = I2mCacheKey::new(&path, settings).expect("应生成键");
        assert_eq!(first, second);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_key_changes_when_settings_change() {
        let path = write_file("lumino_i2m_cache_settings.png", b"image-v1");
        let mut settings = I2mSettings::default();
        let first = I2mCacheKey::new(&path, settings.clone()).expect("应生成键");
        settings.brightness = 10;
        let second = I2mCacheKey::new(&path, settings).expect("应生成键");
        assert_ne!(first, second, "参数变化必须让缓存失效");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_key_changes_when_file_changes() {
        let path = write_file("lumino_i2m_cache_file.png", b"image-v1");
        let settings = I2mSettings::default();
        let first = I2mCacheKey::new(&path, settings.clone()).expect("应生成键");
        // 替换图片（长度不同）
        std::fs::write(&path, b"image-v2-longer").expect("重写临时文件失败");
        let second = I2mCacheKey::new(&path, settings).expect("应生成键");
        assert_ne!(first, second, "图片被替换必须让缓存失效");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_key_is_none_for_missing_file() {
        let path = std::env::temp_dir().join("lumino_i2m_cache_missing_9f3a.png");
        let _ = std::fs::remove_file(&path);
        assert!(I2mCacheKey::new(&path, I2mSettings::default()).is_none());
    }

    #[test]
    fn test_lookup_hits_and_misses() {
        let path = write_file("lumino_i2m_cache_lookup.png", b"image-v1");
        let settings = I2mSettings::default();
        let key = I2mCacheKey::new(&path, settings.clone()).expect("应生成键");
        let cache = Some(I2mCacheEntry {
            key: key.clone(),
            preview: preview(3.0),
        });

        let hit = I2mCacheEntry::lookup(&cache, &Some(key.clone())).expect("应命中");
        assert_eq!(hit.tracks[0][0].tick, 3.0);

        let mut changed = settings;
        changed.sharpen = 5;
        let miss_key = I2mCacheKey::new(&path, changed);
        assert!(I2mCacheEntry::lookup(&cache, &miss_key).is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_lookup_without_cache_or_key_is_none() {
        assert!(I2mCacheEntry::lookup(&None, &None).is_none());
        let path = write_file("lumino_i2m_cache_empty.png", b"image-v1");
        let key = I2mCacheKey::new(&path, I2mSettings::default());
        assert!(I2mCacheEntry::lookup(&None, &key).is_none());
        let _ = std::fs::remove_file(&path);
    }
}
