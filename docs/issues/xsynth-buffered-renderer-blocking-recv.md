# BufferedRenderer::read() blocks on `recv()` in real-time audio callback, causing persistent ALSA underruns

**Repo**: `BlackMIDIDevs/xsynth`  
**Crates**: `xsynth-core` v0.4.0 + `xsynth-realtime` v0.4.0  
**File**: `xsynth-core/src/buffered_renderer.rs` line 232, `xsynth-realtime/src/realtime_synth.rs` lines 163-165, 136-190

---

## Symptom

Repeated `ALSA lib pcm.c:8740:(snd_pcm_recover) underrun occurred` messages on application startup, even before any MIDI playback begins. The underruns appear the moment the audio stream starts and can persist for several seconds.

Typical log output:

```
2026-05-01T03:02:55.312791Z  INFO XSynth: Audio stream created and started
2026-05-01T03:02:55.332762Z  INFO XSynth: Initialization complete
ALSA lib pcm.c:8740:(snd_pcm_recover) underrun occurred
ALSA lib pcm.c:8740:(snd_pcm_recover) underrun occurred
ALSA lib pcm.c:8740:(snd_pcm_recover) underrun occurred
ALSA lib pcm.c:8740:(snd_pcm_recover) underrun occurred
```

---

## Root Cause: Blocking `recv()` in audio callback (Critical)

`BufferedRenderer::read()` calls `self.receive.recv().unwrap()` which **blocks the audio callback thread** until the render thread produces more data:

```rust
// xsynth-core/src/buffered_renderer.rs:210-246
pub fn read(&mut self, dest: &mut [f32]) {
    dest.fill(0.0);
    let mut i = 0;

    // Consume from remainder
    for r in self.remainder.drain(0..dest.len().min(self.remainder.len())) {
        dest[i] = r; i += 1;
    }

    // ⚠️ BLOCKING: if remainder is exhausted, wait for render thread
    while self.remainder.is_empty() {
        let mut buf = self.receive.recv().unwrap();  // ← BLOCKS HERE
        let len = buf.len().min(dest.len() - i);
        for r in buf.drain(0..len) {
            dest[i] = r; i += 1;
        }
        self.remainder = buf;
    }
}
```

This function is called from the real-time audio callback in `xsynth-realtime`:

```rust
// xsynth-realtime/src/realtime_synth.rs:489-497
move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
    output_vec.resize(data.len(), 0.0);
    buffered.lock().unwrap().read(&mut output_vec);  // ← CAN BLOCK
    for (i, s) in limiter.limit_iter(output_vec.drain(0..)).enumerate() {
        data[i] = ConvertSample::from_f32(s);
    }
}
```

**Blocking in a real-time audio callback is unsafe by design.** The callback must return within the ALSA period time. If `recv()` blocks — even once — ALSA's playback buffer underruns (`EPIPE`), which triggers `snd_pcm_recover` and produces the logged warning.

---

## Trigger Condition: cpal ALSA `start_threshold` causes non-real-time consumption

The cpal ALSA backend sets `start_threshold = buffer - period`:

```rust
// cpal/src/host/alsa/mod.rs:1094-1095
let start_threshold = match stream_type {
    alsa::Direction::Playback => buffer - period,
};
```

When the ALSA stream is in PREPARED state (before auto-start), `poll()` returns `POLLOUT` immediately because the buffer is empty. This causes the audio callback to fire at **microsecond intervals** rather than real-time intervals. Each invocation drains a full period of data from the xsynth render buffer instantly.

The root mathematical condition for instability:

```
If:  render_window_ms × sample_rate / 1000  <  ALSA period (in frames)
Then: the first callback consumes more than one render iteration produces
  → remainder empties immediately
  → recv() blocks
  → callback misses deadline
  → ALSA EPIPE / underrun
```

With default settings:
- `render_window_ms = 10.0` (xsynth default, 441 frames)
- ALSA period ≈ 1102 frames (cpal default, 25ms @ 44100Hz)
- **441 < 1102 → first callback blocks → underrun**

With buffered settings tested:
- `render_window_ms = 100.0` (4410 frames)
- ALSA period = 8192 frames (32768 buffer / 4)
- **4410 < 8192 → first callback blocks → underrun**

---

## Secondary Issue: Render thread heap allocation per iteration + fixed sleep granularity

```rust
// xsynth-realtime/src/realtime_synth.rs:163-165 (inside render loop)
let mut vec = vec![Default::default(); size * stream_params.channels.count() as usize];
render.read_samples(&mut vec);
```

A new `Vec<f32>` is heap-allocated every render iteration. For a 500ms render window at 44100Hz stereo: 176,400 elements × 4 bytes ≈ 700KB per allocation. While not severe in isolation, it adds non-determinism to the audio pipeline.

