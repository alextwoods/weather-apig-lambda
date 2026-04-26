use chrono::{DateTime, Datelike, Timelike, Utc};

/// Computes the sun's altitude (elevation angle in degrees above the horizon)
/// for a given UTC datetime, latitude, and longitude using the NOAA solar
/// calculator algorithm.
///
/// Returns a value in the range [-90, 90] degrees.
pub fn sun_altitude(date: DateTime<Utc>, lat: f64, lon: f64) -> f64 {
    let jd = julian_day(date);
    let jc = julian_century(jd);

    // Geometric mean longitude of the sun (degrees)
    let geom_mean_lon = (280.46646 + jc * (36000.76983 + 0.0003032 * jc)) % 360.0;

    // Geometric mean anomaly of the sun (degrees)
    let geom_mean_anom = 357.52911 + jc * (35999.05029 - 0.0001537 * jc);
    let geom_mean_anom_rad = geom_mean_anom.to_radians();

    // Equation of center (degrees)
    let eoc = geom_mean_anom_rad.sin() * (1.914602 - jc * (0.004817 + 0.000014 * jc))
        + (2.0 * geom_mean_anom_rad).sin() * (0.019993 - 0.000101 * jc)
        + (3.0 * geom_mean_anom_rad).sin() * 0.000289;

    // Sun's true longitude (degrees)
    let sun_true_lon = geom_mean_lon + eoc;

    // Sun's apparent longitude (degrees)
    let omega = (125.04 - 1934.136 * jc).to_radians();
    let sun_apparent_lon = sun_true_lon - 0.00569 - 0.00478 * omega.sin();

    // Mean obliquity of the ecliptic (degrees)
    let mean_obliq = 23.0 + (26.0 + (21.448 - jc * (46.815 + jc * (0.00059 - jc * 0.001813))) / 60.0) / 60.0;

    // Corrected obliquity (degrees)
    let obliq_corr = mean_obliq + 0.00256 * omega.cos();
    let obliq_corr_rad = obliq_corr.to_radians();

    // Sun's declination (degrees)
    let sun_decl = (obliq_corr_rad.sin() * sun_apparent_lon.to_radians().sin()).asin().to_degrees();
    let sun_decl_rad = sun_decl.to_radians();

    // Eccentricity of Earth's orbit
    let ecc = 0.016708634 - jc * (0.000042037 + 0.0000001267 * jc);

    // Equation of time (minutes)
    let y = (obliq_corr_rad / 2.0).tan().powi(2);
    let geom_mean_lon_rad = geom_mean_lon.to_radians();
    let eq_of_time = 4.0
        * ((y * (2.0 * geom_mean_lon_rad).sin()
            - 2.0 * ecc * geom_mean_anom_rad.sin()
            + 4.0 * ecc * y * geom_mean_anom_rad.sin() * (2.0 * geom_mean_lon_rad).cos()
            - 0.5 * y * y * (4.0 * geom_mean_lon_rad).sin()
            - 1.25 * ecc * ecc * (2.0 * geom_mean_anom_rad).sin()))
        .to_degrees();

    // Time offset from UTC in minutes
    let hours = date.hour() as f64;
    let minutes = date.minute() as f64;
    let seconds = date.second() as f64;
    let time_in_minutes = hours * 60.0 + minutes + seconds / 60.0;

    // True solar time (minutes)
    let true_solar_time = (time_in_minutes + eq_of_time + 4.0 * lon) % 1440.0;

    // Hour angle (degrees)
    let hour_angle = if true_solar_time / 4.0 < 0.0 {
        true_solar_time / 4.0 + 180.0
    } else {
        true_solar_time / 4.0 - 180.0
    };
    let hour_angle_rad = hour_angle.to_radians();

    // Solar zenith angle (degrees)
    let lat_rad = lat.to_radians();
    let cos_zenith = lat_rad.sin() * sun_decl_rad.sin()
        + lat_rad.cos() * sun_decl_rad.cos() * hour_angle_rad.cos();
    let zenith = cos_zenith.clamp(-1.0, 1.0).acos().to_degrees();

    // Solar altitude = 90 - zenith
    let altitude = 90.0 - zenith;
    altitude.clamp(-90.0, 90.0)
}

