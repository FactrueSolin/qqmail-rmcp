use std::net::SocketAddr;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub qqmail_address: String,
    pub qqmail_auth_code: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub imap_host: String,
    pub imap_port: u16,
    pub mcp_bind: SocketAddr,
    pub mcp_access_token: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        #[cfg(not(test))]
        dotenvy::dotenv().ok();

        let qqmail_address =
            std::env::var("QQMAIL_ADDRESS").map_err(|_| "QQMAIL_ADDRESS is required")?;
        if qqmail_address.is_empty() {
            return Err("QQMAIL_ADDRESS must not be empty".into());
        }
        let qqmail_auth_code =
            std::env::var("QQMAIL_AUTH_CODE").map_err(|_| "QQMAIL_AUTH_CODE is required")?;
        if qqmail_auth_code.is_empty() {
            return Err("QQMAIL_AUTH_CODE must not be empty".into());
        }
        let smtp_host = std::env::var("QQMAIL_SMTP_HOST").unwrap_or_else(|_| "smtp.qq.com".into());
        let smtp_port: u16 = std::env::var("QQMAIL_SMTP_PORT")
            .unwrap_or_else(|_| "465".into())
            .parse()
            .map_err(|_| "QQMAIL_SMTP_PORT must be a valid port number")?;
        let imap_host = std::env::var("QQMAIL_IMAP_HOST").unwrap_or_else(|_| "imap.qq.com".into());
        let imap_port: u16 = std::env::var("QQMAIL_IMAP_PORT")
            .unwrap_or_else(|_| "993".into())
            .parse()
            .map_err(|_| "QQMAIL_IMAP_PORT must be a valid port number")?;
        let mcp_bind: SocketAddr = std::env::var("MCP_HTTP_BIND")
            .unwrap_or_else(|_| "127.0.0.1:3000".into())
            .parse()
            .map_err(|_| "MCP_HTTP_BIND must be a valid address (e.g. 127.0.0.1:3000)")?;
        let mcp_access_token =
            std::env::var("MCP_ACCESS_TOKEN").map_err(|_| "MCP_ACCESS_TOKEN is required")?;

        if mcp_access_token.is_empty() {
            return Err("MCP_ACCESS_TOKEN must not be empty".into());
        }

        Ok(Self {
            qqmail_address,
            qqmail_auth_code,
            smtp_host,
            smtp_port,
            imap_host,
            imap_port,
            mcp_bind,
            mcp_access_token,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clear_env_keys() {
        for key in &[
            "QQMAIL_ADDRESS",
            "QQMAIL_AUTH_CODE",
            "QQMAIL_SMTP_HOST",
            "QQMAIL_SMTP_PORT",
            "QQMAIL_IMAP_HOST",
            "QQMAIL_IMAP_PORT",
            "MCP_HTTP_BIND",
            "MCP_ACCESS_TOKEN",
        ] {
            // SAFETY: test-only, single-threaded test context
            unsafe { std::env::remove_var(key) };
        }
    }

    #[test]
    fn test_config_missing_qqmail_address() {
        clear_env_keys();
        // SAFETY: test-only
        unsafe {
            std::env::set_var("QQMAIL_AUTH_CODE", "test-code");
            std::env::set_var("MCP_ACCESS_TOKEN", "test-token");
        }
        let result = AppConfig::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("QQMAIL_ADDRESS"));
        clear_env_keys();
    }

    #[test]
    fn test_config_empty_qqmail_address() {
        clear_env_keys();
        // SAFETY: test-only
        unsafe {
            std::env::set_var("QQMAIL_ADDRESS", "");
            std::env::set_var("QQMAIL_AUTH_CODE", "test-code");
            std::env::set_var("MCP_ACCESS_TOKEN", "test-token");
        }
        let result = AppConfig::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must not be empty"));
        clear_env_keys();
    }

    #[test]
    fn test_config_empty_auth_code() {
        clear_env_keys();
        // SAFETY: test-only
        unsafe {
            std::env::set_var("QQMAIL_ADDRESS", "test@qq.com");
            std::env::set_var("QQMAIL_AUTH_CODE", "");
            std::env::set_var("MCP_ACCESS_TOKEN", "test-token");
        }
        let result = AppConfig::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("QQMAIL_AUTH_CODE"));
        clear_env_keys();
    }

    #[test]
    fn test_config_empty_access_token() {
        clear_env_keys();
        // SAFETY: test-only
        unsafe {
            std::env::set_var("QQMAIL_ADDRESS", "test@qq.com");
            std::env::set_var("QQMAIL_AUTH_CODE", "test-code");
            std::env::set_var("MCP_ACCESS_TOKEN", "");
        }
        let result = AppConfig::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("MCP_ACCESS_TOKEN"));
        clear_env_keys();
    }

    #[test]
    fn test_config_valid_minimal() {
        clear_env_keys();
        // SAFETY: test-only
        unsafe {
            std::env::set_var("QQMAIL_ADDRESS", "test@qq.com");
            std::env::set_var("QQMAIL_AUTH_CODE", "test-code");
            std::env::set_var("MCP_ACCESS_TOKEN", "test-token");
        }
        let result = AppConfig::from_env();
        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.qqmail_address, "test@qq.com");
        assert_eq!(config.smtp_host, "smtp.qq.com");
        assert_eq!(config.smtp_port, 465);
        assert_eq!(config.imap_host, "imap.qq.com");
        assert_eq!(config.imap_port, 993);
        clear_env_keys();
    }
}
