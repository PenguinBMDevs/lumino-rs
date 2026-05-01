# BufferedRenderer::read() blocks on recv() in real-time audio callback

**Repo**: `BlackMIDIDevs/xsynth`  
**Crate**: `xsynth-core`  
**File**: `src/buffered_renderer.rs`  
**Line**: 232

## Description

`BufferedRenderer::read()` calls `self.receive.recv().unwrap()` which **blocks the calling thread** until the render thread produces the next audio buffer. This function is called from the ALSA/cpal audio callback via `build_output_stream_for`:

```
audio callback (cpal ALSA poll loop)
  → buffered.lock().unwrap().read(&mut output_vec)   ← real-time thread
    → while self.remainder.is_empty()
      → self.receive.recv().unwrap()                 ← BLOCKS here
```

**Blocking in a real-time audio callback is unsafe by design.** The callback must return within the ALSA period time (typically 25–186ms depending on configuration). If `recv()` blocks — even once — the ALSA playback buffer underruns (`EPIPE`), producing audible glitches and kernel messages:

```
ALSA lib pcm.c:8740:(snd_pcm_recover) underrun occurred
```

## Root cause

The render thread (named `xsynth_buffered_rendering`) sleeps for `0.9 × render_window_ms` between iterations. If the audio callback consumes data faster than real-time — which happens during ALSA's `start_threshold` auto-start sequence (PREPARED → RUNNING transition), where `poll()` fires callbacks at microsecond intervals — the render buffer is depleted before the render thread wakes up:

```
render window = 100ms
  → first buffer = 4410 frames
ALSA period = 8192 frames (when buffer=32768)
  → first callback consumes 8192 frames (> 4410!)
  → remainder exhausted → recv() blocks → UNDERRUN
```

## Suggested fix

Replace `recv()` with `try_recv()` and fill remaining buffer with silence:

```rust
pub fn read(&mut self, dest: &mut [f32]) {
    // Always start with silence as the safe default
    dest.fill(0.0);

    let mut i = 0;

    // Consume from remainder first (non-blocking)
    let len = dest.len().min(self.remainder.len());
    for r in self.remainder.drain(0..len) {
        dest[i] = r;
        i += 1;
    }

    // Try to get more data from render thread — non-blocking
    if i < dest.len() {
        if let Ok(mut buf) = self.receive.try_recv() {
            let remaining = dest.len() - i;
            let copy_len = buf.len().min(remaining);
            for r in buf.drain(0..copy_len) {
                dest[i] = r;
                i += 1;
            }
            self.remainder = buf; // save leftover for next call
        }
        // If try_recv failed → dest[i..] is already zeroed → safe silence
    }
}
```

This guarantees the callback always returns within microseconds, regardless of render thread state. The worst case is a pop/click from a single missed buffer, rather than cascading underruns.

## Reproduction

Any system using `RealtimeSynth` with ALSA (Linux + cpal). Trigger rate increases when:
- Small `render_window_ms` (< ALSA period in frames)
- High audio buffer sizes (period > render window)
- Loaded soundfonts with high polyphony

## Impact

- Users with HDA Intel/PCH audio (common on laptops) see repeated `underrun occurred` messages on startup
- Audio playback stutters during initial buffer pre-fill
- Live performance may glitch when render thread is briefly delayed by GC or scheduling
