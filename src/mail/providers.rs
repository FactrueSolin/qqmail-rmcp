use crate::config::MailAccountConfig;
use crate::error::MailError;
use crate::mail::backend::{
    DeleteMessageRequest, GetMessageRequest, ListMessagesRequest, MailBackend, MarkMessageRequest,
    MoveMessageRequest, SendEmailRequest,
};
use crate::mail::oauth::AccessTokenProvider;
use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde_json::json;
use std::path::PathBuf;

const GMAIL_MODIFY_SCOPE: &str = "https://www.googleapis.com/auth/gmail.modify";
const GMAIL_SEND_SCOPE: &str = "https://www.googleapis.com/auth/gmail.send";
const GRAPH_READWRITE_SCOPE: &str = "Mail.ReadWrite";
const GRAPH_SEND_SCOPE: &str = "Mail.Send";

pub struct GmailBackend {
    account: MailAccountConfig,
    client: reqwest::Client,
    tokens: AccessTokenProvider,
}

pub struct OutlookBackend {
    account: MailAccountConfig,
    client: reqwest::Client,
    tokens: AccessTokenProvider,
}

impl GmailBackend {
    pub fn new(account: MailAccountConfig, token_store_path: PathBuf) -> Self {
        Self {
            account,
            client: reqwest::Client::new(),
            tokens: AccessTokenProvider::new(token_store_path),
        }
    }
}

impl OutlookBackend {
    pub fn new(account: MailAccountConfig, token_store_path: PathBuf) -> Self {
        Self {
            account,
            client: reqwest::Client::new(),
            tokens: AccessTokenProvider::new(token_store_path),
        }
    }
}

#[async_trait]
impl MailBackend for GmailBackend {
    async fn send(&self, req: SendEmailRequest) -> Result<String, MailError> {
        let token = self.tokens.get(&self.account, &[GMAIL_SEND_SCOPE]).await?;
        let raw = URL_SAFE_NO_PAD.encode(build_rfc822_message(&self.account, &req)?);
        let value = self
            .post_json(
                "https://gmail.googleapis.com/gmail/v1/users/me/messages/send",
                &token,
                json!({ "raw": raw }),
            )
            .await?;
        Ok(json!({
            "provider": "gmail",
            "account": self.account.id,
            "accepted": true,
            "message_id": value.get("id").cloned().unwrap_or_default(),
            "provider_metadata": value,
        })
        .to_string())
    }

