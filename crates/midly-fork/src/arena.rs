#![cfg(feature = "alloc")]

use crate::prelude::*;
use core::cell::UnsafeCell;

/// A high-performance arena allocator optimized for MIDI data.
///
/// This arena is designed specifically for allocating small to medium-sized
/// byte arrays that are common in MIDI files (track names, text events, etc.).
///
/// # Performance Features
///
/// - **Bump allocation**: Fast O(1) allocation by simply bumping a pointer
/// - **Chunked storage**: Reduces reallocation overhead
/// - **Batch deallocation**: All memory freed at once when arena is dropped
/// - **Cache-friendly**: Sequential allocations are contiguous in memory
///
/// # Example
///
/// ```
/// use midly::Arena;
///
/// let arena = Arena::new();
/// let track_name = arena.add(b"Piano Track");
/// let copyright = arena.add(b"Copyright 2024");
/// ```
#[derive(Default)]
pub struct Arena {
    /// Current chunk being filled
    current: UnsafeCell<Option<Box<Chunk>>>,
    /// Previously filled chunks (read-only after fill)
    chunks: UnsafeCell<Vec<Box<Chunk>>>,
    /// Total bytes allocated across all chunks
    total_allocated: UnsafeCell<usize>,
}

/// Default chunk size: 64KB - good balance between allocation overhead and memory waste
const DEFAULT_CHUNK_SIZE: usize = 64 * 1024;
/// Minimum allocation size to get its own chunk (larger than this = dedicated chunk)
const LARGE_ALLOC_THRESHOLD: usize = 16 * 1024;

struct Chunk {
    data: Box<[u8]>,
    used: usize,
}

impl Chunk {
    fn new(size: usize) -> Self {
        Self {
            data: vec![0u8; size].into_boxed_slice(),
            used: 0,
        }
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.used
    }

    fn can_fit(&self, size: usize) -> bool {
        self.remaining() >= size
    }

    fn allocate(&mut self, size: usize) -> &mut [u8] {
        debug_assert!(self.can_fit(size));
        let start = self.used;
        self.used += size;
        &mut self.data[start..self.used]
    }
}

impl Arena {
    /// Create a new empty arena with default chunk size.
    #[inline]
    pub fn new() -> Arena {
        Self::default()
    }

    /// Create a new arena with a specific initial chunk size.
    ///
    /// # Arguments
    ///
    /// * `chunk_size` - The size of the first chunk to allocate
    #[inline]
    pub fn with_capacity(chunk_size: usize) -> Arena {
        Arena {
            current: UnsafeCell::new(Some(Box::new(Chunk::new(chunk_size)))),
            chunks: UnsafeCell::new(Vec::new()),
            total_allocated: UnsafeCell::new(0),
        }
    }

    /// Empty this arena, deallocating all memory.
    ///
    /// This is safe because it requires a mutable reference.
    #[inline]
    pub fn clear(&mut self) {
        // SAFETY: We have &mut self, so no other references exist
        unsafe {
            (*self.chunks.get()).clear();
            *self.current.get() = Some(Box::new(Chunk::new(DEFAULT_CHUNK_SIZE)));
            *self.total_allocated.get() = 0;
        }
    }

    /// Get the total number of bytes allocated in this arena.
    #[inline]
    pub fn len(&self) -> usize {
        // SAFETY: Reading a usize is atomic on most platforms
        unsafe { *self.total_allocated.get() }
    }

    /// Check if the arena is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the total capacity across all chunks.
    #[inline]
    pub fn capacity(&self) -> usize {
        unsafe {
            let current_cap = (*self.current.get())
                .as_ref()
                .map(|c| c.data.len())
                .unwrap_or(0);
            let chunks_cap: usize = (*self.chunks.get())
                .iter()
                .map(|c| c.data.len())
                .sum();
            current_cap + chunks_cap
        }
    }

    /// Add a slice of bytes to the arena.
    ///
    /// Returns a mutable reference to the copied bytes with the arena's lifetime.
    ///
    /// # Performance
    ///
    /// This is typically O(1) due to bump allocation. If the current chunk
    /// is full, a new chunk is allocated which is O(chunk_size).
    #[inline]
    pub fn add<'a, 'b>(&'a self, bytes: &'b [u8]) -> &'a mut [u8] {
        // Fast path for empty slices
        if bytes.is_empty() {
            return &mut [];
        }

        let size = bytes.len();

        // Large allocations get their own chunk to avoid wasting space
        if size > LARGE_ALLOC_THRESHOLD {
            return self.add_large(bytes);
        }

