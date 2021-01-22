use crate::UserId;
use color_eyre::{eyre::WrapErr, Report, Result};
use reqwest::{StatusCode, Url};
use std::net::IpAddr;

pub struct Client {
    http: reqwest::Client,
    base_url: Url,
}

impl Client {
    pub fn new(base_url: &str) -> Result<Self> {
        let base_url = Url::parse(base_url).wrap_err("Invalid base url")?;
        let http = reqwest::Client::new();
        Ok(Client { http, base_url })
    }

    pub async fn verify_credentials(
        &self,
        username: &str,
        password: &str,
        forwarded_for: Vec<IpAddr>,
    ) -> Result<UserId> {
        let response = self
            .http
            .get(self.base_url.join("index.php/apps/notify_push/uid")?)
            .basic_auth(username, Some(password))
            .header(
                "x-forwarded-for",
                forwarded_for.into_iter().map(|addr| addr.to_string()).fold(
                    String::new(),
                    |mut joined, ip| {
                        joined.reserve(ip.len() + 1);
                        if !joined.is_empty() {
                            joined.push_str(", ");
                        }
                        joined.push_str(&ip);
                        joined
                    },
                ),
            )
            .send()
            .await
            .wrap_err("Error while connecting to nextcloud server")?;

        match response.status() {
            StatusCode::OK => Ok(response.text().await?.into()),
            StatusCode::UNAUTHORIZED => Err(Report::msg(format!("Invalid credentials"))),
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
        Ok(self
            .http
            .get(
                self.base_url
                    .join("index.php/apps/notify_push/test/cookie")?,
            )
            .send()
            .await?
            .json()
            .await?)
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
}
