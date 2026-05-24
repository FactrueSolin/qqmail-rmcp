use crate::config::MailAccountConfig;
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

pub fn build_mailer(account: &MailAccountConfig) -> Mailer {
    debug_assert_eq!(account.provider, crate::config::MailProvider::Qq);
    let smtp = account.smtp.as_ref().expect("QQ SMTP config is required");
    let address = account.address.as_ref().expect("QQ address is required");
    let auth_code = account
        .auth_code
        .as_ref()
        .expect("QQ auth_code is required");
    AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp.host)
        .unwrap()
        .credentials(Credentials::new(address.clone(), auth_code.clone()))
        .port(smtp.port)
        .build()
}

pub async fn send_email(
    mailer: &Mailer,
    account: &MailAccountConfig,
    req: SendEmailRequest,
) -> Result<(bool, Option<String>), MailError> {
    let from: Mailbox = account
        .address
        .as_ref()
        .ok_or_else(|| MailError::ProviderApiError("QQ account address is missing".into()))?
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
            builder
                .multipart(multipart)
                .map_err(|e| MailError::MimeParse(e.to_string()))?
        }
        (Some(text), None) => builder
            .body(text.clone())
            .map_err(|e| MailError::MimeParse(e.to_string()))?,
        (None, Some(html)) => builder
            .header(ContentType::parse("text/html; charset=utf-8").unwrap())
            .body(html.clone())
            .map_err(|e| MailError::MimeParse(e.to_string()))?,
        (None, None) => {
            return Err(MailError::MimeParse(
                "At least one of text or html must be provided".into(),
            ));
        }
    };

    let response = mailer.send(message).await?;

    let message_id = response.first_line().map(|line| line.to_string());

    Ok((true, message_id))
}
