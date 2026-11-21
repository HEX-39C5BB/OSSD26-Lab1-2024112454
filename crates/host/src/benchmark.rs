/// Returns the nearest-rank P50, P95 and P99 of a nonempty sample batch.
pub fn percentiles(samples: &mut [u128]) -> [u128; 3] {
    assert!(!samples.is_empty(), "benchmark has no samples");
    samples.sort_unstable();
    [50, 95, 99].map(|percent| samples[(samples.len() * percent).div_ceil(100) - 1])
}

#[cfg(test)]
mod tests {
    use super::percentiles;

    #[test]
    fn reports_nearest_rank_percentiles_for_small_and_full_batches() {
        assert_eq!(percentiles(&mut [12]), [12, 12, 12]);
        assert_eq!(percentiles(&mut [9, 1, 5]), [5, 9, 9]);
        let mut samples: Vec<_> = (1..=100).rev().collect();
        assert_eq!(percentiles(&mut samples), [50, 95, 99]);
    }
}
