use crate::UserId;
use color_eyre::{eyre::WrapErr, Report, Result};
use reqwest::{StatusCode, Url};
use std::fmt::Write;
use std::net::IpAddr;

pub struct Client {
    http: reqwest::Client,
    base_url: Url,
}

impl Client {
    pub fn new(base_url: &str, allow_self_signed: bool) -> Result<Self> {
        let base_url = Url::parse(base_url).wrap_err("Invalid base url")?;
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(allow_self_signed)
            .build()?;
        Ok(Client { http, base_url })
    }

    pub async fn verify_credentials(
        &self,
        username: &str,
        password: &str,
        forwarded_for: Vec<IpAddr>,
    ) -> Result<UserId> {
        log::debug!("Verifying credentials for {}", username);
        let response = self
            .http
            .get(self.base_url.join("index.php/apps/notify_push/uid")?)
            .basic_auth(username, Some(password))
            .header(
                "x-forwarded-for",
                forwarded_for.iter().fold(
                    String::with_capacity(forwarded_for.len() * 16),
                    |mut joined, ip| {
                        if !joined.is_empty() {
                            write!(&mut joined, ", ").ok();
                        }
                        write!(&mut joined, "{}", ip).ok();
                        joined
                    },
                ),
            )
            .send()
            .await
            .wrap_err("Error while connecting to nextcloud server")?;

        match response.status() {
            StatusCode::OK => Ok(response.text().await?.into()),
            StatusCode::UNAUTHORIZED => Err(Report::msg("Invalid credentials")),
            status if status.is_server_error() => {
                Err(Report::msg(format!("Server error: {}", status)))
            }
            status if status.is_client_error() => {
                Err(Report::msg(format!("Client error: {}", status)))
            }
            status => Err(Report::msg(format!("Unexpected status code: {}", status))),
        }
    }

    pub async fn get_test_cookie(&self) -> Result<u32> {
        let response = self
            .http
            .get(
                self.base_url
                    .join("index.php/apps/notify_push/test/cookie")?,
            )
            .send()
            .await?;
        let status = response.status();
        let text = response.text().await?;
        if status.is_client_error() {
            if text.contains("admin-trusted-domains") {
                Err(Report::msg(format!(
                    "{} is not configured as a trusted domain",
                    self.base_url.host_str().unwrap_or_default()
                )))
            } else {
                Err(Report::msg(status.to_string()))
            }
        } else {
            Ok(text
                .parse()
                .wrap_err("Response from nextcloud is not a number")?)
        }
    }

    pub async fn test_set_remote(&self, addr: IpAddr) -> Result<IpAddr> {
        Ok(self
            .http
            .get(
                self.base_url
                    .join("index.php/apps/notify_push/test/remote")?,
            )
            .header("x-forwarded-for", addr.to_string())
            .send()
            .await?
            .text()
            .await?
            .parse()?)
    }

    /// Ask the app to put it's version number into redis under 'notify_push_app_version'
    pub async fn request_app_version(&self) -> Result<()> {
        self.http
            .get(
                self.base_url
                    .join("index.php/apps/notify_push/test/version")?,
            )
            .send()
            .await?;
        Ok(())
    }
}
