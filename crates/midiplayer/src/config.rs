//! 高精度贴图运行时配置

use std::path::PathBuf;
use thiserror::Error;

/// 每个音轨组包含的最大音轨数
pub const TRACKS_PER_GROUP: u16 = 8;

/// 默认贴图宽度（像素），覆盖 4 小节
pub const DEFAULT_TILE_WIDTH_PX: u32 = 1920;

/// 默认每组小节数
pub const DEFAULT_MEASURES_PER_GROUP: u32 = 4;

/// 默认编辑后重生成冷静期（秒）
pub const DEFAULT_COOLDOWN_SECS: u64 = 10;

/// 默认 GPU 显存上限（MB）
///
/// 用户硬约束：不得限制 GPU 内存使用。设为 u32::MAX 表示无限制，
/// 所有贴图常驻 GPU 显存，避免洋葱皮音符因显存淘汰而消失。
pub const DEFAULT_GPU_MEM_LIMIT_MB: u32 = u32::MAX;

/// 默认整合组内存缓冲上限（MB）
pub const DEFAULT_GROUP_TILE_MEM_LIMIT_MB: u32 = 256;

/// 高精度贴图渲染模式
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HiResRenderMode {
    /// 拉伸模式：贴图随 zoom_x 拉伸填充视口（当前默认行为）
    #[default]
    Stretch,
    /// 原生模式：贴图以原生分辨率渲染，按正确速度均匀滚动
    Native,
}

/// 配置校验错误
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("组内小节数必须在 1..=16 之间，当前为 {0}")]
    MeasuresOutOfRange(u32),
    #[error("贴图宽度必须在 480..=7680 之间，当前为 {0}")]
    TileWidthOutOfRange(u32),
    #[error("冷静期必须在 3..=60 秒之间，当前为 {0}")]
    CooldownOutOfRange(u64),
    // GPU 显存上限校验已删除——用户硬约束：不得限制 GPU 内存使用
}

/// 高精度贴图运行时配置
///
/// 从 `UiConfig` 初始化（P2.4 集成），用户可在设置面板调整。
/// 调整后不自动重绘已生成贴图，需用户手动确认重新生成。
#[derive(Clone, Debug)]
pub struct HiResConfig {
    /// 是否启用高精度贴图
    pub enabled: bool,
    /// 每组小节数（时间组宽度），默认 4
    pub measures_per_group: u32,
    /// 贴图宽度（像素，X 方向），默认 1920
    pub tile_width_px: u32,
    /// 编辑后重生成冷静期（秒），默认 10
    pub cooldown_secs: u64,
    /// GPU 显存上限（MB），默认 512
    pub gpu_mem_limit_mb: u32,
    /// 整合组内存缓冲上限（MB），默认 256
    pub group_tile_mem_limit_mb: u32,
    /// 渲染模式：拉伸（随 zoom_x 缩放）或原生（固定分辨率均匀滚动）
    pub render_mode: HiResRenderMode,
    /// 硬盘缓存目录，默认系统 temp/lumino/onion-cache
    pub cache_dir: PathBuf,
}

impl Default for HiResConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            measures_per_group: DEFAULT_MEASURES_PER_GROUP,
            tile_width_px: DEFAULT_TILE_WIDTH_PX,
            cooldown_secs: DEFAULT_COOLDOWN_SECS,
            gpu_mem_limit_mb: DEFAULT_GPU_MEM_LIMIT_MB,
            group_tile_mem_limit_mb: DEFAULT_GROUP_TILE_MEM_LIMIT_MB,
            render_mode: HiResRenderMode::default(),
            cache_dir: default_cache_dir(),
        }
    }
}

impl HiResConfig {
    /// 计算一个时间组覆盖的 tick 数 = measures_per_group × ppq × 4
    ///
    /// 项目硬编码 4/4 拍号，1 小节 = ppq × 4 tick。
    pub fn ticks_per_group(&self, ppq: u16) -> u32 {
        self.measures_per_group * (ppq as u32) * 4
    }

    /// 计算全曲时间组数 = ceil(total_ticks / ticks_per_group)
    pub fn time_group_count(&self, total_ticks: u32, ppq: u16) -> u32 {
        let per_group = self.ticks_per_group(ppq);
        if per_group == 0 {
            return 0;
        }
        total_ticks.div_ceil(per_group)
    }

