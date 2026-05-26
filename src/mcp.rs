use crate::config::{AppConfig, MailAccountConfig};
use crate::mail::backend;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{
    CallToolResult, Content, ErrorCode, Implementation, ProtocolVersion, ServerCapabilities,
    ServerInfo,
};
use rmcp::{ErrorData as McpError, ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone)]
pub struct QqMailServer {
    pub config: Arc<AppConfig>,
    pub tool_router: ToolRouter<QqMailServer>,
}

fn sanitize_message_with_secrets<'a>(
    message: &str,
    secrets: impl IntoIterator<Item = &'a str>,
) -> String {
    let mut sanitized = message.to_string();
    for value in secrets {
        if !value.is_empty() {
            sanitized = sanitized.replace(value, "***");
        }
    }
    if sanitized.len() > 200 {
        format!("{}...", &sanitized[..200])
    } else {
        sanitized
    }
}

fn tool_error(config: &AppConfig, e: &crate::error::MailError) -> McpError {
    let mut secrets: Vec<&str> = config.auth_codes().collect();
    secrets.push(config.mcp_access_token.as_str());
    let (code, message, retryable) = match e {
        crate::error::MailError::Smtp(_) => {
            ("smtp_error", "SMTP operation failed".to_string(), true)
        }
        crate::error::MailError::AccountNotFound(account) => (
            "account_not_found",
            format!("Unknown account: {}", account),
            false,
        ),
        crate::error::MailError::OAuthNotAuthorized => (
            "oauth_not_authorized",
            "OAuth authorization is required".to_string(),
            false,
        ),
        crate::error::MailError::ReauthorizationRequired => (
            "reauthorization_required",
            "OAuth reauthorization is required".to_string(),
            false,
        ),
        crate::error::MailError::InsufficientScope(scope) => (
            "insufficient_scope",
            format!("OAuth token is missing required scope: {}", scope),
            false,
        ),
        crate::error::MailError::ProviderRateLimited => (
            "provider_rate_limited",
            "Provider rate limited the request".to_string(),
            true,
        ),
        crate::error::MailError::ProviderApiError(msg) => (
            "provider_api_error",
            format!(
                "Provider API error: {}",
                sanitize_message_with_secrets(msg, secrets.iter().copied())
            ),
            true,
        ),
        crate::error::MailError::Lettre(_) => {
            ("smtp_error", "Failed to construct email".to_string(), false)
        }
        crate::error::MailError::Imap(_) => {
            ("imap_error", "IMAP operation failed".to_string(), true)
        }
        crate::error::MailError::ImapLogin(msg) => (
            "imap_auth_failed",
            format!(
                "IMAP authentication failed: {}",
                sanitize_message_with_secrets(msg, secrets.iter().copied())
            ),
            false,
        ),
        crate::error::MailError::ImapMailboxNotFound { mailbox } => (
            "imap_mailbox_not_found",
            format!(
                "Mailbox not found: {}",
                sanitize_message_with_secrets(mailbox, secrets.iter().copied())
            ),
            false,
        ),
        crate::error::MailError::ImapMessageNotFound { mailbox, uid } => (
            "imap_uid_not_found",
            format!(
                "Message uid={} not found in {}",
                uid,
                sanitize_message_with_secrets(mailbox, secrets.iter().copied())
            ),
            false,
        ),
        crate::error::MailError::MimeParse(msg) => (
            "mime_parse_failed",
            format!(
                "Failed to parse email: {}",
                sanitize_message_with_secrets(msg, secrets.iter().copied())
            ),
            false,
        ),
        crate::error::MailError::AddressParse(msg) => (
            "invalid_arguments",
            format!(
                "Invalid email address: {}",
                sanitize_message_with_secrets(msg, secrets.iter().copied())
            ),
            false,
        ),
        crate::error::MailError::Tls(_) => ("tls_error", "TLS connection failed".to_string(), true),
        crate::error::MailError::Io(msg) => (
            "io_error",
            format!(
                "IO error: {}",
                sanitize_message_with_secrets(&msg.to_string(), secrets.iter().copied())
            ),
            true,
        ),
    };

    let data = serde_json::json!({
        "code": code,
        "message": message,
        "retryable": retryable,
    });
    McpError::new(ErrorCode::INTERNAL_ERROR, message, Some(data))
}

fn validate_message_id(message_id: &str) -> Result<(), McpError> {
    if message_id.trim().is_empty() {
        return Err(McpError::invalid_params("message_id is required", None));
    }
    Ok(())
}

