use std::alloc::{GlobalAlloc, Layout};
use std::cell::Cell;
use std::sync::atomic::{AtomicIsize, Ordering};

// ---------------------------------------------------------------------------
// Backend allocator selection
// ---------------------------------------------------------------------------
// macOS: jemalloc — its macOS backend aggressively munmaps freed segments,
//        keeping RSS close to the true live allocation size.
// Other platforms (Linux, Windows): mimalloc — excellent performance and
//        low fragmentation, with acceptable RSS behaviour on those OSes.
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
use tikv_jemallocator::Jemalloc;

#[cfg(target_os = "macos")]
const BACKEND: Jemalloc = Jemalloc;

#[cfg(not(target_os = "macos"))]
use mimalloc::MiMalloc;

#[cfg(not(target_os = "macos"))]
const BACKEND: MiMalloc = MiMalloc;

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

#[repr(C, align(16))]
struct Header {
    tag: u8,
    _pad: [u8; 7],
    user_size: usize,
    user_align: usize,
}

const HEADER_SIZE: usize = std::mem::size_of::<Header>();
const OFFSET_BACKUP_SIZE: usize = std::mem::size_of::<usize>();

/// Round `n` up to the next multiple of `align`.
/// `align` must be a power of two and non-zero.
const fn round_up(n: usize, align: usize) -> usize {
    debug_assert!(align > 0 && align.is_power_of_two());
    (n + align - 1) & !(align - 1)
}

/// Compute the offset from the base allocation pointer to the user pointer.
/// The user pointer will satisfy the requested `user_align`, and there is
/// room for both the `Header` at the base and an offset-backup word just
/// before the user pointer.
const fn user_offset(user_align: usize) -> usize {
    round_up(HEADER_SIZE + OFFSET_BACKUP_SIZE, user_align)
}

static COUNTERS: [AtomicIsize; AllocTag::COUNT] = [
    AtomicIsize::new(0),
    AtomicIsize::new(0),
    AtomicIsize::new(0),
    AtomicIsize::new(0),
    AtomicIsize::new(0),
    AtomicIsize::new(0),
];

/// Tracks GPU resource memory that does not go through the Rust global
/// allocator (e.g. wgpu textures/buffers allocated by the graphics driver).
static GPU_RESOURCE_BYTES: AtomicIsize = AtomicIsize::new(0);

thread_local! {
    static CURRENT_TAG: Cell<AllocTag> = const { Cell::new(AllocTag::Other) };
}

/// Add `bytes` to the GPU resource counter. Called when a wgpu Texture or
/// Buffer is created.
pub fn add_gpu_resource(bytes: u64) {
    GPU_RESOURCE_BYTES.fetch_add(bytes as isize, Ordering::Relaxed);
}

/// Subtract `bytes` from the GPU resource counter. Called when a wgpu Texture
/// or Buffer is dropped/replaced.
pub fn sub_gpu_resource(bytes: u64) {
    GPU_RESOURCE_BYTES.fetch_sub(bytes as isize, Ordering::Relaxed);
}

/// Current GPU resource memory in bytes.
pub fn gpu_resource_bytes() -> isize {
    GPU_RESOURCE_BYTES.load(Ordering::Relaxed)
}

/// Current GPU resource memory in megabytes.
pub fn gpu_resource_mb() -> f64 {
    gpu_resource_bytes() as f64 / 1_048_576.0
}

fn current_tag() -> AllocTag {
    CURRENT_TAG.with(|c| c.get())
}

/// Run `f` with the current thread's allocation tag set to `tag`.
/// The previous tag is restored when `f` returns.
pub fn with_tag<T>(tag: AllocTag, f: impl FnOnce() -> T) -> T {
    CURRENT_TAG.with(|c| {
        let old = c.get();
        c.set(tag);
        let fn_output = f();
        c.set(old);
        fn_output
    })
}

/// Hint to the backend allocator to purge free pages back to the OS.
/// Call after a batch of large allocations is dropped (e.g. after MIDI
/// parsing completes) to reduce RSS without affecting future allocations.
#[cfg(target_os = "macos")]
pub fn purge_free_pages() {
    // jemalloc: force immediate decay of dirty/muzzy pages via mallctl.
    // 遍历所有 arena 进行 purge，否则只清理 arena 0 效果有限。
    use tikv_jemalloc_ctl::{arenas, epoch, raw};
    let _ = epoch::advance();
    if let Ok(narenas) = arenas::narenas::read() {
        for i in 0..narenas {
            let name = format!("arena.{}.purge\0", i);
            unsafe {
                let _ = raw::write(name.as_bytes(), &mut 0u64);
            }
        }
    }
}

/// Hint to the backend allocator to purge free pages back to the OS.
/// Call after a batch of large allocations is dropped (e.g. after MIDI
/// parsing completes) to reduce RSS without affecting future allocations.
#[cfg(not(target_os = "macos"))]
pub fn purge_free_pages() {
    // mimalloc: direct FFI call to mi_collect.
    unsafe extern "C" {
        fn mi_collect(force: bool);
    }
    unsafe { mi_collect(true) };
}

/// Snapshot of memory attributed to each tag, plus GPU resources.
#[derive(Clone, Copy, Debug, Default)]
pub struct Snapshot {
    /// 各分配标签对应的已追踪内存字节数
    pub bytes: [isize; AllocTag::COUNT],
    /// 未经过全局分配器的 GPU 资源字节数
    pub gpu_resources: isize,
}

