use crate::config::AppConfig;
use crate::error::MailError;
use mailparse::ParsedMail;
use serde::Deserialize;

type ImapSession = imap::Session<native_tls::TlsStream<std::net::TcpStream>>;

#[derive(Debug, Deserialize)]
pub struct ListMessagesRequest {
    #[serde(default = "default_mailbox")]
    pub mailbox: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default = "default_order")]
    pub order: String,
}

fn default_mailbox() -> String {
    "INBOX".into()
}
fn default_limit() -> usize {
    20
}
fn default_order() -> String {
    "desc".into()
}

#[derive(Debug, Deserialize)]
pub struct GetMessageRequest {
    #[serde(default = "default_mailbox")]
    pub mailbox: String,
    pub uid: u32,
    #[serde(default)]
    pub mark_seen: bool,
}

#[derive(Debug, Deserialize)]
pub struct DeleteMessageRequest {
    #[serde(default = "default_mailbox")]
    pub mailbox: String,
    pub uid: u32,
    #[serde(default = "default_expunge")]
    pub expunge: bool,
}

fn default_expunge() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct MoveMessageRequest {
    #[serde(default = "default_mailbox")]
    pub from_mailbox: String,
    pub to_mailbox: String,
    pub uid: u32,
}

#[derive(Debug, Deserialize)]
pub struct MarkMessageRequest {
    #[serde(default = "default_mailbox")]
    pub mailbox: String,
    pub uid: u32,
    #[serde(default)]
    pub seen: Option<bool>,
    #[serde(default)]
    pub flagged: Option<bool>,
    #[serde(default)]
    pub answered: Option<bool>,
}

fn connect_imap(config: &AppConfig) -> Result<ImapSession, MailError> {
    let tls = native_tls::TlsConnector::builder().build()?;
    let addr = format!("{}:{}", config.imap_host, config.imap_port);
    let session = imap::connect(&addr, &config.imap_host, &tls)
        .map_err(|e| MailError::ImapLogin(e.to_string()))?;

    session
        .login(&config.qqmail_address, &config.qqmail_auth_code)
        .map_err(|(e, _)| MailError::ImapLogin(e.to_string()))
}

