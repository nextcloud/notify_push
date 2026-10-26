/*
 * SPDX-FileCopyrightText: 2026 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

use std::borrow::Cow;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{header, HeaderName, StatusCode};
use comma_separated::CommaSeparatedIterator;

use crate::error::{ClientRequestError, SelfTestError};
use crate::{App, Result};

enum EscapeState {
    Normal,
    Escaped,
}

fn maybe_quoted(x: &str) -> Cow<'_, str> {
    let mut i = x.chars();
    if i.next() == Some('"') {
        let mut s = String::with_capacity(x.len());
        let mut state = EscapeState::Normal;
        for c in i {
            state = match state {
                EscapeState::Normal => match c {
                    '"' => break,
                    '\\' => EscapeState::Escaped,
                    _ => {
                        s.push(c);
                        EscapeState::Normal
                    }
                },
                EscapeState::Escaped => {
                    s.push(c);
                    EscapeState::Normal
                }
            };
        }
        s.into()
    } else {
        x.into()
    }
}

fn maybe_bracketed(x: &str) -> &str {
    if x.as_bytes().first() == Some(&b'[') && x.as_bytes().last() == Some(&b']') {
        &x[1..x.len() - 1]
    } else {
        x
    }
}

pub struct ValidatedToken(pub String);

impl FromRequestParts<Arc<App>> for ValidatedToken {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<App>,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(HeaderName::from_static("token"))
            .and_then(|value| value.to_str().ok());

        match auth_header {
            Some(auth_header) if validate_token(state, auth_header).await.is_ok() => {
                Ok(Self(auth_header.into()))
            }
            _ => Err(StatusCode::UNAUTHORIZED),
        }
    }
}

async fn validate_token(app: &App, token: &str) -> Result<()> {
    let mut redis = app.redis.connect().await?;
    let expected = redis.get("test-token").await?;
    if !token.is_empty() && token == expected {
        Ok(())
    } else {
        Err(SelfTestError::Token.into())
    }
}

/// Newtype so we can implement FromStr
struct CommaSeparated<T>(Vec<T>);

impl<T> CommaSeparated<T> {
    pub fn into_inner(self) -> Vec<T> {
        self.0
    }
}

impl<T: FromStr> FromStr for CommaSeparated<T> {
    type Err = T::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let vec = CommaSeparatedIterator::new(s)
            .map(|x| T::from_str(maybe_bracketed(&maybe_quoted(x.trim()))))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CommaSeparated(vec))
    }
}

pub struct ForwardedFor(pub Vec<IpAddr>);

impl<S> FromRequestParts<S> for ForwardedFor
where
    S: Send + Sync,
{
    type Rejection = ClientRequestError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if let Some(x_forwarded_for_header) = parts
            .headers
            .get(HeaderName::from_static("x-forwarded-for"))
            .and_then(|value| value.to_str().ok())
        {
            let ips = CommaSeparated::from_str(x_forwarded_for_header)
                .map_err(|_| ClientRequestError::InvalidForwardedHeader)?;
            return Ok(ForwardedFor(ips.into_inner()));
        }
        if let Some(x_real_ip) = parts
            .headers
            .get(HeaderName::from_static("x-real-ip"))
            .and_then(|value| value.to_str().ok())
        {
            let ip = IpAddr::from_str(maybe_bracketed(&maybe_quoted(x_real_ip)))
                .map_err(|_| ClientRequestError::InvalidForwardedHeader)?;
            return Ok(ForwardedFor(vec![ip]));
        }
        if let Some(forwarded_header) = parts
            .headers
            .get(header::FORWARDED)
            .and_then(|value| value.to_str().ok())
        {
            let forwarded = rfc7239::parse(forwarded_header).map(|res| {
                res.map_err(|_| ClientRequestError::InvalidForwardedHeader)
                    .and_then(|forward| {
                        forward
                            .forwarded_for
                            .ok_or(ClientRequestError::InvalidForwardedHeader)
                    })
                    .and_then(|node| {
                        node.ip()
                            .copied()
                            .ok_or(ClientRequestError::InvalidForwardedHeader)
                    })
            });
            let ips = forwarded.collect::<Result<Vec<_>, _>>()?;
            return Ok(ForwardedFor(ips));
        }

        Ok(ForwardedFor(Vec::new()))
    }
}
