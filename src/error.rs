use thiserror::Error;

#[derive(Error, Debug)]
pub enum MailError {
    #[error("SMTP error: {0}")]
    Smtp(#[from] lettre::transport::smtp::Error),

    #[error("Lettre error: {0}")]
    Lettre(#[from] lettre::error::Error),

    #[error("IMAP error: {0}")]
    Imap(#[from] imap::error::Error),

    #[error("IMAP login failed: {0}")]
    ImapLogin(String),

    #[error("IMAP mailbox not found: {mailbox}")]
    ImapMailboxNotFound { mailbox: String },

    #[error("IMAP message not found: uid={uid} in {mailbox}")]
    ImapMessageNotFound { mailbox: String, uid: u32 },

    #[error("MIME parse error: {0}")]
    MimeParse(String),

    #[error("Email address parse error: {0}")]
    AddressParse(String),

    #[error("TLS error: {0}")]
    Tls(#[from] native_tls::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
