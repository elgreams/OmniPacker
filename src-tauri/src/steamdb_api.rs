use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Mutex;

use crate::debug_console::debug_eprintln;

/// Cache for SteamDB build dates to avoid repeated API calls
static BUILD_DATE_CACHE: Mutex<Option<HashMap<String, DateTime<Utc>>>> = Mutex::new(None);

/// Fetches the build release date from SteamDB for a given app and build ID
///
/// This queries SteamDB's patchnotes RSS feed and parses it to find the
/// release date for the specified build.
///
/// # Arguments
/// * `app_id` - Steam app ID
/// * `build_id` - Optional build ID to match (if None, returns most recent build date)
///
/// # Returns
/// * `Ok(DateTime<Utc>)` - The build release date
/// * `Err(String)` - Error message if fetch/parse failed
pub fn fetch_build_date(app_id: &str, build_id: Option<&str>) -> Result<DateTime<Utc>, String> {
    // Check cache first
    let cache_key = format!("{}:{}", app_id, build_id.unwrap_or("latest"));
    if let Ok(guard) = BUILD_DATE_CACHE.lock() {
        if let Some(cache) = guard.as_ref() {
            if let Some(cached_date) = cache.get(&cache_key) {
                debug_eprintln!("[STEAMDB] Cache hit for {}", cache_key);
                return Ok(*cached_date);
            }
        }
    }

    let url = format!("https://steamdb.info/api/PatchnotesRSS/?appid={}", app_id);
    debug_eprintln!("[STEAMDB] Fetching build date from: {}", url);

    // Use reqwest blocking client for HTTP request
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;
    let response = client
        .get(&url)
        .header("User-Agent", "OmniPacker/1.0")
        .send()
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let body = response
        .text()
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    // Parse the RSS XML
    let build_date = parse_patchnotes_rss(&body, build_id)?;

    // Cache the result
    if let Ok(mut guard) = BUILD_DATE_CACHE.lock() {
        let cache = guard.get_or_insert_with(HashMap::new);
        cache.insert(cache_key, build_date);
    }

    Ok(build_date)
}

/// Parses SteamDB patchnotes RSS feed to extract build date
///
/// Real feed format (verified against the live endpoint). The build ID is NOT
/// in the title; it lives in the `<guid>` (`build#<id>`) and at the end of the
/// description (`(SteamDB Build <id>)`):
/// ```xml
/// <item>
///   <guid isPermaLink="false">build#17459173</guid>
///   <title>Balatro update for 24 February 2025</title>
///   <description>Welcoming Friends of Jimbo 4! (SteamDB Build 17459173)</description>
///   <pubDate>Mon, 24 Feb 2025 18:21:58 +0000</pubDate>
/// </item>
/// ```
///
/// With a target build ID, only an item whose build ID matches is accepted; if
/// the build is not in the feed (it only lists recent builds) this returns
/// `Err` so the caller falls back to timestamps from DepotDownloader instead of
/// mislabeling the build with some other build's date.
fn parse_patchnotes_rss(xml: &str, target_build_id: Option<&str>) -> Result<DateTime<Utc>, String> {
    // Simple XML parsing using regex - avoids adding heavy XML dependencies
    // This is acceptable because the RSS format is well-defined and stable

    let item_regex = regex::Regex::new(r"<item>([\s\S]*?)</item>")
        .map_err(|e| format!("Regex error: {}", e))?;

    let pubdate_regex = regex::Regex::new(r"<pubDate>([^<]+)</pubDate>")
        .map_err(|e| format!("Regex error: {}", e))?;

    // Primary: <guid ...>build#123</guid>. Fallback: "Build 123" anywhere in the
    // item (description, or title in older/alternate feed shapes).
    let guid_build_regex = regex::Regex::new(r"<guid[^>]*>\s*build#(\d+)\s*</guid>")
        .map_err(|e| format!("Regex error: {}", e))?;
    let build_id_regex = regex::Regex::new(r"Build\s+(\d+)")
        .map_err(|e| format!("Regex error: {}", e))?;

    for item_cap in item_regex.captures_iter(xml) {
        let item_content = &item_cap[1];

        let Some(pub_date_str) = pubdate_regex
            .captures(item_content)
            .map(|c| c[1].to_string())
        else {
            continue;
        };

        let item_build_id = guid_build_regex
            .captures(item_content)
            .or_else(|| build_id_regex.captures(item_content))
            .map(|c| c[1].to_string());

        if let Some(target) = target_build_id {
            if item_build_id.as_deref() != Some(target) {
                continue; // Not the build we're looking for
            }
        }

        // Parse the pubDate (RFC 2822 format)
        // Example: "Mon, 24 Feb 2025 18:21:58 +0000"
        let parsed_date = parse_rfc2822_date(&pub_date_str)?;

        debug_eprintln!(
            "[STEAMDB] Found build {} with date: {}",
            item_build_id.as_deref().unwrap_or("unknown"),
            parsed_date
        );

        return Ok(parsed_date);
    }

    if let Some(target) = target_build_id {
        return Err(format!("Build {} not found in SteamDB RSS feed", target));
    }

    Err("No builds found in SteamDB RSS feed".to_string())
}