/// Computes the moon's altitude (elevation angle in degrees above the horizon)
/// for a given UTC datetime, latitude, and longitude using a low-precision method.
///
/// Accuracy is approximately 1–2°, sufficient for visual chart overlays.
/// Returns a value in the range [-90, 90] degrees.
pub fn moon_altitude(date: DateTime<Utc>, lat: f64, lon: f64) -> f64 {
    let jd = julian_day(date);
    let t = julian_century(jd);

    // Fundamental arguments (degrees)
    // Mean elongation of the Moon
    let d = 297.8501921 + 445267.1114034 * t - 0.0018819 * t * t;
    // Sun's mean anomaly
    let m = 357.5291092 + 35999.0502909 * t - 0.0001536 * t * t;
    // Moon's mean anomaly
    let mp = 134.9633964 + 477198.8675055 * t + 0.0087414 * t * t;
    // Moon's argument of latitude
    let f = 93.2720950 + 483202.0175233 * t - 0.0036539 * t * t;

    let d_rad = d.to_radians();
    let m_rad = m.to_radians();
    let mp_rad = mp.to_radians();
    let f_rad = f.to_radians();

    // Approximate ecliptic longitude of the Moon (degrees)
    let lambda = 218.3164477 + 481267.88123421 * t
        + 6.289 * mp_rad.sin()
        - 1.274 * (2.0 * d_rad - mp_rad).sin()
        - 0.658 * (2.0 * d_rad).sin()
        + 0.214 * (2.0 * mp_rad).sin()
        - 0.186 * m_rad.sin()
        - 0.114 * (2.0 * f_rad).sin()
        + 0.059 * (2.0 * (d_rad - mp_rad)).sin()
        + 0.057 * (2.0 * d_rad - m_rad - mp_rad).sin();

    // Approximate ecliptic latitude of the Moon (degrees)
    let beta = 5.128 * f_rad.sin()
        + 0.281 * (mp_rad + f_rad).sin()
        - 0.278 * (mp_rad - f_rad).sin()
        - 0.173 * (2.0 * d_rad - f_rad).sin();

    let lambda_rad = lambda.to_radians();
    let beta_rad = beta.to_radians();

    // Mean obliquity of the ecliptic (degrees)
    let epsilon = 23.0 + (26.0 + (21.448 - t * (46.815 + t * (0.00059 - t * 0.001813))) / 60.0) / 60.0;
    let epsilon_rad = epsilon.to_radians();

    // Convert ecliptic to equatorial coordinates
    // Right ascension
    let ra = (lambda_rad.sin() * epsilon_rad.cos() - beta_rad.tan() * epsilon_rad.sin())
        .atan2(lambda_rad.cos());

    // Declination
    let decl = (beta_rad.sin() * epsilon_rad.cos()
        + beta_rad.cos() * epsilon_rad.sin() * lambda_rad.sin())
    .asin();

    // Greenwich Mean Sidereal Time (degrees)
    let gmst = 280.46061837 + 360.98564736629 * (jd - 2451545.0)
        + 0.000387933 * t * t
        - t * t * t / 38710000.0;

    // Local sidereal time (radians)
    let lst = (gmst + lon).to_radians();

    // Hour angle (radians)
    let ha = lst - ra;

    // Altitude
    let lat_rad = lat.to_radians();
    let sin_alt = lat_rad.sin() * decl.sin() + lat_rad.cos() * decl.cos() * ha.cos();
    let altitude = sin_alt.clamp(-1.0, 1.0).asin().to_degrees();

    altitude.clamp(-90.0, 90.0)
}

/// Computes the Julian Day Number for a given UTC datetime.
fn julian_day(date: DateTime<Utc>) -> f64 {
    let year = date.year() as f64;
    let month = date.month() as f64;
    let day = date.day() as f64;
    let hour = date.hour() as f64;
    let minute = date.minute() as f64;
    let second = date.second() as f64;

    let day_fraction = day + (hour + minute / 60.0 + second / 3600.0) / 24.0;

    let (y, m) = if month <= 2.0 {
        (year - 1.0, month + 12.0)
    } else {
        (year, month)
    };

    let a = (y / 100.0).floor();
    let b = 2.0 - a + (a / 4.0).floor();

    (365.25 * (y + 4716.0)).floor() + (30.6001 * (m + 1.0)).floor() + day_fraction + b - 1524.5
}