    async fn list_mailboxes(&self) -> Result<String, MailError> {
        let token = self
            .tokens
            .get(&self.account, &[GMAIL_MODIFY_SCOPE])
            .await?;
        let value = self
            .get_json(
                "https://gmail.googleapis.com/gmail/v1/users/me/labels",
                &token,
            )
            .await?;
        let mailboxes = value
            .get("labels")
            .and_then(|labels| labels.as_array())
            .map(|labels| {
                labels
                    .iter()
                    .map(|label| {
                        json!({
                            "id": label.get("id").cloned().unwrap_or_default(),
                            "name": label.get("name").cloned().unwrap_or_default(),
                            "type": label.get("type").cloned().unwrap_or_default(),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(
            json!({ "provider": "gmail", "account": self.account.id, "mailboxes": mailboxes })
                .to_string(),
        )
    }

    async fn list_messages(&self, req: ListMessagesRequest) -> Result<String, MailError> {
        let token = self
            .tokens
            .get(&self.account, &[GMAIL_MODIFY_SCOPE])
            .await?;
        let mut url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/messages?labelIds={}&maxResults={}",
            req.mailbox_id, req.limit
        );
        if let Some(cursor) = req.cursor {
            url.push_str("&pageToken=");
            url.push_str(&cursor);
        }
        let value = self.get_json(&url, &token).await?;
        let messages = value
            .get("messages")
            .and_then(|messages| messages.as_array())
            .map(|messages| {
                messages
                    .iter()
                    .map(|message| {
                        json!({
                            "provider": "gmail",
                            "account": self.account.id,
                            "message_id": message.get("id").cloned().unwrap_or_default(),
                            "mailbox_id": req.mailbox_id,
                            "subject": null,
                            "from": null,
                            "to": null,
                            "date": null,
                            "flags": {},
                            "labels": [],
                            "has_attachments": null,
                            "provider_metadata": message,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(json!({
            "provider": "gmail",
            "account": self.account.id,
            "mailbox_id": req.mailbox_id,
            "messages": messages,
            "cursor": value.get("nextPageToken").cloned().unwrap_or_default(),
        })
        .to_string())
    }

    async fn get(&self, req: GetMessageRequest) -> Result<String, MailError> {
        let token = self
            .tokens
            .get(&self.account, &[GMAIL_MODIFY_SCOPE])
            .await?;
        let value = self
            .get_json(
                &format!(
                    "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=full",
                    req.message_id
                ),
                &token,
            )
            .await?;
        if req.mark_seen {
            self.post_json(
                &format!(
                    "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/modify",
                    req.message_id
                ),
                &token,
                gmail_mark_seen_body(),
            )
            .await?;
        }
        Ok(normalize_gmail_message(&self.account, &req.mailbox_id, &value).to_string())
    }

    async fn delete(&self, req: DeleteMessageRequest) -> Result<String, MailError> {
        let token = self
            .tokens
            .get(&self.account, &[GMAIL_MODIFY_SCOPE])
            .await?;
        let value = self
            .post_json(
                &format!(
                    "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/trash",
                    req.message_id
                ),
                &token,
                json!({}),
            )
            .await?;
        Ok(json!({ "deleted": true, "message_id": req.message_id, "mailbox_id": req.mailbox_id, "provider_metadata": value }).to_string())
    }

    async fn move_message(&self, req: MoveMessageRequest) -> Result<String, MailError> {
        let token = self
            .tokens
            .get(&self.account, &[GMAIL_MODIFY_SCOPE])
            .await?;
        let value = self
            .post_json(
                &format!(
                    "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/modify",
                    req.message_id
                ),
                &token,
                json!({ "addLabelIds": [req.to_mailbox_id], "removeLabelIds": [req.from_mailbox_id] }),
            )
            .await?;
        Ok(
            json!({ "moved": true, "message_id": req.message_id, "provider_metadata": value })
                .to_string(),
        )
    }

    async fn mark(&self, req: MarkMessageRequest) -> Result<String, MailError> {
        let token = self
            .tokens
            .get(&self.account, &[GMAIL_MODIFY_SCOPE])
            .await?;
        let mut add = Vec::new();
        let mut remove = Vec::new();
        push_label(req.seen, "UNREAD", &mut remove, &mut add);
        push_label(req.flagged, "STARRED", &mut add, &mut remove);
        let value = self
            .post_json(
                &format!(
                    "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/modify",
                    req.message_id
                ),
                &token,
                json!({ "addLabelIds": add, "removeLabelIds": remove }),
            )
            .await?;
        Ok(json!({ "updated": true, "message_id": req.message_id, "mailbox_id": req.mailbox_id, "provider_metadata": value }).to_string())
    }
}

#[async_trait]
impl MailBackend for OutlookBackend {
    async fn send(&self, req: SendEmailRequest) -> Result<String, MailError> {
        let token = self.tokens.get(&self.account, &[GRAPH_SEND_SCOPE]).await?;
        self.post_json(
            "https://graph.microsoft.com/v1.0/me/sendMail",
            &token,
            graph_message(&req),
        )
        .await?;
        Ok(json!({ "provider": "outlook", "account": self.account.id, "accepted": true, "message_id": null }).to_string())
    }

    async fn list_mailboxes(&self) -> Result<String, MailError> {
        let token = self
            .tokens
            .get(&self.account, &[GRAPH_READWRITE_SCOPE])
            .await?;
        let value = self
            .get_json("https://graph.microsoft.com/v1.0/me/mailFolders", &token)
            .await?;
        Ok(json!({ "provider": "outlook", "account": self.account.id, "mailboxes": value.get("value").cloned().unwrap_or_default() }).to_string())
    }

    async fn list_messages(&self, req: ListMessagesRequest) -> Result<String, MailError> {
        let token = self
            .tokens
            .get(&self.account, &[GRAPH_READWRITE_SCOPE])
            .await?;
        let url = match req.cursor {
            Some(cursor) => decode_graph_cursor(&cursor)?,
            None => format!(
                "https://graph.microsoft.com/v1.0/me/mailFolders/{}/messages?$top={}",
                req.mailbox_id, req.limit
            ),
        };
        let value = self.get_json(&url, &token).await?;
        let messages = value
            .get("value")
            .and_then(|messages| messages.as_array())
            .map(|messages| {
                messages
                    .iter()
                    .map(|message| normalize_graph_summary(&self.account, &req.mailbox_id, message))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(json!({
            "provider": "outlook",
            "account": self.account.id,
            "mailbox_id": req.mailbox_id,
            "messages": messages,
            "cursor": value.get("@odata.nextLink").and_then(|v| v.as_str()).map(encode_graph_cursor),
        })
        .to_string())
    }

    async fn get(&self, req: GetMessageRequest) -> Result<String, MailError> {
        let token = self
            .tokens
            .get(&self.account, &[GRAPH_READWRITE_SCOPE])
            .await?;
        let value = self
            .get_json(
                &format!(
                    "https://graph.microsoft.com/v1.0/me/messages/{}",
                    req.message_id
                ),
                &token,
            )
            .await?;
        if req.mark_seen {
            self.patch_json(
                &format!(
                    "https://graph.microsoft.com/v1.0/me/messages/{}",
                    req.message_id
                ),
                &token,
                graph_mark_seen_body(),
            )
            .await?;
        }
        Ok(normalize_graph_message(&self.account, &req.mailbox_id, &value).to_string())
    }

    async fn delete(&self, req: DeleteMessageRequest) -> Result<String, MailError> {
        let token = self
            .tokens
            .get(&self.account, &[GRAPH_READWRITE_SCOPE])
            .await?;
        self.post_json(
            &format!(
                "https://graph.microsoft.com/v1.0/me/messages/{}/move",
                req.message_id
            ),
            &token,
            json!({ "destinationId": "deleteditems" }),
        )
        .await?;
        Ok(
            json!({ "deleted": true, "message_id": req.message_id, "mailbox_id": req.mailbox_id })
                .to_string(),
        )
    }

    async fn move_message(&self, req: MoveMessageRequest) -> Result<String, MailError> {
        let token = self
            .tokens
            .get(&self.account, &[GRAPH_READWRITE_SCOPE])
            .await?;
        let value = self
            .post_json(
                &format!(
                    "https://graph.microsoft.com/v1.0/me/messages/{}/move",
                    req.message_id
                ),
                &token,
                json!({ "destinationId": req.to_mailbox_id }),
            )
            .await?;
        Ok(
            json!({ "moved": true, "message_id": req.message_id, "provider_metadata": value })
                .to_string(),
        )
    }

    async fn mark(&self, req: MarkMessageRequest) -> Result<String, MailError> {
        let token = self
            .tokens
            .get(&self.account, &[GRAPH_READWRITE_SCOPE])
            .await?;
        let mut patch = serde_json::Map::new();
        if let Some(seen) = req.seen {
            patch.insert("isRead".into(), json!(seen));
        }
        if let Some(flagged) = req.flagged {
            patch.insert(
                "flag".into(),
                json!({ "flagStatus": if flagged { "flagged" } else { "notFlagged" } }),
            );
        }
        let value = self
            .patch_json(
                &format!(
                    "https://graph.microsoft.com/v1.0/me/messages/{}",
                    req.message_id
                ),
                &token,
                serde_json::Value::Object(patch),
            )
            .await?;
        Ok(json!({ "updated": true, "message_id": req.message_id, "mailbox_id": req.mailbox_id, "provider_metadata": value }).to_string())
    }
}

impl GmailBackend {
    async fn get_json(&self, url: &str, token: &str) -> Result<serde_json::Value, MailError> {
        provider_json(self.client.get(url).bearer_auth(token)).await
    }

    async fn post_json(
        &self,
        url: &str,
        token: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, MailError> {
        provider_json(self.client.post(url).bearer_auth(token).json(&body)).await
    }
}

impl OutlookBackend {
    async fn get_json(&self, url: &str, token: &str) -> Result<serde_json::Value, MailError> {
        provider_json(self.client.get(url).bearer_auth(token)).await
    }

    async fn post_json(
        &self,
        url: &str,
        token: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, MailError> {
        provider_json(self.client.post(url).bearer_auth(token).json(&body)).await
    }

    async fn patch_json(
        &self,
        url: &str,
        token: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, MailError> {
        provider_json(self.client.patch(url).bearer_auth(token).json(&body)).await
    }
}

async fn provider_json(builder: reqwest::RequestBuilder) -> Result<serde_json::Value, MailError> {
    let response = builder
        .send()
        .await
        .map_err(|e| MailError::ProviderApiError(e.to_string()))?;
    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(MailError::ProviderRateLimited);
    }
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(MailError::ReauthorizationRequired);
    }
    if response.status() == reqwest::StatusCode::FORBIDDEN {
        return Err(MailError::InsufficientScope(
            "provider rejected requested operation".into(),
        ));
    }
    if !response.status().is_success() {
        return Err(MailError::ProviderApiError(response.status().to_string()));
    }
    if response.status() == reqwest::StatusCode::NO_CONTENT {
        return Ok(json!({}));
    }
    response
        .json()
        .await
        .map_err(|e| MailError::ProviderApiError(e.to_string()))
}

fn build_rfc822_message(
    account: &MailAccountConfig,
    req: &SendEmailRequest,
) -> Result<String, MailError> {
    let from = account.address.as_deref().unwrap_or("me");
    reject_header_injection("from", from)?;
    reject_header_injection("subject", &req.subject)?;
    for address in req
        .to
        .iter()
        .chain(req.cc.iter().flatten())
        .chain(req.bcc.iter().flatten())
    {
        reject_header_injection("recipient", address)?;
    }
    let mut message = format!(
        "From: {}\r\nTo: {}\r\nSubject: {}\r\n",
        from,
        req.to.join(", "),
        encode_header_value(&req.subject)
    );
    if let Some(cc) = &req.cc {
        message.push_str(&format!("Cc: {}\r\n", cc.join(", ")));
    }
    message.push_str("MIME-Version: 1.0\r\n");
    if let Some(html) = &req.html {
        message.push_str("Content-Type: text/html; charset=utf-8\r\n\r\n");
        message.push_str(html);
    } else {
        message.push_str("Content-Type: text/plain; charset=utf-8\r\n\r\n");
        message.push_str(req.text.as_deref().unwrap_or_default());
    }
    Ok(message)
}

fn reject_header_injection(field: &str, value: &str) -> Result<(), MailError> {
    if value.contains('\r') || value.contains('\n') {
        return Err(MailError::ProviderApiError(format!(
            "{} must not contain CR or LF",
            field
        )));
    }
    Ok(())
}

fn encode_header_value(value: &str) -> String {
    if value.is_ascii() {
        value.to_string()
    } else {
        format!("=?UTF-8?B?{}?=", STANDARD.encode(value.as_bytes()))
    }
}

fn encode_graph_cursor(next_link: &str) -> String {
    URL_SAFE_NO_PAD.encode(next_link.as_bytes())
}

fn decode_graph_cursor(cursor: &str) -> Result<String, MailError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| MailError::ProviderApiError("invalid Graph cursor".into()))?;
    let next_link = String::from_utf8(bytes)
        .map_err(|_| MailError::ProviderApiError("invalid Graph cursor".into()))?;
    validate_graph_next_link(&next_link)?;
    Ok(next_link)
}

fn validate_graph_next_link(next_link: &str) -> Result<(), MailError> {
    let url = reqwest::Url::parse(next_link)
        .map_err(|_| MailError::ProviderApiError("invalid Graph cursor".into()))?;
    let valid = url.scheme() == "https"
        && url.host_str() == Some("graph.microsoft.com")
        && url.path().starts_with("/v1.0/me/mailFolders/")
        && url.path().ends_with("/messages");
    if valid {
        Ok(())
    } else {
        Err(MailError::ProviderApiError("invalid Graph cursor".into()))
    }
}

fn graph_message(req: &SendEmailRequest) -> serde_json::Value {
    let body = req
        .html
        .as_deref()
        .or(req.text.as_deref())
        .unwrap_or_default();
    json!({
        "message": {
            "subject": req.subject,
            "body": { "contentType": if req.html.is_some() { "HTML" } else { "Text" }, "content": body },
            "toRecipients": req.to.iter().map(|address| json!({ "emailAddress": { "address": address } })).collect::<Vec<_>>(),
            "ccRecipients": req.cc.clone().unwrap_or_default().iter().map(|address| json!({ "emailAddress": { "address": address } })).collect::<Vec<_>>(),
            "bccRecipients": req.bcc.clone().unwrap_or_default().iter().map(|address| json!({ "emailAddress": { "address": address } })).collect::<Vec<_>>()
        },
        "saveToSentItems": true
    })
}

fn gmail_mark_seen_body() -> serde_json::Value {
    json!({ "removeLabelIds": ["UNREAD"] })
}

fn graph_mark_seen_body() -> serde_json::Value {
    json!({ "isRead": true })
}

fn push_label(
    desired: Option<bool>,
    label: &'static str,
    add: &mut Vec<&'static str>,
    remove: &mut Vec<&'static str>,
) {
    match desired {
        Some(true) => add.push(label),
        Some(false) => remove.push(label),
        None => {}
    }
}

fn normalize_gmail_message(
    account: &MailAccountConfig,
    mailbox_id: &str,
    value: &serde_json::Value,
) -> serde_json::Value {
    let headers = value
        .pointer("/payload/headers")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    json!({
        "provider": "gmail",
        "account": account.id,
        "message_id": value.get("id").cloned().unwrap_or_default(),
        "mailbox_id": mailbox_id,
        "subject": header(&headers, "Subject"),
        "from": header(&headers, "From"),
        "to": header(&headers, "To"),
        "date": header(&headers, "Date"),
        "flags": {},
        "labels": value.get("labelIds").cloned().unwrap_or_default(),
        "has_attachments": false,
        "provider_metadata": value,
    })
}

fn normalize_graph_summary(
    account: &MailAccountConfig,
    mailbox_id: &str,
    value: &serde_json::Value,
) -> serde_json::Value {
    json!({
        "provider": "outlook",
        "account": account.id,
        "message_id": value.get("id").cloned().unwrap_or_default(),
        "mailbox_id": mailbox_id,
        "subject": value.get("subject").cloned().unwrap_or_default(),
        "from": value.pointer("/from/emailAddress/address").cloned().unwrap_or_default(),
        "to": value.get("toRecipients").cloned().unwrap_or_default(),
        "date": value.get("receivedDateTime").cloned().unwrap_or_default(),
        "flags": { "seen": value.get("isRead").cloned().unwrap_or_default(), "flag": value.get("flag").cloned().unwrap_or_default() },
        "labels": [],
        "has_attachments": value.get("hasAttachments").cloned().unwrap_or_default(),
        "provider_metadata": value,
    })
}

fn normalize_graph_message(
    account: &MailAccountConfig,
    mailbox_id: &str,
    value: &serde_json::Value,
) -> serde_json::Value {
    let mut summary = normalize_graph_summary(account, mailbox_id, value);
    if let Some(object) = summary.as_object_mut() {
        object.insert(
            "text".into(),
            value.pointer("/body/content").cloned().unwrap_or_default(),
        );
        object.insert(
            "html".into(),
            value.pointer("/body/content").cloned().unwrap_or_default(),
        );
    }
    summary
}

fn header(headers: &[serde_json::Value], name: &str) -> serde_json::Value {
    headers
        .iter()
        .find(|header| {
            header
                .get("name")
                .and_then(|v| v.as_str())
                .is_some_and(|value| value.eq_ignore_ascii_case(name))
        })
        .and_then(|header| header.get("value"))
        .cloned()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MailProvider;

    fn gmail_account() -> MailAccountConfig {
        MailAccountConfig {
            id: "gmail".into(),
            provider: MailProvider::Gmail,
            address: Some("sender@example.com".into()),
            auth_code: None,
            smtp: None,
            imap: None,
            oauth: None,
        }
    }

    fn send_req(subject: &str) -> SendEmailRequest {
        SendEmailRequest {
            to: vec!["to@example.com".into()],
            cc: None,
            bcc: None,
            subject: subject.into(),
            text: Some("body".into()),
            html: None,
        }
    }

    #[test]
    fn gmail_mime_rejects_subject_header_injection() {
        let err = build_rfc822_message(
            &gmail_account(),
            &send_req("hello\r\nBcc: attacker@example.com"),
        )
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("subject must not contain CR or LF")
        );
    }

    #[test]
    fn gmail_mime_rejects_recipient_header_injection() {
        let mut req = send_req("hello");
        req.to = vec!["victim@example.com\nBcc: attacker@example.com".into()];
        let err = build_rfc822_message(&gmail_account(), &req).unwrap_err();
        assert!(
            err.to_string()
                .contains("recipient must not contain CR or LF")
        );
    }

    #[test]
    fn gmail_mime_encodes_non_ascii_subject() {
        let message = build_rfc822_message(&gmail_account(), &send_req("你好")).unwrap();
        assert!(message.contains("Subject: =?UTF-8?B?"));
        assert!(!message.contains("Subject: 你好"));
    }

    #[test]
    fn graph_cursor_is_opaque_and_validates_host() {
        let link = "https://graph.microsoft.com/v1.0/me/mailFolders/inbox/messages?$skiptoken=abc";
        let cursor = encode_graph_cursor(link);
        assert_ne!(cursor, link);
        assert_eq!(decode_graph_cursor(&cursor).unwrap(), link);

        let attacker =
            encode_graph_cursor("https://attacker.example/v1.0/me/mailFolders/inbox/messages");
        assert!(decode_graph_cursor(&attacker).is_err());
    }

    #[test]
    fn mark_seen_bodies_match_provider_apis() {
        assert_eq!(
            gmail_mark_seen_body(),
            json!({ "removeLabelIds": ["UNREAD"] })
        );
        assert_eq!(graph_mark_seen_body(), json!({ "isRead": true }));
    }
}
