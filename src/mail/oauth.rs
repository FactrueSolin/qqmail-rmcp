use crate::config::{MailAccountConfig, MailProvider, OAuthConfig};
use crate::error::MailError;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const REFRESH_SKEW_SECONDS: u64 = 300;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredToken {
    pub account_id: String,
    pub provider: String,
    pub subject: Option<String>,
    pub email: Option<String>,
    pub scopes: Vec<String>,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: u64,
}

#[derive(Clone, Debug)]
pub struct TokenStore {
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Clone, Debug)]
struct PendingOAuthState {
    provider: MailProvider,
    account_id: String,
    expires_at: u64,
}

#[derive(Debug, Default)]
pub struct LocalOAuthStateStore {
    pending: Mutex<BTreeMap<String, PendingOAuthState>>,
}

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
    pub state: String,
    pub code: Option<String>,
    pub error: Option<String>,
}

impl TokenStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn get(&self, account_id: &str) -> Result<Option<StoredToken>, MailError> {
        Ok(self.load()?.get(account_id).cloned())
    }

    pub fn upsert(&self, token: StoredToken) -> Result<(), MailError> {
        let mut tokens = self.load()?;
        tokens.insert(token.account_id.clone(), token);
        self.save(&tokens)
    }

    fn load(&self) -> Result<BTreeMap<String, StoredToken>, MailError> {
        if !self.path.exists() {
            return Ok(BTreeMap::new());
        }
        let content = std::fs::read_to_string(&self.path)?;
        serde_json::from_str(&content).map_err(|e| MailError::ProviderApiError(e.to_string()))
    }

    fn save(&self, tokens: &BTreeMap<String, StoredToken>) -> Result<(), MailError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(tokens)
            .map_err(|e| MailError::ProviderApiError(e.to_string()))?;
        std::fs::write(&self.path, content)?;
        tighten_file_permissions(&self.path)?;
        Ok(())
    }
}

#[cfg(unix)]
fn tighten_file_permissions(path: &Path) -> Result<(), MailError> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(0o600);
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn tighten_file_permissions(_path: &Path) -> Result<(), MailError> {
    Ok(())
}

pub struct AccessTokenProvider {
    client: reqwest::Client,
    store: TokenStore,
}

impl AccessTokenProvider {
    pub fn new(token_store_path: impl Into<PathBuf>) -> Self {
        Self {
            client: reqwest::Client::new(),
            store: TokenStore::new(token_store_path),
        }
    }

    pub async fn exchange_code(
        &self,
        account: &MailAccountConfig,
        code: &str,
    ) -> Result<StoredToken, MailError> {
        let oauth = account
            .oauth
            .as_ref()
            .ok_or(MailError::OAuthNotAuthorized)?;
        let form = authorization_code_form(oauth, code);
        let response = self
            .client
            .post(token_url(&account.provider, oauth))
            .form(&form)
            .send()
            .await
            .map_err(|e| MailError::ProviderApiError(e.to_string()))?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(MailError::ReauthorizationRequired);
        }
        if !response.status().is_success() {
            return Err(MailError::ProviderApiError(response.status().to_string()));
        }
        let body: TokenResponse = response
            .json()
            .await
            .map_err(|e| MailError::ProviderApiError(e.to_string()))?;
        let token = token_from_response(account, body, None);
        self.store.upsert(token.clone())?;
        Ok(token)
    }

    pub async fn get(
        &self,
        account: &MailAccountConfig,
        required_scopes: &[&str],
    ) -> Result<String, MailError> {
        let mut token = self
            .store
            .get(&account.id)?
            .ok_or(MailError::OAuthNotAuthorized)?;
        ensure_scopes(&token, required_scopes)?;

        if token.expires_at > now_seconds() + REFRESH_SKEW_SECONDS {
            return Ok(token.access_token);
        }

        token = self.refresh(account, token).await?;
        ensure_scopes(&token, required_scopes)?;
        let access_token = token.access_token.clone();
        self.store.upsert(token)?;
        Ok(access_token)
    }

    async fn refresh(
        &self,
        account: &MailAccountConfig,
        token: StoredToken,
    ) -> Result<StoredToken, MailError> {
        let oauth = account
            .oauth
            .as_ref()
            .ok_or(MailError::OAuthNotAuthorized)?;
        let _ = (&oauth.redirect_uri, &oauth.scopes);
        let refresh_token = token
            .refresh_token
            .clone()
            .ok_or(MailError::ReauthorizationRequired)?;
        let mut form = vec![
            ("client_id", oauth.client_id.as_str()),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.as_str()),
        ];
        if let Some(secret) = oauth.client_secret.as_deref() {
            form.push(("client_secret", secret));
        }

        let response = self
            .client
            .post(token_url(&account.provider, oauth))
            .form(&form)
            .send()
            .await
            .map_err(|e| MailError::ProviderApiError(e.to_string()))?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(MailError::ReauthorizationRequired);
        }
        if !response.status().is_success() {
            return Err(MailError::ProviderApiError(response.status().to_string()));
        }
        let body: TokenResponse = response
            .json()
            .await
            .map_err(|e| MailError::ProviderApiError(e.to_string()))?;

        Ok(token_from_response(account, body, Some(token)))
    }
}

