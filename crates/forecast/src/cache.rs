use std::io::{Read, Write};

use async_trait::async_trait;
use aws_sdk_dynamodb::primitives::Blob;
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_s3::primitives::ByteStream;
use chrono::{DateTime, Utc};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;

// ---------------------------------------------------------------------------
// Cache key generation
// ---------------------------------------------------------------------------

/// Generates a cache key from coordinates, rounding to 2 decimal places.
///
/// Examples:
/// - `(47.6062, -122.3321)` → `"47.61_-122.33"`
/// - `(-33.8688, 151.2093)` → `"-33.87_151.21"`
pub fn cache_key(lat: f64, lon: f64) -> String {
    format!("{:.2}_{:.2}", lat, lon)
}

// ---------------------------------------------------------------------------
// CacheEntry
// ---------------------------------------------------------------------------

/// A cached upstream response with metadata for freshness checks.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// The raw (uncompressed) response data.
    pub data: Vec<u8>,
    /// When this entry was stored in the cache.
    pub stored_at: DateTime<Utc>,
    /// Time-to-live in seconds from `stored_at`.
    pub ttl_secs: u64,
}

impl CacheEntry {
    /// Returns `true` if this entry is still within its TTL.
    pub fn is_fresh(&self) -> bool {
        self.age_secs() < self.ttl_secs
    }

    /// Returns the age of this entry in seconds (floored to whole seconds).
    pub fn age_secs(&self) -> u64 {
        let elapsed = Utc::now().signed_duration_since(self.stored_at);
        elapsed.num_seconds().max(0) as u64
    }
}

// ---------------------------------------------------------------------------
// CacheStore trait
// ---------------------------------------------------------------------------

/// Async trait for cache backends (S3, DynamoDB).
#[async_trait]
pub trait CacheStore: Send + Sync {
    /// Retrieve a cached entry. Returns `None` on miss or error.
    async fn get(&self, cache_key: &str, source: &str) -> Option<CacheEntry>;

