use crate::config::AppConfig;
use crate::mail;
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

fn sanitize_message(message: &str) -> String {
    let sanitized = message
        .replace(&std::env::var("QQMAIL_AUTH_CODE").unwrap_or_default(), "***")
        .replace(&std::env::var("MCP_ACCESS_TOKEN").unwrap_or_default(), "***");
    if sanitized.len() > 200 {
        format!("{}...", &sanitized[..200])
    } else {
        sanitized
    }
}

fn tool_error(e: &crate::error::MailError) -> McpError {
    let (code, message, retryable) = match e {
        crate::error::MailError::Smtp(_) => (
            "smtp_error",
            "SMTP operation failed".to_string(),
            true,
        ),
        crate::error::MailError::Lettre(_) => (
            "smtp_error",
            "Failed to construct email".to_string(),
            false,
        ),
        crate::error::MailError::Imap(_) => (
            "imap_error",
            "IMAP operation failed".to_string(),
            true,
        ),
        crate::error::MailError::ImapLogin(msg) => (
            "imap_auth_failed",
            format!("IMAP authentication failed: {}", sanitize_message(msg)),
            false,
        ),
        crate::error::MailError::ImapMailboxNotFound { mailbox } => (
            "imap_mailbox_not_found",
            format!("Mailbox not found: {}", sanitize_message(mailbox)),
            false,
        ),
        crate::error::MailError::ImapMessageNotFound { mailbox, uid } => (
            "imap_uid_not_found",
            format!("Message uid={} not found in {}", uid, sanitize_message(mailbox)),
            false,
        ),
        crate::error::MailError::MimeParse(msg) => (
            "mime_parse_failed",
            format!("Failed to parse email: {}", sanitize_message(msg)),
            false,
        ),
        crate::error::MailError::AddressParse(msg) => (
            "invalid_arguments",
            format!("Invalid email address: {}", sanitize_message(msg)),
            false,
        ),
        crate::error::MailError::Tls(_) => (
            "tls_error",
            "TLS connection failed".to_string(),
            true,
        ),
        crate::error::MailError::Io(msg) => (
            "io_error",
            format!("IO error: {}", sanitize_message(&msg.to_string())),
            true,
        ),
    };

    let data = serde_json::json!({
        "code": code,
        "message": message,
        "retryable": retryable,
    });
    McpError::new(
        ErrorCode::INTERNAL_ERROR,
        message,
        Some(data),
    )
}

