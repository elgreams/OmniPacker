use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Mutex;

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
                eprintln!("[STEAMDB] Cache hit for {}", cache_key);
                return Ok(*cached_date);
            }
        }
    }

    let url = format!("https://steamdb.info/api/PatchnotesRSS/?appid={}", app_id);
    eprintln!("[STEAMDB] Fetching build date from: {}", url);

    // Use reqwest blocking client for HTTP request
    let client = reqwest::blocking::Client::new();
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
/// RSS format:
/// ```xml
/// <rss version="2.0">
///   <channel>
///     <item>
///       <title>Update - Build 18674832</title>
///       <pubDate>Mon, 24 Feb 2025 22:02:36 GMT</pubDate>
///       <link>https://steamdb.info/patchnotes/18674832/</link>
///     </item>
///   </channel>
/// </rss>
/// ```
fn parse_patchnotes_rss(xml: &str, target_build_id: Option<&str>) -> Result<DateTime<Utc>, String> {
    // Simple XML parsing using regex - avoids adding heavy XML dependencies
    // This is acceptable because the RSS format is well-defined and stable

    let item_regex = regex::Regex::new(r"<item>([\s\S]*?)</item>")
        .map_err(|e| format!("Regex error: {}", e))?;

    let title_regex = regex::Regex::new(r"<title>([^<]+)</title>")
        .map_err(|e| format!("Regex error: {}", e))?;

    let pubdate_regex = regex::Regex::new(r"<pubDate>([^<]+)</pubDate>")
        .map_err(|e| format!("Regex error: {}", e))?;

    let build_id_regex = regex::Regex::new(r"Build\s+(\d+)")
        .map_err(|e| format!("Regex error: {}", e))?;

    for item_cap in item_regex.captures_iter(xml) {
        let item_content = &item_cap[1];

        // Extract title to get build ID
        let title = title_regex
            .captures(item_content)
            .map(|c| c[1].to_string());

        // Extract pubDate
        let pub_date_str = pubdate_regex
            .captures(item_content)
            .map(|c| c[1].to_string());

        if let (Some(title), Some(pub_date_str)) = (title, pub_date_str) {
            // Extract build ID from title
            let item_build_id = build_id_regex
                .captures(&title)
                .map(|c| c[1].to_string());

            // If we have a target build ID, check if this matches
            if let Some(target) = target_build_id {
                if let Some(ref found_id) = item_build_id {
                    if found_id != target {
                        continue; // Not the build we're looking for
                    }
                }
            }

            // Parse the pubDate (RFC 2822 format)
            // Example: "Mon, 24 Feb 2025 22:02:36 GMT"
            let parsed_date = parse_rfc2822_date(&pub_date_str)?;

            eprintln!(
                "[STEAMDB] Found build {} with date: {}",
                item_build_id.as_deref().unwrap_or("unknown"),
                parsed_date
            );

            return Ok(parsed_date);
        }
    }

    // If we have a target build ID and didn't find it, try returning the latest
    if target_build_id.is_some() {
        eprintln!("[STEAMDB] Target build not found, trying to get latest...");
        return parse_patchnotes_rss(xml, None);
    }

    Err("No builds found in SteamDB RSS feed".to_string())
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

    use chrono::Datelike;
}
