use chrono::{DateTime, Utc};
use lettre::address::AddressError;
use lettre::message::{Mailbox, Message, MultiPart};
use lettre::{
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport,
    AsyncTransport,
    Tokio1Executor,
};
use tracing::instrument;

use crate::config::SmtpConfig;
use crate::error::{AppError, Result};

const EMBEDDED_LOGO_SVG: &str = include_str!("../../Şekerbank_logo.svg");

/// SMTP email sender service.
#[derive(Clone)]
pub struct EmailService {
    smtp_transport: Option<AsyncSmtpTransport<Tokio1Executor>>,
    from_mailbox: Option<Mailbox>,
    logo_url: Option<String>,
}

impl EmailService {
    /// Constructs the email sender from optional SMTP configuration.
    pub fn new(config: Option<SmtpConfig>) -> Result<Self> {
        match config {
            Some(smtp_config) => {
                let credentials = Credentials::new(
                    smtp_config.username,
                    smtp_config.password,
                );

                let transport = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp_config.host)
                    .map_err(|error| AppError::config(format!("SMTP relay olusturulamadi: {error}")))?
                    .port(smtp_config.port)
                    .credentials(credentials)
                    .build();

                let from_mailbox = parse_mailbox(
                    &smtp_config.from_address,
                    Some(smtp_config.from_name.as_str()),
                )?;

                Ok(Self {
                    smtp_transport: Some(transport),
                    from_mailbox: Some(from_mailbox),
                    logo_url: smtp_config.logo_url,
                })
            }
            None => Ok(Self {
                smtp_transport: None,
                from_mailbox: None,
                logo_url: None,
            }),
        }
    }

    /// Sends a formatted customer invite email.
    #[instrument(skip(self, invite_link))]
    pub async fn send_invite_email(
        &self,
        recipient_email: &str,
        customer_national_id: &str,
        invite_link: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<bool> {
        let Some(transport) = self.smtp_transport.as_ref() else {
            return Ok(false);
        };

        let Some(from_mailbox) = self.from_mailbox.clone() else {
            return Ok(false);
        };

        let to_mailbox = parse_mailbox(recipient_email, None)?;

        let plain_body = build_plain_text(customer_national_id, invite_link, expires_at);
        let html_body = build_html_body(
            customer_national_id,
            invite_link,
            expires_at,
            self.logo_url.as_deref(),
        );

        let message = Message::builder()
            .from(from_mailbox)
            .to(to_mailbox)
            .subject("Şekerbank | Çek Tarama Davet Linkiniz Hazır")
            .multipart(MultiPart::alternative_plain_html(plain_body, html_body))
            .map_err(|error| AppError::internal(format!("mail body olusturulamadi: {error}")))?;

        transport
            .send(message)
            .await
            .map_err(|error| AppError::internal(format!("mail gonderilemedi: {error}")))?;

        Ok(true)
    }
}

fn parse_mailbox(address: &str, display_name: Option<&str>) -> Result<Mailbox> {
    let parsed_address = address
        .parse()
        .map_err(|error: AddressError| AppError::invalid_input(format!("gecersiz email adresi: {error}")))?;

    let mailbox = match display_name {
        Some(name) if !name.trim().is_empty() => Mailbox::new(Some(name.trim().to_string()), parsed_address),
        _ => Mailbox::new(None, parsed_address),
    };

    Ok(mailbox)
}

fn build_plain_text(
    customer_national_id: &str,
    invite_link: &str,
    expires_at: DateTime<Utc>,
) -> String {
    format!(
        "Sayın Müşterimiz,\n\n\
Şekerbank çek tarama süreciniz için tek kullanımlık bağlantınız oluşturulmuştur.\n\
Müşteri T.C. Kimlik No: {customer_national_id}\n\
Tarama sürecini başlatmak için aşağıdaki bağlantıyı kullanabilirsiniz:\n\
{invite_link}\n\n\
Eğer butona bastığınızda link açılmazsa aşağıdaki linki kopyalayıp tarayıcınıza yapıştırabilirsiniz:\n\
{invite_link}\n\n\
Bağlantı son geçerlilik zamanı: {}\n\n\
Güvenlik notu: Bu bağlantıyı üçüncü kişilerle paylaşmayınız.\n\
Bu e-posta otomatik olarak oluşturulmuştur, lütfen yanıtlamayınız.\n\n\
Saygılarımızla,\n\
Şekerbank",
        expires_at.format("%d.%m.%Y %H:%M:%S UTC")
    )
}

