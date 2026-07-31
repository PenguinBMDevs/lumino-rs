//! 音频流构建与锁无关回调

use cpal::traits::StreamTrait;
use cpal::{Device, Stream, SupportedStreamConfig};

use super::SendSyncStream;

/// 构建音频流（锁无关回调）
pub(super) fn build_stream(
    device: &Device,
    stream_config: SupportedStreamConfig,
    sample_rx: crossbeam_channel::Receiver<Vec<f32>>,
    vec_return_tx: crossbeam_channel::Sender<Vec<f32>>,
) -> Stream {
    let err_fn = |err| eprintln!("an error occurred on stream: {err}");
    let channels = stream_config.channels();
    let mut limiter = xsynth_core::effects::VolumeLimiter::new(channels);
    let mut remainder = Vec::new();
    let mut output_vec =
        Vec::with_capacity(stream_config.sample_rate().0 as usize * channels as usize / 100);

    device
        .build_output_stream(
            &stream_config.into(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                audio_callback(data, &mut output_vec, &mut remainder,
                    &sample_rx, &vec_return_tx, &mut limiter);
            },
            err_fn,
            None,
        )
        .expect("failed to build output audio stream")
}

/// 音频回调主体：消费余量 → 拉新 Vec → 限幅 → 拷贝输出。
fn audio_callback(
    data: &mut [f32],
    output_vec: &mut Vec<f32>,
    remainder: &mut Vec<f32>,
    sample_rx: &crossbeam_channel::Receiver<Vec<f32>>,
    vec_return_tx: &crossbeam_channel::Sender<Vec<f32>>,
    limiter: &mut xsynth_core::effects::VolumeLimiter,
) {
    output_vec.resize(data.len(), 0.0);
    let mut i = 0;

    // 1) 消费余量（上次未用完的 Vec 尾部）
    for s in remainder.drain(..) {
        output_vec[i] = s;
        i += 1;
        if i >= output_vec.len() {
            break;
        }
    }

    // 2) 从渲染通道拉新 Vec
    while i < output_vec.len() {
        match sample_rx.try_recv() {
            Ok(buf) => {
                let take = buf.len().min(output_vec.len() - i);
                let src = &buf[..take];
                let dst = &mut output_vec[i..i + take];
                dst.copy_from_slice(src);
                i += take;

                if take < buf.len() {
                    remainder.extend_from_slice(&buf[take..]);
                }
                let _ = vec_return_tx.send(buf);
            }
            Err(_) => break,
        }
    }

    // 3) 限幅（防止削波）
    limiter.limit(output_vec);

    // 4) 拷贝到 cpal 输出
    data.copy_from_slice(output_vec);
}
