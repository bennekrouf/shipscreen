//! App Store Connect's API: the app, its version, its localizations and its
//! screenshots.

use super::*;

/// Mint a short-lived ES256 JWT for App Store Connect API authentication.
/// key_id: the key ID from App Store Connect (e.g. "53U79ZJ29Y")
/// issuer_id: the issuer UUID from App Store Connect
/// p8_pem: contents of the downloaded .p8 private key file
pub(super) fn asc_mint_jwt(key_id: &str, issuer_id: &str, p8_pem: &str) -> Result<String, String> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

    #[derive(serde::Serialize)]
    struct AscClaims {
        iss: String,
        iat: u64,
        exp: u64,
        aud: String,
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();

    let claims = AscClaims {
        iss: issuer_id.to_string(),
        iat: now,
        exp: now + 1200, // 20 minutes — ASC max is 20 min
        aud: "appstoreconnect-v1".to_string(),
    };

    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(key_id.to_string());

    let key =
        EncodingKey::from_ec_pem(p8_pem.as_bytes()).map_err(|e| format!("Invalid .p8 key: {e}"))?;

    encode(&header, &claims, &key).map_err(|e| format!("JWT encode error: {e}"))
}

/// Find an app in App Store Connect by bundle ID, return its app ID.
pub(super) async fn asc_find_app(jwt: &str, bundle_id: &str) -> Result<String, String> {
    let url = format!(
        "https://api.appstoreconnect.apple.com/v1/apps?filter[bundleId]={bundle_id}&fields[apps]=bundleId"
    );
    let resp: serde_json::Value = reqwest::Client::new()
        .get(&url)
        .bearer_auth(jwt)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    if let Some(err) = resp["errors"].as_array().and_then(|e| e.first()) {
        let title = err["title"].as_str().unwrap_or("App Store Connect error");
        let detail = err["detail"].as_str().unwrap_or("");
        return Err(format!("{title}: {detail}"));
    }
    resp["data"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|app| app["id"].as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("App with bundle ID '{bundle_id}' not found in App Store Connect"))
}

/// Find the latest iOS App Store version (any state), or create one if none exists.
///
/// Strategy:
///   1. GET all versions without a platform filter (the filter can hide READY_FOR_SALE etc.)
///   2. Prefer an editable state; fall back to any existing version.
///   3. Only attempt POST when the list is truly empty. If POST returns
///      "duplicate" or "invalid state" errors, do a second wider GET and use whatever comes back.
pub(super) async fn asc_find_or_create_version(
    jwt: &str,
    app_id: &str,
    version_string: &str,
) -> Result<String, String> {
    // ── Step 1: fetch all versions (no platform filter, no field pruning) ──────
    if let Some(id) = asc_fetch_any_version(jwt, app_id).await? {
        return Ok(id);
    }

    // ── Step 2: truly no version found — try to create one ────────────────────
    let version_string = if version_string.is_empty() {
        "1.0"
    } else {
        version_string
    };
    tracing::info!("No iOS version found; creating v{version_string} via ASC API");
    let body = serde_json::json!({
        "data": {
            "type": "appStoreVersions",
            "attributes": {
                "platform": "IOS",
                "versionString": version_string
            },
            "relationships": {
                "app": {
                    "data": { "type": "apps", "id": app_id }
                }
            }
        }
    });
    let http_resp = reqwest::Client::new()
        .post("https://api.appstoreconnect.apple.com/v1/appStoreVersions")
        .bearer_auth(jwt)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let create_resp: serde_json::Value = http_resp.json().await.map_err(|e| e.to_string())?;

    // Happy path: POST succeeded and returned the new version.
    if let Some(id) = create_resp["data"]["id"].as_str() {
        return Ok(id.to_string());
    }

    // POST failed — if the error indicates the version already exists or the app
    // is in an incompatible state, fall back to a second wider GET.
    let error_codes: Vec<&str> = create_resp["errors"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|e| e["code"].as_str()).collect())
        .unwrap_or_default();

    let already_exists = error_codes
        .iter()
        .any(|c| c.contains("DUPLICATE") || c.contains("INVALID"));

    if already_exists {
        // The version exists but wasn't returned by our first GET.
        // Try again without any query parameters.
        tracing::warn!("POST said version exists; retrying GET with no filters");
        if let Some(id) = asc_fetch_any_version(jwt, app_id).await? {
            return Ok(id);
        }
    }

    Err(format!("Failed to find or create version: {create_resp}"))
}

