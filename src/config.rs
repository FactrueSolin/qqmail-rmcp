use serde::Deserialize;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

const DEFAULT_ACCOUNT_ID: &str = "default";
const DEFAULT_CONFIG_PATH: &str = "config/qqmail.yaml";
const DEFAULT_MCP_BIND: &str = "127.0.0.1:3000";
const DEFAULT_SMTP_HOST: &str = "smtp.qq.com";
const DEFAULT_SMTP_PORT: u16 = 465;
const DEFAULT_IMAP_HOST: &str = "imap.qq.com";
const DEFAULT_IMAP_PORT: u16 = 993;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub mcp_bind: SocketAddr,
    pub mcp_access_token: String,
    pub token_store_path: PathBuf,
    pub accounts: BTreeMap<String, MailAccountConfig>,
}

#[derive(Clone, Debug)]
pub struct MailAccountConfig {
    pub id: String,
    pub provider: MailProvider,
    pub address: Option<String>,
    pub auth_code: Option<String>,
    pub smtp: Option<MailEndpointConfig>,
    pub imap: Option<MailEndpointConfig>,
    pub oauth: Option<OAuthConfig>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MailProvider {
    Qq,
    Gmail,
    Outlook,
}

impl MailProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            MailProvider::Qq => "qq",
            MailProvider::Gmail => "gmail",
            MailProvider::Outlook => "outlook",
        }
    }
}

#[derive(Clone, Debug)]
pub struct MailEndpointConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Debug)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    pub tenant_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawRootConfig {
    mcp: RawMcpConfig,
    mail: RawMailConfig,
}

#[derive(Debug, Deserialize)]
struct RawMcpConfig {
    bind: Option<String>,
    access_token: String,
    token_store_path: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct RawMailConfig {
    accounts: BTreeMap<String, RawMailAccountConfig>,
}

#[derive(Debug, Deserialize)]
struct RawMailAccountConfig {
    provider: String,
    address: Option<String>,
    auth_code: Option<String>,
    smtp: Option<RawMailEndpointConfig>,
    imap: Option<RawMailEndpointConfig>,
    oauth: Option<RawOAuthConfig>,
}

#[derive(Debug, Deserialize)]
struct RawOAuthConfig {
    client_id: String,
    client_secret: Option<String>,
    redirect_uri: String,
    scopes: Option<Vec<String>>,
    tenant_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawMailEndpointConfig {
    host: Option<String>,
    port: Option<u16>,
}

fn parse_bind(value: &str) -> Result<SocketAddr, String> {
    value
        .parse()
        .map_err(|_| "mcp.bind must be a valid address (e.g. 127.0.0.1:3000)".into())
}

fn validate_account_id(id: &str) -> Result<(), String> {
    if id.trim().is_empty() {
        return Err("account id must not be empty".into());
    }
    if !id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(format!(
            "account id must contain only ASCII letters, numbers, '_' or '-': {}",
            id
        ));
    }
    Ok(())
}

fn default_token_store_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("qqmail-rmcp")
        .join("tokens.json")
}

fn parse_provider(value: &str) -> Result<MailProvider, String> {
    match value {
        "qq" => Ok(MailProvider::Qq),
        "gmail" => Ok(MailProvider::Gmail),
        "outlook" => Ok(MailProvider::Outlook),
        _ => Err("provider must be one of qq, gmail, outlook".into()),
    }
}

fn validate_mcp_access_token(mcp_access_token: &str) -> Result<(), String> {
    if mcp_access_token.trim().is_empty() {
        return Err("mcp.access_token must not be empty".into());
    }
    Ok(())
}

fn validate_qq_required(address: Option<&str>, auth_code: Option<&str>) -> Result<(), String> {
    if address.unwrap_or_default().trim().is_empty() {
        return Err("mail account address must not be empty".into());
    }
    if auth_code.unwrap_or_default().trim().is_empty() {
        return Err("mail account auth_code must not be empty".into());
    }
    Ok(())
}

fn validate_oauth(
    provider: &MailProvider,
    oauth: Option<RawOAuthConfig>,
) -> Result<OAuthConfig, String> {
    let oauth =
        oauth.ok_or_else(|| format!("{} account requires oauth config", provider.as_str()))?;
    if oauth.client_id.trim().is_empty() {
        return Err("oauth.client_id must not be empty".into());
    }
    if oauth.redirect_uri.trim().is_empty() {
        return Err("oauth.redirect_uri must not be empty".into());
    }
    let scopes = oauth.scopes.unwrap_or_else(|| match provider {
        MailProvider::Gmail => vec![
            "openid".into(),
            "email".into(),
            "profile".into(),
            "https://www.googleapis.com/auth/gmail.modify".into(),
            "https://www.googleapis.com/auth/gmail.send".into(),
        ],
        MailProvider::Outlook => vec![
            "openid".into(),
            "email".into(),
            "profile".into(),
            "User.Read".into(),
            "offline_access".into(),
            "Mail.ReadWrite".into(),
            "Mail.Send".into(),
        ],
        MailProvider::Qq => Vec::new(),
    });
    Ok(OAuthConfig {
        client_id: oauth.client_id,
        client_secret: oauth.client_secret,
        redirect_uri: oauth.redirect_uri,
        scopes,
        tenant_id: oauth.tenant_id,
    })
}

