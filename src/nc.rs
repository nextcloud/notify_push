/*
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

use crate::error::{AuthenticationError, NextCloudError};
use crate::{Result, UserId};
use reqwest::header::HeaderName;
use reqwest::{Response, StatusCode, Url};
use std::fmt::Write;
use std::net::IpAddr;

static X_FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");

pub struct Client {
    http: reqwest::Client,
    base_url: Url,
}

impl Client {
    pub fn new(base_url: &str, allow_self_signed: bool) -> Result<Self, NextCloudError> {
        let base_url = Url::parse(base_url)?;
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
    ) -> Result<UserId, AuthenticationError> {
        log::debug!("Verifying credentials for {username}");
        let response = self.auth_request(username, password, forwarded_for).await?;

        match response.status() {
            StatusCode::OK => Ok(response
                .text()
                .await
                .map_err(|_| AuthenticationError::InvalidMessage)?
                .into()),
            StatusCode::UNAUTHORIZED => Err(AuthenticationError::Invalid),
            status if status.is_server_error() => Err(NextCloudError::Server(status).into()),
            status if status.is_client_error() => Err(NextCloudError::Client(status).into()),
            status => Err(NextCloudError::Other(status).into()),
        }
    }

    async fn auth_request(
        &self,
        username: &str,
        password: &str,
        forwarded_for: Vec<IpAddr>,
    ) -> Result<Response, NextCloudError> {
        self.http
            .get(self.base_url.join("index.php/apps/notify_push/uid")?)
            .basic_auth(username, Some(password))
            .header(
                &X_FORWARDED_FOR,
                forwarded_for.iter().fold(
                    String::with_capacity(forwarded_for.len() * 16),
                    |mut joined, ip| {
                        if !joined.is_empty() {
                            write!(&mut joined, ", ").ok();
                        }
                        write!(&mut joined, "{ip}").ok();
                        joined
                    },
                ),
            )
            .send()
            .await
            .map_err(NextCloudError::NextcloudConnect)
    }

    pub async fn get_test_cookie(&self) -> Result<u32, NextCloudError> {
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
                Err(NextCloudError::NotATrustedDomain(
                    self.base_url.host_str().unwrap_or_default().into(),
                ))
            } else {
                Err(NextCloudError::Client(status))
            }
        } else {
            Ok(text
                .parse()
                .map_err(NextCloudError::MalformedCookieResponse)?)
        }
    }

    pub async fn test_set_remote(&self, addr: IpAddr) -> Result<IpAddr, NextCloudError> {
        self.http
            .get(
                self.base_url
                    .join("index.php/apps/notify_push/test/remote")
                    .map_err(NextCloudError::from)?,
            )
            .header(&X_FORWARDED_FOR, addr.to_string())
            .send()
            .await?
            .text()
            .await?
            .parse()
            .map_err(NextCloudError::MalformedRemote)
    }

    /// Ask the app to put it's version number into redis under 'notify_push_app_version'
    pub async fn request_app_version(&self) -> Result<(), NextCloudError> {
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
