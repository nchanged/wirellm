use axum::{
    body::Body,
    extract::{ConnectInfo, Query, State},
    http::{header, HeaderMap, HeaderValue, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
    Json,
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};
use wirellm_core::config::AuthConfig;
use wirellm_core::state::AppState;

/// Checks if an IP address is a localhost / loopback address
pub fn is_localhost_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback() || v6.to_canonical().is_loopback(),
    }
}

/// Checks if a request originated from localhost / loopback address
pub fn is_localhost_request(req: &Request<Body>) -> bool {
    if let Some(ci) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
        return is_localhost_ip(ci.0.ip());
    }
    false
}

#[derive(Clone, Copy, Debug)]
pub struct IsLocalhost(pub bool);

impl<S> axum::extract::FromRequestParts<S> for IsLocalhost
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let is_local = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| is_localhost_ip(ci.0.ip()))
            .unwrap_or(false);
        Ok(IsLocalhost(is_local))
    }
}

type HmacSha256 = Hmac<Sha256>;

const SESSION_COOKIE_NAME: &str = "wirellm_session";
const SESSION_DURATION_SECS: u64 = 7 * 24 * 3600; // 7 days

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionUser {
    pub email: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,
    pub exp: u64,
}

#[derive(Debug, Serialize)]
pub struct AuthStatusResponse {
    pub google_auth_enabled: bool,
    pub api_auth_required: bool,
    pub authenticated: bool,
    pub user: Option<SessionUser>,
}

#[derive(Debug, Deserialize)]
pub struct GoogleCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GoogleTokenResponse {
    access_token: String,
    token_type: Option<String>,
    expires_in: Option<u64>,
    id_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GoogleUserInfo {
    email: String,
    #[serde(default)]
    email_verified: bool,
    name: Option<String>,
    picture: Option<String>,
}

/// Signs a SessionUser with HMAC-SHA256 and returns `base64_payload.base64_signature`
pub fn sign_session(user: &SessionUser, secret: &str) -> String {
    let payload = serde_json::to_string(user).unwrap_or_default();
    let b64_payload = B64.encode(payload.as_bytes());

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC-SHA256 accepts keys of any size");
    mac.update(b64_payload.as_bytes());
    let sig = B64.encode(mac.finalize().into_bytes());

    format!("{}.{}", b64_payload, sig)
}

/// Verifies HMAC-SHA256 signature and returns SessionUser if valid and not expired
pub fn verify_session(token: &str, secret: &str) -> Option<SessionUser> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 2 {
        return None;
    }
    let b64_payload = parts[0];
    let b64_sig = parts[1];

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(b64_payload.as_bytes());
    let expected_sig = mac.finalize().into_bytes();
    let provided_sig = B64.decode(b64_sig).ok()?;

    if expected_sig.as_slice() != provided_sig.as_slice() {
        return None;
    }

    let payload_bytes = B64.decode(b64_payload).ok()?;
    let user: SessionUser = serde_json::from_slice(&payload_bytes).ok()?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if user.exp < now {
        return None;
    }

    Some(user)
}

/// Extracts session user from Cookie header
pub fn extract_session_user(headers: &HeaderMap, secret: &str) -> Option<SessionUser> {
    if let Some(cookie_val) = headers.get(header::COOKIE).and_then(|h| h.to_str().ok()) {
        for cookie in cookie_val.split(';') {
            let trimmed = cookie.trim();
            if let Some(token) = trimmed.strip_prefix(&format!("{}=", SESSION_COOKIE_NAME)) {
                return verify_session(token, secret);
            }
        }
    }
    None
}

