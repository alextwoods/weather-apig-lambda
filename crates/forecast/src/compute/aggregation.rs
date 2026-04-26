use std::collections::HashMap;

use serde::Serialize;

use super::compass::degrees_to_compass;

/// A calendar-day summary derived from hourly forecast data.
///
/// Each `DailySection` covers a contiguous range of hourly time steps
/// (`start_index..=end_index`) that share the same UTC calendar date.
#[derive(Debug, Clone, Serialize)]
pub struct DailySection {
    /// UTC calendar date, e.g. "2026-04-24"
    pub date: String,
    /// First index (inclusive) into the hourly arrays for this day
    pub start_index: usize,
    /// Last index (inclusive) into the hourly arrays for this day
    pub end_index: usize,
    /// Maximum of the median temperature values in this day's hours
    pub high_temp: Option<f64>,
    /// Minimum of the median temperature values in this day's hours
    pub low_temp: Option<f64>,
    /// Sum of the median precipitation values in this day's hours
    pub total_precip: Option<f64>,
    /// Maximum of the median wind speed values in this day's hours
    pub max_wind: Option<f64>,
    /// Mode of the 16-point compass directions from the median wind direction values
    pub dominant_wind_direction: Option<String>,
}

/// Groups hourly time steps by UTC calendar day and computes daily aggregates.
///
/// `times` is a slice of ISO 8601 time strings (e.g. "2026-04-24T00:00").
/// The date portion (first 10 characters) is used for grouping.
///
/// For each day:
/// - `high_temp` / `low_temp`: max / min of non-nil median temperature values
/// - `total_precip`: sum of non-nil median precipitation values (None if all nil)
/// - `max_wind`: max of non-nil median wind speed values
/// - `dominant_wind_direction`: mode of compass directions converted from non-nil
///   median wind direction degrees via [`degrees_to_compass`]
pub fn compute_daily_sections(
    times: &[String],
    median_temp: &[Option<f64>],
    median_precip: &[Option<f64>],
    median_wind_speed: &[Option<f64>],
    median_wind_direction: &[Option<f64>],
) -> Vec<DailySection> {
    if times.is_empty() {
        return Vec::new();
    }

    // Identify day boundaries by grouping consecutive time steps with the same date.
    let mut sections: Vec<(String, usize, usize)> = Vec::new();
    let mut current_date = extract_date(&times[0]);
    let mut start = 0;

    for (i, time) in times.iter().enumerate().skip(1) {
        let date = extract_date(time);
        if date != current_date {
            sections.push((current_date, start, i - 1));
            current_date = date;
            start = i;
        }
    }
    // Push the final group
    sections.push((current_date, start, times.len() - 1));

    sections
        .into_iter()
        .map(|(date, start_index, end_index)| {
            let high_temp = max_in_range(median_temp, start_index, end_index);
            let low_temp = min_in_range(median_temp, start_index, end_index);
            let total_precip = sum_in_range(median_precip, start_index, end_index);
            let max_wind = max_in_range(median_wind_speed, start_index, end_index);
            let dominant_wind_direction =
                mode_compass_in_range(median_wind_direction, start_index, end_index);

            DailySection {
                date,
                start_index,
                end_index,
                high_temp,
                low_temp,
                total_precip,
                max_wind,
                dominant_wind_direction,
            }
        })
        .collect()
}

/// Extracts the date portion from an ISO 8601 time string.
///
/// Expects strings like "2026-04-24T00:00" and returns "2026-04-24".
/// Falls back to the full string (or first 10 chars) if the format is unexpected.
fn extract_date(time: &str) -> String {
    if let Some(t_pos) = time.find('T') {
        time[..t_pos].to_string()
    } else if time.len() >= 10 {
        time[..10].to_string()
    } else {
        time.to_string()
    }
}

/// Returns the maximum non-nil value in `data[start..=end]`.
fn max_in_range(data: &[Option<f64>], start: usize, end: usize) -> Option<f64> {
    data[start..=end]
        .iter()
        .filter_map(|v| *v)
        .fold(None, |acc: Option<f64>, val| {
            Some(acc.map_or(val, |a| a.max(val)))
        })
}