The render thread also uses a fixed sleep pattern that assumes real-time consumption:

```rust
// xsynth-core/src/buffered_renderer.rs:136-190
let delay = Duration::from_secs(1) * size as u32 / stream_params.sample_rate * 90 / 100;

// Check if ahead, sleep in coarse steps
if samples > last_requested * 110 / 100 {
    spin_sleep::sleep(delay / 10);  // 45-90ms granularity
}
```

When the consumer is faster than real-time (as during the PREPARED→RUNNING transition), the render thread cannot react quickly enough because it only wakes up in coarse intervals.

---

## Suggested Fix

### Primary: Make `read()` non-blocking

Replace the blocking `recv()` with `try_recv()`, filling remaining buffer with silence when no data is available:

```rust
pub fn read(&mut self, dest: &mut [f32]) {
    // Always start with silence as the safe default
    dest.fill(0.0);

    let mut i = 0;

    // Consume from remainder first (non-blocking)
    let copy_len = dest.len().min(self.remainder.len());
    for r in self.remainder.drain(0..copy_len) {
        dest[i] = r;
        i += 1;
    }

    // Non-blocking: try to get next buffer, use silence if not ready
    if i < dest.len() {
        if let Ok(mut buf) = self.receive.try_recv() {
            let remaining = dest.len() - i;
            let copy_len = buf.len().min(remaining);
            for r in buf.drain(0..copy_len) {
                dest[i] = r;
                i += 1;
            }
            self.remainder = buf; // save for next callback
        }
        // If try_recv() fails → dest[i..] is already zeroed → safe silence
    }
}
```

This guarantees the callback always returns in deterministic time. The worst case is a single missed buffer (silence fill) rather than cascading underruns and kernel recovery.

### Secondary: Pre-allocate Vec in render loop

Move the `vec!` allocation outside the loop and reuse via `clear()` + `resize()`.

---

## Reproduction Environment

| Item | Value |
|------|-------|
| **OS** | Ubuntu 24.04.2 LTS (Noble Numbat) |
| **Kernel** | 6.17.0-22-generic |
| **ALSA** | aplay version 1.2.9 |
| **Audio card** | HDA Intel PCH (Realtek ALC295) |
| **Hardware** | HP Laptop 15-dc1xxx |
| **Rust** | rustc 1.95.0 (59807616e 2026-04-14) |
| **xsynth-core** | 0.4.0 (crates.io) |
| **xsynth-realtime** | 0.4.0 (crates.io) |
| **cpal** | 0.15.3 (crates.io) |
| **Soundfont** | Aqua's JV-2080.sf2 (13MB) |
| **Sample rate** | 44100 Hz |
| **Channels** | Stereo |

### Log excerpt (reproduced every launch)

```
2026-05-01T02:34:32.632567Z  INFO lumino_rs: Puffin profiler server started
2026-05-01T02:34:33.263632Z  INFO XSynth: Initializing, soundfont path: "Aqua s JV-2080.sf2"
2026-05-01T02:34:33.263681Z  INFO SoundfontCache: Cache miss, loading soundfont "Aqua s JV-2080.sf2"
2026-05-01T02:34:38.029001Z  INFO XSynth: Soundfont loaded, took 4.77s
Output device: default
2026-05-01T02:34:38.059838Z  INFO XSynth: Audio stream created and started
2026-05-01T02:34:38.061503Z  INFO XSynth: Initialization complete
2026-05-01T02:34:38.074615Z  INFO XSynth: Background init succeeded
ALSA lib pcm.c:8740:(snd_pcm_recover) underrun occurred
ALSA lib pcm.c:8740:(snd_pcm_recover) underrun occurred
ALSA lib pcm.c:8740:(snd_pcm_recover) underrun occurred
ALSA lib pcm.c:8740:(snd_pcm_recover) underrun occurred
```

The underruns appear immediately after audio stream creation, before any MIDI notes are played. The stream was created with `render_window_ms = 500.0` and ALSA buffer = 32768 frames in our local workaround, but the root cause (blocking `recv()`) remains regardless of buffer size.

### Files referenced

- `xsynth-core/src/buffered_renderer.rs:210-246` — `read()` method with blocking `recv()`
- `xsynth-realtime/src/realtime_synth.rs:489-497` — audio callback calling `read()`
- `xsynth-realtime/src/realtime_synth.rs:163-165` — Vec allocation per render iteration
- `xsynth-core/src/buffered_renderer.rs:136-190` — render thread sleep pattern
- `cpal/src/host/alsa/mod.rs:1094-1095` — `start_threshold = buffer - period`