/// Extracts API key from Authorization header (`Bearer <key>`), `x-api-key`, or query param
pub fn extract_api_key(headers: &HeaderMap, query_token: Option<&str>) -> Option<String> {
    if let Some(auth_val) = headers.get(header::AUTHORIZATION).and_then(|h| h.to_str().ok()) {
        if let Some(token) = auth_val.strip_prefix("Bearer ") {
            let t = token.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    if let Some(key_val) = headers.get("x-api-key").and_then(|h| h.to_str().ok()) {
        let t = key_val.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    if let Some(t) = query_token {
        let t = t.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    None
}

/// Checks if a provided API key matches any configured API key
pub fn is_api_key_valid(provided: &str, configured_keys: &[String]) -> bool {
    for key in configured_keys {
        if key == provided {
            return true;
        }
    }
    false
}

/// Helper to get or derive secret for signing session cookies
pub fn resolve_session_secret(auth: &AuthConfig) -> String {
    if let Some(ref sec) = auth.session_secret {
        if !sec.trim().is_empty() {
            return sec.trim().to_string();
        }
    }
    // Fallback: stable deterministic or process-lifetime secret
    static DEFAULT_SECRET: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    DEFAULT_SECRET
        .get_or_init(|| {
            format!(
                "wirellm-session-secret-{:016x}{:016x}",
                rand::random::<u64>(),
                rand::random::<u64>()
            )
        })
        .clone()
}

/// Determines the public base URL for redirects
pub fn determine_base_url(headers: &HeaderMap, auth: &AuthConfig) -> String {
    if let Some(ref url) = auth.public_url {
        let trimmed = url.trim_end_matches('/');
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("http");

    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(header::HOST))
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:8765");

    format!("{}://{}", proto, host)
}

/// Check if user's email is allowed
fn is_email_allowed(email: &str, allowed_emails: &[String]) -> bool {
    if allowed_emails.is_empty() {
        return true;
    }
    let email_lower = email.to_lowercase();
    for pattern in allowed_emails {
        let p_lower = pattern.to_lowercase();
        if p_lower == email_lower {
            return true;
        }
        if p_lower.starts_with("*@") {
            let domain = &p_lower[1..]; // "@domain.com"
            if email_lower.ends_with(domain) {
                return true;
            }
        }
        if p_lower.starts_with('@') {
            if email_lower.ends_with(&p_lower) {
                return true;
            }
        }
    }
    false
}

// =========================================================================
// HTTP Handlers
// =========================================================================

/// GET /auth/status: Returns current authentication status
pub async fn get_auth_status(
    State(app_state): State<Arc<AppState>>,
    IsLocalhost(is_local): IsLocalhost,
    headers: HeaderMap,
) -> impl IntoResponse {
    if is_local {
        return Json(AuthStatusResponse {
            google_auth_enabled: false,
            api_auth_required: false,
            authenticated: true,
            user: Some(SessionUser {
                email: "localhost".to_string(),
                name: "Local Administrator".to_string(),
                picture: None,
                exp: u64::MAX,
            }),
        });
    }

    let auth = &app_state.auth;
    let secret = resolve_session_secret(auth);
    let session_user = extract_session_user(&headers, &secret);

    let status = AuthStatusResponse {
        google_auth_enabled: auth.is_google_auth_enabled(),
        api_auth_required: auth.is_api_auth_required(),
        authenticated: session_user.is_some(),
        user: session_user,
    };

    Json(status)
}

/// GET /auth/google/login: Initiates Google OAuth redirect
pub async fn google_login_handler(
    State(app_state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    let auth = &app_state.auth;
    let google = match auth.google {
        Some(ref g) if auth.is_google_auth_enabled() => g,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                "Google OAuth is not configured on this WireLLM instance",
            )
                .into_response();
        }
    };

    let client_id = match google.client_id {
        Some(ref id) => id,
        None => return (StatusCode::BAD_REQUEST, "Missing client_id").into_response(),
    };

    let base_url = determine_base_url(&headers, auth);
    let redirect_uri = google
        .redirect_uri
        .clone()
        .unwrap_or_else(|| format!("{}/auth/google/callback", base_url));

    let state = format!("{:016x}{:016x}", rand::random::<u64>(), rand::random::<u64>());

    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?\
        client_id={}&\
        redirect_uri={}&\
        response_type=code&\
        scope=openid%20email%20profile&\
        state={}&\
        access_type=online&\
        prompt=select_account",
        urlencoding_encode(client_id),
        urlencoding_encode(&redirect_uri),
        urlencoding_encode(&state),
    );

    Redirect::temporary(&auth_url).into_response()
}

/// GET /auth/google/callback: Handles OAuth code exchange
pub async fn google_callback_handler(
    State(app_state): State<Arc<AppState>>,
    Query(query): Query<GoogleCallbackQuery>,
    headers: HeaderMap,
) -> Response {
    let auth = &app_state.auth;
    let google = match auth.google {
        Some(ref g) if auth.is_google_auth_enabled() => g,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                "Google OAuth is not configured on this WireLLM instance",
            )
                .into_response();
        }
    };

    if let Some(err) = query.error {
        let desc = query.error_description.unwrap_or_default();
        return (
            StatusCode::BAD_REQUEST,
            format!("Google authentication failed: {} ({})", err, desc),
        )
            .into_response();
    }

    let code = match query.code {
        Some(c) => c,
        None => return (StatusCode::BAD_REQUEST, "Missing code in callback").into_response(),
    };

    let client_id = google.client_id.as_deref().unwrap_or_default();
    let client_secret = google.client_secret.as_deref().unwrap_or_default();
    let base_url = determine_base_url(&headers, auth);
    let redirect_uri = google
        .redirect_uri
        .clone()
        .unwrap_or_else(|| format!("{}/auth/google/callback", base_url));

    // 1. Exchange code for access token with Google
    let http = &app_state.http_client;
    let token_res = match http
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", &code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", &redirect_uri),
        ])
        .send()
        .await
    {
        Ok(res) => res,
        Err(e) => {
            error!("Failed to contact Google token endpoint: {}", e);
            return (
                StatusCode::BAD_GATEWAY,
                "Failed to exchange authorization code with Google",
            )
                .into_response();
        }
    };

    if !token_res.status().is_success() {
        let body = token_res.text().await.unwrap_or_default();
        error!("Google token exchange error: {}", body);
        return (
            StatusCode::UNAUTHORIZED,
            format!("Google token exchange failed: {}", body),
        )
            .into_response();
    }

    let token_data: GoogleTokenResponse = match token_res.json().await {
        Ok(t) => t,
        Err(e) => {
            error!("Failed to parse Google token JSON: {}", e);
            return (
                StatusCode::BAD_GATEWAY,
                "Failed to parse Google token response",
            )
                .into_response();
        }
    };

    // 2. Fetch User Profile
    let userinfo_res = match http
        .get("https://www.googleapis.com/oauth2/v3/userinfo")
        .bearer_auth(&token_data.access_token)
        .send()
        .await
    {
        Ok(res) => res,
        Err(e) => {
            error!("Failed to fetch Google userinfo: {}", e);
            return (
                StatusCode::BAD_GATEWAY,
                "Failed to fetch user profile from Google",
            )
                .into_response();
        }
    };

    if !userinfo_res.status().is_success() {
        return (
            StatusCode::UNAUTHORIZED,
            "Failed to fetch user profile from Google",
        )
            .into_response();
    }

    let user_info: GoogleUserInfo = match userinfo_res.json().await {
        Ok(u) => u,
        Err(e) => {
            error!("Failed to parse Google userinfo JSON: {}", e);
            return (
                StatusCode::BAD_GATEWAY,
                "Failed to parse Google user profile",
            )
                .into_response();
        }
    };

    // 3. Authorization check: is user's email in allowed_emails?
    if !is_email_allowed(&user_info.email, &google.allowed_emails) {
        warn!(
            "Unauthorized login attempt by Google account: {}",
            user_info.email
        );
        let html = format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>WireLLM - Access Denied</title>
    <style>
        body {{ background-color: #060206; color: #f1f5f9; font-family: monospace; display: flex; align-items: center; justify-content: center; height: 100vh; margin: 0; }}
        .box {{ background: #0d040b; border: 1px solid #f43f5e; border-radius: 12px; padding: 32px; max-width: 480px; text-align: center; box-shadow: 0 0 25px rgba(244,63,94,0.2); }}
        h1 {{ color: #f43f5e; font-size: 20px; margin-bottom: 12px; }}
        p {{ color: #94a3b8; font-size: 13px; line-height: 1.5; }}
        .email {{ color: #fff; font-weight: bold; background: #260812; padding: 4px 8px; border-radius: 6px; display: inline-block; margin: 8px 0; }}
        a {{ color: #ff1493; text-decoration: none; display: inline-block; margin-top: 16px; font-weight: bold; }}
    </style>
</head>
<body>
    <div class="box">
        <h1>ACCESS DENIED</h1>
        <p>Your Google account is authenticated, but is not authorized to access this WireLLM cluster instance:</p>
        <div class="email">{}</div>
        <p>Please contact your cluster administrator to add your email to WIRELLM_GOOGLE_ALLOWED_EMAILS.</p>
        <a href="/ui">&larr; Return to Sign In</a>
    </div>
</body>
</html>"#,
            user_info.email
        );
        let mut res = Response::new(Body::from(html));
        *res.status_mut() = StatusCode::FORBIDDEN;
        res.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );
        return res;
    }

    // 4. Create signed session token
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let session_user = SessionUser {
        email: user_info.email,
        name: user_info.name.unwrap_or_else(|| "User".to_string()),
        picture: user_info.picture,
        exp: now + SESSION_DURATION_SECS,
    };

    let secret = resolve_session_secret(auth);
    let session_token = sign_session(&session_user, &secret);

    let is_secure = base_url.starts_with("https://");
    let cookie_val = format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        SESSION_COOKIE_NAME,
        session_token,
        SESSION_DURATION_SECS,
        if is_secure { "; Secure" } else { "" }
    );

    info!("Authenticated Google user: {}", session_user.email);

    let mut response = Redirect::temporary("/ui").into_response();
    if let Ok(hv) = HeaderValue::from_str(&cookie_val) {
        response.headers_mut().insert(header::SET_COOKIE, hv);
    }
    response
}

/// GET/POST /auth/logout: Clears session cookie and redirects
pub async fn logout_handler() -> Response {
    let cookie_val = format!(
        "{}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0",
        SESSION_COOKIE_NAME
    );

    let mut response = Redirect::temporary("/ui").into_response();
    if let Ok(hv) = HeaderValue::from_str(&cookie_val) {
        response.headers_mut().insert(header::SET_COOKIE, hv);
    }
    response
}

// =========================================================================
// Middlewares
// =========================================================================

/// Middleware for `/v1/*` and `/mcp/*`:
/// If API keys are configured, enforces Bearer token or x-api-key authentication.
/// If no API keys are configured, requests are unprotected (open access).
pub async fn require_api_auth(
    State(app_state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // Auth is disabled for localhost
    if is_localhost_request(&req) {
        return next.run(req).await;
    }

    let auth = &app_state.auth;

    // If API token access is not configured, access is open (unprotected)
    if !auth.is_api_auth_required() {
        return next.run(req).await;
    }

    let headers = req.headers();
    if let Some(token) = extract_api_key(headers, None) {
        if is_api_key_valid(&token, &auth.api_keys) {
            return next.run(req).await;
        }
    }

    warn!("Unauthorized API request to {}", req.uri().path());
    let error_body = serde_json::json!({
        "error": {
            "message": "Invalid or missing API key. Please provide 'Authorization: Bearer <key>' or 'x-api-key'.",
            "type": "authentication_error",
            "code": 401
        }
    });

    let bytes = serde_json::to_vec(&error_body).unwrap_or_default();
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = StatusCode::UNAUTHORIZED;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

/// Middleware for `/api/*` and management endpoints:
/// Allows request if:
/// 1. Request is from localhost -> Open access.
/// 2. Google Auth is disabled AND no API keys configured -> Open access.
/// 3. Valid UI session cookie exists -> Allowed.
/// 4. Valid API key is provided -> Allowed.
/// Otherwise returns 401 Unauthorized.
pub async fn require_admin_or_ui_auth(
    State(app_state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // Auth is disabled for localhost
    if is_localhost_request(&req) {
        return next.run(req).await;
    }

    let auth = &app_state.auth;
    let google_enabled = auth.is_google_auth_enabled();
    let api_auth_required = auth.is_api_auth_required();

    // If neither Google Auth nor API keys are enabled, access is open
    if !google_enabled && !api_auth_required {
        return next.run(req).await;
    }

    let headers = req.headers();

    // Check 1: UI Session Cookie
    let secret = resolve_session_secret(auth);
    if extract_session_user(headers, &secret).is_some() {
        return next.run(req).await;
    }

    // Check 2: API Key
    if let Some(token) = extract_api_key(headers, None) {
        if !auth.api_keys.is_empty() && is_api_key_valid(&token, &auth.api_keys) {
            return next.run(req).await;
        }
    }

    // Unauthorized
    let error_body = serde_json::json!({
        "error": {
            "message": "Authentication required. Please sign in via Google OAuth or provide a valid API key.",
            "type": "authentication_error",
            "code": 401
        }
    });

    let bytes = serde_json::to_vec(&error_body).unwrap_or_default();
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = StatusCode::UNAUTHORIZED;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

fn urlencoding_encode(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{:02X}", b));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_signing_and_verification() {
        let secret = "super-secret-key-12345678901234567890";
        let user = SessionUser {
            email: "alice@example.com".to_string(),
            name: "Alice L".to_string(),
            picture: Some("https://example.com/avatar.png".to_string()),
            exp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 3600,
        };

        let token = sign_session(&user, secret);
        assert!(!token.is_empty());
        assert!(token.contains('.'));

        let recovered = verify_session(&token, secret).expect("session should verify");
        assert_eq!(recovered.email, "alice@example.com");
        assert_eq!(recovered.name, "Alice L");
        assert_eq!(
            recovered.picture,
            Some("https://example.com/avatar.png".to_string())
        );

        // Verification with wrong secret fails
        assert!(verify_session(&token, "wrong-secret").is_none());

        // Verification with tampered signature fails
        let tampered = format!("{}x", token);
        assert!(verify_session(&tampered, secret).is_none());
    }

    #[test]
    fn test_expired_session() {
        let secret = "secret-key-test";
        let user = SessionUser {
            email: "bob@example.com".to_string(),
            name: "Bob".to_string(),
            picture: None,
            exp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - 10, // Expired 10s ago
        };

        let token = sign_session(&user, secret);
        assert!(verify_session(&token, secret).is_none());
    }

    #[test]
    fn test_extract_api_key_formats() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer sk-wire-test-token-123"),
        );
        let key = extract_api_key(&headers, None);
        assert_eq!(key, Some("sk-wire-test-token-123".to_string()));

        let mut headers_x = HeaderMap::new();
        headers_x.insert("x-api-key", HeaderValue::from_static("sk-wire-x-key"));
        let key_x = extract_api_key(&headers_x, None);
        assert_eq!(key_x, Some("sk-wire-x-key".to_string()));

        let empty_headers = HeaderMap::new();
        let key_query = extract_api_key(&empty_headers, Some("sk-query-key"));
        assert_eq!(key_query, Some("sk-query-key".to_string()));
    }

    #[test]
    fn test_api_key_validation() {
        let keys = vec![
            "sk-wire-prod-1".to_string(),
            "sk-wire-prod-2".to_string(),
        ];
        assert!(is_api_key_valid("sk-wire-prod-1", &keys));
        assert!(is_api_key_valid("sk-wire-prod-2", &keys));
        assert!(!is_api_key_valid("sk-invalid", &keys));
        assert!(!is_api_key_valid("", &keys));
    }

    #[test]
    fn test_email_whitelist_rules() {
        // Empty allowed list allows all
        assert!(is_email_allowed("anyone@example.com", &[]));

        // Exact match
        let whitelist = vec!["alice@company.com".to_string(), "*@partner.org".to_string()];
        assert!(is_email_allowed("alice@company.com", &whitelist));
        assert!(is_email_allowed("ALICE@COMPANY.COM", &whitelist)); // Case insensitive
        assert!(!is_email_allowed("bob@company.com", &whitelist));

        // Wildcard domain match
        assert!(is_email_allowed("charlie@partner.org", &whitelist));
        assert!(!is_email_allowed("charlie@other.org", &whitelist));
    }

    #[test]
    fn test_localhost_detection() {
        use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

        // IPv4 loopback
        assert!(is_localhost_ip(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(is_localhost_ip(IpAddr::V4(Ipv4Addr::new(127, 10, 0, 1))));

        // IPv6 loopback
        assert!(is_localhost_ip(IpAddr::V6(Ipv6Addr::LOCALHOST)));

        // Non-local IPs
        assert!(!is_localhost_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(!is_localhost_ip(IpAddr::V4(Ipv4Addr::new(10, 244, 0, 15))));
        assert!(!is_localhost_ip(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
    }
}