/// Fetches every (build_id, release date) the SteamDB patch-notes feed lists
/// for an app, newest first. The feed only carries recent updates (~10), so
/// older builds won't appear.
pub fn fetch_build_history(app_id: &str) -> Result<Vec<(String, DateTime<Utc>)>, String> {
    let url = format!("https://steamdb.info/api/PatchnotesRSS/?appid={}", app_id);
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;
    let body = client
        .get(&url)
        .header("User-Agent", "OmniPacker/1.0")
        .send()
        .map_err(|e| format!("HTTP request failed: {}", e))?
        .text()
        .map_err(|e| format!("Failed to read response body: {}", e))?;
    Ok(parse_build_history(&body))
}

fn parse_build_history(xml: &str) -> Vec<(String, DateTime<Utc>)> {
    let item_re = regex::Regex::new(r"<item>([\s\S]*?)</item>").unwrap();
    let guid_re = regex::Regex::new(r"<guid[^>]*>\s*build#(\d+)\s*</guid>").unwrap();
    let build_re = regex::Regex::new(r"Build\s+(\d+)").unwrap();
    let date_re = regex::Regex::new(r"<pubDate>([^<]+)</pubDate>").unwrap();
    item_re
        .captures_iter(xml)
        .filter_map(|item| {
            let body = &item[1];
            let id = guid_re
                .captures(body)
                .or_else(|| build_re.captures(body))
                .map(|c| c[1].to_string())?;
            let date = parse_rfc2822_date(&date_re.captures(body)?[1]).ok()?;
            Some((id, date))
        })
        .collect()
}

/// Upper bound between a manifest's creation and its build going public.
/// Devs usually publish within minutes; a day covers slow pipelines without
/// pairing the manifest with an unrelated later update.
const MANIFEST_TO_RELEASE_WINDOW_HOURS: i64 = 24;

/// Picks the build a pinned manifest most likely belongs to: the earliest
/// release at or after the manifest's creation time, within a day. A manifest
/// is uploaded before (or as) its build goes live, so the matching build is
/// the next release after it. `None` when nothing in the history fits.
pub fn match_manifest_to_build(
    manifest_created: DateTime<Utc>,
    history: &[(String, DateTime<Utc>)],
) -> Option<String> {
    let window = chrono::Duration::hours(MANIFEST_TO_RELEASE_WINDOW_HOURS);
    // Allow a few minutes of clock skew between Steam's manifest timestamp
    // and SteamDB's recorded release time.
    let skew = chrono::Duration::minutes(10);
    history
        .iter()
        .filter(|(_, released)| *released >= manifest_created - skew && *released <= manifest_created + window)
        .min_by_key(|(_, released)| *released)
        .map(|(id, _)| id.clone())
}

/// Parses RFC 2822 date format used in RSS feeds
/// Example: "Mon, 24 Feb 2025 22:02:36 GMT"
fn parse_rfc2822_date(date_str: &str) -> Result<DateTime<Utc>, String> {
    // Try chrono's RFC 2822 parser
    DateTime::parse_from_rfc2822(date_str)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            // Fallback: try common variations
            // Some servers might use slightly different formats
            let cleaned = date_str.trim();

            // Try parsing with a more lenient approach
            chrono::DateTime::parse_from_str(cleaned, "%a, %d %b %Y %H:%M:%S %Z")
                .map(|dt| dt.with_timezone(&Utc))
                .or_else(|_| {
                    chrono::DateTime::parse_from_str(cleaned, "%a, %d %b %Y %H:%M:%S GMT")
                        .map(|dt| dt.with_timezone(&Utc))
                })
        })
        .map_err(|e| format!("Failed to parse date '{}': {}", date_str, e))
}

