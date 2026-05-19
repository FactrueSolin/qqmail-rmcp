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

fn validate_required(
    qqmail_address: &str,
    qqmail_auth_code: &str,
    mcp_access_token: &str,
) -> Result<(), String> {
    if qqmail_address.is_empty() {
        return Err("QQMAIL_ADDRESS must not be empty".into());
    }
    if qqmail_auth_code.is_empty() {
        return Err("QQMAIL_AUTH_CODE must not be empty".into());
    }
    if mcp_access_token.is_empty() {
        return Err("MCP_ACCESS_TOKEN must not be empty".into());
    }
    Ok(())
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        #[cfg(not(test))]
        dotenvy::dotenv().ok();

        let qqmail_address =
            std::env::var("QQMAIL_ADDRESS").map_err(|_| "QQMAIL_ADDRESS is required")?;
        let qqmail_auth_code =
            std::env::var("QQMAIL_AUTH_CODE").map_err(|_| "QQMAIL_AUTH_CODE is required")?;
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

        validate_required(&qqmail_address, &qqmail_auth_code, &mcp_access_token)?;

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

    #[test]
    fn test_validate_missing_address() {
        let result = validate_required("", "code", "token");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("QQMAIL_ADDRESS"));
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
        assert!(result.unwrap_err().contains("QQMAIL_AUTH_CODE"));
    }

    #[test]
    fn test_validate_empty_access_token() {
        let result = validate_required("test@qq.com", "code", "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("MCP_ACCESS_TOKEN"));
    }

    #[test]
    fn test_validate_all_present() {
        let result = validate_required("test@qq.com", "code", "token");
        assert!(result.is_ok());
    }
}
