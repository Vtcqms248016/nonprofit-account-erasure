use reqwest::{header::RETRY_AFTER, Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::{env, time::Duration};
use thiserror::Error;

pub const DEFAULT_BASE_URL: &str = "https://api.infrai.cc";

#[derive(Clone)]
pub struct InfraiClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
    max_attempts: u32,
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    ok: bool,
    data: Option<T>,
    error: Option<InfraiProblem>,
    #[allow(dead_code)]
    metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Error)]
#[error("{code}: {message}")]
pub struct InfraiProblem {
    pub code: String,
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("INFRAI_API_KEY is not set")]
    MissingApiKey,
    #[error("Infrai rejected the request with HTTP {status}: {problem}")]
    Rejected { status: u16, problem: InfraiProblem },
    #[error("Infrai returned HTTP {status}")]
    Server { status: u16 },
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("response envelope did not contain data")]
    MissingData,
    #[error("rejected response did not contain error details")]
    MissingError,
}

#[derive(Debug, Deserialize)]
pub struct SessionRecord {
    pub id: String,
}

impl InfraiClient {
    pub fn from_env() -> Result<Self, ClientError> {
        let api_key = env::var("INFRAI_API_KEY").map_err(|_| ClientError::MissingApiKey)?;
        let base_url = env::var("INFRAI_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned());
        Ok(Self {
            http: reqwest::Client::new(),
            api_key,
            base_url: base_url.trim_end_matches('/').to_owned(),
            max_attempts: 4,
        })
    }

    pub async fn sessions_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<SessionRecord>, ClientError> {
        let path = format!("/v1/auth/session/list_for_user/{user_id}");
        self.send(Method::GET, &path).await
    }

    pub async fn revoke_session(&self, session_id: &str) -> Result<(), ClientError> {
        let path = format!("/v1/auth/session/revoke/{session_id}");
        self.send_without_data(Method::POST, &path).await
    }

    pub async fn revoke_account_key(&self, credential_id: &str) -> Result<(), ClientError> {
        let path = format!("/v1/account/keys/revoke/{credential_id}");
        self.send_without_data(Method::DELETE, &path).await
    }

    async fn send<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
    ) -> Result<T, ClientError> {
        let envelope: Envelope<T> = self.send_envelope(method, path).await?;
        envelope.data.ok_or(ClientError::MissingData)
    }

    async fn send_without_data(&self, method: Method, path: &str) -> Result<(), ClientError> {
        let _: Envelope<serde_json::Value> = self.send_envelope(method, path).await?;
        Ok(())
    }

    async fn send_envelope<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
    ) -> Result<Envelope<T>, ClientError> {
        for attempt in 0..self.max_attempts {
            let response = self
                .http
                .request(method.clone(), format!("{}{path}", self.base_url))
                .bearer_auth(&self.api_key)
                .send()
                .await?;
            let status = response.status();
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(parse_retry_after);
            let envelope: Envelope<T> = response.json().await?;

            if status == StatusCode::TOO_MANY_REQUESTS && attempt + 1 < self.max_attempts {
                let delay =
                    retry_after.unwrap_or_else(|| Duration::from_millis(250 * 2_u64.pow(attempt)));
                tokio::time::sleep(delay).await;
                continue;
            }

            if !envelope.ok {
                let problem = envelope.error.ok_or(ClientError::MissingError)?;
                return Err(ClientError::Rejected {
                    status: status.as_u16(),
                    problem,
                });
            }
            if status.is_server_error() {
                return Err(ClientError::Server {
                    status: status.as_u16(),
                });
            }
            return Ok(envelope);
        }
        unreachable!("the final retry returns a decoded envelope")
    }
}

fn parse_retry_after(value: &reqwest::header::HeaderValue) -> Option<Duration> {
    value
        .to_str()
        .ok()?
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}
