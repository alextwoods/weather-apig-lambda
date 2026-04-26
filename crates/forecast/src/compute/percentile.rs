/// Percentile statistics computed from ensemble member arrays.
///
/// Each field is a `Vec<Option<f64>>` aligned with the hourly time array.
/// A `None` entry indicates that all member values were nil at that time step.
pub struct PercentileStats {
    pub p10: Vec<Option<f64>>,
    pub p25: Vec<Option<f64>>,
    pub median: Vec<Option<f64>>,
    pub p75: Vec<Option<f64>>,
    pub p90: Vec<Option<f64>>,
}

/// Computes p10, p25, median (p50), p75, and p90 percentile statistics from
/// ensemble member arrays using linear interpolation.
///
/// `member_arrays` is a slice of member arrays (one per ensemble member), each
/// of length `time_step_count`. Values are `Option<f64>` — `None` represents a
/// missing/nil value.
///
/// For each time step, non-nil values are collected and sorted. If no non-nil
/// values exist, all percentiles are `None` for that step. Otherwise, linear
/// interpolation between the two nearest ranks is used.
pub fn compute_percentiles(
    member_arrays: &[Vec<Option<f64>>],
    time_step_count: usize,
) -> PercentileStats {
    let mut p10 = Vec::with_capacity(time_step_count);
    let mut p25 = Vec::with_capacity(time_step_count);
    let mut median = Vec::with_capacity(time_step_count);
    let mut p75 = Vec::with_capacity(time_step_count);
    let mut p90 = Vec::with_capacity(time_step_count);

    for t in 0..time_step_count {
        let mut values: Vec<f64> = member_arrays
            .iter()
            .filter_map(|member| member.get(t).copied().flatten())
            .collect();

        if values.is_empty() {
            p10.push(None);
            p25.push(None);
            median.push(None);
            p75.push(None);
            p90.push(None);
        } else {
            values.sort_by(|a, b| a.partial_cmp(b).unwrap());
            p10.push(Some(interpolate_percentile(&values, 0.10)));
            p25.push(Some(interpolate_percentile(&values, 0.25)));
            median.push(Some(interpolate_percentile(&values, 0.50)));
            p75.push(Some(interpolate_percentile(&values, 0.75)));
            p90.push(Some(interpolate_percentile(&values, 0.90)));
        }
    }

    PercentileStats {
        p10,
        p25,
        median,
        p75,
        p90,
    }
}

