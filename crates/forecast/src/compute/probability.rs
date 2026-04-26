/// Precipitation probability statistics computed from ensemble member arrays.
///
/// Each field is a `Vec<Option<f64>>` aligned with the hourly time array.
/// Values are percentages in the range [0.0, 100.0].
/// A `None` entry indicates that all member values were nil at that time step.
pub struct PrecipProbability {
    /// Probability of precipitation > 0.1 mm
    pub any: Vec<Option<f64>>,
    /// Probability of precipitation > 2.5 mm
    pub moderate: Vec<Option<f64>>,
    /// Probability of precipitation > 7.5 mm
    pub heavy: Vec<Option<f64>>,
}

/// Thresholds in millimeters for precipitation probability categories.
const THRESHOLD_ANY: f64 = 0.1;
const THRESHOLD_MODERATE: f64 = 2.5;
const THRESHOLD_HEAVY: f64 = 7.5;

/// Computes multi-threshold precipitation probability from ensemble member arrays.
///
/// `member_arrays` is a slice of member arrays (one per ensemble member), each
/// of length `time_step_count`. Values are `Option<f64>` — `None` represents a
/// missing/nil value.
///
/// For each time step, non-nil member values are collected. Members exceeding
/// each threshold (>0.1mm, >2.5mm, >7.5mm) are counted, and the probability is
/// `(count / total_non_nil) * 100.0`. All three thresholds are computed in a
/// single pass per time step for efficiency.
///
/// If all members are nil at a time step, all probabilities are `None`.
pub fn compute_precip_probability(
    member_arrays: &[Vec<Option<f64>>],
    time_step_count: usize,
) -> PrecipProbability {
    let mut any = Vec::with_capacity(time_step_count);
    let mut moderate = Vec::with_capacity(time_step_count);
    let mut heavy = Vec::with_capacity(time_step_count);

    for t in 0..time_step_count {
        let mut total = 0u32;
        let mut count_any = 0u32;
        let mut count_moderate = 0u32;
        let mut count_heavy = 0u32;

        for member in member_arrays {
            if let Some(Some(value)) = member.get(t) {
                total += 1;
                if *value > THRESHOLD_ANY {
                    count_any += 1;
                }
                if *value > THRESHOLD_MODERATE {
                    count_moderate += 1;
                }
                if *value > THRESHOLD_HEAVY {
                    count_heavy += 1;
                }
            }
        }

        if total == 0 {
            any.push(None);
            moderate.push(None);
            heavy.push(None);
        } else {
            let total_f = total as f64;
            any.push(Some((count_any as f64 / total_f) * 100.0));
            moderate.push(Some((count_moderate as f64 / total_f) * 100.0));
            heavy.push(Some((count_heavy as f64 / total_f) * 100.0));
        }
    }

    PrecipProbability { any, moderate, heavy }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_members_above_any_threshold() {
        // All 3 members have values > 0.1mm → any == 100%
        let members: Vec<Vec<Option<f64>>> = vec![
            vec![Some(1.0)],
            vec![Some(5.0)],
            vec![Some(0.5)],
        ];

        let prob = compute_precip_probability(&members, 1);

        assert_eq!(prob.any[0], Some(100.0));
    }

    #[test]
    fn all_members_zero() {
        // All 3 members have value 0.0 → any == 0%
        let members: Vec<Vec<Option<f64>>> = vec![
            vec![Some(0.0)],
            vec![Some(0.0)],
            vec![Some(0.0)],
        ];

        let prob = compute_precip_probability(&members, 1);

        assert_eq!(prob.any[0], Some(0.0));
        assert_eq!(prob.moderate[0], Some(0.0));
        assert_eq!(prob.heavy[0], Some(0.0));
    }

    #[test]
    fn all_nil_produces_none() {
        let members: Vec<Vec<Option<f64>>> = vec![
            vec![None],
            vec![None],
            vec![None],
        ];

        let prob = compute_precip_probability(&members, 1);

        assert_eq!(prob.any[0], None);
        assert_eq!(prob.moderate[0], None);
        assert_eq!(prob.heavy[0], None);
    }

    #[test]
    fn mixed_thresholds() {
        // 4 members: 0.0, 0.5, 3.0, 10.0
        // any (>0.1): 3 of 4 = 75%
        // moderate (>2.5): 2 of 4 = 50%
        // heavy (>7.5): 1 of 4 = 25%
        let members: Vec<Vec<Option<f64>>> = vec![
            vec![Some(0.0)],
            vec![Some(0.5)],
            vec![Some(3.0)],
            vec![Some(10.0)],
        ];

        let prob = compute_precip_probability(&members, 1);

        assert_eq!(prob.any[0], Some(75.0));
        assert_eq!(prob.moderate[0], Some(50.0));
        assert_eq!(prob.heavy[0], Some(25.0));
    }

    #[test]
    fn mixed_nil_and_values() {
        // 4 members: None, 0.0, 3.0, None
        // Non-nil count: 2
        // any (>0.1): 1 of 2 = 50%
        // moderate (>2.5): 1 of 2 = 50%
        // heavy (>7.5): 0 of 2 = 0%
        let members: Vec<Vec<Option<f64>>> = vec![
            vec![None],
            vec![Some(0.0)],
            vec![Some(3.0)],
            vec![None],
        ];

        let prob = compute_precip_probability(&members, 1);

        assert_eq!(prob.any[0], Some(50.0));
        assert_eq!(prob.moderate[0], Some(50.0));
        assert_eq!(prob.heavy[0], Some(0.0));
    }

    #[test]
    fn multiple_time_steps() {
        // 2 members, 3 time steps
        let members: Vec<Vec<Option<f64>>> = vec![
            vec![Some(0.0), Some(5.0), None],
            vec![Some(1.0), Some(10.0), None],
        ];

        let prob = compute_precip_probability(&members, 3);

        assert_eq!(prob.any.len(), 3);
        // t=0: 0.0 and 1.0 → any: 1/2=50%, moderate: 0%, heavy: 0%
        assert_eq!(prob.any[0], Some(50.0));
        assert_eq!(prob.moderate[0], Some(0.0));
        assert_eq!(prob.heavy[0], Some(0.0));
        // t=1: 5.0 and 10.0 → any: 100%, moderate: 100%, heavy: 1/2=50%
        assert_eq!(prob.any[1], Some(100.0));
        assert_eq!(prob.moderate[1], Some(100.0));
        assert_eq!(prob.heavy[1], Some(50.0));
        // t=2: all nil
        assert_eq!(prob.any[2], None);
        assert_eq!(prob.moderate[2], None);
        assert_eq!(prob.heavy[2], None);
    }

    #[test]
    fn empty_member_arrays() {
        let members: Vec<Vec<Option<f64>>> = vec![];
        let prob = compute_precip_probability(&members, 3);

        assert_eq!(prob.any.len(), 3);
        assert!(prob.any.iter().all(|v| v.is_none()));
        assert!(prob.moderate.iter().all(|v| v.is_none()));
        assert!(prob.heavy.iter().all(|v| v.is_none()));
    }

    #[test]
    fn values_at_exact_thresholds() {
        // Values exactly at thresholds should NOT count (thresholds are strictly greater than)
        // 0.1, 2.5, 7.5 → none exceed their respective thresholds
        let members: Vec<Vec<Option<f64>>> = vec![
            vec![Some(0.1)],
            vec![Some(2.5)],
            vec![Some(7.5)],
        ];

        let prob = compute_precip_probability(&members, 1);

        // 0.1 is not > 0.1, but 2.5 > 0.1 and 7.5 > 0.1 → any: 2/3
        assert!((prob.any[0].unwrap() - 200.0 / 3.0).abs() < 1e-9);
        // 2.5 is not > 2.5, but 7.5 > 2.5 → moderate: 1/3
        assert!((prob.moderate[0].unwrap() - 100.0 / 3.0).abs() < 1e-9);
        // 7.5 is not > 7.5 → heavy: 0/3
        assert_eq!(prob.heavy[0], Some(0.0));
    }

    #[test]
    fn single_member() {
        let members: Vec<Vec<Option<f64>>> = vec![vec![Some(5.0)]];

        let prob = compute_precip_probability(&members, 1);

        assert_eq!(prob.any[0], Some(100.0));
        assert_eq!(prob.moderate[0], Some(100.0));
        assert_eq!(prob.heavy[0], Some(0.0));
    }

    /// Feature: weather-backend-api, Property 2: Precipitation probability ordering and range
    ///
    /// **Validates: Requirements 2.5, 3.6**
    mod prop_probability {
        use super::*;
        use proptest::prelude::*;

        fn precip_member_arrays_strategy()
            -> impl Strategy<Value = (usize, Vec<Vec<Option<f64>>>)>
        {
            (1usize..=100).prop_flat_map(|time_steps| {
                let members = prop::collection::vec(
                    prop::collection::vec(
                        prop::option::of(0.0f64..100.0f64),
                        time_steps..=time_steps,
                    ),
                    1..=161,
                );
                (Just(time_steps), members)
            })
        }

        proptest! {
            #[test]
            fn prop_precip_probability_ordering_and_range(
                (time_steps, members) in precip_member_arrays_strategy()
            ) {
                let prob = compute_precip_probability(&members, time_steps);

                for t in 0..time_steps {
                    // Collect non-nil values at this time step
                    let has_values = members
                        .iter()
                        .any(|m| m[t].is_some());

                    if !has_values {
                        // All nil → all probabilities should be None
                        prop_assert!(prob.any[t].is_none());
                        prop_assert!(prob.moderate[t].is_none());
                        prop_assert!(prob.heavy[t].is_none());
                    } else {
                        let any_val = prob.any[t].unwrap();
                        let mod_val = prob.moderate[t].unwrap();
                        let heavy_val = prob.heavy[t].unwrap();

                        // Verify ordering: heavy ≤ moderate ≤ any
                        prop_assert!(
                            heavy_val <= mod_val,
                            "t={}: heavy ({}) > moderate ({})", t, heavy_val, mod_val
                        );
                        prop_assert!(
                            mod_val <= any_val,
                            "t={}: moderate ({}) > any ({})", t, mod_val, any_val
                        );

                        // Verify range: all values in [0.0, 100.0]
                        for (name, val) in [
                            ("any", any_val), ("moderate", mod_val), ("heavy", heavy_val),
                        ] {
                            prop_assert!(
                                val >= 0.0,
                                "t={}: {} ({}) < 0.0", t, name, val
                            );
                            prop_assert!(
                                val <= 100.0,
                                "t={}: {} ({}) > 100.0", t, name, val
                            );
                        }
                    }
                }
            }
        }
    }
}
