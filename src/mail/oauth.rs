use crate::config::{MailAccountConfig, MailProvider, OAuthConfig};
use crate::error::MailError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
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

        Ok(StoredToken {
            account_id: token.account_id,
            provider: token.provider,
            subject: token.subject,
            email: token.email,
            scopes: body
                .scope
                .map(|scope| scope.split_whitespace().map(str::to_string).collect())
                .unwrap_or(token.scopes),
            access_token: body.access_token,
            refresh_token: body.refresh_token.or(token.refresh_token),
            expires_at: now_seconds() + body.expires_in.unwrap_or(3600),
        })
    }
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
