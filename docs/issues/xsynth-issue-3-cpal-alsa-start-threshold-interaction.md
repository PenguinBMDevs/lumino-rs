# Interaction with cpal ALSA start_threshold causes rapid-fire callbacks

**This is a documentation/awareness issue, not necessarily a bug in either crate alone.**

## Background

When cpal's ALSA backend creates a playback stream, it sets:

```rust
// cpal/src/host/alsa/mod.rs:1094-1095
let start_threshold = match stream_type {
    alsa::Direction::Playback => buffer - period,
    ...
};
```

The ALSA stream starts in PREPARED state. Before `start_threshold` bytes are written, `poll()` returns `POLLOUT` immediately (the buffer is empty and ready). This causes the audio callback to fire at **microsecond intervals** rather than real-time intervals — each callback invocation consumes a full period of audio data from the render pipeline instantly.

When `start_threshold` is reached, the stream auto-starts and real-time consumption begins. But by then, the xsynth render pipeline may have had its entire buffer drained.

## Impact

This is the triggering condition for the blocking `recv()` issue described in Issue #1. Specifically:

- start_threshold = buffer - period frames need to be written before auto-start
- At period frames per callback, that's `(buffer/period - 1)` rapid-fire callbacks
- Each callback calls `BufferedRenderer::read()` which may `recv()` if its remainder is empty
- As shown in Issue #1, if render_window_ms frames < period frames, the remainder empties on the first callback

## Suggested fix for xsynth

xsynth doesn't control cpal's buffer configuration. To avoid this interaction, xsynth should either:

1. **Make `read()` non-blocking** (Issue #1 fix — the primary fix)
2. Document that `render_window_ms` must be > the cpal period in frames
3. Or, use `RealtimeSynth::open()` with a custom `SupportedStreamConfig` that sets `BufferSize::Fixed` with known period/ buffer sizes

For cpal, a secondary fix would be changing `start_threshold` from `buffer - period` to `1` for playback streams, which eliminates the rapid-fire window entirely.
