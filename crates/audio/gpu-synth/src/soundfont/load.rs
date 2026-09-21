use super::*;

/// SFZ 采样数据：各声道 PCM（原生采样率）与原生采样率。
type SfzSampleData = (Vec<Arc<[f32]>>, u32);

impl SoundFont {
    pub fn load(
        path: impl AsRef<Path>,
        bank: u16,
        preset: u16,
        use_effects: bool,
        sample_rate: u32,
    ) -> Result<Self, SoundFontError> {
        let p = path.as_ref();
        let is_sfz = p
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("sfz"))
            .unwrap_or(false);
        if is_sfz {
            return Self::load_sfz(p, bank, preset, use_effects, sample_rate);
        }
        // SF2 path：必须传引擎采样率。XSynth 的加载器会把每份样本重采样到
        // 该目标率，并把 offset/loop 位置转换到该域；写死 44_100 会让 48 kHz
        // 音色库被错误重采样，播放时整体升高 ~147 cents。
        let presets =
            load_soundfont(p, sample_rate).map_err(|e| SoundFontError::Parse(format!("{e}")))?;

        let target = presets
            .iter()
            .find(|p| p.bank == bank && p.preset == preset)
            .ok_or(SoundFontError::MissingPreset(bank, preset))?;

        let mut sf = Self {
            bank,
            preset,
            samples: Vec::new(),
            zone_matrix: (0..128 * 128).map(|_| Vec::new()).collect(),
            zones: Vec::new(),
            sample_ids: HashMap::new(),
            use_effects,
            sample_rate,
            resample_cache: HashMap::new(),
        };

        for region in &target.regions {
            sf.add_region(region);
        }

        Ok(sf)
    }

    /// Loads an SFZ file (bank/preset ignored, kept for API symmetry).
    fn load_sfz(
        path: &Path,
        bank: u16,
        preset: u16,
        use_effects: bool,
        sample_rate: u32,
    ) -> Result<Self, SoundFontError> {
        use xsynth_soundfonts::sfz::parse_soundfont;

        let regions = parse_soundfont(path)
            .map_err(|e| SoundFontError::Parse(format!("SFZ parse error: {e:?}")))?;

        // Unique sample files
        let unique: HashSet<PathBuf> = regions.iter().map(|r| r.sample_path.clone()).collect();
        // Load samples in parallel at native rate
        let samples: HashMap<PathBuf, SfzSampleData> = unique
            .into_par_iter()
            .map(|p| {
                let (data, rate) = Self::load_sfz_sample(&p)
                    .map_err(|e| SoundFontError::Parse(format!("SFZ sample {:?}: {e}", p)))?;
                Ok::<_, SoundFontError>((p, (data, rate)))
            })
            .collect::<Result<_, _>>()?;

        let mut sf = Self {
            bank,
            preset,
            samples: Vec::new(),
            zone_matrix: (0..128 * 128).map(|_| Vec::new()).collect(),
            zones: Vec::new(),
            sample_ids: HashMap::new(),
            use_effects,
            sample_rate,
            resample_cache: HashMap::new(),
        };

        for region in regions {
            // CC triggered regions (key -1) not supported
            if region.keyrange.contains(&-1) {
                continue;
            }
            // Find sample data for this region
            let (sample_data, native_rate) = match samples.get(&region.sample_path) {
                Some(v) => v,
                None => continue,
            };
            sf.add_sfz_region(&region, sample_data, *native_rate);
        }

        Ok(sf)
    }

    /// Loads an SFZ sample file at its native rate (no resampling), returns per-channel Arc<[f32]> and rate.
    fn load_sfz_sample(path: &PathBuf) -> Result<SfzSampleData, String> {
        use std::fs::File;
        use symphonia::core::codecs::DecoderOptions;
        use symphonia::core::formats::FormatOptions;
        use symphonia::core::io::MediaSourceStream;
        use symphonia::core::meta::MetadataOptions;
        use symphonia::core::probe::Hint;

        let file = File::open(path).map_err(|e| format!("open {path:?}: {e}"))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            hint.with_extension(ext);
        }
        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| format!("probe {path:?}: {e:?}"))?;
        let mut format = probed.format;
        let track = format
            .default_track()
            .ok_or_else(|| format!("no track {path:?}"))?;
        let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
        let track_id = track.id;
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| format!("decoder {path:?}: {e:?}"))?;

        // Collect per-channel f32
        let mut chans: Vec<Vec<f32>> = Vec::new();
        let mut inited = false;
        loop {
            let packet = match format.next_packet() {
                Ok(p) => p,
                Err(symphonia::core::errors::Error::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                Err(e) => return Err(format!("packet {path:?}: {e:?}")),
            };
            if packet.track_id() != track_id {
                continue;
            }
            let decoded = decoder
                .decode(&packet)
                .map_err(|e| format!("decode {path:?}: {e:?}"))?;
            // Lazy init channels on first decoded buffer
            if !inited {
                let spec = decoded.spec();
                let n = spec.channels.count();
                chans = vec![Vec::new(); n];
                inited = true;
            }
            // Copy samples per channel
            use symphonia::core::audio::{AudioBufferRef, Signal};
            use symphonia::core::conv::IntoSample;
            macro_rules! copy_chan {
                ($buf:expr) => {
                    for c in 0..$buf.spec().channels.count() {
                        let ch = $buf.chan(c);
                        chans[c].extend(ch.iter().map(|s| IntoSample::<f32>::into_sample(*s)));
                    }
                };
            }
            match decoded {
                AudioBufferRef::U8(b) => copy_chan!(b),
                AudioBufferRef::U16(b) => copy_chan!(b),
                AudioBufferRef::U24(b) => copy_chan!(b),
                AudioBufferRef::U32(b) => copy_chan!(b),
                AudioBufferRef::S8(b) => copy_chan!(b),
                AudioBufferRef::S16(b) => copy_chan!(b),
                AudioBufferRef::S24(b) => copy_chan!(b),
                AudioBufferRef::S32(b) => copy_chan!(b),
                AudioBufferRef::F32(b) => copy_chan!(b),
                AudioBufferRef::F64(b) => copy_chan!(b),
            }
        }
        if chans.is_empty() {
            return Err(format!("no audio data {path:?}"));
        }
        let arcs: Vec<Arc<[f32]>> = chans
            .into_iter()
            .map(|v| Arc::from(v.into_boxed_slice()))
            .collect();
        Ok((arcs, sample_rate))
    }
}
