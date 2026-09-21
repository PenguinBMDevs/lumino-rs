use super::*;

/// Negotiates the `StreamConfig` for `device`.
///
/// Strategy (verified against WASAPI shared mode): the *enumerated* sample
/// rates are a broad claim and opening a stream at a non-default rate often
/// fails with "not supported in shared mode". The robust choice is the
/// device's *default* configuration: it is guaranteed to open. When the
/// device default rate differs from the engine rate, the render thread
/// resamples (see [`SincResampler`]). `resampled` reports that case.
pub(super) fn negotiate_config(
    device: &cpal::Device,
    engine_rate: u32,
    channels: usize,
    block: usize,
) -> Result<(cpal::StreamConfig, bool), SynthError> {
    let default = device
        .default_output_config()
        .map_err(|e| SynthError::Gpu(format!("default output config: {e}")))?;
    let mut stream: cpal::StreamConfig = default.into();
    // The device default is authoritative; keep the engine's channel count
    // only when the device has at least that many (mono on a stereo device
    // is up-mixed by the callback writing L/R, so we keep stereo).
    if (stream.channels as usize) < channels {
        stream.channels = channels as u16;
    }
    // Fixed buffer size is not guaranteed either; prefer the device default
    // unless the device explicitly supports our block size.
    if let Ok(mut iter) = device.supported_output_configs() {
        let compatible = iter.any(|cfg| {
            matches!(
                cfg.buffer_size(),
                cpal::SupportedBufferSize::Range { min, max }
                    if block as u32 >= *min && block as u32 <= *max
            )
        });
        if compatible {
            stream.buffer_size = cpal::BufferSize::Fixed(block as u32);
        }
    }
    let resampled = stream.sample_rate.0 != engine_rate;
    Ok((stream, resampled))
}
