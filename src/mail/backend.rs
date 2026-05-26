use crate::config::{MailAccountConfig, MailProvider};
use crate::error::MailError;
use crate::mail::{imap, providers, smtp};
use async_trait::async_trait;
use std::path::PathBuf;

#[derive(Debug)]
pub struct SendEmailRequest {
    pub to: Vec<String>,
    pub cc: Option<Vec<String>>,
    pub bcc: Option<Vec<String>>,
    pub subject: String,
    pub text: Option<String>,
    pub html: Option<String>,
}

#[derive(Debug)]
pub struct ListMessagesRequest {
    pub mailbox_id: String,
    pub limit: usize,
    pub cursor: Option<String>,
    pub order: String,
}

#[derive(Debug)]
pub struct GetMessageRequest {
    pub mailbox_id: String,
    pub message_id: String,
    pub mark_seen: bool,
}

#[derive(Debug)]
pub struct DeleteMessageRequest {
    pub mailbox_id: String,
    pub message_id: String,
    pub expunge: bool,
}

#[derive(Debug)]
pub struct MoveMessageRequest {
    pub from_mailbox_id: String,
    pub to_mailbox_id: String,
    pub message_id: String,
}

#[derive(Debug)]
pub struct MarkMessageRequest {
    pub mailbox_id: String,
    pub message_id: String,
    pub seen: Option<bool>,
    pub flagged: Option<bool>,
    pub answered: Option<bool>,
}

#[async_trait]
pub trait MailBackend: Send + Sync {
    async fn send(&self, req: SendEmailRequest) -> Result<String, MailError>;
    async fn list_mailboxes(&self) -> Result<String, MailError>;
    async fn list_messages(&self, req: ListMessagesRequest) -> Result<String, MailError>;
    async fn get(&self, req: GetMessageRequest) -> Result<String, MailError>;
    async fn delete(&self, req: DeleteMessageRequest) -> Result<String, MailError>;
    async fn move_message(&self, req: MoveMessageRequest) -> Result<String, MailError>;
    async fn mark(&self, req: MarkMessageRequest) -> Result<String, MailError>;
}

pub fn for_account(account: MailAccountConfig, token_store_path: PathBuf) -> Box<dyn MailBackend> {
    match account.provider {
        MailProvider::Qq => Box::new(QqBackend { account }),
        MailProvider::Gmail => Box::new(providers::GmailBackend::new(account, token_store_path)),
        MailProvider::Outlook => {
            Box::new(providers::OutlookBackend::new(account, token_store_path))
        }
    }
}

struct QqBackend {
    account: MailAccountConfig,
}

#[async_trait]
impl MailBackend for QqBackend {
    async fn send(&self, req: SendEmailRequest) -> Result<String, MailError> {
        let mailer = smtp::build_mailer(&self.account);
        let (accepted, message_id) = smtp::send_email(
            &mailer,
            &self.account,
            smtp::SendEmailRequest {
                to: req.to,
                cc: req.cc,
                bcc: req.bcc,
                subject: req.subject,
                text: req.text,
                html: req.html,
            },
        )
        .await?;
        Ok(serde_json::json!({
            "provider": "qq",
            "account": self.account.id,
            "accepted": accepted,
            "message_id": message_id,
        })
        .to_string())
    }

    async fn list_mailboxes(&self) -> Result<String, MailError> {
        let value: serde_json::Value =
            serde_json::from_str(&imap::list_mailboxes(&self.account).await?)
                .map_err(|e| MailError::ProviderApiError(e.to_string()))?;
        Ok(serde_json::json!({
            "provider": "qq",
            "account": self.account.id,
            "mailboxes": value.get("mailboxes").cloned().unwrap_or_default(),
        })
        .to_string())
    }

