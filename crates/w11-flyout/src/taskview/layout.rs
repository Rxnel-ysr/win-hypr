//! Pure geometry for the Task View window grid (no GTK, unit-tested).

use super::{CARD_PAD, GAP_X, ROW_GAP, TITLE_H};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Slot {
    pub x: f64,
    pub y: f64,
    /// thumbnail size (the card adds a title row and padding around it)
    pub w: f64,
    pub h: f64,
    pub row: usize,
}

/// Justified rows of equal-height thumbnails, centred in `avail_w` x `avail_h`.
/// Tries every row count and keeps the one that lets the thumbnails be biggest.
pub fn compute(aspects: &[f64], avail_w: f64, avail_h: f64) -> Vec<Slot> {
    let n = aspects.len();
    if n == 0 {
        return Vec::new();
    }
    let a: Vec<f64> = aspects.iter().map(|x| x.clamp(0.6, 2.4)).collect();
    let total: f64 = a.iter().sum();
    let chrome = TITLE_H + 2.0 * CARD_PAD;

    let mut best: Option<(f64, Vec<Vec<usize>>)> = None;
    for r in 1..=n {
        let target = total / r as f64;
        let mut rows: Vec<Vec<usize>> = vec![Vec::new(); r];
        let mut cum = 0.0;
        for (i, ai) in a.iter().enumerate() {
            let k = (((cum + ai / 2.0) / target).floor() as usize).min(r - 1);
            rows[k].push(i);
            cum += ai;
        }
        rows.retain(|v| !v.is_empty());
        let nr = rows.len() as f64;
        let h_v = (avail_h - (nr - 1.0) * ROW_GAP) / nr - chrome;
        let h_w = rows
            .iter()
            .map(|row| {
                let s: f64 = row.iter().map(|&i| a[i]).sum();
                let fixed = GAP_X * (row.len() as f64 - 1.0) + 2.0 * CARD_PAD * row.len() as f64;
                (avail_w - fixed) / s
            })
            .fold(f64::INFINITY, f64::min);
        let h = h_v.min(h_w);
        if best.as_ref().map_or(true, |(bh, _)| h > *bh + 0.5) {
            best = Some((h, rows));
        }
    }
    let (h, rows) = best.unwrap_or((120.0, vec![(0..n).collect()]));
    let h = h.min(avail_h * 0.62).max(40.0);

    let nr = rows.len() as f64;
    let block_h = nr * (h + chrome) + (nr - 1.0) * ROW_GAP;
    let mut y = ((avail_h - block_h) / 2.0).max(0.0);
    let mut out = vec![Slot::default(); n];
    for (ri, row) in rows.iter().enumerate() {
        let row_w: f64 = row.iter().map(|&i| a[i] * h + 2.0 * CARD_PAD).sum::<f64>()
            + GAP_X * (row.len() as f64 - 1.0);
        let mut x = ((avail_w - row_w) / 2.0).max(0.0);
        for &i in row {
            let w = a[i] * h;
            out[i] = Slot {
                x,
                y,
                w,
                h,
                row: ri,
            };
            x += w + 2.0 * CARD_PAD + GAP_X;
        }
        y += h + chrome + ROW_GAP;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds_ok(slots: &[Slot], w: f64, h: f64) {
        for s in slots {
            assert!(
                s.x >= -0.5 && s.x + s.w + 2.0 * CARD_PAD <= w + 0.5,
                "x out of bounds: {s:?}"
            );
            assert!(
                s.y >= -0.5 && s.y + s.h + TITLE_H + 2.0 * CARD_PAD <= h + 0.5,
                "y out of bounds: {s:?}"
            );
        }
    }

    #[test]
    fn single_window_is_centred_and_capped() {
        let s = compute(&[16.0 / 9.0], 1400.0, 630.0);
        assert_eq!(s.len(), 1);
        assert!(s[0].h <= 630.0 * 0.62 + 0.01);
        let left = s[0].x;
        let right = 1400.0 - (s[0].x + s[0].w + 2.0 * CARD_PAD);
        assert!((left - right).abs() < 1.0);
        bounds_ok(&s, 1400.0, 630.0);
    }

    #[test]
    fn many_windows_fit() {
        for n in 1..=24 {
            let asp: Vec<f64> = (0..n).map(|i| [1.78, 1.33, 0.8, 2.1][i % 4]).collect();
            let s = compute(&asp, 1400.0, 630.0);
            assert_eq!(s.len(), n);
            bounds_ok(&s, 1400.0, 630.0);
        }
    }

    #[test]
    fn empty_is_empty() {
        assert!(compute(&[], 100.0, 100.0).is_empty());
    }

    #[test]
    fn rows_do_not_overlap() {
        let asp = vec![1.78; 8];
        let s = compute(&asp, 1400.0, 630.0);
        for a in &s {
            for b in &s {
                if a == b {
                    continue;
                }
                let ax2 = a.x + a.w + 2.0 * CARD_PAD;
                let bx2 = b.x + b.w + 2.0 * CARD_PAD;
                let ay2 = a.y + a.h + TITLE_H + 2.0 * CARD_PAD;
                let by2 = b.y + b.h + TITLE_H + 2.0 * CARD_PAD;
                let overlap =
                    a.x < bx2 - 0.5 && b.x < ax2 - 0.5 && a.y < by2 - 0.5 && b.y < ay2 - 0.5;
                assert!(!overlap, "{a:?} overlaps {b:?}");
            }
        }
    }
}
