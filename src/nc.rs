use color_eyre::{eyre::WrapErr, Report, Result};
use reqwest::{StatusCode, Url};

pub struct Client {
    http: reqwest::Client,
    dav_url: Url,
    base_url: Url,
}

impl Client {
    pub fn new(base_url: &str) -> Result<Self> {
        let base_url = Url::parse(base_url).wrap_err("Invalid base url")?;
        let dav_url = base_url.join("remote.php/dav").unwrap();
        let http = reqwest::Client::new();
        Ok(Client {
            http,
            dav_url,
            base_url,
        })
    }

    pub async fn verify_credentials(&self, username: &str, password: &str) -> Result<bool> {
        let response = self
            .http
            .head(self.dav_url.clone())
            .basic_auth(username, Some(password))
            .send()
            .await
            .wrap_err("Error while connecting to nextcloud server")?;

        match response.status() {
            StatusCode::OK => Ok(true),
            StatusCode::UNAUTHORIZED => Ok(false),
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
            .get(self.base_url.join("apps/notify_push/test_cookie")?)
            .send()
            .await?
            .json()
            .await?)
    }
}
