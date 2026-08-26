//! Waveform comparison metrics used to validate renders against a reference.

/// A segment report for one portion of the audio.
#[derive(Debug, Clone)]
pub struct SegmentReport {
    /// Label of the segment (e.g. a note index or "silence").
    pub label: String,
    /// First frame of the segment (inclusive).
    pub start_frame: usize,
    /// Last frame of the segment (exclusive).
    pub end_frame: usize,
    /// Pearson correlation over the segment (1.0 = identical shape).
    pub correlation: f32,
    /// Normalized RMS error relative to the reference segment energy.
    pub rms_error: f32,
    /// Peak absolute difference within the segment.
    pub peak_error: f32,
}

/// The full comparison report between a rendered signal and a reference.
#[derive(Debug, Clone)]
pub struct CompareReport {
    /// Number of channels compared.
    pub channels: usize,
    /// Frames compared (the minimum length of both signals per channel).
    pub frames: usize,
    /// Pearson correlation over all compared samples.
    pub correlation: f32,
    /// Normalized RMS error (0 = identical, 1 = full-scale difference).
    pub rms_error: f32,
    /// Peak absolute sample difference.
    pub peak_error: f32,
    /// Per-segment breakdowns.
    pub segments: Vec<SegmentReport>,
}

/// Compares two interleaved stereo signals sample-by-sample.
///
/// Both inputs must have the same channel count. The comparison uses the
/// minimum length of the two signals; the remainder is ignored (report the
/// frame counts so callers can detect length differences).
///
/// # Returns
///
/// A [`CompareReport`] with overall metrics plus per-note segments if
/// `segments` is provided (each segment is a frame range `(start, end)`).
pub fn compare(
    reference: &[f32],
    rendered: &[f32],
    channels: usize,
    segments: &[(usize, usize)],
) -> CompareReport {
    let frames = reference
        .len()
        .div_euclid(channels)
        .min(rendered.len().div_euclid(channels));
    let n = frames * channels;

    let corr = correlation(&reference[..n], &rendered[..n]);
    let rms = rms_error(&reference[..n], &rendered[..n]);
    let peak = peak_error(&reference[..n], &rendered[..n]);

    let segment_reports = segments
        .iter()
        .map(|&(s, e)| {
            let s = s.min(frames);
            let e = e.min(frames).max(s);
            let label = format!("frames[{s}..{e})");
            let cs = correlation(
                &reference[s * channels..e * channels],
                &rendered[s * channels..e * channels],
            );
            let re = rms_error(
                &reference[s * channels..e * channels],
                &rendered[s * channels..e * channels],
            );
            let pe = peak_error(
                &reference[s * channels..e * channels],
                &rendered[s * channels..e * channels],
            );
            SegmentReport {
                label,
                start_frame: s,
                end_frame: e,
                correlation: cs,
                rms_error: re,
                peak_error: pe,
            }
        })
        .collect();

    CompareReport {
        channels,
        frames,
        correlation: corr,
        rms_error: rms,
        peak_error: peak,
        segments: segment_reports,
    }
}

/// Pearson correlation between two equal-length sample slices.
pub fn correlation(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    let n = a.len();
    if n == 0 {
        return 1.0;
    }
    let mean_a: f64 = a.iter().map(|&x| x as f64).sum::<f64>() / n as f64;
    let mean_b: f64 = b.iter().map(|&x| x as f64).sum::<f64>() / n as f64;
    let mut cov = 0.0f64;
    let mut va = 0.0f64;
    let mut vb = 0.0f64;
    for i in 0..n {
        let da = a[i] as f64 - mean_a;
        let db = b[i] as f64 - mean_b;
        cov += da * db;
        va += da * da;
        vb += db * db;
    }
    if va == 0.0 && vb == 0.0 {
        1.0
    } else if va == 0.0 || vb == 0.0 {
        0.0
    } else {
        (cov / (va * vb).sqrt()) as f32
    }
}

/// Normalized RMS error: `sqrt(mean((a-b)^2)) / sqrt(mean(a^2))`.
/// Returns 0.0 when the reference is silent.
pub fn rms_error(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len();
    if n == 0 {
        return 0.0;
    }
    let mut se = 0.0f64;
    let mut sa = 0.0f64;
    for i in 0..n {
        let d = a[i] as f64 - b[i] as f64;
        se += d * d;
        sa += a[i] as f64 * a[i] as f64;
    }
    if sa == 0.0 {
        0.0
    } else {
        (se / sa).sqrt() as f32
    }
}

/// Peak absolute sample difference.
pub fn peak_error(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .fold(0.0f32, |acc, (x, y)| acc.max((x - y).abs()))
}

/// Formats a report as a human-readable summary string.
pub fn format_report(report: &CompareReport) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "compare: {} frames x {}ch\n",
        report.frames, report.channels
    ));
    s.push_str(&format!(
        "  correlation : {:.6}\n  rms error   : {:.4}\n  peak error  : {:.6}\n",
        report.correlation, report.rms_error, report.peak_error
    ));
    for seg in &report.segments {
        s.push_str(&format!(
            "  {}: corr={:.6} rms={:.4} peak={:.6}\n",
            seg.label, seg.correlation, seg.rms_error, seg.peak_error
        ));
    }
    s
}