fn normalize_endpoint(
    raw: Option<RawMailEndpointConfig>,
    default_host: &str,
    default_port: u16,
) -> Result<MailEndpointConfig, String> {
    let raw = raw.unwrap_or(RawMailEndpointConfig {
        host: None,
        port: None,
    });
    let port = raw.port.unwrap_or(default_port);
    if port == 0 {
        return Err("mail endpoint port must be greater than 0".into());
    }

    Ok(MailEndpointConfig {
        host: raw.host.unwrap_or_else(|| default_host.into()),
        port,
    })
}

fn normalize_yaml_config(raw: RawRootConfig) -> Result<AppConfig, String> {
    if raw.mail.accounts.is_empty() {
        return Err("mail.accounts must contain at least one account".into());
    }

    let mcp_bind = parse_bind(raw.mcp.bind.as_deref().unwrap_or(DEFAULT_MCP_BIND))?;
    let mcp_access_token = raw.mcp.access_token;
    validate_mcp_access_token(&mcp_access_token)?;
    let token_store_path = raw
        .mcp
        .token_store_path
        .unwrap_or_else(default_token_store_path);

    let mut accounts = BTreeMap::new();
    for (id, account) in raw.mail.accounts {
        validate_account_id(&id)?;
        let provider = parse_provider(&account.provider)?;
        let (smtp, imap, oauth) = match provider {
            MailProvider::Qq => {
                validate_qq_required(account.address.as_deref(), account.auth_code.as_deref())?;
                (
                    Some(normalize_endpoint(
                        account.smtp,
                        DEFAULT_SMTP_HOST,
                        DEFAULT_SMTP_PORT,
                    )?),
                    Some(normalize_endpoint(
                        account.imap,
                        DEFAULT_IMAP_HOST,
                        DEFAULT_IMAP_PORT,
                    )?),
                    None,
                )
            }
            MailProvider::Gmail | MailProvider::Outlook => {
                let oauth = validate_oauth(&provider, account.oauth)?;
                (None, None, Some(oauth))
            }
        };

        accounts.insert(
            id.clone(),
            MailAccountConfig {
                id,
                provider,
                address: account.address,
                auth_code: account.auth_code,
                smtp,
                imap,
                oauth,
            },
        );
    }

    Ok(AppConfig {
        mcp_bind,
        mcp_access_token,
        token_store_path,
        accounts,
    })
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        #[cfg(not(test))]
        dotenvy::dotenv().ok();

        if let Ok(path) = std::env::var("QQMAIL_CONFIG") {
            return Self::from_yaml_path(path);
        }

        if Path::new(DEFAULT_CONFIG_PATH).exists() {
            return Self::from_yaml_path(DEFAULT_CONFIG_PATH);
        }

        Self::from_legacy_env()
    }

    pub fn from_yaml_path(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
        let raw: RawRootConfig = serde_yaml::from_str(&content)
            .map_err(|e| format!("failed to parse {}: {}", path.display(), e))?;
        normalize_yaml_config(raw)
    }

    fn from_legacy_env() -> Result<Self, String> {
        let address = std::env::var("QQMAIL_ADDRESS").map_err(|_| "QQMAIL_ADDRESS is required")?;
        let auth_code =
            std::env::var("QQMAIL_AUTH_CODE").map_err(|_| "QQMAIL_AUTH_CODE is required")?;
        let smtp_host =
            std::env::var("QQMAIL_SMTP_HOST").unwrap_or_else(|_| DEFAULT_SMTP_HOST.into());
        let smtp_port: u16 = std::env::var("QQMAIL_SMTP_PORT")
            .unwrap_or_else(|_| DEFAULT_SMTP_PORT.to_string())
            .parse()
            .map_err(|_| "QQMAIL_SMTP_PORT must be a valid port number")?;
        if smtp_port == 0 {
            return Err("QQMAIL_SMTP_PORT must be greater than 0".into());
        }
        let imap_host =
            std::env::var("QQMAIL_IMAP_HOST").unwrap_or_else(|_| DEFAULT_IMAP_HOST.into());
        let imap_port: u16 = std::env::var("QQMAIL_IMAP_PORT")
            .unwrap_or_else(|_| DEFAULT_IMAP_PORT.to_string())
            .parse()
            .map_err(|_| "QQMAIL_IMAP_PORT must be a valid port number")?;
        if imap_port == 0 {
            return Err("QQMAIL_IMAP_PORT must be greater than 0".into());
        }
        let mcp_bind = parse_bind(
            &std::env::var("MCP_HTTP_BIND").unwrap_or_else(|_| DEFAULT_MCP_BIND.into()),
        )?;
        let mcp_access_token =
            std::env::var("MCP_ACCESS_TOKEN").map_err(|_| "MCP_ACCESS_TOKEN is required")?;

        validate_mcp_access_token(&mcp_access_token)?;
        validate_qq_required(Some(&address), Some(&auth_code))?;

        let mut accounts = BTreeMap::new();
        accounts.insert(
            DEFAULT_ACCOUNT_ID.into(),
            MailAccountConfig {
                id: DEFAULT_ACCOUNT_ID.into(),
                provider: MailProvider::Qq,
                address: Some(address),
                auth_code: Some(auth_code),
                smtp: Some(MailEndpointConfig {
                    host: smtp_host,
                    port: smtp_port,
                }),
                imap: Some(MailEndpointConfig {
                    host: imap_host,
                    port: imap_port,
                }),
                oauth: None,
            },
        );

        Ok(Self {
            mcp_bind,
            mcp_access_token,
            token_store_path: default_token_store_path(),
            accounts,
        })
    }

    pub fn account(&self, id: &str) -> Option<&MailAccountConfig> {
        self.accounts.get(id)
    }

    pub fn auth_codes(&self) -> impl Iterator<Item = &str> {
        self.accounts
            .values()
            .filter_map(|account| account.auth_code.as_deref())
            .chain(
                self.accounts
                    .values()
                    .filter_map(|account| account.oauth.as_ref())
                    .filter_map(|oauth| oauth.client_secret.as_deref()),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml_config() -> &'static str {
        r#"
mcp:
  bind: 127.0.0.1:3001
  access_token: token
mail:
  accounts:
    personal:
      provider: qq
      address: personal@qq.com
      auth_code: personal-code
    work:
      provider: qq
      address: work@qq.com
      auth_code: work-code
      smtp:
        host: smtp.qq.com
        port: 465
      imap:
        host: imap.qq.com
        port: 993
"#
    }

    fn parse_yaml(input: &str) -> Result<AppConfig, String> {
        let raw: RawRootConfig = serde_yaml::from_str(input).unwrap();
        normalize_yaml_config(raw)
    }

    #[test]
    fn test_validate_missing_address() {
        let result = validate_qq_required(Some(""), Some("code"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("address"));
    }

    #[test]
    fn test_validate_empty_address() {
        let result = validate_qq_required(Some(""), Some("code"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must not be empty"));
    }

    #[test]
    fn test_validate_empty_auth_code() {
        let result = validate_qq_required(Some("test@qq.com"), Some(""));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("auth_code"));
    }

    #[test]
    fn test_validate_empty_access_token() {
        let result = validate_mcp_access_token("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("access_token"));
    }

    #[test]
    fn test_validate_all_present() {
        let result = validate_qq_required(Some("test@qq.com"), Some("code"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_blank_address_rejected() {
        let result = validate_qq_required(Some("   "), Some("code"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("address"));
    }

    #[test]
    fn test_validate_blank_auth_code_rejected() {
        let result = validate_qq_required(Some("test@qq.com"), Some("\t"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("auth_code"));
    }

    #[test]
    fn test_validate_blank_access_token_rejected() {
        let result = validate_mcp_access_token("\n");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("access_token"));
    }

    #[test]
    fn test_yaml_allows_multiple_accounts_without_default_account() {
        let config = parse_yaml(yaml_config()).unwrap();
        assert_eq!(config.accounts.len(), 2);
        assert!(config.account("personal").is_some());
        assert!(config.account("work").is_some());
    }

    #[test]
    fn test_yaml_applies_endpoint_defaults() {
        let config = parse_yaml(yaml_config()).unwrap();
        let personal = config.account("personal").unwrap();
        let smtp = personal.smtp.as_ref().unwrap();
        let imap = personal.imap.as_ref().unwrap();
        assert_eq!(smtp.host, "smtp.qq.com");
        assert_eq!(smtp.port, 465);
        assert_eq!(imap.host, "imap.qq.com");
        assert_eq!(imap.port, 993);
    }

    #[test]
    fn test_yaml_rejects_unsupported_provider() {
        let result = parse_yaml(
            r#"
mcp:
  access_token: token
mail:
  accounts:
    gmail:
      provider: gmail
      address: test@qq.com
      auth_code: code
"#,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("oauth config"));
    }

    #[test]
    fn test_yaml_rejects_invalid_account_id() {
        let result = parse_yaml(
            r#"
mcp:
  access_token: token
mail:
  accounts:
    "bad id":
      provider: qq
      address: test@qq.com
      auth_code: code
"#,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("account id"));
    }
}
