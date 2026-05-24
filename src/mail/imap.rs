use crate::config::MailAccountConfig;
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

fn connect_imap(account: &MailAccountConfig) -> Result<ImapSession, MailError> {
    debug_assert_eq!(account.provider, crate::config::MailProvider::Qq);
    let imap = account.imap.as_ref().expect("QQ IMAP config is required");
    let address = account.address.as_ref().expect("QQ address is required");
    let auth_code = account
        .auth_code
        .as_ref()
        .expect("QQ auth_code is required");
    let tls = native_tls::TlsConnector::builder().build()?;
    let addr = format!("{}:{}", imap.host, imap.port);
    let session =
        imap::connect(&addr, &imap.host, &tls).map_err(|e| MailError::ImapLogin(e.to_string()))?;

    session
        .login(address, auth_code)
        .map_err(|(e, _)| MailError::ImapLogin(e.to_string()))
}

pub async fn list_mailboxes(account: &MailAccountConfig) -> Result<String, MailError> {
    let account = account.clone();
    tokio::task::spawn_blocking(move || {
        let mut session = connect_imap(&account)?;

        let mut mailboxes = Vec::new();
        let list_result = session.list(None, Some("*"))?;
        for mb in list_result.iter() {
            let name: String = mb.name().to_string();
            let delimiter: String = mb.delimiter().unwrap_or("").to_string();
            let attrs: Vec<String> = mb.attributes().iter().map(|a| format!("{:?}", a)).collect();

            mailboxes.push(serde_json::json!({
                "name": name,
                "delimiter": delimiter,
                "attributes": attrs,
            }));
        }

        Ok(serde_json::json!({ "mailboxes": mailboxes }).to_string())
    })
    .await
    .map_err(|e| {
        MailError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
    })?
}