impl Snapshot {
    /// 捕获当前所有内存计数器的快照。
    ///
    /// 读取各分配标签的原子计数器与 GPU 资源计数器。
    ///
    /// # 返回值
    /// 返回代表当前内存状态的一份 `Snapshot`
    pub fn capture() -> Self {
        let mut bytes = [0; AllocTag::COUNT];
        for (i, counter) in COUNTERS.iter().enumerate() {
            bytes[i] = counter.load(Ordering::Relaxed);
        }
        Self {
            bytes,
            gpu_resources: gpu_resource_bytes(),
        }
    }

    /// 返回指定分配标签已追踪的内存字节数。
    ///
    /// # 参数
    /// * `tag` — 目标分配标签
    ///
    /// # 返回值
    /// 该标签对应的已追踪字节数（可为负，表示释放多于分配的异常情况）
    pub fn get(&self, tag: AllocTag) -> isize {
        self.bytes[tag as usize]
    }

    /// 返回全部分配标签的已追踪字节数总和。
    ///
    /// # 返回值
    /// 各标签已追踪字节数之和（不含 GPU 资源）
    pub fn total_tracked(&self) -> isize {
        self.bytes.iter().sum()
    }

    /// Total tracked memory including GPU resources.
    pub fn total_with_gpu(&self) -> isize {
        self.total_tracked().saturating_add(self.gpu_resources)
    }

    /// 返回指定 tag 当前追踪的内存用量（单位：MB）。
    pub fn tracked_mb(&self, tag: AllocTag) -> f64 {
        self.get(tag) as f64 / 1_048_576.0
    }

    /// 返回所有已追踪内存用量（不含 GPU 资源），单位为 MB。
    ///
    /// # 返回值
    /// 已追踪内存总量的兆字节数
    pub fn total_mb(&self) -> f64 {
        self.total_tracked() as f64 / 1_048_576.0
    }

    /// 返回含 GPU 资源的总内存用量，单位为 MB。
    ///
    /// # 返回值
    /// 已追踪内存与 GPU 资源合计的兆字节数
    pub fn total_with_gpu_mb(&self) -> f64 {
        self.total_with_gpu() as f64 / 1_048_576.0
    }

    /// 返回 GPU 资源内存用量，单位为 MB。
    ///
    /// # 返回值
    /// GPU 资源字节数折算的兆字节数
    pub fn gpu_mb(&self) -> f64 {
        self.gpu_resources as f64 / 1_048_576.0
    }
}

/// Global allocator that attributes every allocation to the current thread's tag.
pub struct TaggedAlloc;

unsafe impl GlobalAlloc for TaggedAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let tag = current_tag() as u8;
        let user_size = layout.size();
        let user_align = layout.align();
        let offset = user_offset(user_align);

        let combined_size = offset.saturating_add(user_size);
        let header_align = std::mem::align_of::<Header>();
        let combined_align = header_align.max(user_align);

        let combined = match Layout::from_size_align(combined_size, combined_align) {
            Ok(l) => l,
            Err(_) => return std::ptr::null_mut(),
        };

        let ptr = unsafe { BACKEND.alloc(combined) };
        if ptr.is_null() {
            return std::ptr::null_mut();
        }

        let header = ptr as *mut Header;
        unsafe {
            (*header).tag = tag;
            (*header).user_size = user_size;
            (*header).user_align = user_align;
        }

        // Store the offset just before the user pointer so dealloc can locate
        // the header without needing to know the original alignment.
        let offset_backup_ptr = unsafe { ptr.add(offset - OFFSET_BACKUP_SIZE) as *mut usize };
        unsafe {
            *offset_backup_ptr = offset;
        }

        COUNTERS[tag as usize].fetch_add(user_size as isize, Ordering::Relaxed);

        unsafe { ptr.add(offset) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        let offset = unsafe { *(ptr.sub(OFFSET_BACKUP_SIZE) as *const usize) };
        let header_ptr = unsafe { ptr.sub(offset) as *mut Header };
        let header = unsafe { &*header_ptr };
        let tag = header.tag as usize;
        let user_size = header.user_size;
        let user_align = header.user_align;

        COUNTERS[tag].fetch_sub(user_size as isize, Ordering::Relaxed);

        let combined_size = offset.saturating_add(user_size);
        let header_align = std::mem::align_of::<Header>();
        let combined_align = header_align.max(user_align);
        let combined = unsafe { Layout::from_size_align_unchecked(combined_size, combined_align) };
        unsafe { BACKEND.dealloc(header_ptr as *mut u8, combined) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_layout = match Layout::from_size_align(new_size, layout.align()) {
            Ok(l) => l,
            Err(_) => return std::ptr::null_mut(),
        };
        let new_ptr = unsafe { self.alloc(new_layout) };
        if !new_ptr.is_null() {
            let copy_size = layout.size().min(new_size);
            unsafe {
                std::ptr::copy_nonoverlapping(ptr, new_ptr, copy_size);
                self.dealloc(ptr, layout);
            }
        }
        new_ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { self.alloc(layout) };
        if !ptr.is_null() {
            unsafe { std::ptr::write_bytes(ptr, 0, layout.size()) };
        }
        ptr
    }
}

#[cfg(test)]
mod tests {
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