fn validate_uid(uid: u32) -> Result<(), McpError> {
    if uid == 0 {
        return Err(McpError::invalid_params(
            "UID must be greater than 0",
            None,
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SendEmailParams {
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
pub struct ListMessagesParams {
    #[schemars(description = "Mailbox name, default INBOX")]
    pub mailbox: Option<String>,
    #[schemars(description = "Max messages to return (default 20, max 100)")]
    pub limit: Option<usize>,
    #[schemars(description = "Offset for pagination")]
    pub offset: Option<usize>,
    #[schemars(description = "Sort order: desc or asc, default desc")]
    pub order: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetMessageParams {
    #[schemars(description = "Mailbox name, default INBOX")]
    pub mailbox: Option<String>,
    #[schemars(description = "Message UID")]
    pub uid: u32,
    #[schemars(description = "Whether to mark as seen after reading, default false")]
    pub mark_seen: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteMessageParams {
    #[schemars(description = "Mailbox name, default INBOX")]
    pub mailbox: Option<String>,
    #[schemars(description = "Message UID")]
    pub uid: u32,
    #[schemars(description = "Whether to expunge immediately, default true")]
    pub expunge: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MoveMessageParams {
    #[schemars(description = "Source mailbox name, default INBOX")]
    pub from_mailbox: Option<String>,
    #[schemars(description = "Destination mailbox name")]
    pub to_mailbox: String,
    #[schemars(description = "Message UID")]
    pub uid: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MarkMessageParams {
    #[schemars(description = "Mailbox name, default INBOX")]
    pub mailbox: Option<String>,
    #[schemars(description = "Message UID")]
    pub uid: u32,
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
        let to: Vec<String> = params.to.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
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

        let mailer = mail::smtp::build_mailer(&self.config);
        let req = mail::smtp::SendEmailRequest {
            to,
            cc: params.cc,
            bcc: params.bcc,
            subject: params.subject.trim().to_string(),
            text: params.text,
            html: params.html,
        };

        match mail::smtp::send_email(&mailer, &self.config, req).await {
            Ok((accepted, message_id)) => {
                let result = serde_json::json!({
                    "accepted": accepted,
                    "message_id": message_id,
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result).unwrap(),
                )]))
            }
            Err(e) => Err(tool_error(&e)),
        }
    }

    #[tool(description = "List all mailboxes/folders available in the QQ IMAP account.")]
    async fn list_mailboxes(
        &self,
        _params: rmcp::handler::server::wrapper::Parameters<serde_json::Value>,
    ) -> Result<CallToolResult, McpError> {
        match mail::imap::list_mailboxes(&self.config).await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Err(tool_error(&e)),
        }
    }

    #[tool(description = "List messages in a mailbox with pagination. Returns message summaries (uid, from, to, subject, date, flags) without body content.")]
    async fn list_messages(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<ListMessagesParams>,
    ) -> Result<CallToolResult, McpError> {
        let order = params.order.unwrap_or_else(|| "desc".into());
        if order != "desc" && order != "asc" {
            return Err(McpError::invalid_params(
                "order must be 'desc' or 'asc'",
                None,
            ));
        }

        let req = mail::imap::ListMessagesRequest {
            mailbox: params.mailbox.unwrap_or_else(|| "INBOX".into()),
            limit: params.limit.unwrap_or(20).min(100),
            offset: params.offset,
            order,
        };

        match mail::imap::list_messages(&self.config, req).await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Err(tool_error(&e)),
        }
    }

    #[tool(description = "Get a single email by UID. Returns full text and html body content. Set mark_seen=false (default) to read without marking as seen.")]
    async fn get_message(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<GetMessageParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_uid(params.uid)?;
        let req = mail::imap::GetMessageRequest {
            mailbox: params.mailbox.unwrap_or_else(|| "INBOX".into()),
            uid: params.uid,
            mark_seen: params.mark_seen.unwrap_or(false),
        };

        match mail::imap::get_message(&self.config, req).await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Err(tool_error(&e)),
        }
    }

    #[tool(description = "Delete a message by UID. This is a destructive action. Set expunge=false to flag for deletion without immediate removal.")]
    async fn delete_message(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<DeleteMessageParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_uid(params.uid)?;
        let req = mail::imap::DeleteMessageRequest {
            mailbox: params.mailbox.unwrap_or_else(|| "INBOX".into()),
            uid: params.uid,
            expunge: params.expunge.unwrap_or(true),
        };

        match mail::imap::delete_message(&self.config, req).await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Err(tool_error(&e)),
        }
    }

    #[tool(description = "Move a message from one mailbox to another by UID.")]
    async fn move_message(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<MoveMessageParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_uid(params.uid)?;
        let req = mail::imap::MoveMessageRequest {
            from_mailbox: params.from_mailbox.unwrap_or_else(|| "INBOX".into()),
            to_mailbox: params.to_mailbox,
            uid: params.uid,
        };

        match mail::imap::move_message(&self.config, req).await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Err(tool_error(&e)),
        }
    }

    #[tool(description = "Update flags on a message by UID. Can set seen, flagged (starred), and answered flags. Pass true/false to set, omit to leave unchanged.")]
    async fn mark_message(
        &self,
        rmcp::handler::server::wrapper::Parameters(params): rmcp::handler::server::wrapper::Parameters<MarkMessageParams>,
    ) -> Result<CallToolResult, McpError> {
        if params.seen.is_none() && params.flagged.is_none() && params.answered.is_none() {
            return Err(McpError::invalid_params(
                "At least one of seen, flagged, or answered must be specified",
                None,
            ));
        }
        validate_uid(params.uid)?;

        let req = mail::imap::MarkMessageRequest {
            mailbox: params.mailbox.unwrap_or_else(|| "INBOX".into()),
            uid: params.uid,
            seen: params.seen,
            flagged: params.flagged,
            answered: params.answered,
        };

        match mail::imap::mark_message(&self.config, req).await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Err(tool_error(&e)),
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
                "QQ Mail MCP server. Tools: send_email (real send), list_mailboxes, list_messages, get_message, delete_message, move_message, mark_message. All operations use a single QQ account configured via .env.".to_string(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_uid_zero_rejected() {
        assert!(validate_uid(0).is_err());
    }

    #[test]
    fn test_validate_uid_one_accepted() {
        assert!(validate_uid(1).is_ok());
    }

    #[test]
    fn test_validate_uid_large_accepted() {
        assert!(validate_uid(999999).is_ok());
    }

    #[test]
    fn test_validate_uid_zero_returns_invalid_params() {
        let err = validate_uid(0).unwrap_err();
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("invalid params") || json.contains("-32602"));
    }
}