    async fn list_messages(&self, req: ListMessagesRequest) -> Result<String, MailError> {
        let offset = req.cursor.as_deref().map(parse_cursor).transpose()?;
        let value: serde_json::Value = serde_json::from_str(
            &imap::list_messages(
                &self.account,
                imap::ListMessagesRequest {
                    mailbox: req.mailbox_id.clone(),
                    limit: req.limit,
                    offset,
                    order: req.order,
                },
            )
            .await?,
        )
        .map_err(|e| MailError::ProviderApiError(e.to_string()))?;
        let messages = value
            .get("messages")
            .and_then(|messages| messages.as_array())
            .map(|messages| {
                messages
                    .iter()
                    .map(|message| normalize_qq_summary(&self.account, &req.mailbox_id, message))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(serde_json::json!({
            "provider": "qq",
            "account": self.account.id,
            "mailbox_id": req.mailbox_id,
            "messages": messages,
            "cursor": value.get("next_offset").and_then(|offset| offset.as_u64()).map(|offset| offset.to_string()),
        })
        .to_string())
    }

    async fn get(&self, req: GetMessageRequest) -> Result<String, MailError> {
        let uid = parse_message_id(&req.message_id)?;
        let value: serde_json::Value = serde_json::from_str(
            &imap::get_message(
                &self.account,
                imap::GetMessageRequest {
                    mailbox: req.mailbox_id.clone(),
                    uid,
                    mark_seen: req.mark_seen,
                },
            )
            .await?,
        )
        .map_err(|e| MailError::ProviderApiError(e.to_string()))?;
        Ok(
            normalize_qq_message(&self.account, &req.mailbox_id, &req.message_id, uid, &value)
                .to_string(),
        )
    }

    async fn delete(&self, req: DeleteMessageRequest) -> Result<String, MailError> {
        let uid = parse_message_id(&req.message_id)?;
        imap::delete_message(
            &self.account,
            imap::DeleteMessageRequest {
                mailbox: req.mailbox_id,
                uid,
                expunge: req.expunge,
            },
        )
        .await
    }

    async fn move_message(&self, req: MoveMessageRequest) -> Result<String, MailError> {
        let uid = parse_message_id(&req.message_id)?;
        imap::move_message(
            &self.account,
            imap::MoveMessageRequest {
                from_mailbox: req.from_mailbox_id,
                to_mailbox: req.to_mailbox_id,
                uid,
            },
        )
        .await
    }

    async fn mark(&self, req: MarkMessageRequest) -> Result<String, MailError> {
        let uid = parse_message_id(&req.message_id)?;
        imap::mark_message(
            &self.account,
            imap::MarkMessageRequest {
                mailbox: req.mailbox_id,
                uid,
                seen: req.seen,
                flagged: req.flagged,
                answered: req.answered,
            },
        )
        .await
    }
}

fn parse_cursor(cursor: &str) -> Result<usize, MailError> {
    cursor
        .parse()
        .map_err(|_| MailError::ProviderApiError("invalid cursor".into()))
}

fn parse_message_id(message_id: &str) -> Result<u32, MailError> {
    message_id
        .parse()
        .map_err(|_| MailError::ProviderApiError("invalid QQ message_id".into()))
}

fn normalize_qq_summary(
    account: &MailAccountConfig,
    mailbox_id: &str,
    message: &serde_json::Value,
) -> serde_json::Value {
    let uid = message
        .get("uid")
        .and_then(|uid| uid.as_u64())
        .unwrap_or_default();
    serde_json::json!({
        "provider": "qq",
        "account": account.id,
        "message_id": uid.to_string(),
        "mailbox_id": mailbox_id,
        "subject": message.get("subject").cloned().unwrap_or_default(),
        "from": message.get("from").cloned().unwrap_or_default(),
        "to": message.get("to").cloned().unwrap_or_default(),
        "date": message.get("date").cloned().unwrap_or_default(),
        "flags": {
            "seen": message.get("seen").cloned().unwrap_or_default(),
            "answered": message.get("answered").cloned().unwrap_or_default(),
            "flagged": message.get("flagged").cloned().unwrap_or_default()
        },
        "labels": [],
        "has_attachments": message.get("has_attachments").cloned().unwrap_or_default(),
        "provider_metadata": { "uid": uid, "size": message.get("size").cloned().unwrap_or_default() }
    })
}

fn normalize_qq_message(
    account: &MailAccountConfig,
    mailbox_id: &str,
    message_id: &str,
    uid: u32,
    value: &serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "provider": "qq",
        "account": account.id,
        "message_id": message_id,
        "mailbox_id": mailbox_id,
        "subject": value.get("subject").cloned().unwrap_or_default(),
        "from": value.get("from").cloned().unwrap_or_default(),
        "to": value.get("to").cloned().unwrap_or_default(),
        "date": value.get("date").cloned().unwrap_or_default(),
        "flags": { "seen": value.get("seen").cloned().unwrap_or_default() },
        "labels": [],
        "has_attachments": value.get("attachments").and_then(|v| v.as_array()).map(|v| !v.is_empty()).unwrap_or(false),
        "text": value.get("text").cloned().unwrap_or_default(),
        "html": value.get("html").cloned().unwrap_or_default(),
        "attachments": value.get("attachments").cloned().unwrap_or_default(),
        "provider_metadata": { "uid": uid, "cc": value.get("cc").cloned().unwrap_or_default() }
    })
}