fn build_html_body(
    customer_national_id: &str,
    invite_link: &str,
    expires_at: DateTime<Utc>,
    logo_url: Option<&str>,
) -> String {
    let logo_html = match logo_url {
        Some(url) => {
            format!(
                "<img src=\"{url}\" alt=\"Şekerbank\" style=\"display:block;height:32px;width:auto;max-width:180px;margin:0 0 14px 0;\" />"
            )
        }
        None => format!(
            "<div style=\"display:inline-block;margin:0 0 14px 0;border-radius:8px;overflow:hidden;line-height:0;\">{EMBEDDED_LOGO_SVG}</div>"
        ),
    };

    format!(
        "<!doctype html>\
<html lang=\"tr\">\
  <body style=\"margin:0;padding:0;background:#f2f5f3;font-family:'Segoe UI',Roboto,Arial,sans-serif;color:#243028;\">\
    <table role=\"presentation\" width=\"100%\" cellspacing=\"0\" cellpadding=\"0\" style=\"padding:24px 12px;\">\
      <tr>\
        <td align=\"center\">\
          <table role=\"presentation\" width=\"100%\" cellspacing=\"0\" cellpadding=\"0\" style=\"max-width:620px;background:#ffffff;border-radius:16px;overflow:hidden;border:1px solid #d7e5dc;box-shadow:0 12px 30px rgba(0,122,61,0.08);\">\
            <tr>\
              <td style=\"background:#007a3d;padding:22px 24px;color:#ffffff;\">\
                {logo_html}\
                <h1 style=\"margin:0;font-size:21px;line-height:1.35;font-weight:700;\">Çek Tarama Davet Linkiniz Hazır</h1>\
                <p style=\"margin:8px 0 0 0;font-size:13px;line-height:1.5;color:#e5f4ec;\">Şekerbank İstihbarat Sistemi</p>\
              </td>\
            </tr>\
            <tr>\
              <td style=\"padding:24px;\">\
                <p style=\"margin:0 0 12px 0;font-size:15px;line-height:1.6;\">Sayın Müşterimiz,</p>\
                <p style=\"margin:0 0 14px 0;font-size:14px;line-height:1.7;color:#34463b;\">\
                  Şekerbank çek tarama süreciniz için tek kullanımlık bağlantınız oluşturulmuştur.\
                </p>\
                <table role=\"presentation\" width=\"100%\" cellspacing=\"0\" cellpadding=\"0\" style=\"margin:0 0 16px 0;border-collapse:collapse;\">\
                  <tr>\
                    <td style=\"padding:10px 12px;border:1px solid #d7e5dc;border-radius:10px;background:#f7fbf8;\">\
                      <p style=\"margin:0;font-size:12px;color:#5d6c62;\">Müşteri T.C. Kimlik No</p>\
                      <p style=\"margin:6px 0 0 0;font-size:15px;font-weight:700;color:#1e2b24;\">{customer_national_id}</p>\
                    </td>\
                  </tr>\
                </table>\
                <p style=\"margin:0 0 16px 0;font-size:14px;line-height:1.7;color:#34463b;\">\
                  Tarama sürecini başlatmak için aşağıdaki butona tıklayınız:\
                </p>\
                <p style=\"margin:0 0 18px 0;\">\
                  <a href=\"{invite_link}\" style=\"display:inline-block;background:#007a3d;color:#ffffff;text-decoration:none;font-weight:700;font-size:14px;padding:12px 18px;border-radius:10px;\">Çek Tarama Ekranını Aç</a>\
                </p>\
                <p style=\"margin:0 0 8px 0;font-size:13px;line-height:1.6;color:#516057;\">\
                  Eğer butona bastığınızda link açılmazsa aşağıdaki linki kopyalayıp tarayıcınıza yapıştırabilirsiniz:\
                </p>\
                <p style=\"margin:0 0 16px 0;padding:10px 12px;border:1px solid #d7e5dc;border-radius:10px;background:#f7fbf8;word-break:break-all;\">\
                  <a href=\"{invite_link}\" style=\"font-size:12px;line-height:1.6;color:#006a35;text-decoration:none;\">{invite_link}</a>\
                </p>\
                <p style=\"margin:0 0 8px 0;font-size:13px;line-height:1.6;color:#516057;\">\
                  Bağlantı son geçerlilik zamanı: <strong>{}</strong>\
                </p>\
                <p style=\"margin:0;font-size:12px;line-height:1.6;color:#6f7e74;\">\
                  Güvenlik notu: Bu bağlantıyı üçüncü kişilerle paylaşmayınız.\
                </p>\
              </td>\
            </tr>\
            <tr>\
              <td style=\"padding:14px 24px;background:#f7fbf8;border-top:1px solid #e2ece6;\">\
                <p style=\"margin:0;font-size:11px;line-height:1.6;color:#7d8b82;\">\
                  Bu e-posta otomatik olarak oluşturulmuştur, lütfen yanıtlamayınız.\
                </p>\
              </td>\
            </tr>\
          </table>\
        </td>\
      </tr>\
    </table>\
  </body>\
</html>",
        expires_at.format("%d.%m.%Y %H:%M:%S UTC")
    )
}