fn get_header_value(headers: &[(String, String)], name: &str) -> String {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

fn has_attachment_content_type(headers: &[(String, String)]) -> bool {
    headers.iter().any(|(k, v)| {
        k.eq_ignore_ascii_case("Content-Type")
            && (v.contains("multipart/mixed") || v.contains("name=") || v.contains("filename="))
    })
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

const MESSAGE_SUMMARY_FETCH_ITEMS: &str = "(UID FLAGS RFC822.SIZE BODY.PEEK[HEADER])";

fn message_has_flag(email: &imap::types::Fetch, expected: &str) -> bool {
    email
        .flags()
        .iter()
        .map(flag_to_str)
        .any(|flag| flag == expected)
}

fn plan_flag_mutation(
    desired: Option<bool>,
    current: bool,
    flag: &'static str,
    add_flags: &mut Vec<&'static str>,
    remove_flags: &mut Vec<&'static str>,
) {
    match (desired, current) {
        (Some(true), false) => add_flags.push(flag),
        (Some(false), true) => remove_flags.push(flag),
        _ => {}
    }
}

fn assert_uid_exists(session: &mut ImapSession, mailbox: &str, uid: u32) -> Result<(), MailError> {
    let fetch_result = session.uid_fetch(uid.to_string(), "(UID)")?;
    if fetch_result.into_iter().next().is_some() {
        Ok(())
    } else {
        Err(MailError::ImapMessageNotFound {
            mailbox: mailbox.to_string(),
            uid,
        })
    }
}

pub async fn list_messages(
    account: &MailAccountConfig,
    req: ListMessagesRequest,
) -> Result<String, MailError> {
    let account = account.clone();
    tokio::task::spawn_blocking(move || {
        let mut session = connect_imap(&account)?;

        session
            .select(&req.mailbox)
            .map_err(|_| MailError::ImapMailboxNotFound {
                mailbox: req.mailbox.clone(),
            })?;

        let uids = session.uid_search("ALL")?;
        let mut uids: Vec<u32> = uids.into_iter().collect();

        if req.order == "desc" {
            uids.sort_unstable_by(|a: &u32, b: &u32| b.cmp(a));
        } else {
            uids.sort_unstable();
        }

        let offset = req.offset.unwrap_or(0).min(uids.len());
        let limit = req.limit.min(100);
        let end = (offset + limit).min(uids.len());
        let page_uids = &uids[offset..end];

        let mut messages = Vec::new();
        for uid in page_uids {
            let fetch_result = session.uid_fetch(uid.to_string(), MESSAGE_SUMMARY_FETCH_ITEMS)?;

            if let Some(email) = fetch_result.into_iter().next() {
                let flags: Vec<&str> = email.flags().iter().map(flag_to_str).collect();
                let size = email.size.unwrap_or(0) as u64;
                let header_bytes = email.header().unwrap_or_default();
                let headers = parse_email_headers(header_bytes);
                let has_attachments = has_attachment_content_type(&headers);

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
                    "has_attachments": has_attachments,
                    "size": size,
                }));
            }
        }

        let next_offset = if offset + limit < uids.len() {
            Some(offset + limit)
        } else {
            None
        };

        Ok(serde_json::json!({
            "mailbox": req.mailbox,
            "messages": messages,
            "next_offset": next_offset,
        })
        .to_string())
    })
    .await
    .map_err(|e| {
        MailError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
    })?
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
    account: &MailAccountConfig,
    req: GetMessageRequest,
) -> Result<String, MailError> {
    let account = account.clone();
    tokio::task::spawn_blocking(move || {
        let mut session = connect_imap(&account)?;

        session
            .select(&req.mailbox)
            .map_err(|_| MailError::ImapMailboxNotFound {
                mailbox: req.mailbox.clone(),
            })?;

        let fetch_query = if req.mark_seen {
            "BODY[]"
        } else {
            "BODY.PEEK[]"
        };

        let fetch_result = session.uid_fetch(format!("{}", req.uid), fetch_query)?;

        let email =
            fetch_result
                .into_iter()
                .next()
                .ok_or_else(|| MailError::ImapMessageNotFound {
                    mailbox: req.mailbox.clone(),
                    uid: req.uid,
                })?;

        let body = email
            .body()
            .ok_or_else(|| MailError::MimeParse("No body found in message".into()))?;

        let headers = parse_email_headers(body);

        let parsed_mail =
            mailparse::parse_mail(body).map_err(|e| MailError::MimeParse(e.to_string()))?;

        let text = extract_text(&parsed_mail);
        let html = extract_html(&parsed_mail);

        let message_id = get_header_value(&headers, "Message-ID");
        let from = get_header_value(&headers, "From");
        let to = get_header_value(&headers, "To");
        let cc = get_header_value(&headers, "Cc");
        let subject = get_header_value(&headers, "Subject");
        let date = get_header_value(&headers, "Date");

        let flags: Vec<&str> = email.flags().iter().map(flag_to_str).collect();
        let seen = flags.contains(&"\\Seen");

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
            "mailbox": req.mailbox,
            "uid": req.uid,
            "message_id": message_id,
            "from": from,
            "to": to,
            "cc": cc,
            "subject": subject,
            "date": date,
            "seen": seen,
            "text": text,
            "html": html,
            "attachments": attachments,
        })
        .to_string())
    })
    .await
    .map_err(|e| {
        MailError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
    })?
}

