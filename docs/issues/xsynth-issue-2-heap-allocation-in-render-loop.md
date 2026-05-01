# Render thread allocates a new Vec on every iteration and uses fixed sleep pattern

**Repo**: `BlackMIDIDevs/xsynth`  
**Crate**: `xsynth-realtime` (render loop) + `xsynth-core` (BufferedRenderer scheduling)  
**File**: `src/realtime_synth.rs` lines 163-165 + `src/buffered_renderer.rs` lines 136-190

## Issue 2a: Heap allocation per render iteration

The render thread allocates a new `Vec<f32>` on every iteration:

```rust
// realtime_synth.rs:163-165
let mut vec = vec![Default::default(); size * stream_params.channels.count() as usize];
render.read_samples(&mut vec);
```

For a 500ms render window at 44100Hz stereo, this is a 176,400-element allocation (~700KB) per iteration, or ~2.2 allocations/second. While not catastrophic at this rate, it puts pressure on the allocator in what should be a deterministic real-time pipeline.

**Suggestion**: Pre-allocate the Vec outside the loop and reuse it (clear + resize instead of re-allocate). Or use a `VecDeque` pool similar to the one already used in the render pipe.

## Issue 2b: Fixed sleep pattern assumes real-time consumption rate

The render thread schedules its next iteration using a fixed delay based on `render_window_ms`:

```rust
// buffered_renderer.rs:141-142
let delay = Duration::from_secs(1) * size as u32 / stream_params.sample_rate * 90 / 100;

// buffered_renderer.rs:148-150
if samples > last_requested * 110 / 100 {
    spin_sleep::sleep(delay / 10);
}
```

This assumes the audio callback always consumes data at real-time rate. When the consumer is faster (e.g., ALSA PREPARED→RUNNING auto-start rapid-fires callbacks), the render thread can fall behind because it's blocked sleeping.

The `samples > last_requested * 110/100` check is meant to prevent over-production, but it doesn't help when `samples` goes negative (consumer has read more than produced) — it actually lets the render thread run continuously when behind, which is correct. However, the fixed `sleep(delay / 10)` granularity means the render thread can only wake up in coarse steps (e.g., 45ms intervals for a 500ms render window), adding unnecessary latency during catch-up.

**Suggestion**: Replace the polling-based back-pressure with a push-based model, or use a condition variable (`Condvar`) that the audio callback signals when buffer is low, allowing the render thread to wake immediately when needed.