/// Linearly interpolates a percentile value from a sorted slice.
///
/// Uses the formula: rank = p × (n − 1), then interpolates between the
/// floor and ceil indices. When the rank is an integer, returns the exact
/// value at that index.
pub fn interpolate_percentile(sorted: &[f64], p: f64) -> f64 {
    let rank = p * (sorted.len() as f64 - 1.0);
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    let fraction = rank - lower as f64;
    sorted[lower] + fraction * (sorted[upper] - sorted[lower])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_values_percentiles() {
        // Single time step with 5 members: [1, 2, 3, 4, 5]
        let members: Vec<Vec<Option<f64>>> = vec![
            vec![Some(1.0)],
            vec![Some(2.0)],
            vec![Some(3.0)],
            vec![Some(4.0)],
            vec![Some(5.0)],
        ];

        let stats = compute_percentiles(&members, 1);

        assert_eq!(stats.median[0], Some(3.0));
        assert!((stats.p10[0].unwrap() - 1.4).abs() < 1e-9, "p10 should be 1.4, got {}", stats.p10[0].unwrap());
        assert!((stats.p90[0].unwrap() - 4.6).abs() < 1e-9, "p90 should be 4.6, got {}", stats.p90[0].unwrap());
    }

    #[test]
    fn five_values_p25_p75() {
        let members: Vec<Vec<Option<f64>>> = vec![
            vec![Some(1.0)],
            vec![Some(2.0)],
            vec![Some(3.0)],
            vec![Some(4.0)],
            vec![Some(5.0)],
        ];

        let stats = compute_percentiles(&members, 1);

        // p25: rank = 0.25 * 4 = 1.0 → exact value at index 1 → 2.0
        assert_eq!(stats.p25[0], Some(2.0));
        // p75: rank = 0.75 * 4 = 3.0 → exact value at index 3 → 4.0
        assert_eq!(stats.p75[0], Some(4.0));
    }

    #[test]
    fn all_nil_produces_none() {
        let members: Vec<Vec<Option<f64>>> = vec![
            vec![None],
            vec![None],
            vec![None],
        ];

        let stats = compute_percentiles(&members, 1);

        assert_eq!(stats.p10[0], None);
        assert_eq!(stats.p25[0], None);
        assert_eq!(stats.median[0], None);
        assert_eq!(stats.p75[0], None);
        assert_eq!(stats.p90[0], None);
    }

    #[test]
    fn mixed_nil_and_values() {
        // 5 members, 2 are nil → only [2, 4, 5] contribute
        let members: Vec<Vec<Option<f64>>> = vec![
            vec![None],
            vec![Some(2.0)],
            vec![None],
            vec![Some(4.0)],
            vec![Some(5.0)],
        ];

        let stats = compute_percentiles(&members, 1);

        // sorted: [2, 4, 5], n=3
        // median: rank = 0.5 * 2 = 1.0 → index 1 → 4.0
        assert_eq!(stats.median[0], Some(4.0));
        // All percentiles should be within [2.0, 5.0]
        let p10 = stats.p10[0].unwrap();
        let p90 = stats.p90[0].unwrap();
        assert!(p10 >= 2.0 && p10 <= 5.0);
        assert!(p90 >= 2.0 && p90 <= 5.0);
    }

    #[test]
    fn single_member_all_percentiles_equal() {
        let members: Vec<Vec<Option<f64>>> = vec![vec![Some(42.0)]];

        let stats = compute_percentiles(&members, 1);

        assert_eq!(stats.p10[0], Some(42.0));
        assert_eq!(stats.p25[0], Some(42.0));
        assert_eq!(stats.median[0], Some(42.0));
        assert_eq!(stats.p75[0], Some(42.0));
        assert_eq!(stats.p90[0], Some(42.0));
    }

    #[test]
    fn multiple_time_steps() {
        // 2 members, 3 time steps
        let members: Vec<Vec<Option<f64>>> = vec![
            vec![Some(10.0), Some(20.0), None],
            vec![Some(30.0), Some(40.0), None],
        ];

        let stats = compute_percentiles(&members, 3);

        assert_eq!(stats.median.len(), 3);
        // t=0: sorted [10, 30], median = 10 + 0.5*(30-10) = 20
        assert_eq!(stats.median[0], Some(20.0));
        // t=1: sorted [20, 40], median = 20 + 0.5*(40-20) = 30
        assert_eq!(stats.median[1], Some(30.0));
        // t=2: all nil
        assert_eq!(stats.median[2], None);
    }

    #[test]
    fn empty_member_arrays() {
        let members: Vec<Vec<Option<f64>>> = vec![];
        let stats = compute_percentiles(&members, 3);

        assert_eq!(stats.p10.len(), 3);
        assert!(stats.p10.iter().all(|v| v.is_none()));
        assert!(stats.median.iter().all(|v| v.is_none()));
    }

    #[test]
    fn interpolate_percentile_exact_index() {
        // sorted: [1, 2, 3, 4, 5]
        // p=0.5 → rank = 0.5 * 4 = 2.0 → exact index 2 → 3.0
        assert_eq!(interpolate_percentile(&[1.0, 2.0, 3.0, 4.0, 5.0], 0.5), 3.0);
    }

    #[test]
    fn interpolate_percentile_fractional_rank() {
        // sorted: [1, 2, 3, 4, 5]
        // p=0.1 → rank = 0.1 * 4 = 0.4 → lower=0, upper=1, frac=0.4
        // result = 1.0 + 0.4 * (2.0 - 1.0) = 1.4
        let result = interpolate_percentile(&[1.0, 2.0, 3.0, 4.0, 5.0], 0.1);
        assert!((result - 1.4).abs() < 1e-9);
    }

    #[test]
    fn interpolate_percentile_endpoints() {
        let sorted = [1.0, 2.0, 3.0];
        assert_eq!(interpolate_percentile(&sorted, 0.0), 1.0);
        assert_eq!(interpolate_percentile(&sorted, 1.0), 3.0);
    }

    /// Feature: weather-backend-api, Property 1: Percentile ordering and bounds
    ///
    /// **Validates: Requirements 2.4, 3.4, 3.5**
    mod prop_percentile {
        use super::*;
        use proptest::prelude::*;

        fn member_arrays_strategy()
            -> impl Strategy<Value = (usize, Vec<Vec<Option<f64>>>)>
        {
            (1usize..=100).prop_flat_map(|time_steps| {
                let members = prop::collection::vec(
                    prop::collection::vec(
                        prop::option::of(-1000.0f64..1000.0f64),
                        time_steps..=time_steps,
                    ),
                    1..=161,
                );
                (Just(time_steps), members)
            })
        }

        proptest! {
            #[test]
            fn prop_percentile_ordering_and_bounds(
                (time_steps, members) in member_arrays_strategy()
            ) {
                let stats = compute_percentiles(&members, time_steps);

                for t in 0..time_steps {
                    // Collect non-nil values at this time step
                    let values: Vec<f64> = members
                        .iter()
                        .filter_map(|m| m[t])
                        .collect();

                    if values.is_empty() {
                        // All nil → all percentiles should be None
                        prop_assert!(stats.p10[t].is_none());
                        prop_assert!(stats.p25[t].is_none());
                        prop_assert!(stats.median[t].is_none());
                        prop_assert!(stats.p75[t].is_none());
                        prop_assert!(stats.p90[t].is_none());
                    } else {
                        // All percentiles should be Some
                        let p10 = stats.p10[t].unwrap();
                        let p25 = stats.p25[t].unwrap();
                        let med = stats.median[t].unwrap();
                        let p75 = stats.p75[t].unwrap();
                        let p90 = stats.p90[t].unwrap();

                        // Verify ordering: p10 ≤ p25 ≤ median ≤ p75 ≤ p90
                        prop_assert!(
                            p10 <= p25,
                            "t={}: p10 ({}) > p25 ({})", t, p10, p25
                        );
                        prop_assert!(
                            p25 <= med,
                            "t={}: p25 ({}) > median ({})", t, p25, med
                        );
                        prop_assert!(
                            med <= p75,
                            "t={}: median ({}) > p75 ({})", t, med, p75
                        );
                        prop_assert!(
                            p75 <= p90,
                            "t={}: p75 ({}) > p90 ({})", t, p75, p90
                        );

                        // Verify bounds: all percentiles within [min, max] of members
                        let min_val = values.iter().cloned().fold(f64::INFINITY, f64::min);
                        let max_val = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

                        for (name, val) in [
                            ("p10", p10), ("p25", p25), ("median", med),
                            ("p75", p75), ("p90", p90),
                        ] {
                            prop_assert!(
                                val >= min_val - 1e-9,
                                "t={}: {} ({}) < min ({})", t, name, val, min_val
                            );
                            prop_assert!(
                                val <= max_val + 1e-9,
                                "t={}: {} ({}) > max ({})", t, name, val, max_val
                            );
                        }
                    }
                }
            }
        }
    }
}