impl LocalOAuthStateStore {
    pub fn authorization_url(&self, account: &MailAccountConfig) -> Result<String, MailError> {
        let oauth = account
            .oauth
            .as_ref()
            .ok_or(MailError::OAuthNotAuthorized)?;
        let nonce = format!("{}", now_seconds_nanos());
        let expires_at = now_seconds() + 600;
        let state = encode_state(&account.provider, &account.id, &nonce, expires_at);
        self.pending
            .lock()
            .map_err(|_| MailError::ProviderApiError("OAuth state lock poisoned".into()))?
            .insert(
                state.clone(),
                PendingOAuthState {
                    provider: account.provider.clone(),
                    account_id: account.id.clone(),
                    expires_at,
                },
            );

        let base_url = authorization_url(&account.provider, oauth);
        let separator = if base_url.contains('?') { '&' } else { '?' };
        Ok(format!(
            "{}{}response_type=code&client_id={}&redirect_uri={}&scope={}&state={}",
            base_url,
            separator,
            percent_encode(&oauth.client_id),
            percent_encode(&oauth.redirect_uri),
            percent_encode(&oauth.scopes.join(" ")),
            percent_encode(&state),
        ))
    }

    pub fn consume_state(&self, state: &str, account: &MailAccountConfig) -> Result<(), MailError> {
        let pending = self
            .pending
            .lock()
            .map_err(|_| MailError::ProviderApiError("OAuth state lock poisoned".into()))?
            .remove(state)
            .ok_or(MailError::ReauthorizationRequired)?;
        if pending.expires_at < now_seconds()
            || pending.provider != account.provider
            || pending.account_id != account.id
        {
            return Err(MailError::ReauthorizationRequired);
        }
        Ok(())
    }
}

pub async fn complete_local_oauth_callback(
    states: &LocalOAuthStateStore,
    token_store_path: impl Into<PathBuf>,
    account: &MailAccountConfig,
    callback: OAuthCallbackQuery,
) -> Result<StoredToken, MailError> {
    if let Some(error) = callback.error {
        return Err(MailError::ProviderApiError(format!(
            "OAuth provider returned error: {}",
            error
        )));
    }
    states.consume_state(&callback.state, account)?;
    let code = callback.code.ok_or(MailError::ReauthorizationRequired)?;
    AccessTokenProvider::new(token_store_path)
        .exchange_code(account, &code)
        .await
}

pub fn account_id_from_state(state: &str) -> Result<String, MailError> {
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(state)
        .map_err(|_| MailError::ReauthorizationRequired)?;
    let decoded = String::from_utf8(decoded).map_err(|_| MailError::ReauthorizationRequired)?;
    decoded
        .split(':')
        .nth(1)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or(MailError::ReauthorizationRequired)
}

fn token_from_response(
    account: &MailAccountConfig,
    body: TokenResponse,
    previous: Option<StoredToken>,
) -> StoredToken {
    let previous = previous.unwrap_or_else(|| StoredToken {
        account_id: account.id.clone(),
        provider: account.provider.as_str().to_string(),
        subject: None,
        email: account.address.clone(),
        scopes: account
            .oauth
            .as_ref()
            .map(|oauth| oauth.scopes.clone())
            .unwrap_or_default(),
        access_token: String::new(),
        refresh_token: None,
        expires_at: 0,
    });

    StoredToken {
        account_id: previous.account_id,
        provider: previous.provider,
        subject: previous.subject,
        email: previous.email,
        scopes: body
            .scope
            .map(|scope| scope.split_whitespace().map(str::to_string).collect())
            .unwrap_or(previous.scopes),
        access_token: body.access_token,
        refresh_token: body.refresh_token.or(previous.refresh_token),
        expires_at: now_seconds() + body.expires_in.unwrap_or(3600),
    }
}

