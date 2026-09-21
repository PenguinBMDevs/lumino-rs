use super::*;

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

#[repr(C, align(16))]
struct Header {
    tag: u8,
    _pad: [u8; 7],
    user_size: usize,
    user_align: usize,
}

pub(crate) const HEADER_SIZE: usize = std::mem::size_of::<Header>();
pub(crate) const OFFSET_BACKUP_SIZE: usize = std::mem::size_of::<usize>();

/// Round `n` up to the next multiple of `align`.
/// `align` must be a power of two and non-zero.
pub(crate) const fn round_up(n: usize, align: usize) -> usize {
    debug_assert!(align > 0 && align.is_power_of_two());
    (n + align - 1) & !(align - 1)
}

/// Compute the offset from the base allocation pointer to the user pointer.
/// The user pointer will satisfy the requested `user_align`, and there is
/// room for both the `Header` at the base and an offset-backup word just
/// before the user pointer.
pub(crate) const fn user_offset(user_align: usize) -> usize {
    round_up(HEADER_SIZE + OFFSET_BACKUP_SIZE, user_align)
}

pub(crate) static COUNTERS: [AtomicIsize; AllocTag::COUNT] = [
    AtomicIsize::new(0),
    AtomicIsize::new(0),
    AtomicIsize::new(0),
    AtomicIsize::new(0),
    AtomicIsize::new(0),
    AtomicIsize::new(0),
];

thread_local! {
    static CURRENT_TAG: Cell<AllocTag> = const { Cell::new(AllocTag::Other) };
}

pub(crate) fn current_tag() -> AllocTag {
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
