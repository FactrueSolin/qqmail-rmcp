use serde::Deserialize;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::Path;

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
    pub accounts: BTreeMap<String, MailAccountConfig>,
}

#[derive(Clone, Debug)]
pub struct MailAccountConfig {
    pub provider: MailProvider,
    pub address: String,
    pub auth_code: String,
    pub smtp: MailEndpointConfig,
    pub imap: MailEndpointConfig,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MailProvider {
    Qq,
}

#[derive(Clone, Debug)]
pub struct MailEndpointConfig {
    pub host: String,
    pub port: u16,
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
}

#[derive(Debug, Deserialize)]
struct RawMailConfig {
    accounts: BTreeMap<String, RawMailAccountConfig>,
}

#[derive(Debug, Deserialize)]
struct RawMailAccountConfig {
    provider: String,
    address: String,
    auth_code: String,
    smtp: Option<RawMailEndpointConfig>,
    imap: Option<RawMailEndpointConfig>,
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

fn validate_required(address: &str, auth_code: &str, mcp_access_token: &str) -> Result<(), String> {
    if address.trim().is_empty() {
        return Err("mail account address must not be empty".into());
    }
    if auth_code.trim().is_empty() {
        return Err("mail account auth_code must not be empty".into());
    }
    if mcp_access_token.trim().is_empty() {
        return Err("mcp.access_token must not be empty".into());
    }
    Ok(())
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
    if mcp_access_token.trim().is_empty() {
        return Err("mcp.access_token must not be empty".into());
    }

    let mut accounts = BTreeMap::new();
    for (id, account) in raw.mail.accounts {
        validate_account_id(&id)?;
        validate_required(&account.address, &account.auth_code, &mcp_access_token)?;
        if account.provider != "qq" {
            return Err("Only provider \"qq\" is supported".into());
        }

        let smtp = normalize_endpoint(account.smtp, DEFAULT_SMTP_HOST, DEFAULT_SMTP_PORT)?;
        let imap = normalize_endpoint(account.imap, DEFAULT_IMAP_HOST, DEFAULT_IMAP_PORT)?;

        accounts.insert(
            id,
            MailAccountConfig {
                provider: MailProvider::Qq,
                address: account.address,
                auth_code: account.auth_code,
                smtp,
                imap,
            },
        );
    }

    Ok(AppConfig {
        mcp_bind,
        mcp_access_token,
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

        validate_required(&address, &auth_code, &mcp_access_token)?;

        let mut accounts = BTreeMap::new();
        accounts.insert(
            DEFAULT_ACCOUNT_ID.into(),
            MailAccountConfig {
                provider: MailProvider::Qq,
                address,
                auth_code,
                smtp: MailEndpointConfig {
                    host: smtp_host,
                    port: smtp_port,
                },
                imap: MailEndpointConfig {
                    host: imap_host,
                    port: imap_port,
                },
            },
        );

        Ok(Self {
            mcp_bind,
            mcp_access_token,
            accounts,
        })
    }

    pub fn account(&self, id: &str) -> Option<&MailAccountConfig> {
        self.accounts.get(id)
    }

    pub fn auth_codes(&self) -> impl Iterator<Item = &str> {
        self.accounts
            .values()
            .map(|account| account.auth_code.as_str())
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
        let result = validate_required("", "code", "token");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("address"));
    }

    #[test]
    fn test_validate_empty_address() {
        let result = validate_required("", "code", "token");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must not be empty"));
    }

    #[test]
    fn test_validate_empty_auth_code() {
        let result = validate_required("test@qq.com", "", "token");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("auth_code"));
    }

    #[test]
    fn test_validate_empty_access_token() {
        let result = validate_required("test@qq.com", "code", "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("access_token"));
    }

    #[test]
    fn test_validate_all_present() {
        let result = validate_required("test@qq.com", "code", "token");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_blank_address_rejected() {
        let result = validate_required("   ", "code", "token");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("address"));
    }

    #[test]
    fn test_validate_blank_auth_code_rejected() {
        let result = validate_required("test@qq.com", "\t", "token");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("auth_code"));
    }

    #[test]
    fn test_validate_blank_access_token_rejected() {
        let result = validate_required("test@qq.com", "code", "\n");
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
        assert_eq!(personal.smtp.host, "smtp.qq.com");
        assert_eq!(personal.smtp.port, 465);
        assert_eq!(personal.imap.host, "imap.qq.com");
        assert_eq!(personal.imap.port, 993);
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
        assert!(result.unwrap_err().contains("Only provider"));
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