/// GET /v1/apps/{id}/appStoreVersions with no filters and return the ID of the
/// best available version (editable states preferred, any state as fallback).
pub(super) async fn asc_fetch_any_version(
    jwt: &str,
    app_id: &str,
) -> Result<Option<String>, String> {
    let url = format!(
        "https://api.appstoreconnect.apple.com/v1/apps/{app_id}/appStoreVersions\
         ?sort=-createdDate&limit=20"
    );
    let resp: serde_json::Value = reqwest::Client::new()
        .get(&url)
        .bearer_auth(jwt)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let versions = resp["data"].as_array().cloned().unwrap_or_default();

    let editable_states = [
        "PREPARE_FOR_SUBMISSION",
        "WAITING_FOR_REVIEW",
        "IN_REVIEW",
        "REJECTED",
        "DEVELOPER_REJECTED",
        "METADATA_REJECTED",
        "INVALID_BINARY",
        "READY_FOR_REVIEW",
    ];

    // Prefer editable.
    for v in versions.iter() {
        let state = v["attributes"]["appStoreState"].as_str().unwrap_or("");
        if editable_states.contains(&state) {
            if let Some(id) = v["id"].as_str() {
                return Ok(Some(id.to_string()));
            }
        }
    }

    // Fall back to any version.
    Ok(versions
        .first()
        .and_then(|v| v["id"].as_str())
        .map(|s| s.to_string()))
}

/// Return a map of locale → localization ID for a given version.
pub(super) async fn asc_get_localizations(
    jwt: &str,
    version_id: &str,
) -> Result<std::collections::HashMap<String, String>, String> {
    let url = format!(
        "https://api.appstoreconnect.apple.com/v1/appStoreVersions/{version_id}/appStoreVersionLocalizations\
         ?fields[appStoreVersionLocalizations]=locale\
         &limit=50"
    );
    let resp: serde_json::Value = reqwest::Client::new()
        .get(&url)
        .bearer_auth(jwt)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let mut map = std::collections::HashMap::new();
    if let Some(arr) = resp["data"].as_array() {
        for item in arr {
            if let (Some(id), Some(locale)) =
                (item["id"].as_str(), item["attributes"]["locale"].as_str())
            {
                map.insert(locale.to_string(), id.to_string());
            }
        }
    }
    Ok(map)
}

/// PATCH an appStoreVersionLocalization to set the app name.
/// ASC requires `name` to be non-empty for the version to pass review.
pub(super) async fn asc_set_localization_name(
    jwt: &str,
    localization_id: &str,
    name: &str,
) -> Result<(), String> {
    let body = serde_json::json!({
        "data": {
            "type": "appStoreVersionLocalizations",
            "id": localization_id,
            "attributes": {
                "name": name
            }
        }
    });
    let resp = reqwest::Client::new()
        .patch(format!(
            "https://api.appstoreconnect.apple.com/v1/appStoreVersionLocalizations/{localization_id}"
        ))
        .bearer_auth(jwt)
        .json(&body)
        .send().await.map_err(|e| e.to_string())?;

    if resp.status().is_success() {
        Ok(())
    } else {
        let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        Err(format!("{body}"))
    }
}