pub async fn list_mailboxes(config: &AppConfig) -> Result<String, MailError> {
    let config = config.clone();
    tokio::task::spawn_blocking(move || {
        let mut session = connect_imap(&config)?;

        let mut mailboxes = Vec::new();
        let list_result = session.list(None, Some("*"))?;
        for mb in list_result.iter() {
            let name: String = mb.name().to_string();
            let delimiter: String = mb
                .delimiter()
                .unwrap_or("")
                .to_string();
            let attrs: Vec<String> = mb
                .attributes()
                .iter()
                .map(|a| format!("{:?}", a))
                .collect();

            mailboxes.push(serde_json::json!({
                "name": name,
                "delimiter": delimiter,
                "attributes": attrs,
            }));
        }

        Ok(serde_json::json!({ "mailboxes": mailboxes }).to_string())
    })
    .await
    .map_err(|e| MailError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?
}

fn get_header_value(headers: &[(String, String)], name: &str) -> String {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

fn parse_email_headers(header_bytes: &[u8]) -> Vec<(String, String)> {
    let parsed = mailparse::parse_headers(header_bytes);
    match parsed {
        Ok((headers, _)) => headers
            .iter()
            .map(|h| {
                let key = h.get_key().to_string();
                let value = h.get_value();
                (key, value)
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn flag_to_str(flag: &imap::types::Flag<'_>) -> &'static str {
    match flag {
        imap::types::Flag::Seen => "\\Seen",
        imap::types::Flag::Answered => "\\Answered",
        imap::types::Flag::Flagged => "\\Flagged",
        imap::types::Flag::Deleted => "\\Deleted",
        imap::types::Flag::Draft => "\\Draft",
        imap::types::Flag::Recent => "\\Recent",
        imap::types::Flag::Custom(_) => "\\Custom",
        _ => "\\Unknown",
    }
}

pub async fn list_messages(
    config: &AppConfig,
    req: ListMessagesRequest,
) -> Result<String, MailError> {
    let config = config.clone();
    tokio::task::spawn_blocking(move || {
        let mut session = connect_imap(&config)?;

        session
            .select(&req.mailbox)
            .map_err(|_| MailError::ImapMailboxNotFound { mailbox: req.mailbox.clone() })?;

        let uids = session.uid_search("ALL")?;
        let mut uids: Vec<u32> = uids.into_iter().collect();

        if req.order == "desc" {
            uids.sort_unstable_by(|a: &u32, b: &u32| b.cmp(a));
        } else {
            uids.sort_unstable();
        }

        let offset = req.offset.unwrap_or(0);
        let limit = req.limit.min(100);
        let end = (offset + limit).min(uids.len());
        let page_uids = &uids[offset..end];

        let mut messages = Vec::new();
        for uid in page_uids {
            let fetch_result = session
                .uid_fetch(format!("{}", uid), "UID FLAGS BODY.PEEK[HEADER]")?;

            if let Some(email) = fetch_result.into_iter().next() {
                let flags: Vec<&str> = email.flags().iter().map(flag_to_str).collect();
                let header_bytes = email.header().unwrap_or_default();
                let headers = parse_email_headers(header_bytes);

                messages.push(serde_json::json!({
                    "uid": uid,
                    "message_id": get_header_value(&headers, "Message-ID"),
                    "from": get_header_value(&headers, "From"),
                    "to": get_header_value(&headers, "To"),
                    "subject": get_header_value(&headers, "Subject"),
                    "date": get_header_value(&headers, "Date"),
                    "seen": flags.contains(&"\\Seen"),
                    "answered": flags.contains(&"\\Answered"),
                    "flagged": flags.contains(&"\\Flagged"),
                    "has_attachments": false,
                    "size": 0,
                }));
            }
        }

        let next_offset = if offset + limit < uids.len() {
            Some(offset + limit)
        } else {
            None
        };

        Ok(serde_json::json!({
            "messages": messages,
            "next_offset": next_offset,
        })
        .to_string())
    })
    .await
    .map_err(|e| MailError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?
}

fn extract_text(parsed: &ParsedMail) -> Option<String> {
    if parsed.ctype.mimetype.starts_with("text/plain") {
        return parsed.get_body().ok();
    }
    for sub in &parsed.subparts {
        if let Some(body) = extract_text(sub) {
            return Some(body);
        }
    }
    None
}

fn extract_html(parsed: &ParsedMail) -> Option<String> {
    if parsed.ctype.mimetype.starts_with("text/html") {
        return parsed.get_body().ok();
    }
    for sub in &parsed.subparts {
        if let Some(body) = extract_html(sub) {
            return Some(body);
        }
    }
    None
}

pub async fn get_message(
    config: &AppConfig,
    req: GetMessageRequest,
) -> Result<String, MailError> {
    let config = config.clone();
    tokio::task::spawn_blocking(move || {
        let mut session = connect_imap(&config)?;

        session
            .select(&req.mailbox)
            .map_err(|_| MailError::ImapMailboxNotFound { mailbox: req.mailbox.clone() })?;

        let fetch_query = if req.mark_seen {
            "BODY[]"
        } else {
            "BODY.PEEK[]"
        };

        let fetch_result = session
            .uid_fetch(format!("{}", req.uid), fetch_query)?;

        let email = fetch_result.into_iter().next().ok_or_else(|| {
            MailError::ImapMessageNotFound {
                mailbox: req.mailbox.clone(),
                uid: req.uid,
            }
        })?;

        let body = email
            .body()
            .ok_or_else(|| MailError::MimeParse("No body found in message".into()))?;

        let parsed_mail = mailparse::parse_mail(body)
            .map_err(|e| MailError::MimeParse(e.to_string()))?;

        let text = extract_text(&parsed_mail);
        let html = extract_html(&parsed_mail);

        let headers = parse_email_headers(&parsed_mail.get_body_raw().unwrap_or_default());

        let message_id = get_header_value(&headers, "Message-ID");
        let from = get_header_value(&headers, "From");
        let to = get_header_value(&headers, "To");
        let subject = get_header_value(&headers, "Subject");
        let date = get_header_value(&headers, "Date");

        let attachments: Vec<serde_json::Value> = parsed_mail
            .subparts
            .iter()
            .filter(|sp| {
                sp.ctype
                    .params
                    .get("name")
                    .or(sp.ctype.params.get("filename"))
                    .is_some()
            })
            .map(|sp| {
                serde_json::json!({
                    "content_type": sp.ctype.mimetype.clone(),
                    "filename": sp.ctype.params.get("name")
                        .or(sp.ctype.params.get("filename"))
                        .cloned()
                        .unwrap_or_default(),
                })
            })
            .collect();

        Ok(serde_json::json!({
            "uid": req.uid,
            "message_id": message_id,
            "from": from,
            "to": to,
            "subject": subject,
            "date": date,
            "text": text,
            "html": html,
            "attachments": attachments,
        })
        .to_string())
    })
    .await
    .map_err(|e| MailError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?
}

pub async fn delete_message(
    config: &AppConfig,
    req: DeleteMessageRequest,
) -> Result<String, MailError> {
    let config = config.clone();
    tokio::task::spawn_blocking(move || {
        let mut session = connect_imap(&config)?;

        session
            .select(&req.mailbox)
            .map_err(|_| MailError::ImapMailboxNotFound { mailbox: req.mailbox.clone() })?;

        session
            .uid_store(format!("{}", req.uid), "+FLAGS.SILENT (\\Deleted)")?;

        if req.expunge {
            session.expunge()?;
        }

        Ok(serde_json::json!({
            "deleted": true,
            "uid": req.uid,
            "mailbox": req.mailbox,
            "expunged": req.expunge,
        })
        .to_string())
    })
    .await
    .map_err(|e| MailError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?
}

pub async fn move_message(
    config: &AppConfig,
    req: MoveMessageRequest,
) -> Result<String, MailError> {
    let config = config.clone();
    tokio::task::spawn_blocking(move || {
        let mut session = connect_imap(&config)?;

        session
            .select(&req.from_mailbox)
            .map_err(|_| MailError::ImapMailboxNotFound { mailbox: req.from_mailbox.clone() })?;

        session
            .uid_copy(format!("{}", req.uid), &req.to_mailbox)?;
        session
            .uid_store(format!("{}", req.uid), "+FLAGS.SILENT (\\Deleted)")?;
        session.expunge()?;

        Ok(serde_json::json!({
            "moved": true,
            "from_mailbox": req.from_mailbox,
            "to_mailbox": req.to_mailbox,
            "uid": req.uid,
            "new_uid": null,
        })
        .to_string())
    })
    .await
    .map_err(|e| MailError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?
}

pub async fn mark_message(
    config: &AppConfig,
    req: MarkMessageRequest,
) -> Result<String, MailError> {
    let config = config.clone();
    tokio::task::spawn_blocking(move || {
        let mut session = connect_imap(&config)?;

        session
            .select(&req.mailbox)
            .map_err(|_| MailError::ImapMailboxNotFound { mailbox: req.mailbox.clone() })?;

        let mut add_flags = Vec::new();
        let mut remove_flags = Vec::new();

        if let Some(seen) = req.seen {
            if seen {
                add_flags.push("\\Seen");
            } else {
                remove_flags.push("\\Seen");
            }
        }
        if let Some(flagged) = req.flagged {
            if flagged {
                add_flags.push("\\Flagged");
            } else {
                remove_flags.push("\\Flagged");
            }
        }
        if let Some(answered) = req.answered {
            if answered {
                add_flags.push("\\Answered");
            } else {
                remove_flags.push("\\Answered");
            }
        }

        if !add_flags.is_empty() {
            let flag_str = add_flags.join(" ");
            session
                .uid_store(
                    format!("{}", req.uid),
                    &format!("+FLAGS.SILENT ({})", flag_str),
                )?;
        }

        if !remove_flags.is_empty() {
            let flag_str = remove_flags.join(" ");
            session
                .uid_store(
                    format!("{}", req.uid),
                    &format!("-FLAGS.SILENT ({})", flag_str),
                )?;
        }

        Ok(serde_json::json!({
            "updated": true,
            "uid": req.uid,
            "mailbox": req.mailbox,
            "flags_applied": {
                "seen": req.seen,
                "flagged": req.flagged,
                "answered": req.answered,
            },
        })
        .to_string())
    })
    .await
    .map_err(|e| MailError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?
}