    /// 计算音轨组数 = ceil(track_count / TRACKS_PER_GROUP)
    pub fn track_group_count(&self, track_count: u16) -> u32 {
        if track_count == 0 {
            return 0;
        }
        (track_count as u32).div_ceil(TRACKS_PER_GROUP as u32)
    }

    /// 计算单张贴图像素字节数 = tile_width_px × key_count × 4
    pub fn tile_byte_len(&self, key_count: u16) -> usize {
        (self.tile_width_px as usize) * (key_count as usize) * 4
    }

    /// 校验配置合理性
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !(1..=16).contains(&self.measures_per_group) {
            return Err(ConfigError::MeasuresOutOfRange(self.measures_per_group));
        }
        if !(480..=7680).contains(&self.tile_width_px) {
            return Err(ConfigError::TileWidthOutOfRange(self.tile_width_px));
        }
        if !(3..=60).contains(&self.cooldown_secs) {
            return Err(ConfigError::CooldownOutOfRange(self.cooldown_secs));
        }
        // GPU 显存上限校验已删除——用户硬约束：不得限制 GPU 内存使用
        Ok(())
    }
}

/// 默认缓存目录 = 系统 temp / lumino / onion-cache
fn default_cache_dir() -> PathBuf {
    std::env::temp_dir().join("lumino").join("onion-cache")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = HiResConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.measures_per_group, 4);
        assert_eq!(cfg.tile_width_px, 1920);
        assert_eq!(cfg.cooldown_secs, 10);
        // GPU 显存上限已改为无限制（u32::MAX）
        assert_eq!(cfg.gpu_mem_limit_mb, u32::MAX);
        assert_eq!(cfg.group_tile_mem_limit_mb, 256);
        assert!(cfg.cache_dir.ends_with("onion-cache"));
    }

    #[test]
    fn test_ticks_per_group() {
        let cfg = HiResConfig::default();
        // 默认 ppq=1920: 4 × 1920 × 4 = 30720
        assert_eq!(cfg.ticks_per_group(1920), 30720);
        // ppq=480: 4 × 480 × 4 = 7680
        assert_eq!(cfg.ticks_per_group(480), 7680);
    }

    #[test]
    fn test_time_group_count() {
        let cfg = HiResConfig::default();
        // ppq=1920, 每组 30720 tick
        // total=30720 → 1 组
        assert_eq!(cfg.time_group_count(30720, 1920), 1);
        // total=30721 → 2 组（向上取整）
        assert_eq!(cfg.time_group_count(30721, 1920), 2);
        // total=768000 (默认全曲) → 25 组
        assert_eq!(cfg.time_group_count(768000, 1920), 25);
        // total=0 → 0 组
        assert_eq!(cfg.time_group_count(0, 1920), 0);
    }

    #[test]
    fn test_track_group_count() {
        let cfg = HiResConfig::default();
        assert_eq!(cfg.track_group_count(0), 0);
        assert_eq!(cfg.track_group_count(1), 1);
        assert_eq!(cfg.track_group_count(8), 1);
        assert_eq!(cfg.track_group_count(9), 2);
        assert_eq!(cfg.track_group_count(16), 2);
        assert_eq!(cfg.track_group_count(17), 3);
    }

    #[test]
    fn test_tile_byte_len() {
        let cfg = HiResConfig::default();
        // 1920 × 128 × 4 = 983040
        assert_eq!(cfg.tile_byte_len(128), 1920 * 128 * 4);
        // 1920 × 256 × 4 = 1966080
        assert_eq!(cfg.tile_byte_len(256), 1920 * 256 * 4);
    }

    #[test]
    fn test_validate_ok() {
        let cfg = HiResConfig::default();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_validate_failures() {
        let cfg = HiResConfig {
            measures_per_group: 0,
            ..HiResConfig::default()
        };
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::MeasuresOutOfRange(0))
        ));

        let cfg = HiResConfig {
            measures_per_group: 17,
            ..HiResConfig::default()
        };
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::MeasuresOutOfRange(17))
        ));

        let cfg = HiResConfig {
            tile_width_px: 100,
            ..HiResConfig::default()
        };
        assert!(cfg.validate().is_err());

        let cfg = HiResConfig {
            cooldown_secs: 1,
            ..HiResConfig::default()
        };
        assert!(cfg.validate().is_err());

        // GPU 显存上限校验已删除——任意值都应通过
        let cfg = HiResConfig {
            gpu_mem_limit_mb: 64,
            ..HiResConfig::default()
        };
        assert!(cfg.validate().is_ok());
    }
}
