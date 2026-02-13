use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, CsrfToken, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
};
use oauth2::basic::BasicClient;
use oauth2::reqwest::http_client;
use sha2::{Digest, Sha256};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tiny_http::{Response, Server};

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const AUTH_URL: &str = "https://auth.openai.com/oauth/authorize";
const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const SCOPES: &[&str] = &["openid", "profile", "email", "offline_access"];

pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub account_id: Option<String>,
}

/// Generates a cryptographically secure random string for PKCE
fn generate_random_string(length: usize) -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

/// Initiates OAuth flow and returns tokens
pub fn initiate_oauth_flow() -> Result<OAuthTokens, String> {
    // Generate PKCE code verifier and challenge
    let code_verifier = generate_random_string(43);
    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());

    use base64::Engine;
    let _challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(hasher.finalize());

    let pkce_verifier = PkceCodeVerifier::new(code_verifier);
    let pkce_challenge = PkceCodeChallenge::from_code_verifier_sha256(&pkce_verifier);

    // Build OAuth client
    let client = BasicClient::new(
        ClientId::new(CLIENT_ID.to_string()),
        None,
        AuthUrl::new(AUTH_URL.to_string()).map_err(|e| e.to_string())?,
        Some(TokenUrl::new(TOKEN_URL.to_string()).map_err(|e| e.to_string())?),
    )
    .set_redirect_uri(RedirectUrl::new(REDIRECT_URI.to_string()).map_err(|e| e.to_string())?);

    // Generate authorization URL with OpenCode-specific parameters
    let (mut auth_url, csrf_state) = client
        .authorize_url(CsrfToken::new_random)
        .add_scopes(SCOPES.iter().map(|s| Scope::new(s.to_string())))
        .set_pkce_challenge(pkce_challenge)
        .url();

    // Add OpenCode-specific query parameters
    auth_url.query_pairs_mut()
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("originator", "drastis");

    // Start local HTTP server for callback
    let server = Server::http("127.0.0.1:1455")
        .map_err(|e| format!("Failed to start callback server on port 1455: {}", e))?;

    log::info!("Starting OAuth flow, opening browser...");

    // Open browser
    open::that(auth_url.to_string())
        .map_err(|e| format!("Failed to open browser: {}", e))?;

    // Wait for callback with timeout
    let (tx, rx) = mpsc::channel();
    let csrf_secret = csrf_state.secret().clone();

    thread::spawn(move || {
        // Listen for callback (2 minute timeout)
        if let Ok(Some(request)) = server.recv_timeout(Duration::from_secs(120)) {
            let url_str = format!("http://localhost{}", request.url());

            // Send success HTML response
            let html = r#"
                <html>
                <head><title>Authentication Successful</title></head>
                <body>
                    <h1>✓ Authentication Successful</h1>
                    <p>You can close this window and return to the CLI.</p>
                    <script>setTimeout(() => window.close(), 2000);</script>
                </body>
                </html>
            "#;
            let _ = request.respond(Response::from_string(html)
                .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..]).unwrap()));

            let _ = tx.send(url_str);
        }
    });

    // Wait for callback
    let callback_url = rx.recv_timeout(Duration::from_secs(120))
        .map_err(|_| "OAuth timeout: No response received within 2 minutes".to_string())?;

    // Parse callback URL
    let url = url::Url::parse(&callback_url)
        .map_err(|e| format!("Invalid callback URL: {}", e))?;

    // Extract code and state from query parameters
    let mut code = None;
    let mut state = None;

    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.to_string()),
            "state" => state = Some(value.to_string()),
            _ => {}
        }
    }

    let code = code.ok_or("No authorization code in callback")?;
    let state = state.ok_or("No state in callback")?;

    // Verify CSRF token
    if state != csrf_secret {
        return Err("CSRF token mismatch - possible security attack".to_string());
    }

    // Exchange code for tokens
    log::info!("Exchanging authorization code for tokens...");

    let token_response = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(pkce_verifier)
        .request(http_client)
        .map_err(|e| format!("Token exchange failed: {}", e))?;

    let access_token = token_response.access_token().secret().clone();
    let refresh_token = token_response.refresh_token()
        .ok_or("No refresh token received")?
        .secret()
        .clone();
    let expires_in = token_response.expires_in()
        .ok_or("No expiration time received")?
        .as_secs() as i64;

    // Extract account ID from access token (JWT)
    let account_id = extract_account_id_from_token(&access_token);

    log::info!("OAuth authentication successful!");

    Ok(OAuthTokens {
        access_token,
        refresh_token,
        expires_in,
        account_id,
    })
}

/// Refreshes OAuth tokens using refresh token
pub fn refresh_oauth_tokens(refresh_token: &str) -> Result<OAuthTokens, String> {
    let client = BasicClient::new(
        ClientId::new(CLIENT_ID.to_string()),
        None,
        AuthUrl::new(AUTH_URL.to_string()).map_err(|e| e.to_string())?,
        Some(TokenUrl::new(TOKEN_URL.to_string()).map_err(|e| e.to_string())?),
    );

    log::info!("Refreshing OAuth tokens...");

    let token_response = client
        .exchange_refresh_token(&RefreshToken::new(refresh_token.to_string()))
        .request(http_client)
        .map_err(|e| format!("Token refresh failed: {}", e))?;

    let access_token = token_response.access_token().secret().clone();
    let refresh_token = token_response.refresh_token()
        .map(|t| t.secret().clone())
        .unwrap_or_else(|| refresh_token.to_string());
    let expires_in = token_response.expires_in()
        .ok_or("No expiration time received")?
        .as_secs() as i64;

    let account_id = extract_account_id_from_token(&access_token);

    Ok(OAuthTokens {
        access_token,
        refresh_token,
        expires_in,
        account_id,
    })
}

/// Extracts ChatGPT account ID from JWT token
fn extract_account_id_from_token(token: &str) -> Option<String> {
    use base64::Engine;

    // JWT format: header.payload.signature
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }

    // Decode payload (base64url)
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .ok()?;

    // Parse JSON
    let json: serde_json::Value = serde_json::from_slice(&payload).ok()?;

    // Extract chatgpt_account_id
    json.get("chatgpt_account_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}