/// Get the screenshot set ID for a given localization + display type,
/// creating the set if it doesn't exist yet.
pub(super) async fn asc_get_or_create_screenshot_set(
    jwt: &str,
    localization_id: &str,
    display_type: &str,
) -> Result<String, String> {
    // List existing sets for this localization.
    let list_url = format!(
        "https://api.appstoreconnect.apple.com/v1/appStoreVersionLocalizations/{localization_id}/appScreenshotSets\
         ?fields[appScreenshotSets]=screenshotDisplayType\
         &limit=40"
    );
    let resp: serde_json::Value = reqwest::Client::new()
        .get(&list_url)
        .bearer_auth(jwt)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    if let Some(arr) = resp["data"].as_array() {
        for set in arr {
            if set["attributes"]["screenshotDisplayType"].as_str() == Some(display_type) {
                if let Some(id) = set["id"].as_str() {
                    return Ok(id.to_string());
                }
            }
        }
    }

    // Create the set.
    let body = serde_json::json!({
        "data": {
            "type": "appScreenshotSets",
            "attributes": { "screenshotDisplayType": display_type },
            "relationships": {
                "appStoreVersionLocalization": {
                    "data": { "type": "appStoreVersionLocalizations", "id": localization_id }
                }
            }
        }
    });
    let create_resp: serde_json::Value = reqwest::Client::new()
        .post("https://api.appstoreconnect.apple.com/v1/appScreenshotSets")
        .bearer_auth(jwt)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    create_resp["data"]["id"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Failed to create screenshot set: {create_resp}"))
}

/// Delete all screenshots currently in a screenshot set.
pub(super) async fn asc_delete_all_screenshots_in_set(
    jwt: &str,
    set_id: &str,
) -> Result<(), String> {
    let list_url = format!(
        "https://api.appstoreconnect.apple.com/v1/appScreenshotSets/{set_id}/appScreenshots\
         ?fields[appScreenshots]=fileName\
         &limit=50"
    );
    let resp: serde_json::Value = reqwest::Client::new()
        .get(&list_url)
        .bearer_auth(jwt)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    if let Some(arr) = resp["data"].as_array() {
        for screenshot in arr {
            if let Some(id) = screenshot["id"].as_str() {
                let del_url =
                    format!("https://api.appstoreconnect.apple.com/v1/appScreenshots/{id}");
                let _ = reqwest::Client::new()
                    .delete(&del_url)
                    .bearer_auth(jwt)
                    .send()
                    .await;
            }
        }
    }
    Ok(())
}

/// Reserve + upload a single screenshot to an appScreenshotSet.
/// Follows the ASC two-phase upload: POST to reserve → PUT to upload bytes → PATCH to commit.
pub(super) async fn asc_upload_screenshot(
    jwt: &str,
    set_id: &str,
    path: &PathBuf,
) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Read file: {e}"))?;
    let file_size = bytes.len() as u64;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("screenshot.png")
        .to_string();

    // Phase 1: Reserve the upload.
    let reserve_body = serde_json::json!({
        "data": {
            "type": "appScreenshots",
            "attributes": {
                "fileSize": file_size,
                "fileName": file_name,
            },
            "relationships": {
                "appScreenshotSet": {
                    "data": { "type": "appScreenshotSets", "id": set_id }
                }
            }
        }
    });
    let reserve_resp: serde_json::Value = reqwest::Client::new()
        .post("https://api.appstoreconnect.apple.com/v1/appScreenshots")
        .bearer_auth(jwt)
        .json(&reserve_body)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let screenshot_id = reserve_resp["data"]["id"]
        .as_str()
        .ok_or_else(|| format!("No screenshot ID in reserve response: {reserve_resp}"))?
        .to_string();

    // Phase 2: Upload bytes to each part URL provided by ASC.
    let upload_ops = reserve_resp["data"]["attributes"]["uploadOperations"]
        .as_array()
        .ok_or_else(|| "No uploadOperations in reserve response".to_string())?;

    let client = reqwest::Client::new();
    for op in upload_ops {
        let url = op["url"]
            .as_str()
            .ok_or_else(|| "Upload operation missing url".to_string())?;
        let offset = op["offset"].as_u64().unwrap_or(0) as usize;
        let length = op["length"].as_u64().unwrap_or(file_size) as usize;
        let chunk = bytes
            .get(offset..offset + length)
            .ok_or_else(|| {
                format!("Chunk out of range: offset={offset} length={length} file_size={file_size}")
            })?
            .to_vec();

        // Build request with any required headers from the operation.
        let mut req = client.put(url).body(chunk);
        if let Some(headers) = op["requestHeaders"].as_array() {
            for h in headers {
                if let (Some(name), Some(value)) = (h["name"].as_str(), h["value"].as_str()) {
                    req = req.header(name, value);
                }
            }
        }
        let put_resp = req
            .send()
            .await
            .map_err(|e| format!("Upload PUT failed: {e}"))?;
        if !put_resp.status().is_success() {
            return Err(format!("Upload PUT returned HTTP {}", put_resp.status()));
        }
    }

    // Phase 3: Commit — tell ASC the upload is complete.
    let commit_body = serde_json::json!({
        "data": {
            "type": "appScreenshots",
            "id": screenshot_id,
            "attributes": { "uploaded": true }
        }
    });
    let commit_url =
        format!("https://api.appstoreconnect.apple.com/v1/appScreenshots/{screenshot_id}");
    let commit_resp = reqwest::Client::new()
        .patch(&commit_url)
        .bearer_auth(jwt)
        .json(&commit_body)
        .send()
        .await
        .map_err(|e| format!("Commit PATCH failed: {e}"))?;

    if !commit_resp.status().is_success() {
        let body = commit_resp.text().await.unwrap_or_default();
        return Err(format!("Commit failed: {body}"));
    }
    Ok(())
}