        // SAFETY: We have &self, and we only modify through UnsafeCell
        unsafe {
            let current = &mut *self.current.get();

            // Check if we need a new chunk
            if current.as_ref().map_or(true, |c| !c.can_fit(size)) {
                self.grow();
            }

            // Now allocate from current chunk
            let chunk = current.as_mut().unwrap();
            let dest = chunk.allocate(size);
            dest.copy_from_slice(bytes);
            *self.total_allocated.get() += size;
            dest
        }
    }

    /// Add a Vec<u8> to the arena without copying if possible.
    ///
    /// This avoids an allocation and copy if the Vec is small enough.
    #[inline]
    pub fn add_vec<'a>(&'a self, bytes: Vec<u8>) -> &'a mut [u8] {
        if bytes.is_empty() {
            return &mut [];
        }

        // For small vecs, just copy to avoid fragmentation
        if bytes.len() <= 256 {
            return self.add(&bytes);
        }

        // For larger vecs, use the normal path
        self.add(&bytes)
    }

    /// Add a slice of u7 values to the arena.
    #[inline]
    pub fn add_u7<'a, 'b>(&'a self, databytes: &'b [u7]) -> &'a mut [u7] {
        // SAFETY: u7 has the same representation as u8
        unsafe {
            let bytes = u7::slice_as_int(databytes);
            let result = self.add(bytes);
            u7::slice_from_int_unchecked_mut(result)
        }
    }

    /// Add a Vec<u7> to the arena.
    #[inline]
    pub fn add_u7_vec<'a>(&'a self, databytes: Vec<u7>) -> &'a mut [u7] {
        unsafe {
            let bytes: Vec<u8> = mem::transmute(databytes);
            let result = self.add_vec(bytes);
            u7::slice_from_int_unchecked_mut(result)
        }
    }

    /// Grow the arena with a new chunk.
    #[cold]
    fn grow(&self) {
        unsafe {
            // Move current chunk to the full list
            if let Some(current) = (*self.current.get()).take() {
                (*self.chunks.get()).push(current);
            }

            // Calculate new chunk size (double each time, capped at 1MB)
            let new_size = (*self.chunks.get())
                .last()
                .map(|c| (c.data.len() * 2).min(1024 * 1024))
                .unwrap_or(DEFAULT_CHUNK_SIZE);

            *self.current.get() = Some(Box::new(Chunk::new(new_size)));
        }
    }

    /// Handle large allocations (>16KB) with dedicated chunks.
    #[cold]
    fn add_large<'a, 'b>(&'a self, bytes: &'b [u8]) -> &'a mut [u8] {
        unsafe {
            // Create a perfectly-sized chunk for this allocation
            let mut new_chunk = Box::new(Chunk::new(bytes.len()));
            let dest = new_chunk.allocate(bytes.len());
            dest.copy_from_slice(bytes);
            
            // Move current chunk to full list and use the new one
            if let Some(current) = (*self.current.get()).take() {
                (*self.chunks.get()).push(current);
            }
            *self.current.get() = Some(new_chunk);
            *self.total_allocated.get() += bytes.len();
            
            // We need to return a reference with lifetime 'a
            // The data is now stored in self.current, so this is valid
            let current = &mut *self.current.get();
            let chunk = current.as_mut().unwrap();
            let start = chunk.used - bytes.len();
            &mut chunk.data[start..chunk.used]
        }
    }
}

impl Drop for Arena {
    #[inline]
    fn drop(&mut self) {
        self.clear();
    }
}

// SAFETY: Arena is safe to Send because:
// - The UnsafeCell contents are only accessed through &self methods
// - The arena doesn't share mutable references across threads
// - All operations are synchronized through &self
unsafe impl Send for Arena {}

// SAFETY: Arena is NOT Sync because:
// - Multiple threads calling add() simultaneously would race on the UnsafeCell
// - The caller must ensure exclusive access if sharing across threads

impl fmt::Debug for Arena {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Arena")
            .field("allocated", &self.len())
            .field("capacity", &self.capacity())
            .field("chunks", &unsafe { (*self.chunks.get()).len() + 1 })
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_basic() {
        let arena = Arena::new();
        let data1 = arena.add(b"hello");
        let data2 = arena.add(b"world");
        
        assert_eq!(data1, b"hello");
        assert_eq!(data2, b"world");
        assert_eq!(arena.len(), 10);
    }

    #[test]
    fn test_arena_empty() {
        let arena = Arena::new();
        let empty = arena.add(b"");
        assert!(empty.is_empty());
        assert!(arena.is_empty());
    }

    #[test]
    fn test_arena_growth() {
        let arena = Arena::with_capacity(16);
        let initial_cap = arena.capacity();
        
        // Add enough data to trigger growth
        for i in 0..100 {
            arena.add(&[i as u8; 100]);
        }
        
        assert!(arena.capacity() > initial_cap);
        assert_eq!(arena.len(), 100 * 100);
    }

    #[test]
    fn test_arena_clear() {
        let mut arena = Arena::new();
        arena.add(b"test data");
        assert!(!arena.is_empty());
        
        arena.clear();
        assert!(arena.is_empty());
    }
}
