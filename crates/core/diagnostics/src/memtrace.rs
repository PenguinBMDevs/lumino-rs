//! # 安全契约（Safety Contract）— DRI: lumino-core 内存诊断负责人
//!
//! 本模块实现进程级全局分配器 `TaggedAlloc`，替换默认分配器。
//! 一旦触发 UB，整个进程内存布局即损坏，属「爆炸半径最大」级别。
//!
//! ## 安全不变量
//! - `OFFSET_BACKUP_SIZE` 与 `Header` 布局必须一致：`alloc` 在用户数据前插入
//!   `Header` + 备份 offset，`dealloc` 据此逆推原始指针与 Header。
//! - `BACKEND`（mimalloc/jemalloc）在 `alloc`/`dealloc`/`realloc` 中对称调用，
//!   且仅经 `#[global_allocator]` 路径使用，禁止手动构造实例。
//! - `realloc` 先按新布局分配再拷贝，旧指针拷贝完成后才释放。
//! - 所有 `unsafe` 解引用均假设分配器运行时自身传入合法 `Layout`。
//!
//! ## 验证要求
//! - 任何改动建议在 CI 接入 `cargo +nightly miri test -p lumino-core-diagnostics`
//!   以捕获数据竞争与 UB，未通过不得合并。

mod allocator;
mod stats;

pub use allocator::{purge_free_pages, with_tag};
pub use stats::{add_gpu_resource, gpu_resource_bytes, gpu_resource_mb, sub_gpu_resource};

/// Allocation tag used to attribute heap memory to a subsystem.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum AllocTag {
    /// MIDI 数据分配
    Midi = 0,
    /// 音色库采样数据分配
    SoundFont = 1,
    /// 音频引擎分配
    Audio = 2,
    /// GPU 显存/缓冲分配
    Gpu = 3,
    /// UI / 状态分配
    Ui = 4,
    /// 唯一兜底桶：未显式归类的分配（含 3rd-party crate 在其自有线程上的分配，
    /// 如 xsynth 语音缓冲、iced/wgpu 内部线程）默认落入此处。
    Other = 5,
}

impl AllocTag {
    const COUNT: usize = 6;

    /// 全部分配标签的固定顺序列表（共 `AllocTag::COUNT` 个）
    pub const ALL: [AllocTag; Self::COUNT] = [
        AllocTag::Midi,
        AllocTag::SoundFont,
        AllocTag::Audio,
        AllocTag::Gpu,
        AllocTag::Ui,
        AllocTag::Other,
    ];

    /// 返回该标签可读的中文名称。
    ///
    /// # 参数
    /// * `self` — 需要获取名称的分配标签
    ///
    /// # 返回值
    /// 标签对应的静态字符串描述
    pub fn name(self) -> &'static str {
        match self {
            AllocTag::Midi => "MIDI 数据",
            AllocTag::SoundFont => "音色库采样",
            AllocTag::Audio => "音频引擎",
            AllocTag::Gpu => "GPU 显存/缓冲",
            AllocTag::Ui => "UI / 状态",
            AllocTag::Other => "其他（未显式归类）",
        }
    }
}

/// Snapshot of memory attributed to each tag, plus GPU resources.
#[derive(Clone, Copy, Debug, Default)]
pub struct Snapshot {
    /// 各分配标签对应的已追踪内存字节数
    pub bytes: [isize; AllocTag::COUNT],
    /// 未经过全局分配器的 GPU 资源字节数
    pub gpu_resources: isize,
}

/// Global allocator that attributes every allocation to the current thread's tag.
pub struct TaggedAlloc;

#[cfg(test)]
mod tests {
    use super::allocator::{HEADER_SIZE, OFFSET_BACKUP_SIZE, current_tag, round_up, user_offset};
    use super::*;

    #[test]
    fn round_up_boundary_cases() {
        assert_eq!(round_up(0, 16), 0);
        assert_eq!(round_up(1, 16), 16);
        assert_eq!(round_up(16, 16), 16);
        assert_eq!(round_up(17, 16), 32);
    }

    #[test]
    fn round_up_small_aligns() {
        assert_eq!(round_up(0, 1), 0);
        assert_eq!(round_up(1, 1), 1);
        assert_eq!(round_up(2, 1), 2);
        assert_eq!(round_up(3, 2), 4);
        assert_eq!(round_up(3, 4), 4);
        assert_eq!(round_up(5, 8), 8);
    }

    #[test]
    fn user_offset_minimum_and_alignment() {
        for &align in &[1, 2, 4, 8, 16, 32, 64, 128] {
            let off = user_offset(align);
            // Must have room for Header + OFFSET_BACKUP_SIZE
            assert!(
                off >= HEADER_SIZE + OFFSET_BACKUP_SIZE,
                "user_offset({}) = {} < HEADER_SIZE + OFFSET_BACKUP_SIZE = {}",
                align,
                off,
                HEADER_SIZE + OFFSET_BACKUP_SIZE
            );
            // Must be a multiple of the requested alignment
            assert_eq!(
                off % align,
                0,
                "user_offset({}) = {} not aligned",
                align,
                off
            );
        }
    }

    #[test]
    fn snapshot_capture_does_not_panic() {
        let snap = Snapshot::capture();
        let _ = snap.total_tracked();
        let _ = snap.total_with_gpu();
        let _ = snap.total_mb();
        let _ = snap.total_with_gpu_mb();
        let _ = snap.gpu_mb();
    }

    #[test]
    fn alloc_tag_all_names_nonempty() {
        for &tag in &AllocTag::ALL {
            assert!(!tag.name().is_empty(), "tag {:?} has empty name", tag);
        }
    }

    #[test]
    fn with_tag_sets_and_restores() {
        let original = current_tag();
        let tag_result = with_tag(AllocTag::Audio, || {
            assert_eq!(current_tag(), AllocTag::Audio);
            "done"
        });
        assert_eq!(tag_result, "done");
        assert_eq!(current_tag(), original);
    }

    #[test]
    fn with_tag_nested() {
        let original = current_tag();
        with_tag(AllocTag::Gpu, || {
            with_tag(AllocTag::Audio, || {
                assert_eq!(current_tag(), AllocTag::Audio);
            });
            assert_eq!(current_tag(), AllocTag::Gpu);
        });
        assert_eq!(current_tag(), original);
    }
}
