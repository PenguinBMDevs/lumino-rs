use super::*;

/// A storage buffer with dynamic (grow-on-demand) capacity.
#[derive(Debug)]
pub struct GrowableBuffer {
    buffer: wgpu::Buffer,
    size: u64,
    max_capacity: u64,
    usage: wgpu::BufferUsages,
    label: String,
}

impl GrowableBuffer {
    /// Creates a growable storage buffer with an initial `capacity` bytes.
    /// The initial allocation is zeroed (see `with_max_capacity`).
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        capacity: u64,
        usage: wgpu::BufferUsages,
    ) -> Self {
        Self::with_max_capacity(device, queue, label, capacity, u64::MAX, usage)
    }

    /// Creates a growable storage buffer whose size is capped at
    /// `max_capacity` bytes. Growth past the cap fails on write instead of
    /// allocating unbounded memory.
    pub fn with_max_capacity(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        capacity: u64,
        max_capacity: u64,
        usage: wgpu::BufferUsages,
    ) -> Self {
        let size = capacity.max(16).min(max_capacity);
        let effective = Self::effective_usage(usage);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: effective,
            mapped_at_creation: false,
        });
        // Zero the initial allocation. wgpu does NOT zero storage buffers,
        // and several buffers (the chunked sample storage above all) can be
        // read past their written region at runtime - a voice may play past
        // the resampled data when the SF2's declared sample_end exceeds the
        // actual rendered length, and that slot must read as silence, not as
        // uninitialized garbage (measured: single samples in the hundreds of
        // millions, audible as loud pops at high polyphony).
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        if size > 0 {
            encoder.clear_buffer(&buffer, 0, Some(size));
        }
        queue.submit(Some(encoder.finish()));
        Self {
            buffer,
            size,
            max_capacity,
            usage,
            label: label.to_string(),
        }
    }

    /// `MAP_READ` buffers are copy *destinations* only (`COPY_SRC` combined
    /// with `MAP_READ` is rejected by wgpu); everything else also needs
    /// `COPY_SRC` so growth can carry the old contents over.
    fn effective_usage(usage: wgpu::BufferUsages) -> wgpu::BufferUsages {
        let mut u = usage;
        if !u.contains(wgpu::BufferUsages::MAP_READ) {
            u |= wgpu::BufferUsages::COPY_SRC;
        }
        u | wgpu::BufferUsages::COPY_DST
    }

    /// Returns the current backing buffer.
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    /// Returns the allocated size in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Grows the buffer so it holds at least `needed` bytes, preserving the
    /// old contents when the buffer can act as a copy source (returns
    /// `true` if it grew, so callers can rebuild bind groups).
    pub fn ensure(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, needed: u64) -> bool {
        if needed <= self.size {
            return false;
        }
        let new_size = (self.size * 2).max(needed).min(self.max_capacity);
        let new_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&self.label),
            size: new_size,
            usage: Self::effective_usage(self.usage),
            mapped_at_creation: false,
        });
        let can_copy_src = !self.usage.contains(wgpu::BufferUsages::MAP_READ);
        let old_size = self.size;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        // The ENTIRE new buffer is zeroed: wgpu does not zero storage
        // buffers, and the voice output buffer is only fully written when
        // the voice count is at its historical maximum - on the block where
        // it grows, slots above the old size are not covered by the render
        // pass yet, and the mix pass would sum that garbage into the output
        // (measured: single samples in the hundreds of millions, audible as
        // pops/crackle at high polyphony). Old contents are carried over via
        // copy only when the buffer needs them (growable working buffers);
        // zeroing first keeps the semantic "everything unwritten reads as
        // silence" for every slot.
        if new_size > 0 {
            encoder.clear_buffer(&new_buf, 0, Some(new_size));
        }
        if old_size > 0 && can_copy_src {
            encoder.copy_buffer_to_buffer(&self.buffer, 0, &new_buf, 0, old_size);
        }
        queue.submit(Some(encoder.finish()));
        self.buffer = new_buf;
        self.size = new_size;
        true
    }

    /// Writes `data` at `offset`, growing the buffer first if needed.
    ///
    /// Growing creates a new buffer, copies the old contents into it, and
    /// returns `true` so callers can rebuild bind groups that reference it.
    /// Fails with an error when the write would exceed `max_capacity`.
    pub fn write(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        offset: u64,
        data: &[u8],
    ) -> Result<bool, SynthError> {
        let end = offset + data.len() as u64;
        if end > self.max_capacity {
            return Err(SynthError::Gpu(format!(
                "buffer '{}' write would exceed capacity {} bytes (need {end})",
                self.label, self.max_capacity
            )));
        }
        if end > self.size {
            let new_size = (self.size * 2).max(end.max(1024)).min(self.max_capacity);
            let new_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&self.label),
                size: new_size,
                usage: Self::effective_usage(self.usage),
                mapped_at_creation: false,
            });
            // Zero the freshly allocated region (wgpu does not zero storage
            // buffers): un-written slots must read as silence. The chunked
            // sample storage is only written up to the last sample's end -
            // a voice that plays past it (SF2 sample_end > rendered length)
            // would otherwise read uninitialized garbage (measured: recurring
            // single-sample pops ~40000 in dense MIDI).
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            if new_size > self.size {
                encoder.clear_buffer(&new_buf, self.size, Some(new_size - self.size));
            }
            // Copy old contents (if any) into the new buffer.
            let can_copy_src = !self.usage.contains(wgpu::BufferUsages::MAP_READ);
            if self.size > 0 && can_copy_src {
                encoder.copy_buffer_to_buffer(&self.buffer, 0, &new_buf, 0, self.size);
            }
            queue.submit(Some(encoder.finish()));
            self.buffer = new_buf;
            self.size = new_size;
            queue.write_buffer(&self.buffer, offset, data);
            return Ok(true);
        }
        queue.write_buffer(&self.buffer, offset, data);
        Ok(false)
    }

    /// Clears the buffer contents to zero.
    pub fn clear(&self, device: &wgpu::Device, queue: &wgpu::Queue) {
        // Zero via write of a small zero chunk (buffers are re-uploaded every
        // block anyway for the fixed-size ones).
        let zeros = vec![0u8; self.size.min(4096) as usize];
        let mut off = 0u64;
        while off < self.size {
            let n = (self.size - off).min(4096);
            queue.write_buffer(&self.buffer, off, &zeros[..n as usize]);
            off += n;
        }
        let _ = device; // device unused; kept for signature symmetry
    }
}