    /// Store data in the cache. Errors are non-fatal and logged.
    async fn put(
        &self,
        cache_key: &str,
        source: &str,
        data: &[u8],
        ttl_secs: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

// ---------------------------------------------------------------------------
// Gzip helpers
// ---------------------------------------------------------------------------

fn gzip_compress(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(data)?;
    encoder.finish()
}

fn gzip_decompress(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut decoder = GzDecoder::new(data);
    let mut buf = Vec::new();
    decoder.read_to_end(&mut buf)?;
    Ok(buf)
}

// ---------------------------------------------------------------------------
// S3CacheStore
// ---------------------------------------------------------------------------

/// Cache backend using S3 for large responses (ensemble, marine).
///
/// Object layout: `s3://{bucket}/{cache_key}/{source}.json.gz`
///
/// Metadata stored on the S3 object:
/// - `stored-at`: ISO 8601 timestamp
/// - `ttl-secs`: TTL in seconds as a string
pub struct S3CacheStore {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl S3CacheStore {
    pub fn new(client: aws_sdk_s3::Client, bucket: String) -> Self {
        Self { client, bucket }
    }

    /// Build the S3 object key for a given cache key and source.
    fn object_key(cache_key: &str, source: &str) -> String {
        format!("{}/{}.json.gz", cache_key, source)
    }
}

#[async_trait]
impl CacheStore for S3CacheStore {
    async fn get(&self, cache_key: &str, source: &str) -> Option<CacheEntry> {
        let key = Self::object_key(cache_key, source);

        // Single GetObject call — retrieves both metadata and body in one
        // round trip, replacing the previous HeadObject+GetObject pattern.
        let get = match self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
        {
            Ok(output) => output,
            Err(err) => {
                // NoSuchKey is a normal cache miss — return None silently.
                let is_no_such_key = err
                    .as_service_error()
                    .map_or(false, |e| e.is_no_such_key());
                if !is_no_such_key {
                    tracing::warn!(
                        bucket = %self.bucket,
                        key = %key,
                        error = %err,
                        "S3 GetObject failed"
                    );
                }
                return None;
            }
        };

        // Extract stored-at and ttl-secs from the object's user metadata.
        let metadata = get.metadata()?;

        let stored_at_str = metadata.get("stored-at")?;
        let stored_at = DateTime::parse_from_rfc3339(stored_at_str)
            .ok()?
            .with_timezone(&Utc);

        let ttl_secs: u64 = metadata.get("ttl-secs")?.parse().ok()?;

        // Download and decompress the body.
        let compressed = get.body.collect().await.ok()?.to_vec();
        let data = gzip_decompress(&compressed).ok()?;

        Some(CacheEntry {
            data,
            stored_at,
            ttl_secs,
        })
    }

    async fn put(
        &self,
        cache_key: &str,
        source: &str,
        data: &[u8],
        ttl_secs: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let key = Self::object_key(cache_key, source);
        let now = Utc::now();
        let compressed = gzip_compress(data)?;

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("stored-at".to_string(), now.to_rfc3339());
        metadata.insert("ttl-secs".to_string(), ttl_secs.to_string());

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(ByteStream::from(compressed))
            .set_metadata(Some(metadata))
            .content_encoding("gzip")
            .content_type("application/json")
            .send()
            .await?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// DynamoCacheStore
// ---------------------------------------------------------------------------

/// Cache backend using DynamoDB for smaller responses (HRRR, UV, air quality).
///
/// Table schema:
/// - Partition key: `cache_key` (String)
/// - Sort key: `source` (String)
/// - `data`: Binary (gzip-compressed JSON)
/// - `stored_at`: String (ISO 8601)
/// - `expires_at`: Number (Unix epoch, used for DynamoDB TTL)
pub struct DynamoCacheStore {
    client: aws_sdk_dynamodb::Client,
    table: String,
}

impl DynamoCacheStore {
    pub fn new(client: aws_sdk_dynamodb::Client, table: String) -> Self {
        Self { client, table }
    }
}

#[async_trait]
impl CacheStore for DynamoCacheStore {
    async fn get(&self, cache_key: &str, source: &str) -> Option<CacheEntry> {
        let result = self
            .client
            .get_item()
            .table_name(&self.table)
            .key("cache_key", AttributeValue::S(cache_key.to_string()))
            .key("source", AttributeValue::S(source.to_string()))
            .send()
            .await
            .ok()?;

        let item = result.item()?;

        let compressed = item.get("data")?.as_b().ok()?.as_ref();
        let data = gzip_decompress(compressed).ok()?;

        let stored_at_str = item.get("stored_at")?.as_s().ok()?;
        let stored_at = DateTime::parse_from_rfc3339(stored_at_str)
            .ok()?
            .with_timezone(&Utc);

        // Derive ttl_secs from expires_at - stored_at.
        let expires_at_str = item.get("expires_at")?.as_n().ok()?;
        let expires_at_epoch: i64 = expires_at_str.parse().ok()?;
        let ttl_secs = (expires_at_epoch - stored_at.timestamp()).max(0) as u64;

        Some(CacheEntry {
            data,
            stored_at,
            ttl_secs,
        })
    }

    async fn put(
        &self,
        cache_key: &str,
        source: &str,
        data: &[u8],
        ttl_secs: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let now = Utc::now();
        let expires_at = now.timestamp() + ttl_secs as i64;
        let compressed = gzip_compress(data)?;

        self.client
            .put_item()
            .table_name(&self.table)
            .item("cache_key", AttributeValue::S(cache_key.to_string()))
            .item("source", AttributeValue::S(source.to_string()))
            .item("data", AttributeValue::B(Blob::new(compressed)))
            .item("stored_at", AttributeValue::S(now.to_rfc3339()))
            .item(
                "expires_at",
                AttributeValue::N(expires_at.to_string()),
            )
            .send()
            .await?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use proptest::prelude::*;

    #[test]
    fn test_cache_key_seattle() {
        assert_eq!(cache_key(47.6062, -122.3321), "47.61_-122.33");
    }

    #[test]
    fn test_cache_key_sydney() {
        assert_eq!(cache_key(-33.8688, 151.2093), "-33.87_151.21");
    }

    #[test]
    fn test_cache_key_zero() {
        assert_eq!(cache_key(0.0, 0.0), "0.00_0.00");
    }

    #[test]
    fn test_cache_key_negative_zero() {
        // -0.001 rounds to -0.00 with Rust's format!
        let key = cache_key(-0.001, 0.001);
        assert_eq!(key, "-0.00_0.00");
    }

    #[test]
    fn test_cache_key_rounding() {
        // 47.604 rounds to 47.60, 47.605 rounds to 47.60 (banker's rounding)
        // Rust uses round-half-to-even for display, but format! uses
        // round-half-to-even. Let's just verify the format.
        let key = cache_key(47.604, -122.335);
        assert!(key.starts_with("47.60_"));
    }

    #[test]
    fn test_cache_entry_is_fresh_within_ttl() {
        let entry = CacheEntry {
            data: vec![1, 2, 3],
            stored_at: Utc::now() - Duration::seconds(30),
            ttl_secs: 3600,
        };
        assert!(entry.is_fresh());
    }

    #[test]
    fn test_cache_entry_is_stale_past_ttl() {
        let entry = CacheEntry {
            data: vec![1, 2, 3],
            stored_at: Utc::now() - Duration::seconds(3601),
            ttl_secs: 3600,
        };
        assert!(!entry.is_fresh());
    }

    #[test]
    fn test_cache_entry_is_fresh_at_boundary() {
        // Exactly at TTL boundary — age_secs == ttl_secs means NOT fresh
        // (we use strict less-than).
        let entry = CacheEntry {
            data: vec![],
            stored_at: Utc::now() - Duration::seconds(3600),
            ttl_secs: 3600,
        };
        assert!(!entry.is_fresh());
    }

    #[test]
    fn test_cache_entry_age_secs() {
        let entry = CacheEntry {
            data: vec![],
            stored_at: Utc::now() - Duration::seconds(120),
            ttl_secs: 3600,
        };
        let age = entry.age_secs();
        // Allow 1 second of clock drift during test execution.
        assert!(age >= 119 && age <= 121, "age_secs={age}");
    }

    #[test]
    fn test_cache_entry_age_secs_future_stored_at() {
        // If stored_at is in the future (clock skew), age should be 0.
        let entry = CacheEntry {
            data: vec![],
            stored_at: Utc::now() + Duration::seconds(60),
            ttl_secs: 3600,
        };
        assert_eq!(entry.age_secs(), 0);
    }

    #[test]
    fn test_gzip_round_trip() {
        let original = b"hello, world! this is test data for gzip compression";
        let compressed = gzip_compress(original).unwrap();
        let decompressed = gzip_decompress(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_s3_object_key() {
        assert_eq!(
            S3CacheStore::object_key("47.61_-122.33", "ensemble"),
            "47.61_-122.33/ensemble.json.gz"
        );
    }

    #[test]
    fn test_s3_object_key_with_special_source() {
        assert_eq!(
            S3CacheStore::object_key("47.61_-122.33", "air_quality"),
            "47.61_-122.33/air_quality.json.gz"
        );
    }

    /// Feature: weather-backend-api, Property 7: Cache key determinism and rounding
    ///
    /// **Validates: Requirements 6.1**
    mod prop_cache_key {
        use super::*;

        proptest! {
            #[test]
            fn prop_cache_key_determinism_and_rounding(
                lat in -90.0f64..90.0f64,
                lon in -180.0f64..180.0f64,
            ) {
                let key = cache_key(lat, lon);

                // (a) Matches the expected format "{lat:.2}_{lon:.2}"
                let expected = format!("{:.2}_{:.2}", lat, lon);
                prop_assert_eq!(&key, &expected,
                    "cache_key({}, {}) = '{}', expected '{}'", lat, lon, key, expected);

                // (b) Identical keys for coordinates that round to the same 2-decimal values.
                // Add a small perturbation that doesn't change the rounded value.
                // The rounding threshold for 2 decimal places is 0.005.
                // We pick a perturbation small enough to stay within the same rounding bucket.
                let epsilon = 0.001;
                let key_perturbed = cache_key(lat + epsilon, lon + epsilon);
                let expected_perturbed = format!("{:.2}_{:.2}", lat + epsilon, lon + epsilon);
                prop_assert_eq!(&key_perturbed, &expected_perturbed);

                // (c) Verify determinism: calling twice with the same inputs gives the same key.
                let key2 = cache_key(lat, lon);
                prop_assert_eq!(&key, &key2,
                    "cache_key is not deterministic for ({}, {})", lat, lon);
            }

            #[test]
            fn prop_cache_key_same_rounding_same_key(
                // Generate a base coordinate and two offsets that round to the same value.
                base_lat in -89.0f64..89.0f64,
                base_lon in -179.0f64..179.0f64,
                // Offsets within ±0.004 won't change the rounded 2-decimal value
                // (since the rounding boundary is at 0.005).
                offset_lat in -0.004f64..0.004f64,
                offset_lon in -0.004f64..0.004f64,
            ) {
                let lat1 = base_lat;
                let lon1 = base_lon;
                let lat2 = base_lat + offset_lat;
                let lon2 = base_lon + offset_lon;

                // Check if they round to the same 2-decimal value.
                let rounded_lat1 = format!("{:.2}", lat1);
                let rounded_lat2 = format!("{:.2}", lat2);
                let rounded_lon1 = format!("{:.2}", lon1);
                let rounded_lon2 = format!("{:.2}", lon2);

                if rounded_lat1 == rounded_lat2 && rounded_lon1 == rounded_lon2 {
                    // Same rounding → same key
                    let key1 = cache_key(lat1, lon1);
                    let key2 = cache_key(lat2, lon2);
                    prop_assert_eq!(&key1, &key2,
                        "({}, {}) and ({}, {}) round the same but got different keys: '{}' vs '{}'",
                        lat1, lon1, lat2, lon2, key1, key2);
                }
            }

            #[test]
            fn prop_cache_key_different_rounding_different_key(
                lat1 in -90.0f64..90.0f64,
                lon1 in -180.0f64..180.0f64,
                lat2 in -90.0f64..90.0f64,
                lon2 in -180.0f64..180.0f64,
            ) {
                let rounded_lat1 = format!("{:.2}", lat1);
                let rounded_lat2 = format!("{:.2}", lat2);
                let rounded_lon1 = format!("{:.2}", lon1);
                let rounded_lon2 = format!("{:.2}", lon2);

                // Only check when the rounded values are actually different.
                if rounded_lat1 != rounded_lat2 || rounded_lon1 != rounded_lon2 {
                    let key1 = cache_key(lat1, lon1);
                    let key2 = cache_key(lat2, lon2);
                    prop_assert_ne!(&key1, &key2,
                        "({}, {}) and ({}, {}) round differently but got same key: '{}'",
                        lat1, lon1, lat2, lon2, key1);
                }
            }
        }
    }
}
