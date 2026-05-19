use crate::config::AppConfig;
use crate::error::MailError;
use lettre::address::AddressError;
use lettre::message::header::ContentType;
use lettre::message::{Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SendEmailRequest {
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Option<Vec<String>>,
    #[serde(default)]
    pub bcc: Option<Vec<String>>,
    pub subject: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub html: Option<String>,
}

pub type Mailer = AsyncSmtpTransport<Tokio1Executor>;

pub fn build_mailer(config: &AppConfig) -> Mailer {
    AsyncSmtpTransport::<Tokio1Executor>::relay(&config.smtp_host)
        .unwrap()
        .credentials(Credentials::new(
            config.qqmail_address.clone(),
            config.qqmail_auth_code.clone(),
        ))
        .port(config.smtp_port)
        .build()
}

pub async fn send_email(
    mailer: &Mailer,
    config: &AppConfig,
    req: SendEmailRequest,
) -> Result<(bool, Option<String>), MailError> {
    let from: Mailbox = config
        .qqmail_address
        .parse()
        .map_err(|e: AddressError| MailError::AddressParse(e.to_string()))?;

    let mut builder = Message::builder().from(from).subject(&req.subject);

    for addr in &req.to {
        let mailbox: Mailbox = addr
            .parse()
            .map_err(|e: AddressError| MailError::AddressParse(e.to_string()))?;
        builder = builder.to(mailbox);
    }
    if let Some(cc) = &req.cc {
        for addr in cc {
            let mailbox: Mailbox = addr
                .parse()
                .map_err(|e: AddressError| MailError::AddressParse(e.to_string()))?;
            builder = builder.cc(mailbox);
        }
    }
    if let Some(bcc) = &req.bcc {
        for addr in bcc {
            let mailbox: Mailbox = addr
                .parse()
                .map_err(|e: AddressError| MailError::AddressParse(e.to_string()))?;
            builder = builder.bcc(mailbox);
        }
    }

    let message = match (&req.text, &req.html) {
        (Some(text), Some(html)) => {
            let multipart = MultiPart::alternative()
                .singlepart(SinglePart::plain(text.clone()))
                .singlepart(SinglePart::html(html.clone()));
            builder.multipart(multipart).map_err(|e| MailError::MimeParse(e.to_string()))?
        }
        (Some(text), None) => builder.body(text.clone()).map_err(|e| MailError::MimeParse(e.to_string()))?,
        (None, Some(html)) => {
            builder
                .header(ContentType::parse("text/html; charset=utf-8").unwrap())
                .body(html.clone())
                .map_err(|e| MailError::MimeParse(e.to_string()))?
        }
        (None, None) => {
            return Err(MailError::MimeParse(
                "At least one of text or html must be provided".into(),
            ));
        }
    };

    let response = mailer.send(message).await?;

    let message_id = response
        .first_line()
        .map(|line| line.to_string());

    Ok((true, message_id))
}