/// Returns the minimum non-nil value in `data[start..=end]`.
fn min_in_range(data: &[Option<f64>], start: usize, end: usize) -> Option<f64> {
    data[start..=end]
        .iter()
        .filter_map(|v| *v)
        .fold(None, |acc: Option<f64>, val| {
            Some(acc.map_or(val, |a| a.min(val)))
        })
}

/// Returns the sum of non-nil values in `data[start..=end]`, or None if all nil.
fn sum_in_range(data: &[Option<f64>], start: usize, end: usize) -> Option<f64> {
    let mut sum = 0.0;
    let mut any = false;
    for val in &data[start..=end] {
        if let Some(v) = val {
            sum += v;
            any = true;
        }
    }
    if any { Some(sum) } else { None }
}

/// Returns the mode compass direction from non-nil degree values in `data[start..=end]`.
///
/// Each degree value is converted to a 16-point compass direction, then the most
/// frequent direction is returned. Ties are broken arbitrarily (first encountered).
fn mode_compass_in_range(
    data: &[Option<f64>],
    start: usize,
    end: usize,
) -> Option<String> {
    let mut counts: HashMap<&str, usize> = HashMap::new();

    for val in &data[start..=end] {
        if let Some(deg) = val {
            let dir = degrees_to_compass(*deg);
            *counts.entry(dir).or_insert(0) += 1;
        }
    }

    counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(dir, _)| dir.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: generates ISO 8601 hourly time strings starting from a given date and hour.
    fn make_times(date: &str, start_hour: u32, count: u32) -> Vec<String> {
        (0..count)
            .map(|i| {
                let hour = start_hour + i;
                let day_offset = hour / 24;
                let h = hour % 24;
                // Simple date arithmetic for tests (assumes months don't roll over)
                let day: u32 = date[8..10].parse().unwrap();
                let new_day = day + day_offset;
                format!(
                    "{}{:02}T{:02}:00",
                    &date[..8],
                    new_day,
                    h
                )
            })
            .collect()
    }

    #[test]
    fn two_day_48_hour_sequence() {
        // 48 hours spanning 2026-04-24 (24h) and 2026-04-25 (24h)
        let times = make_times("2026-04-24", 0, 48);
        assert_eq!(times[0], "2026-04-24T00:00");
        assert_eq!(times[23], "2026-04-24T23:00");
        assert_eq!(times[24], "2026-04-25T00:00");
        assert_eq!(times[47], "2026-04-25T23:00");

        // Temperature: day 1 ranges 5..28, day 2 ranges 3..26
        let median_temp: Vec<Option<f64>> = (0..48)
            .map(|i| {
                if i < 24 {
                    Some(5.0 + (i as f64))
                } else {
                    Some(3.0 + ((i - 24) as f64))
                }
            })
            .collect();

        // Precipitation: 0.5mm each hour
        let median_precip: Vec<Option<f64>> = vec![Some(0.5); 48];

        // Wind speed: constant 15 km/h
        let median_wind_speed: Vec<Option<f64>> = vec![Some(15.0); 48];

        // Wind direction: day 1 all 180° (S), day 2 all 270° (W)
        let median_wind_direction: Vec<Option<f64>> = (0..48)
            .map(|i| if i < 24 { Some(180.0) } else { Some(270.0) })
            .collect();

        let sections = compute_daily_sections(
            &times,
            &median_temp,
            &median_precip,
            &median_wind_speed,
            &median_wind_direction,
        );

        assert_eq!(sections.len(), 2);

        // Day 1: 2026-04-24
        let d1 = &sections[0];
        assert_eq!(d1.date, "2026-04-24");
        assert_eq!(d1.start_index, 0);
        assert_eq!(d1.end_index, 23);
        assert_eq!(d1.high_temp, Some(28.0)); // 5 + 23
        assert_eq!(d1.low_temp, Some(5.0));
        assert!((d1.total_precip.unwrap() - 12.0).abs() < 1e-9); // 24 * 0.5
        assert_eq!(d1.max_wind, Some(15.0));
        assert_eq!(d1.dominant_wind_direction.as_deref(), Some("S"));

        // Day 2: 2026-04-25
        let d2 = &sections[1];
        assert_eq!(d2.date, "2026-04-25");
        assert_eq!(d2.start_index, 24);
        assert_eq!(d2.end_index, 47);
        assert_eq!(d2.high_temp, Some(26.0)); // 3 + 23
        assert_eq!(d2.low_temp, Some(3.0));
        assert!((d2.total_precip.unwrap() - 12.0).abs() < 1e-9);
        assert_eq!(d2.max_wind, Some(15.0));
        assert_eq!(d2.dominant_wind_direction.as_deref(), Some("W"));
    }

    #[test]
    fn all_nil_values() {
        let times = make_times("2026-04-24", 0, 24);
        let nils: Vec<Option<f64>> = vec![None; 24];

        let sections = compute_daily_sections(&times, &nils, &nils, &nils, &nils);

        assert_eq!(sections.len(), 1);
        let d = &sections[0];
        assert_eq!(d.high_temp, None);
        assert_eq!(d.low_temp, None);
        assert_eq!(d.total_precip, None);
        assert_eq!(d.max_wind, None);
        assert_eq!(d.dominant_wind_direction, None);
    }

    #[test]
    fn partial_nil_values() {
        let times = make_times("2026-04-24", 0, 4);
        let temp = vec![Some(10.0), None, Some(20.0), None];
        let precip = vec![None, Some(1.0), None, Some(2.0)];
        let wind_speed = vec![Some(5.0), Some(10.0), None, None];
        let wind_dir = vec![Some(90.0), None, Some(90.0), None];

        let sections = compute_daily_sections(&times, &temp, &precip, &wind_speed, &wind_dir);

        assert_eq!(sections.len(), 1);
        let d = &sections[0];
        assert_eq!(d.high_temp, Some(20.0));
        assert_eq!(d.low_temp, Some(10.0));
        assert!((d.total_precip.unwrap() - 3.0).abs() < 1e-9);
        assert_eq!(d.max_wind, Some(10.0));
        assert_eq!(d.dominant_wind_direction.as_deref(), Some("E"));
    }

    #[test]
    fn empty_input() {
        let sections = compute_daily_sections(&[], &[], &[], &[], &[]);
        assert!(sections.is_empty());
    }

    #[test]
    fn single_time_step() {
        let times = vec!["2026-04-24T12:00".to_string()];
        let temp = vec![Some(15.0)];
        let precip = vec![Some(0.3)];
        let wind_speed = vec![Some(8.0)];
        let wind_dir = vec![Some(45.0)];

        let sections = compute_daily_sections(&times, &temp, &precip, &wind_speed, &wind_dir);

        assert_eq!(sections.len(), 1);
        let d = &sections[0];
        assert_eq!(d.date, "2026-04-24");
        assert_eq!(d.start_index, 0);
        assert_eq!(d.end_index, 0);
        assert_eq!(d.high_temp, Some(15.0));
        assert_eq!(d.low_temp, Some(15.0));
        assert_eq!(d.total_precip, Some(0.3));
        assert_eq!(d.max_wind, Some(8.0));
        assert_eq!(d.dominant_wind_direction.as_deref(), Some("NE"));
    }

    #[test]
    fn dominant_wind_direction_mode() {
        // 6 hours: 3 × S (180°), 2 × N (0°), 1 × E (90°) → mode is S
        let times = make_times("2026-04-24", 0, 6);
        let temp = vec![Some(10.0); 6];
        let precip = vec![Some(0.0); 6];
        let wind_speed = vec![Some(10.0); 6];
        let wind_dir = vec![
            Some(180.0), Some(180.0), Some(180.0),
            Some(0.0), Some(0.0),
            Some(90.0),
        ];

        let sections = compute_daily_sections(&times, &temp, &precip, &wind_speed, &wind_dir);

        assert_eq!(sections[0].dominant_wind_direction.as_deref(), Some("S"));
    }

    #[test]
    fn three_days_partial() {
        // 3 days: 6h + 24h + 18h = 48h total
        let times = make_times("2026-04-24", 18, 48);
        // Day 1 (24th): hours 18-23 → 6 steps
        // Day 2 (25th): hours 0-23 → 24 steps
        // Day 3 (26th): hours 0-17 → 18 steps

        let temp: Vec<Option<f64>> = (0..48).map(|i| Some(i as f64)).collect();
        let precip = vec![Some(1.0); 48];
        let wind_speed = vec![Some(20.0); 48];
        let wind_dir = vec![Some(0.0); 48]; // all N

        let sections = compute_daily_sections(&times, &temp, &precip, &wind_speed, &wind_dir);

        assert_eq!(sections.len(), 3);

        assert_eq!(sections[0].date, "2026-04-24");
        assert_eq!(sections[0].start_index, 0);
        assert_eq!(sections[0].end_index, 5);

        assert_eq!(sections[1].date, "2026-04-25");
        assert_eq!(sections[1].start_index, 6);
        assert_eq!(sections[1].end_index, 29);

        assert_eq!(sections[2].date, "2026-04-26");
        assert_eq!(sections[2].start_index, 30);
        assert_eq!(sections[2].end_index, 47);

        // Verify contiguous partition
        assert_eq!(sections[0].end_index + 1, sections[1].start_index);
        assert_eq!(sections[1].end_index + 1, sections[2].start_index);
    }

    #[test]
    fn serialization_produces_expected_json() {
        let section = DailySection {
            date: "2026-04-24".to_string(),
            start_index: 0,
            end_index: 23,
            high_temp: Some(18.5),
            low_temp: Some(9.2),
            total_precip: Some(2.1),
            max_wind: Some(25.3),
            dominant_wind_direction: Some("SSW".to_string()),
        };

        let json = serde_json::to_value(&section).unwrap();
        assert_eq!(json["date"], "2026-04-24");
        assert_eq!(json["start_index"], 0);
        assert_eq!(json["end_index"], 23);
        assert_eq!(json["high_temp"], 18.5);
        assert_eq!(json["low_temp"], 9.2);
        assert_eq!(json["total_precip"], 2.1);
        assert_eq!(json["max_wind"], 25.3);
        assert_eq!(json["dominant_wind_direction"], "SSW");
    }
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use crate::compute::compass::COMPASS_DIRECTIONS;
    use proptest::prelude::*;

    /// Feature: weather-backend-api, Property 3: Daily aggregation invariants
    ///
    /// **Validates: Requirements 2.6, 4.1, 4.2, 4.3, 4.4, 4.5, 4.6, 4.7**
    ///
    /// Generates 24–840 hourly time steps with random UTC timestamps and median
    /// arrays, then verifies:
    /// - high_temp ≥ low_temp for each day
    /// - total_precip ≥ 0 for each day
    /// - start/end indices form a contiguous, non-overlapping partition covering
    ///   all time steps
    /// - dominant_wind_direction is one of 16 valid compass directions
    /// - each day's indices contain only time steps from the same UTC calendar day

    /// Strategy: generate a realistic sequence of hourly ISO 8601 timestamps.
    ///
    /// Picks a random start date (2020-01-01 through 2030-12-28) and start hour,
    /// then generates `count` consecutive hourly timestamps.
    fn hourly_times_strategy(count: usize) -> impl Strategy<Value = Vec<String>> {
        // year 2020–2030, month 1–12, day 1–28 (safe for all months), hour 0–23
        (2020u32..=2030, 1u32..=12, 1u32..=28, 0u32..=23).prop_map(
            move |(year, month, day, start_hour)| {
                // Use chrono for correct date arithmetic
                let start = chrono::NaiveDate::from_ymd_opt(year as i32, month, day)
                    .unwrap()
                    .and_hms_opt(start_hour, 0, 0)
                    .unwrap();

                (0..count)
                    .map(|i| {
                        let dt = start + chrono::Duration::hours(i as i64);
                        dt.format("%Y-%m-%dT%H:%M").to_string()
                    })
                    .collect()
            },
        )
    }

    /// Strategy: generate median arrays of `Option<f64>` with the given length.
    fn optional_f64_vec(len: usize, min: f64, max: f64) -> impl Strategy<Value = Vec<Option<f64>>> {
        prop::collection::vec(prop::option::of(min..max), len..=len)
    }

    /// Strategy: generate non-negative precipitation values.
    fn precip_vec(len: usize) -> impl Strategy<Value = Vec<Option<f64>>> {
        prop::collection::vec(prop::option::of(0.0f64..100.0), len..=len)
    }

    /// Strategy: generate wind direction degrees in [0, 360).
    fn wind_dir_vec(len: usize) -> impl Strategy<Value = Vec<Option<f64>>> {
        prop::collection::vec(prop::option::of(0.0f64..360.0), len..=len)
    }

    // Combined strategy for generating all aggregation inputs
    fn aggregation_input_strategy()
        -> impl Strategy<Value = (Vec<String>, Vec<Option<f64>>, Vec<Option<f64>>, Vec<Option<f64>>, Vec<Option<f64>>)>
    {
        (24usize..=840).prop_flat_map(|count| {
            (
                hourly_times_strategy(count),
                optional_f64_vec(count, -50.0, 50.0),
                precip_vec(count),
                optional_f64_vec(count, 0.0, 150.0),
                wind_dir_vec(count),
            )
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        /// Feature: weather-backend-api, Property 3: Daily aggregation invariants
        ///
        /// **Validates: Requirements 2.6, 4.1, 4.2, 4.3, 4.4, 4.5, 4.6, 4.7**
        #[test]
        fn prop_daily_aggregation_invariants_full(
            (times, median_temp, median_precip, median_wind_speed, median_wind_direction)
                in aggregation_input_strategy()
        ) {
            let sections = compute_daily_sections(
                &times,
                &median_temp,
                &median_precip,
                &median_wind_speed,
                &median_wind_direction,
            );

            let n = times.len();
            prop_assert!(!sections.is_empty(), "should produce at least one section for non-empty input");

            // (a) high_temp ≥ low_temp for each day
            for section in &sections {
                if let (Some(high), Some(low)) = (section.high_temp, section.low_temp) {
                    prop_assert!(
                        high >= low,
                        "day {}: high_temp ({}) < low_temp ({})",
                        section.date, high, low
                    );
                }
            }

            // (b) total_precip ≥ 0 for each day
            for section in &sections {
                if let Some(precip) = section.total_precip {
                    prop_assert!(
                        precip >= 0.0,
                        "day {}: total_precip ({}) < 0",
                        section.date, precip
                    );
                }
            }

            // (c) start/end indices form a contiguous, non-overlapping partition
            //     covering all time steps
            prop_assert_eq!(
                sections[0].start_index, 0,
                "first section should start at index 0"
            );
            prop_assert_eq!(
                sections.last().unwrap().end_index, n - 1,
                "last section should end at index {}",
                n - 1
            );

            for i in 1..sections.len() {
                prop_assert_eq!(
                    sections[i].start_index,
                    sections[i - 1].end_index + 1,
                    "sections {} and {} are not contiguous: end={}, start={}",
                    i - 1, i, sections[i - 1].end_index, sections[i].start_index
                );
            }

            for section in &sections {
                prop_assert!(
                    section.start_index <= section.end_index,
                    "day {}: start_index ({}) > end_index ({})",
                    section.date, section.start_index, section.end_index
                );
            }

            // (d) dominant_wind_direction is one of 16 valid compass directions
            for section in &sections {
                if let Some(ref dir) = section.dominant_wind_direction {
                    prop_assert!(
                        COMPASS_DIRECTIONS.contains(&dir.as_str()),
                        "day {}: '{}' is not a valid compass direction",
                        section.date, dir
                    );
                }
            }

            // (e) each day's indices contain only time steps from the same UTC calendar day
            for section in &sections {
                for idx in section.start_index..=section.end_index {
                    let date = extract_date(&times[idx]);
                    prop_assert_eq!(
                        &date, &section.date,
                        "index {} has date '{}' but section date is '{}'",
                        idx, &date, &section.date
                    );
                }
            }
        }
    }
}