fn validate_account_id_param(account: &str) -> Result<(), McpError> {
    if account.trim().is_empty() {
        return Err(McpError::invalid_params("account is required", None));
    }
    Ok(())
}

fn resolve_account(config: &AppConfig, account: &str) -> Result<MailAccountConfig, McpError> {
    validate_account_id_param(account)?;
    config.account(account.trim()).cloned().ok_or_else(|| {
        tool_error(
            config,
            &crate::error::MailError::AccountNotFound(account.into()),
        )
    })
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SendEmailParams {
    #[schemars(description = "Configured account id. Required; no default account is used.")]
    pub account: String,
    #[schemars(description = "List of recipient email addresses (at least one required)")]
    pub to: Vec<String>,
    #[schemars(description = "Optional CC recipients")]
    pub cc: Option<Vec<String>>,
    #[schemars(description = "Optional BCC recipients")]
    pub bcc: Option<Vec<String>>,
    #[schemars(description = "Email subject line")]
    pub subject: String,
    #[schemars(description = "Plain text body")]
    pub text: Option<String>,
    #[schemars(description = "HTML body")]
    pub html: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListMailboxesParams {
    #[schemars(description = "Configured account id. Required; no default account is used.")]
    pub account: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListMessagesParams {
    #[schemars(description = "Configured account id. Required; no default account is used.")]
    pub account: String,
    #[schemars(description = "Mailbox name, default INBOX")]
    pub mailbox: Option<String>,
    #[schemars(description = "Max messages to return (default 20, max 100)")]
    pub limit: Option<usize>,
    #[schemars(description = "Opaque cursor for pagination")]
    pub cursor: Option<String>,
    #[schemars(description = "Deprecated QQ-only offset; use cursor")]
    pub offset: Option<usize>,
    #[schemars(description = "Sort order: desc or asc, default desc")]
    pub order: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetMessageParams {
    #[schemars(description = "Configured account id. Required; no default account is used.")]
    pub account: String,
    #[schemars(description = "Mailbox name, default INBOX")]
    pub mailbox: Option<String>,
    #[serde(alias = "uid")]
    #[schemars(description = "Opaque provider message id")]
    pub message_id: String,
    #[schemars(description = "Whether to mark as seen after reading, default false")]
    pub mark_seen: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteMessageParams {
    #[schemars(description = "Configured account id. Required; no default account is used.")]
    pub account: String,
    #[schemars(description = "Mailbox name, default INBOX")]
    pub mailbox: Option<String>,
    #[serde(alias = "uid")]
    #[schemars(description = "Opaque provider message id")]
    pub message_id: String,
    #[schemars(description = "Whether to expunge immediately, default true")]
    pub expunge: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MoveMessageParams {
    #[schemars(description = "Configured account id. Required; no default account is used.")]
    pub account: String,
    #[schemars(description = "Source mailbox name, default INBOX")]
    pub from_mailbox: Option<String>,
    #[schemars(description = "Destination mailbox name")]
    pub to_mailbox: String,
    #[serde(alias = "uid")]
    #[schemars(description = "Opaque provider message id")]
    pub message_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MarkMessageParams {
    #[schemars(description = "Configured account id. Required; no default account is used.")]
    pub account: String,
    #[schemars(description = "Mailbox name, default INBOX")]
    pub mailbox: Option<String>,
    #[serde(alias = "uid")]
    #[schemars(description = "Opaque provider message id")]
    pub message_id: String,
    #[schemars(description = "Set seen flag")]
    pub seen: Option<bool>,
    #[schemars(description = "Set flagged/starred flag")]
    pub flagged: Option<bool>,
    #[schemars(description = "Set answered flag")]
    pub answered: Option<bool>,
}

#[tool_router]
impl QqMailServer {
    pub fn new(config: Arc<AppConfig>) -> Self {
        Self {
            config,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Send an email via QQ SMTP. This is a real send action with immediate side effects. At least one recipient (to) and subject are required. Provide text and/or html body."
    )]
    async fn send_email(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<SendEmailParams>,
    ) -> Result<CallToolResult, McpError> {
        let account = resolve_account(&self.config, &params.account)?;
        let to: Vec<String> = params
            .to
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if to.is_empty() {
            return Err(McpError::invalid_params(
                "At least one recipient is required",
                None,
            ));
        }
        if params.subject.trim().is_empty() {
            return Err(McpError::invalid_params("Subject is required", None));
        }
        if params.text.is_none() && params.html.is_none() {
            return Err(McpError::invalid_params(
                "At least one of text or html must be provided",
                None,
            ));
        }

        let backend = backend::for_account(account, self.config.token_store_path.clone());
        let req = backend::SendEmailRequest {
            to,
            cc: params.cc,
            bcc: params.bcc,
            subject: params.subject.trim().to_string(),
            text: params.text,
            html: params.html,
        };

        match backend.send(req).await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Err(tool_error(&self.config, &e)),
        }
    }

    #[tool(description = "List all mailboxes/folders available in the QQ IMAP account.")]
    async fn list_mailboxes(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<ListMailboxesParams>,
    ) -> Result<CallToolResult, McpError> {
        let account = resolve_account(&self.config, &params.account)?;
        let backend = backend::for_account(account, self.config.token_store_path.clone());
        match backend.list_mailboxes().await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Err(tool_error(&self.config, &e)),
        }
    }

    #[tool(
        description = "List messages in a mailbox with pagination. Returns message summaries (uid, from, to, subject, date, flags) without body content."
    )]
    async fn list_messages(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<ListMessagesParams>,
    ) -> Result<CallToolResult, McpError> {
        let account = resolve_account(&self.config, &params.account)?;
        let order = params.order.unwrap_or_else(|| "desc".into());
        if order != "desc" && order != "asc" {
            return Err(McpError::invalid_params(
                "order must be 'desc' or 'asc'",
                None,
            ));
        }

        let req = backend::ListMessagesRequest {
            mailbox_id: params.mailbox.unwrap_or_else(|| "INBOX".into()),
            limit: params.limit.unwrap_or(20).min(100),
            cursor: params
                .cursor
                .or_else(|| params.offset.map(|offset| offset.to_string())),
            order,
        };

        let backend = backend::for_account(account, self.config.token_store_path.clone());
        match backend.list_messages(req).await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Err(tool_error(&self.config, &e)),
        }
    }

    #[tool(
        description = "Get a single email by UID. Returns full text and html body content. Set mark_seen=false (default) to read without marking as seen."
    )]
    async fn get_message(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<GetMessageParams>,
    ) -> Result<CallToolResult, McpError> {
        let account = resolve_account(&self.config, &params.account)?;
        validate_message_id(&params.message_id)?;
        let req = backend::GetMessageRequest {
            mailbox_id: params.mailbox.unwrap_or_else(|| "INBOX".into()),
            message_id: params.message_id,
            mark_seen: params.mark_seen.unwrap_or(false),
        };

        let backend = backend::for_account(account, self.config.token_store_path.clone());
        match backend.get(req).await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Err(tool_error(&self.config, &e)),
        }
    }

    #[tool(
        description = "Delete a message by UID. This is a destructive action. Set expunge=false to flag for deletion without immediate removal."
    )]
    async fn delete_message(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<DeleteMessageParams>,
    ) -> Result<CallToolResult, McpError> {
        let account = resolve_account(&self.config, &params.account)?;
        validate_message_id(&params.message_id)?;
        let req = backend::DeleteMessageRequest {
            mailbox_id: params.mailbox.unwrap_or_else(|| "INBOX".into()),
            message_id: params.message_id,
            expunge: params.expunge.unwrap_or(true),
        };

        let backend = backend::for_account(account, self.config.token_store_path.clone());
        match backend.delete(req).await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Err(tool_error(&self.config, &e)),
        }
    }

    #[tool(description = "Move a message from one mailbox to another by UID.")]
    async fn move_message(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<MoveMessageParams>,
    ) -> Result<CallToolResult, McpError> {
        let account = resolve_account(&self.config, &params.account)?;
        validate_message_id(&params.message_id)?;
        let req = backend::MoveMessageRequest {
            from_mailbox_id: params.from_mailbox.unwrap_or_else(|| "INBOX".into()),
            to_mailbox_id: params.to_mailbox,
            message_id: params.message_id,
        };

        let backend = backend::for_account(account, self.config.token_store_path.clone());
        match backend.move_message(req).await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Err(tool_error(&self.config, &e)),
        }
    }

    #[tool(
        description = "Update flags on a message by UID. Can set seen, flagged (starred), and answered flags. Pass true/false to set, omit to leave unchanged."
    )]
    async fn mark_message(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<MarkMessageParams>,
    ) -> Result<CallToolResult, McpError> {
        let account = resolve_account(&self.config, &params.account)?;
        if params.seen.is_none() && params.flagged.is_none() && params.answered.is_none() {
            return Err(McpError::invalid_params(
                "At least one of seen, flagged, or answered must be specified",
                None,
            ));
        }
        validate_message_id(&params.message_id)?;

        let req = backend::MarkMessageRequest {
            mailbox_id: params.mailbox.unwrap_or_else(|| "INBOX".into()),
            message_id: params.message_id,
            seen: params.seen,
            flagged: params.flagged,
            answered: params.answered,
        };

        let backend = backend::for_account(account, self.config.token_store_path.clone());
        match backend.mark(req).await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Err(tool_error(&self.config, &e)),
        }
    }
}