pub async fn delete_message(
    account: &MailAccountConfig,
    req: DeleteMessageRequest,
) -> Result<String, MailError> {
    let account = account.clone();
    tokio::task::spawn_blocking(move || {
        let mut session = connect_imap(&account)?;

        session
            .select(&req.mailbox)
            .map_err(|_| MailError::ImapMailboxNotFound {
                mailbox: req.mailbox.clone(),
            })?;

        assert_uid_exists(&mut session, &req.mailbox, req.uid)?;

        session.uid_store(format!("{}", req.uid), "+FLAGS.SILENT (\\Deleted)")?;

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
    .map_err(|e| {
        MailError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
    })?
}

pub async fn move_message(
    account: &MailAccountConfig,
    req: MoveMessageRequest,
) -> Result<String, MailError> {
    let account = account.clone();
    tokio::task::spawn_blocking(move || {
        let mut session = connect_imap(&account)?;

        session
            .select(&req.from_mailbox)
            .map_err(|_| MailError::ImapMailboxNotFound {
                mailbox: req.from_mailbox.clone(),
            })?;

        assert_uid_exists(&mut session, &req.from_mailbox, req.uid)?;

        session.uid_copy(format!("{}", req.uid), &req.to_mailbox)?;
        session.uid_store(format!("{}", req.uid), "+FLAGS.SILENT (\\Deleted)")?;
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
    .map_err(|e| {
        MailError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
    })?
}

pub async fn mark_message(
    account: &MailAccountConfig,
    req: MarkMessageRequest,
) -> Result<String, MailError> {
    let account = account.clone();
    tokio::task::spawn_blocking(move || {
        let mut session = connect_imap(&account)?;

        session
            .select(&req.mailbox)
            .map_err(|_| MailError::ImapMailboxNotFound {
                mailbox: req.mailbox.clone(),
            })?;

        let fetch_result = session.uid_fetch(req.uid.to_string(), "(UID FLAGS)")?;
        let email =
            fetch_result
                .into_iter()
                .next()
                .ok_or_else(|| MailError::ImapMessageNotFound {
                    mailbox: req.mailbox.clone(),
                    uid: req.uid,
                })?;

        let current_seen = message_has_flag(&email, "\\Seen");
        let current_flagged = message_has_flag(&email, "\\Flagged");
        let current_answered = message_has_flag(&email, "\\Answered");

        let mut add_flags = Vec::new();
        let mut remove_flags = Vec::new();

        plan_flag_mutation(
            req.seen,
            current_seen,
            "\\Seen",
            &mut add_flags,
            &mut remove_flags,
        );
        plan_flag_mutation(
            req.flagged,
            current_flagged,
            "\\Flagged",
            &mut add_flags,
            &mut remove_flags,
        );
        plan_flag_mutation(
            req.answered,
            current_answered,
            "\\Answered",
            &mut add_flags,
            &mut remove_flags,
        );

        if !add_flags.is_empty() {
            let flag_str = add_flags.join(" ");
            session.uid_store(
                format!("{}", req.uid),
                &format!("+FLAGS.SILENT ({})", flag_str),
            )?;
        }

        if !remove_flags.is_empty() {
            let flag_str = remove_flags.join(" ");
            session.uid_store(
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
    .map_err(|e| {
        MailError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
    })?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_email_headers_basic() {
        let raw = b"From: test@example.com\r\nTo: recipient@example.com\r\nSubject: Hello World\r\nMessage-ID: <abc123@test.com>\r\nDate: Mon, 1 Jan 2024 00:00:00 +0000\r\n\r\n";
        let headers = parse_email_headers(raw);
        assert_eq!(headers.len(), 5);
        assert_eq!(get_header_value(&headers, "From"), "test@example.com");
        assert_eq!(get_header_value(&headers, "To"), "recipient@example.com");
        assert_eq!(get_header_value(&headers, "Subject"), "Hello World");
        assert_eq!(
            get_header_value(&headers, "Message-ID"),
            "<abc123@test.com>"
        );
    }

    #[test]
    fn test_parse_email_headers_case_insensitive() {
        let raw = b"from: test@example.com\r\nSUBJECT: Test\r\n\r\n";
        let headers = parse_email_headers(raw);
        assert_eq!(get_header_value(&headers, "From"), "test@example.com");
        assert_eq!(get_header_value(&headers, "from"), "test@example.com");
        assert_eq!(get_header_value(&headers, "SUBJECT"), "Test");
        assert_eq!(get_header_value(&headers, "subject"), "Test");
    }

    #[test]
    fn test_parse_email_headers_missing() {
        let raw = b"From: test@example.com\r\n\r\n";
        let headers = parse_email_headers(raw);
        assert_eq!(get_header_value(&headers, "Cc"), "");
        assert_eq!(get_header_value(&headers, "NonExistent"), "");
    }

    #[test]
    fn test_parse_email_headers_empty() {
        let headers = parse_email_headers(&[]);
        assert!(headers.is_empty());
        assert_eq!(get_header_value(&headers, "From"), "");
    }

    #[test]
    fn test_has_attachment_content_type_multipart_mixed() {
        let headers = vec![(
            "Content-Type".to_string(),
            "multipart/mixed; boundary=\"------=_Part_0\"".to_string(),
        )];
        assert!(has_attachment_content_type(&headers));
    }

    #[test]
    fn test_has_attachment_content_type_name_param() {
        let headers = vec![(
            "Content-Type".to_string(),
            "application/pdf; name=\"report.pdf\"".to_string(),
        )];
        assert!(has_attachment_content_type(&headers));
    }

    #[test]
    fn test_has_attachment_content_type_no_attachment() {
        let headers = vec![(
            "Content-Type".to_string(),
            "text/plain; charset=utf-8".to_string(),
        )];
        assert!(!has_attachment_content_type(&headers));
    }

    #[test]
    fn test_has_attachment_content_type_filename_param() {
        let headers = vec![(
            "Content-Type".to_string(),
            "image/png; filename=chart.png".to_string(),
        )];
        assert!(has_attachment_content_type(&headers));
    }

    #[test]
    fn test_has_attachment_content_type_case_insensitive_key() {
        let headers = vec![(
            "content-type".to_string(),
            "application/pdf; name=report.pdf".to_string(),
        )];
        assert!(has_attachment_content_type(&headers));
    }

    #[test]
    fn test_parse_email_headers_decodes_encoded_subject() {
        let raw = b"Subject: =?UTF-8?B?5rWL6K+V?=\r\n\r\n";
        let headers = parse_email_headers(raw);
        assert_eq!(get_header_value(&headers, "Subject"), "测试");
    }

    #[test]
    fn test_list_messages_offset_boundary_empty() {
        let uids: Vec<u32> = vec![];
        let offset = Some(0);
        let limit = 20;
        let end = (offset.unwrap_or(0) + limit).min(uids.len());
        assert!(offset.unwrap_or(0) <= uids.len());
        assert_eq!(end, 0);
        let page: &[u32] = &uids[offset.unwrap_or(0)..end];
        assert!(page.is_empty());
    }

    #[test]
    fn test_list_messages_offset_beyond_length() {
        let uids: Vec<u32> = vec![1, 2, 3];
        let offset = Some(10);
        let limit = 20;
        let clamped_offset = offset.unwrap_or(0).min(uids.len());
        let end = (clamped_offset + limit).min(uids.len());
        let page: &[u32] = &uids[clamped_offset..end];
        assert!(page.is_empty());
    }

    #[test]
    fn test_list_messages_offset_at_boundary() {
        let uids: Vec<u32> = vec![1, 2, 3];
        let offset = Some(3);
        let limit = 20;
        let clamped_offset = offset.unwrap_or(0).min(uids.len());
        let end = (clamped_offset + limit).min(uids.len());
        let page: &[u32] = &uids[clamped_offset..end];
        assert!(page.is_empty());
    }

    #[test]
    fn test_list_messages_normal_pagination() {
        let uids: Vec<u32> = (1..=50).collect();
        let offset = Some(0);
        let limit = 20;
        let clamped_offset = offset.unwrap_or(0).min(uids.len());
        let end = (clamped_offset + limit).min(uids.len());
        let page: &[u32] = &uids[clamped_offset..end];
        assert_eq!(page.len(), 20);
        assert_eq!(page[0], 1);
        assert_eq!(page[19], 20);
    }

    #[test]
    fn test_plan_flag_mutation_skips_noop_false_for_absent_flag() {
        let mut add_flags = Vec::new();
        let mut remove_flags = Vec::new();

        plan_flag_mutation(
            Some(false),
            false,
            "\\Answered",
            &mut add_flags,
            &mut remove_flags,
        );

        assert!(add_flags.is_empty());
        assert!(remove_flags.is_empty());
    }

    #[test]
    fn test_plan_flag_mutation_adds_only_missing_enabled_flag() {
        let mut add_flags = Vec::new();
        let mut remove_flags = Vec::new();

        plan_flag_mutation(
            Some(true),
            false,
            "\\Flagged",
            &mut add_flags,
            &mut remove_flags,
        );

        assert_eq!(add_flags, vec!["\\Flagged"]);
        assert!(remove_flags.is_empty());
    }

    #[test]
    fn test_plan_flag_mutation_removes_only_present_disabled_flag() {
        let mut add_flags = Vec::new();
        let mut remove_flags = Vec::new();

        plan_flag_mutation(
            Some(false),
            true,
            "\\Seen",
            &mut add_flags,
            &mut remove_flags,
        );

        assert!(add_flags.is_empty());
        assert_eq!(remove_flags, vec!["\\Seen"]);
    }

    #[test]
    fn test_plan_flag_mutation_skips_omitted_flag() {
        let mut add_flags = Vec::new();
        let mut remove_flags = Vec::new();

        plan_flag_mutation(None, true, "\\Seen", &mut add_flags, &mut remove_flags);

        assert!(add_flags.is_empty());
        assert!(remove_flags.is_empty());
    }
}