fn authorization_code_form<'a>(oauth: &'a OAuthConfig, code: &'a str) -> Vec<(&'a str, &'a str)> {
    let mut form = vec![
        ("client_id", oauth.client_id.as_str()),
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", oauth.redirect_uri.as_str()),
    ];
    if let Some(secret) = oauth.client_secret.as_deref() {
        form.push(("client_secret", secret));
    }
    form
}

fn token_url(provider: &MailProvider, oauth: &OAuthConfig) -> String {
    match provider {
        MailProvider::Gmail => "https://oauth2.googleapis.com/token".into(),
        MailProvider::Outlook => format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            oauth.tenant_id.as_deref().unwrap_or("common")
        ),
        MailProvider::Qq => unreachable!("QQ does not use OAuth token refresh"),
    }
}

fn authorization_url(provider: &MailProvider, oauth: &OAuthConfig) -> String {
    match provider {
        MailProvider::Gmail => {
            "https://accounts.google.com/o/oauth2/v2/auth?access_type=offline&prompt=consent".into()
        }
        MailProvider::Outlook => format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/authorize",
            oauth.tenant_id.as_deref().unwrap_or("common")
        ),
        MailProvider::Qq => unreachable!("QQ does not use OAuth authorization"),
    }
}

fn encode_state(provider: &MailProvider, account_id: &str, nonce: &str, expires_at: u64) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!(
        "{}:{}:{}:{}",
        provider.as_str(),
        account_id,
        nonce,
        expires_at
    ))
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{:02X}", byte));
        }
    }
    encoded
}

fn ensure_scopes(token: &StoredToken, required_scopes: &[&str]) -> Result<(), MailError> {
    for scope in required_scopes {
        if !token.scopes.iter().any(|value| value == scope) {
            return Err(MailError::InsufficientScope((*scope).into()));
        }
    }
    Ok(())
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn now_seconds_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{MailAccountConfig, MailProvider};

    fn oauth_config() -> OAuthConfig {
        OAuthConfig {
            client_id: "client-id".into(),
            client_secret: Some("secret".into()),
            redirect_uri: "http://127.0.0.1:3000/oauth/callback".into(),
            scopes: vec!["openid".into(), "email".into(), "Mail.ReadWrite".into()],
            tenant_id: None,
        }
    }

    fn account(provider: MailProvider) -> MailAccountConfig {
        MailAccountConfig {
            id: "acct".into(),
            provider,
            address: Some("acct@example.com".into()),
            auth_code: None,
            smtp: None,
            imap: None,
            oauth: Some(oauth_config()),
        }
    }

    #[test]
    fn local_authorization_url_binds_state_to_account() {
        let states = LocalOAuthStateStore::default();
        let account = account(MailProvider::Gmail);
        let url = states.authorization_url(&account).unwrap();
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth"));
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("client_id=client-id"));

        let state = url.split("state=").nth(1).unwrap();
        assert_eq!(account_id_from_state(state).unwrap(), "acct");
        assert!(states.consume_state(state, &account).is_ok());
        assert!(states.consume_state(state, &account).is_err());
    }

    #[test]
    fn state_rejects_wrong_provider() {
        let states = LocalOAuthStateStore::default();
        let gmail = account(MailProvider::Gmail);
        let outlook = account(MailProvider::Outlook);
        let url = states.authorization_url(&gmail).unwrap();
        let state = url.split("state=").nth(1).unwrap();
        assert!(states.consume_state(state, &outlook).is_err());
    }

    #[test]
    fn code_exchange_form_uses_authorization_code_grant() {
        let oauth = oauth_config();
        let form = authorization_code_form(&oauth, "code-123");
        assert!(form.contains(&("grant_type", "authorization_code")));
        assert!(form.contains(&("code", "code-123")));
        assert!(form.contains(&("redirect_uri", oauth.redirect_uri.as_str())));
        assert!(form.contains(&("client_secret", "secret")));
    }

    #[test]
    fn token_response_preserves_default_scopes_without_provider_scope() {
        let account = account(MailProvider::Outlook);
        let token = token_from_response(
            &account,
            TokenResponse {
                access_token: "access".into(),
                refresh_token: Some("refresh".into()),
                expires_in: Some(3600),
                scope: None,
            },
            None,
        );
        assert_eq!(token.account_id, "acct");
        assert_eq!(token.provider, "outlook");
        assert!(token.scopes.contains(&"Mail.ReadWrite".into()));
        assert_eq!(token.refresh_token.as_deref(), Some("refresh"));
    }
}
