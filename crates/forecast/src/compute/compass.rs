/// 16-point compass directions, starting at North (0°) and proceeding clockwise
/// in 22.5° increments.
pub const COMPASS_DIRECTIONS: [&str; 16] = [
    "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE",
    "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW",
];

/// Converts a wind direction in degrees to a 16-point compass label.
///
/// The input is normalized to [0, 360) before mapping to the nearest
/// 22.5° sector. Negative values and values ≥ 360 are handled correctly.
pub fn degrees_to_compass(degrees: f64) -> &'static str {
    let normalized = ((degrees % 360.0) + 360.0) % 360.0;
    let index = ((normalized + 11.25) / 22.5) as usize % 16;
    COMPASS_DIRECTIONS[index]
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn cardinal_directions() {
        assert_eq!(degrees_to_compass(0.0), "N");
        assert_eq!(degrees_to_compass(90.0), "E");
        assert_eq!(degrees_to_compass(180.0), "S");
        assert_eq!(degrees_to_compass(270.0), "W");
    }

    #[test]
    fn near_360_wraps_to_north() {
        assert_eq!(degrees_to_compass(359.0), "N");
    }

    #[test]
    fn negative_degrees_normalize() {
        assert_eq!(degrees_to_compass(-10.0), "N");
    }

    #[test]
    fn sector_boundaries() {
        // Each sector spans 22.5°. North covers [348.75, 11.25).
        assert_eq!(degrees_to_compass(11.24), "N");
        assert_eq!(degrees_to_compass(11.25), "NNE");
        assert_eq!(degrees_to_compass(348.75), "N");
        assert_eq!(degrees_to_compass(348.74), "NNW");
    }

    #[test]
    fn all_sixteen_midpoints() {
        let expected = [
            (0.0, "N"),
            (22.5, "NNE"),
            (45.0, "NE"),
            (67.5, "ENE"),
            (90.0, "E"),
            (112.5, "ESE"),
            (135.0, "SE"),
            (157.5, "SSE"),
            (180.0, "S"),
            (202.5, "SSW"),
            (225.0, "SW"),
            (247.5, "WSW"),
            (270.0, "W"),
            (292.5, "WNW"),
            (315.0, "NW"),
            (337.5, "NNW"),
        ];
        for (deg, dir) in expected {
            assert_eq!(degrees_to_compass(deg), dir, "failed for {deg}°");
        }
    }

    #[test]
    fn large_positive_values() {
        assert_eq!(degrees_to_compass(720.0), "N");
        assert_eq!(degrees_to_compass(450.0), "E");
    }

    #[test]
    fn large_negative_values() {
        assert_eq!(degrees_to_compass(-90.0), "W");
        assert_eq!(degrees_to_compass(-360.0), "N");
    }

    /// Feature: weather-backend-api, Property 6: Compass direction conversion consistency
    ///
    /// **Validates: Requirements 5.1, 5.2**
    mod prop_compass {
        use super::*;

        proptest! {
            #[test]
            fn prop_compass_direction_consistency(d in -720.0f64..720.0f64) {
                let result = degrees_to_compass(d);

                // (a) Result is one of the 16 valid compass directions
                prop_assert!(
                    COMPASS_DIRECTIONS.contains(&result),
                    "degrees_to_compass({}) returned '{}', which is not a valid compass direction",
                    d, result
                );

                // (b) Normalization: d and d + 360*k produce the same result
                let result_plus_360 = degrees_to_compass(d + 360.0);
                prop_assert_eq!(result, result_plus_360);

                let result_minus_360 = degrees_to_compass(d - 360.0);
                prop_assert_eq!(result, result_minus_360);

                // (c) The midpoint angle of the returned sector is the closest sector midpoint
                //     to the normalized input
                let normalized = ((d % 360.0) + 360.0) % 360.0;
                let result_index = COMPASS_DIRECTIONS.iter().position(|&c| c == result).unwrap();
                let result_midpoint = result_index as f64 * 22.5;

                // Compute angular distance on the circle [0, 360)
                let angular_dist = |a: f64, b: f64| -> f64 {
                    let diff = (a - b).abs();
                    diff.min(360.0 - diff)
                };

                let dist_to_result = angular_dist(normalized, result_midpoint);

                // Check that no other sector midpoint is closer
                for (i, _) in COMPASS_DIRECTIONS.iter().enumerate() {
                    let midpoint = i as f64 * 22.5;
                    let dist = angular_dist(normalized, midpoint);
                    // Allow equality (at exact sector boundaries, either neighbor is valid)
                    prop_assert!(
                        dist_to_result <= dist + 1e-9,
                        "For d={} (normalized={}), sector '{}' (midpoint={}) \
                         has distance {}, but sector '{}' (midpoint={}) has distance {}",
                        d, normalized, result, result_midpoint,
                        dist_to_result, COMPASS_DIRECTIONS[i], midpoint, dist
                    );
                }
            }
        }
    }
}
