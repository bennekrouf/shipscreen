//! Google Play's publishing API: an edit, its images, and committing it.

use super::*;

/// Mint a short-lived JWT and exchange it for a Google OAuth2 access token.
pub(super) async fn google_play_access_token(
    client_email: &str,
    private_key_pem: &str,
) -> Result<String, String> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

    #[derive(serde::Serialize)]
    struct Claims {
        iss: String,
        scope: String,
        aud: String,
        iat: u64,
        exp: u64,
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();

    let claims = Claims {
        iss: client_email.to_string(),
        scope: "https://www.googleapis.com/auth/androidpublisher".to_string(),
        aud: "https://oauth2.googleapis.com/token".to_string(),
        iat: now,
        exp: now + 3600,
    };

    let key = EncodingKey::from_rsa_pem(private_key_pem.as_bytes())
        .map_err(|e| format!("Invalid private key: {e}"))?;
    let jwt = encode(&Header::new(Algorithm::RS256), &claims, &key)
        .map_err(|e| format!("JWT encode error: {e}"))?;

    let client = reqwest::Client::new();
    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", &jwt),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    body["access_token"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("No access_token in response: {body}"))
}

/// Create a new edit and return its ID.
pub(super) async fn google_play_create_edit(
    token: &str,
    package_name: &str,
) -> Result<String, String> {
    let url = format!(
        "https://androidpublisher.googleapis.com/androidpublisher/v3/applications/{package_name}/edits"
    );
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(token)
        .json(&serde_json::json!({}))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        // Surface the activation URL when the API is disabled in the GCP project.
        let activation_url = body["error"]["details"].as_array().and_then(|details| {
            details
                .iter()
                .find_map(|d| d["metadata"]["activationUrl"].as_str())
        });
        if let Some(url) = activation_url {
            return Err(format!(
                "Google Play Android Developer API is disabled.\n\
                Enable it here, wait ~1 min, then retry:\n{url}"
            ));
        }
        let message = body["error"]["message"]
            .as_str()
            .unwrap_or("unknown error")
            .to_string();
        // 404 "Package not found" means the app hasn't been created in Play Console yet.
        if status.as_u16() == 404 && message.contains("Package not found") {
            return Err(format!(
                "{message}\n\n\
                The app must exist in Google Play Console before the API can manage it.\n\
                Go to https://play.google.com/console and create the app, then upload\n\
                at least one APK/AAB manually (even as a draft internal release).\n\
                After that, this upload will work."
            ));
        }
        return Err(format!("HTTP {status}: {message}"));
    }
    body["id"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("No edit id in response: {body}"))
}

/// Delete all existing images for a given language + imageType (clears before re-upload).
pub(super) async fn google_play_delete_images(
    token: &str,
    package_name: &str,
    edit_id: &str,
    language: &str,
    image_type: &str,
) -> Result<(), String> {
    let url = format!(
        "https://androidpublisher.googleapis.com/androidpublisher/v3/applications/{package_name}/edits/{edit_id}/listings/{language}/{image_type}"
    );
    let client = reqwest::Client::new();
    let resp = client
        .delete(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    // 204 No Content or 404 (nothing to delete) are both fine
    if !resp.status().is_success() && resp.status().as_u16() != 404 {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Delete images HTTP {status}: {body}"));
    }
    Ok(())
}

/// Upload a single image file.
pub(super) async fn google_play_upload_image(
    token: &str,
    package_name: &str,
    edit_id: &str,
    language: &str,
    image_type: &str,
    path: &std::path::Path,
) -> Result<(), String> {
    let url = format!(
        "https://androidpublisher.googleapis.com/upload/androidpublisher/v3/applications/{package_name}/edits/{edit_id}/listings/{language}/{image_type}"
    );
    let bytes = std::fs::read(path).map_err(|e| format!("Read {}: {e}", path.display()))?;
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(token)
        .query(&[("uploadType", "media")])
        .header("Content-Type", "image/png")
        .body(bytes)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {body}"));
    }
    Ok(())
}

/// Commit an edit to make it live.
pub(super) async fn google_play_commit_edit(
    token: &str,
    package_name: &str,
    edit_id: &str,
) -> Result<(), String> {
    let url = format!(
        "https://androidpublisher.googleapis.com/androidpublisher/v3/applications/{package_name}/edits/{edit_id}:commit"
    );
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(token)
        .json(&serde_json::json!({}))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {body}"));
    }
    Ok(())
}