/// Clears the build date cache (useful for testing or forcing refresh)
#[allow(dead_code)]
pub fn clear_cache() {
    if let Ok(mut guard) = BUILD_DATE_CACHE.lock() {
        *guard = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rfc2822_date() {
        let date_str = "Mon, 24 Feb 2025 22:02:36 GMT";
        let result = parse_rfc2822_date(date_str);
        assert!(result.is_ok());
        let dt = result.unwrap();
        assert_eq!(dt.year(), 2025);
        assert_eq!(dt.month(), 2);
        assert_eq!(dt.day(), 24);
    }

    #[test]
    fn test_parse_patchnotes_rss() {
        let xml = r#"
        <rss version="2.0">
          <channel>
            <item>
              <title>Update - Build 18674832</title>
              <pubDate>Mon, 24 Feb 2025 22:02:36 GMT</pubDate>
              <link>https://steamdb.info/patchnotes/18674832/</link>
            </item>
            <item>
              <title>Update - Build 18674000</title>
              <pubDate>Thu, 20 Feb 2025 10:00:00 GMT</pubDate>
              <link>https://steamdb.info/patchnotes/18674000/</link>
            </item>
          </channel>
        </rss>
        "#;

        // Test getting specific build
        let result = parse_patchnotes_rss(xml, Some("18674832"));
        assert!(result.is_ok());
        let dt = result.unwrap();
        assert_eq!(dt.day(), 24);

        // Test getting latest (first item)
        let result = parse_patchnotes_rss(xml, None);
        assert!(result.is_ok());
        let dt = result.unwrap();
        assert_eq!(dt.day(), 24);

        // Test getting different build
        let result = parse_patchnotes_rss(xml, Some("18674000"));
        assert!(result.is_ok());
        let dt = result.unwrap();
        assert_eq!(dt.day(), 20);
    }

    /// Real-shape fixture captured from the live feed: the build ID is in the
    /// guid/description, never the title. Regression for the parser keying off
    /// the title, which matched nothing and returned the newest item's date for
    /// every build.
    const REAL_FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel><title>SteamDB Builds for Balatro</title>
<item><guid isPermaLink="false">build#17459173</guid><title>Balatro update for 24 February 2025</title><link>https://steamdb.info/patchnotes/17459173/</link><description>Welcoming Friends of Jimbo 4! (SteamDB Build 17459173)</description><pubDate>Mon, 24 Feb 2025 18:21:58 +0000</pubDate></item>
<item><guid isPermaLink="false">build#16541019</guid><title>Balatro update for 12 December 2024</title><link>https://steamdb.info/patchnotes/16541019/</link><description>Friends of Jimbo 3 LIVE! (SteamDB Build 16541019)</description><pubDate>Thu, 12 Dec 2024 18:00:39 +0000</pubDate></item>
</channel></rss>"#;

    #[test]
    fn real_feed_matches_build_id_from_guid() {
        let older = parse_patchnotes_rss(REAL_FEED, Some("16541019")).unwrap();
        assert_eq!((older.year(), older.month(), older.day()), (2024, 12, 12));

        let newest = parse_patchnotes_rss(REAL_FEED, Some("17459173")).unwrap();
        assert_eq!((newest.year(), newest.month(), newest.day()), (2025, 2, 24));
    }

    #[test]
    fn unknown_build_is_an_error_not_the_newest_date() {
        // A build not in the feed must not borrow another build's date; the
        // caller falls back to DepotDownloader-derived timestamps instead.
        assert!(parse_patchnotes_rss(REAL_FEED, Some("99999999")).is_err());
    }

    #[test]
    fn no_target_returns_newest_item() {
        let dt = parse_patchnotes_rss(REAL_FEED, None).unwrap();
        assert_eq!((dt.year(), dt.month(), dt.day()), (2025, 2, 24));
    }

    #[test]
    fn manifest_matches_next_release_within_a_day() {
        use chrono::TimeZone;
        let history = parse_build_history(REAL_FEED);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].0, "17459173");

        // Manifest made an hour before the Feb 24 18:21 release -> that build.
        let created = Utc.with_ymd_and_hms(2025, 2, 24, 17, 20, 0).unwrap();
        assert_eq!(match_manifest_to_build(created, &history).as_deref(), Some("17459173"));
        // Manifest from the Dec 12 build day -> the Dec build, not the newer one.
        let created = Utc.with_ymd_and_hms(2024, 12, 12, 17, 0, 0).unwrap();
        assert_eq!(match_manifest_to_build(created, &history).as_deref(), Some("16541019"));
        // Weeks before anything in the feed -> no guess.
        let created = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        assert_eq!(match_manifest_to_build(created, &history), None);
    }

    use chrono::Datelike;
}