#[tool_handler]
impl ServerHandler for QqMailServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2025_03_26,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "qqmail-rmcp".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: None,
                website_url: None,
                icons: None,
            },
            instructions: Some(
                "QQ Mail MCP server. Tools: send_email (real send), list_mailboxes, list_messages, get_message, delete_message, move_message, mark_message. All mail tools require an explicit account id from the YAML config; legacy .env fallback exposes account \"default\".".to_string(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{MailEndpointConfig, MailProvider};
    use std::collections::BTreeMap;

    fn test_config() -> Arc<AppConfig> {
        let mut accounts = BTreeMap::new();
        accounts.insert(
            "test".to_string(),
            MailAccountConfig {
                id: "test".to_string(),
                provider: MailProvider::Qq,
                address: Some("test@qq.com".to_string()),
                auth_code: Some("auth-code".to_string()),
                smtp: Some(MailEndpointConfig {
                    host: "smtp.qq.com".to_string(),
                    port: 465,
                }),
                imap: Some(MailEndpointConfig {
                    host: "imap.qq.com".to_string(),
                    port: 993,
                }),
                oauth: None,
            },
        );

        Arc::new(AppConfig {
            mcp_bind: "127.0.0.1:0".parse().unwrap(),
            mcp_access_token: "token".to_string(),
            token_store_path: std::path::PathBuf::from("tokens.json"),
            accounts,
        })
    }

    #[test]
    fn test_validate_message_id_blank_rejected() {
        assert!(validate_message_id(" ").is_err());
    }

    #[test]
    fn test_validate_message_id_accepted() {
        assert!(validate_message_id("opaque-id").is_ok());
    }

    #[test]
    fn test_validate_message_id_blank_returns_invalid_params() {
        let err = validate_message_id("").unwrap_err();
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("invalid params") || json.contains("-32602"));
    }

    #[test]
    fn test_sanitize_message_preserves_text_when_secrets_unset() {
        assert_eq!(
            sanitize_message_with_secrets("imap failed", []),
            "imap failed"
        );
    }

    #[test]
    fn test_sanitize_message_masks_configured_secrets() {
        let sanitized = sanitize_message_with_secrets(
            "secret-code and secret-token leaked",
            ["secret-code", "secret-token"],
        );

        assert_eq!(sanitized, "*** and *** leaked");
    }

    #[test]
    fn test_sanitize_message_truncates_long_messages() {
        let sanitized = sanitize_message_with_secrets(&"x".repeat(240), []);
        assert_eq!(sanitized.len(), 203);
        assert!(sanitized.ends_with("..."));
    }

    #[test]
    fn test_resolve_account_rejects_blank_account() {
        let config = test_config();
        let err = resolve_account(&config, " ").unwrap_err();
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("invalid params") || json.contains("-32602"));
    }

    #[test]
    fn test_resolve_account_rejects_unknown_account() {
        let config = test_config();
        let err = resolve_account(&config, "missing").unwrap_err();
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("Unknown account") || json.contains("-32602"));
    }

    #[test]
    fn test_resolve_account_accepts_known_account() {
        let config = test_config();
        let account = resolve_account(&config, "test").unwrap();
        assert_eq!(account.address.as_deref(), Some("test@qq.com"));
    }

    #[test]
    fn test_tool_input_schemas_are_objects() {
        let server = QqMailServer::new(test_config());

        let tools = server.tool_router.list_all();
        assert_eq!(tools.len(), 7);
        for tool in tools {
            assert_eq!(
                tool.input_schema
                    .get("type")
                    .and_then(|value| value.as_str()),
                Some("object"),
                "tool {} input schema must be an object schema",
                tool.name
            );
        }
    }

    #[test]
    fn test_tool_input_schemas_require_account() {
        let server = QqMailServer::new(test_config());

        let tools = server.tool_router.list_all();
        assert_eq!(tools.len(), 7);
        for tool in tools {
            let required = tool
                .input_schema
                .get("required")
                .and_then(|value| value.as_array())
                .unwrap_or_else(|| panic!("tool {} must declare required fields", tool.name));
            assert!(
                required
                    .iter()
                    .any(|field| field.as_str() == Some("account")),
                "tool {} must require account",
                tool.name
            );
        }
    }
}