/// Computes the Julian Century from a Julian Day.
fn julian_century(jd: f64) -> f64 {
    (jd - 2451545.0) / 36525.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use proptest::prelude::*;

    #[test]
    fn sun_altitude_solar_noon_equator_near_equinox() {
        // March 20, 2024 ~12:07 UTC is close to solar noon at lon=0 near the equinox.
        // The sun should be very close to directly overhead (altitude ~90°).
        let date = Utc.with_ymd_and_hms(2024, 3, 20, 12, 7, 0).unwrap();
        let alt = sun_altitude(date, 0.0, 0.0);
        // Should be close to 90° (within a few degrees due to the equinox not being
        // exactly at noon and the sun's declination not being exactly 0)
        assert!(
            alt > 85.0,
            "Expected sun altitude near 90° at solar noon on equator near equinox, got {alt}°"
        );
    }

    #[test]
    fn sun_altitude_at_midnight_is_negative() {
        // Midnight UTC at lon=0 — the sun should be well below the horizon
        let date = Utc.with_ymd_and_hms(2024, 6, 21, 0, 0, 0).unwrap();
        let alt = sun_altitude(date, 0.0, 0.0);
        assert!(
            alt < 0.0,
            "Expected negative sun altitude at midnight, got {alt}°"
        );
    }

    #[test]
    fn sun_altitude_at_known_location() {
        // Seattle (47.6°N, -122.3°W) at solar noon on summer solstice
        // Solar noon in Seattle is roughly 13:10 UTC (PDT noon = 19:00 UTC... actually
        // solar noon at lon=-122.3 is about 12:00 + 122.3/15 hours ≈ 20:09 UTC)
        let date = Utc.with_ymd_and_hms(2024, 6, 21, 20, 9, 0).unwrap();
        let alt = sun_altitude(date, 47.6, -122.3);
        // At summer solstice, Seattle's max solar altitude ≈ 90 - 47.6 + 23.44 ≈ 65.8°
        assert!(
            (60.0..=70.0).contains(&alt),
            "Expected sun altitude ~66° at Seattle summer solstice noon, got {alt}°"
        );
    }

    #[test]
    fn moon_altitude_returns_valid_range() {
        let date = Utc.with_ymd_and_hms(2024, 6, 21, 12, 0, 0).unwrap();
        let alt = moon_altitude(date, 47.6, -122.3);
        assert!(
            (-90.0..=90.0).contains(&alt),
            "Moon altitude should be in [-90, 90], got {alt}°"
        );
    }

    #[test]
    fn moon_altitude_varies_with_time() {
        // The moon altitude should change over the course of a day
        let date1 = Utc.with_ymd_and_hms(2024, 6, 21, 0, 0, 0).unwrap();
        let date2 = Utc.with_ymd_and_hms(2024, 6, 21, 12, 0, 0).unwrap();
        let alt1 = moon_altitude(date1, 47.6, -122.3);
        let alt2 = moon_altitude(date2, 47.6, -122.3);
        assert!(
            (alt1 - alt2).abs() > 0.1,
            "Moon altitude should vary over 12 hours: {alt1}° vs {alt2}°"
        );
    }

    #[test]
    fn sun_altitude_at_poles() {
        // At the North Pole during summer solstice, the sun should be above the horizon
        let date = Utc.with_ymd_and_hms(2024, 6, 21, 12, 0, 0).unwrap();
        let alt = sun_altitude(date, 90.0, 0.0);
        assert!(
            alt > 0.0,
            "Expected positive sun altitude at North Pole during summer solstice, got {alt}°"
        );

        // At the South Pole during June, the sun should be below the horizon
        let alt_south = sun_altitude(date, -90.0, 0.0);
        assert!(
            alt_south < 0.0,
            "Expected negative sun altitude at South Pole during June, got {alt_south}°"
        );
    }

    #[test]
    fn julian_day_known_value() {
        // J2000.0 epoch: January 1, 2000 at 12:00 TT ≈ 12:00 UTC
        // JD = 2451545.0
        let date = Utc.with_ymd_and_hms(2000, 1, 1, 12, 0, 0).unwrap();
        let jd = julian_day(date);
        assert!(
            (jd - 2451545.0).abs() < 0.001,
            "Expected JD ≈ 2451545.0 for J2000.0 epoch, got {jd}"
        );
    }

    /// Feature: weather-backend-api, Property 12: Astronomical altitude range
    ///
    /// **Validates: Requirements 15.1, 15.2**
    mod prop_astronomy {
        use super::*;

        /// Generates a random DateTime<Utc> within a reasonable range (2000–2050).
        fn arb_datetime() -> impl Strategy<Value = DateTime<Utc>> {
            // Unix timestamps from 2000-01-01 to 2050-01-01
            (946684800i64..2524608000i64).prop_map(|ts| {
                Utc.timestamp_opt(ts, 0).unwrap()
            })
        }

        proptest! {
            #[test]
            fn prop_astronomy_altitude_range(
                dt in arb_datetime(),
                lat in -90.0f64..=90.0f64,
                lon in -180.0f64..=180.0f64,
            ) {
                let sun_alt = sun_altitude(dt, lat, lon);
                let moon_alt = moon_altitude(dt, lat, lon);

                prop_assert!(
                    (-90.0..=90.0).contains(&sun_alt),
                    "Sun altitude {} out of range [-90, 90] for dt={}, lat={}, lon={}",
                    sun_alt, dt, lat, lon
                );

                prop_assert!(
                    (-90.0..=90.0).contains(&moon_alt),
                    "Moon altitude {} out of range [-90, 90] for dt={}, lat={}, lon={}",
                    moon_alt, dt, lat, lon
                );
            }
        }
    }
}
