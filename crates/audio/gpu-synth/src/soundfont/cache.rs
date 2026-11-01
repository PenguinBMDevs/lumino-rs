//! REF-LGS-014：进程内音色库缓存。
//!
//! 实时播放、离线导出与端口布局重建可能同时/先后创建多个 `GpuSynth`。
//! 此前每个实例都完整解析一遍 SF2（`xsynth-soundfonts` 会把每份采样重采样
//! 到引擎率），同一文件在进程内驻留多份采样数据：对 30–300 MB 音源是数百
//! MB，对 3.4 GB 的 `F2 Jasmine 2.1` 是 GB 级。
//!
//! 这里按 **(路径, mtime, 大小, 采样率, bank, preset, use_effects)** 缓存解析
//! 结果，缓存项是 `Weak<SoundFontCore>`：
//!
//! - 只要还有引擎持有该核心（实时 + 导出并存、重建时新旧并存），后续加载
//!   就命中共用，采样数据只有一份；
//! - 所有持有者都释放后核心自动回收，缓存不会把大音源长期钉在内存里；
//! - mtime/大小任一变化都会自然 miss，不会读到旧解析。

use super::*;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{OnceLock, Weak};
use std::time::SystemTime;

/// REF-DATA-002 验证：进程级解析次数 / 缓存命中次数（诊断用，不参与控制）。
static PARSE_COUNT: AtomicU64 = AtomicU64::new(0);
static HIT_COUNT: AtomicU64 = AtomicU64::new(0);

/// 读取 `(实际解析次数, 缓存命中次数)`。
pub fn soundfont_cache_stats() -> (u64, u64) {
    (
        PARSE_COUNT.load(Ordering::Relaxed),
        HIT_COUNT.load(Ordering::Relaxed),
    )
}

/// 清零计数（测试隔离；不影响缓存内容）。
pub fn reset_soundfont_cache_stats() {
    PARSE_COUNT.store(0, Ordering::Relaxed);
    HIT_COUNT.store(0, Ordering::Relaxed);
}

/// 缓存键：文件身份 + 解析参数。路径不做 `canonicalize`（避免额外 IO 与
/// Windows 长路径差异）；同一路径字符串 + mtime + size 足以识别文件内容。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    path: PathBuf,
    modified: SystemTime,
    size: u64,
    sample_rate: u32,
    bank: u16,
    preset: u16,
    use_effects: bool,
}

/// 进程级缓存。条目数等于「当前存活的、不同参数的音色库」数量，极小。
static CACHE: OnceLock<Mutex<HashMap<CacheKey, Weak<SoundFontCore>>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<CacheKey, Weak<SoundFontCore>>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_cache() -> std::sync::MutexGuard<'static, HashMap<CacheKey, Weak<SoundFontCore>>> {
    // 缓存只存 Weak，锁中毒不会影响音频正确性；继续用旧表即可。
    cache().lock().unwrap_or_else(|e| e.into_inner())
}

fn is_sfz(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("sfz"))
}

impl SoundFont {
    /// Loads a soundfont through the process-wide cache (REF-LGS-014).
    ///
    /// Behaves exactly like [`SoundFont::load`] (same errors, same data), but
    /// a file that is already parsed and still in use by another engine is
    /// shared instead of parsed again. The cache key includes mtime and size,
    /// so replacing the file on disk always yields a fresh parse.
    ///
    /// SFZ files bypass the cache: their samples live in many files, which a
    /// single `.sfz` mtime cannot represent.
    ///
    /// # Errors
    ///
    /// Same as [`SoundFont::load`]; additionally returns
    /// [`SoundFontError::Parse`] when the file metadata cannot be read.
    pub fn load_cached(
        path: impl AsRef<Path>,
        bank: u16,
        preset: u16,
        use_effects: bool,
        sample_rate: u32,
    ) -> Result<Self, SoundFontError> {
        let p = path.as_ref();
        if is_sfz(p) {
            return Self::load(p, bank, preset, use_effects, sample_rate);
        }
        let meta = std::fs::metadata(p)
            .map_err(|e| SoundFontError::Parse(format!("{p:?} 元数据读取失败: {e}")))?;
        let modified = meta
            .modified()
            .map_err(|e| SoundFontError::Parse(format!("{p:?} mtime 读取失败: {e}")))?;
        let key = CacheKey {
            path: p.to_path_buf(),
            modified,
            size: meta.len(),
            sample_rate,
            bank,
            preset,
            use_effects,
        };

        {
            let mut guard = lock_cache();
            // 顺手清理已无人持有的条目（Weak 失效），保持表小且无陈旧项。
            guard.retain(|_, w| w.strong_count() > 0);
            if let Some(core) = guard.get(&key).and_then(Weak::upgrade) {
                HIT_COUNT.fetch_add(1, Ordering::Relaxed);
                tracing::debug!(
                    "SoundFontCache: 命中 {:?} (sr={sample_rate} bank={bank} preset={preset}，样本 {} B)",
                    p,
                    core.samples.iter().map(|s| s.len() as u64 * 4).sum::<u64>()
                );
                return Ok(Self::from_core(core));
            }
        }

        tracing::debug!("SoundFontCache: 未命中，解析 {:?} (sr={sample_rate})", p);
        let core = Arc::new(Self::load_core(p, bank, preset, use_effects, sample_rate)?);
        PARSE_COUNT.fetch_add(1, Ordering::Relaxed);
        lock_cache().insert(key, Arc::downgrade(&core));
        Ok(Self::from_core(core))
    }

    /// Whether two handles share the same parsed core (test/diagnostic aid).
    pub(crate) fn shares_core_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.core, &other.core)
    }
}
