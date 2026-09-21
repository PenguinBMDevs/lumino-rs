pub(super) fn build_tempo_segs(tempos: &[(u64, u32)], tpb: u64) -> Vec<(u64, f64, f64)> {
    let mut segs = Vec::with_capacity(tempos.len() + 1);
    let mut prev_tick = 0u64;
    let mut prev_tempo = 500_000.0;
    let mut cum = 0.0;
    for &(tick, us) in tempos {
        segs.push((prev_tick, cum, prev_tempo));
        cum += (tick - prev_tick) as f64 * prev_tempo / 1_000_000.0 / tpb as f64;
        prev_tick = tick;
        prev_tempo = us as f64;
    }
    segs.push((prev_tick, cum, prev_tempo));
    segs
}
pub(super) fn ticks_to_sample(tick: u64, segs: &[(u64, f64, f64)], tpb: u64, sr: u32) -> u32 {
    let i = segs
        .partition_point(|&(s, _, _)| s <= tick)
        .saturating_sub(1);
    let (st, cum, us) = segs[i];
    let sec = cum + (tick - st) as f64 * us / 1_000_000.0 / tpb as f64;
    (sec * sr as f64).round() as u32
}
