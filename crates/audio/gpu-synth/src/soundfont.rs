//! Soundfont loading and zone pre-computation.
//!
//! SF2 files are parsed with `xsynth-soundfonts` (the same parser used by the
//! XSynth engine), which means the region data - key/velocity ranges, root
//! keys, loop points, volume envelopes, filter cutoffs and baked note-on
//! modulators - is identical to what XSynth consumes.
//!
//! Sample data handling: SF2 sample data is resampled at load time to the
//! engine `sample_rate` passed by the caller (XSynth's loader requires a
//! target rate and bakes offsets/loops into that domain), so playback needs
//! no further resampling. SFZ sample data is kept at its native rate and
//! **resampled lazily** into the output sample rate the first time it is
//! needed (using the same `rubato` sinc resampler as XSynth), so a 400 MB
//! soundfont costs nothing until its samples are actually played.
//!
//! 注意：SF2 的加载目标必须是引擎采样率。旧实现硬编码 `44_100`，把 48 kHz
//! 音色库的样本重采样成 44.1 kHz 却按原生率标签播放，导致整体升高约
//! 147 cents（44100/48000）；44.1 kHz 音色库恰好不受影响。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rayon::prelude::*;
use xsynth_soundfonts::LoopMode;
use xsynth_soundfonts::sf2::load_soundfont;

use crate::error::SoundFontError;
use crate::synth::dsp::{EnvelopeDescriptor, cents_factor};

mod accessors;
mod load;
mod regions;
mod types;

pub use types::{SoundFont, Zone, ZonePositions};

#[cfg(test)]
mod tests;
