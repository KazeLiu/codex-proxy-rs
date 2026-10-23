//! API Key 上游额度展示的受限查询、快照读取与刷新，不写入原生额度状态

use super::*;
use gateway_admin::model::quota::{
    MAX_QUOTA_RESPONSE_BYTES, QuotaObservationStatus, QuotaResponse,
};
use gateway_admin::{
    model::upstream_quota::{AccountUpstreamQuota, QuotaQueryMode},
    ports::provider::{ProviderAdminError, ProviderAdminErrorKind},
};
use gateway_core::account::{UpstreamQuotaRefreshClaim, UpstreamQuotaRefreshResult};

pub(super) type UpstreamRefreshLock = Mutex<Option<Result<(), ProviderAdminError>>>;

impl CodexCredentialQuotaService {
    pub(crate) async fn upstream_snapshots(
        &self,
        ids: &[ProviderAccountId],
    ) -> Result<BTreeMap<ProviderAccountId, AccountUpstreamQuota>, ProviderAdminError> {
        let snapshots = self
            .store
            .upstream_quota_snapshots(ids)
            .await
            .map_err(store_failure)?;
        snapshots
            .into_iter()
            .map(|snapshot| {
                let mut accounts = match snapshot.payload {
                    Some(payload) => {
                        let response: QuotaResponse =
                            serde_json::from_value(Value::Object(payload.into_inner()))
                                .map_err(|_| invalid_response())?;
                        response.validate().map_err(|_| invalid_response())?;
                        response.accounts
                    }
                    None => Vec::new(),
                };
                let stale = snapshot.last_error_code.is_some()
                    || snapshot.fetched_at.is_some_and(|at| {
                        SystemTime::now()
                            .duration_since(at)
                            .is_ok_and(|age| age >= Duration::from_secs(600))
                    })
                    || accounts
                        .iter()
                        .any(|entry| entry.status == QuotaObservationStatus::Stale);
                if stale {
                    for entry in &mut accounts {
                        if entry.status == QuotaObservationStatus::Observed {
                            entry.status = QuotaObservationStatus::Stale;
                        }
                    }
                }
                Ok((
                    snapshot.account_id,
                    AccountUpstreamQuota {
                        credential_revision: snapshot.credential_revision.get(),
                        fetched_at: snapshot.fetched_at.map(Into::into),
                        last_attempt_at: snapshot.last_attempt_at.map(Into::into),
                        last_error_code: snapshot.last_error_code,
                        forwarding_blocked: snapshot.forwarding_blocked,
                        stale,
                        accounts,
                    },
                ))
            })
            .collect()
    }

    pub(crate) async fn refresh_upstream_due(&self) -> Result<(), ProviderAdminError> {
        let accounts = self.repository.list_for_provider().await.map_err(|error| {
            ProviderAdminError::new(ProviderAdminErrorKind::Unavailable).with_source(error)
        })?;
        let ids = accounts
            .into_iter()
            .filter(|account| account.enabled() && account.authentication_kind() == "api_key")
            .map(|account| account.id().clone())
            .collect::<Vec<_>>();
        self.refresh_upstream_ids(&ids).await
    }

    async fn refresh_upstream_ids(
        &self,
        ids: &[ProviderAccountId],
    ) -> Result<(), ProviderAdminError> {
        let snapshots = self
            .store
            .upstream_quota_snapshots(ids)
            .await
            .map_err(store_failure)?;
        let now = SystemTime::now();
        let mut pending = futures::stream::iter(snapshots.into_iter().filter(|snapshot| {
            snapshot.last_attempt_at.is_none_or(|at| now.duration_since(at).is_ok_and(|age| age >= Duration::from_secs(60)))
        }).map(|snapshot| async move {
            if let Err(error) = self.refresh_upstream(&snapshot.account_id, false).await {
                tracing::warn!(account_id = %snapshot.account_id, kind = ?error.kind(), "upstream quota refresh failed");
            }
        })).buffer_unordered(4);
        while pending.next().await.is_some() {}
        Ok(())
    }

    pub(crate) async fn refresh_upstream(
        &self,
        id: &ProviderAccountId,
        manual: bool,
    ) -> Result<(), ProviderAdminError> {
        let loaded = self
            .store
            .load_current_credential(id)
            .await
            .map_err(store_failure)?;
        let key = (id.clone(), loaded.account.revision());
        let lock = {
            let mut locks = self.upstream_refresh_locks.lock().await;
            locks.retain(|_, lock| lock.strong_count() > 0);
            if let Some(lock) = locks.get(&key).and_then(std::sync::Weak::upgrade) {
                lock
            } else {
                let lock = Arc::new(Mutex::new(None));
                locks.insert(key, Arc::downgrade(&lock));
                lock
            }
        };
        let mut result_slot = match lock.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                let result = lock.lock().await;
                return result.clone().unwrap_or_else(|| {
                    Err(ProviderAdminError::new(ProviderAdminErrorKind::Conflict))
                });
            }
        };
        let result = self.refresh_upstream_once(loaded, manual).await;
        *result_slot = Some(result.clone());
        result
    }

    async fn refresh_upstream_once(
        &self,
        loaded: gateway_core::account::LoadedCredential,
        manual: bool,
    ) -> Result<(), ProviderAdminError> {
        let id = loaded.account.id();
        if loaded.account.provider().as_str() != "openai"
            || loaded.account.authentication_kind() != "api_key"
            || !loaded.account.enabled()
        {
            return Err(ProviderAdminError::new(ProviderAdminErrorKind::Unsupported));
        }
        let mode = crate::credential::quota_query_mode(&loaded.credential)
            .map_err(|_| invalid_response())?;
        if mode == QuotaQueryMode::Disabled {
            return Err(ProviderAdminError::new(ProviderAdminErrorKind::Unsupported));
        }
        let timeout = query_timeout(mode);
        let deadline = tokio::time::Instant::now() + timeout;
        let claim = UpstreamQuotaRefreshClaim {
            account_id: id.clone(),
            credential_revision: loaded.account.revision(),
            attempt_id: Uuid::new_v4().to_string(),
            attempted_at: SystemTime::now(),
            minimum_interval: if manual {
                timeout.max(Duration::from_secs(10))
            } else {
                mode.refresh_interval()
            },
        };
        if !self
            .store
            .claim_upstream_quota_refresh(&claim)
            .await
            .map_err(store_failure)?
        {
            return if manual {
                Err(ProviderAdminError::new(ProviderAdminErrorKind::RateLimited)
                    .with_public_message("额度刷新过于频繁，请稍后再试"))
            } else {
                Ok(())
            };
        }
        let (fetched, models) = tokio::join!(
            fetch(&loaded, mode, deadline),
            self.upstream_models(&loaded.account, mode, deadline),
        );
        let (payload, error_code) = match fetched {
            Ok(mut response) => {
                if mode != QuotaQueryMode::Cpr {
                    for account in &mut response.accounts {
                        account.models.clone_from(&models);
                    }
                }
                let value = serde_json::to_value(response).map_err(|_| invalid_response())?;
                (
                    Some(OpaqueProviderData::new(
                        value.as_object().cloned().ok_or_else(invalid_response)?,
                    )),
                    None,
                )
            }
            Err(code) => (None, Some(code.to_owned())),
        };
        let failure_code = error_code.clone();
        let written = self
            .store
            .finish_upstream_quota_refresh(UpstreamQuotaRefreshResult {
                claim,
                payload,
                error_code,
            })
            .await
            .map_err(store_failure)?;
        if !written {
            return Err(ProviderAdminError::new(ProviderAdminErrorKind::Conflict)
                .with_public_message("账号连接已变更，请重新获取额度"));
        }
        match failure_code {
            Some(code) => Err(query_failure(&code)),
            None => Ok(()),
        }
    }

    async fn upstream_models(
        &self,
        account: &ProviderAccount,
        mode: QuotaQueryMode,
        deadline: tokio::time::Instant,
    ) -> Option<Vec<String>> {
        // CPR 未返回模型可能是上游未授权分享，不能用整站目录替代
        if mode == QuotaQueryMode::Cpr || deadline <= tokio::time::Instant::now() {
            return None;
        }
        let catalog = self.model_catalog.as_ref()?;
        match tokio::time::timeout_at(deadline, catalog.cached_or_refresh_account_catalog(account))
            .await
        {
            Ok(Ok(catalog)) => Some(catalog.models().to_vec()),
            _ => {
                tracing::warn!(account_id = %account.id(), "upstream quota model catalog unavailable");
                None
            }
        }
    }
}

async fn fetch(
    loaded: &gateway_core::account::LoadedCredential,
    mode: QuotaQueryMode,
    deadline: tokio::time::Instant,
) -> Result<QuotaResponse, &'static str> {
    let observed_at = chrono::Utc::now();
    if deadline <= tokio::time::Instant::now() {
        return Err("timeout");
    }
    let maximum_bytes = if mode == QuotaQueryMode::Cpr {
        MAX_QUOTA_RESPONSE_BYTES
    } else {
        256 * 1024
    };
    let data = crate::credential::CodexCredentialCodec::decode_complete(&loaded.credential)
        .map_err(|_| "invalid_response")?;
    let crate::credential::CodexCredentialData::ApiKey(data) = data else {
        return Err("invalid_response");
    };
    let client = crate::transport::client::build_account_http_client(
        loaded.account.id().as_str(),
        loaded.account.outbound_proxy(),
    )
    .map_err(|_| "network_error")?;
    let mut response = if mode == QuotaQueryMode::CommandCode {
        let value = fetch_command_code(&client, &data.api_key, deadline).await?;
        super::balance::parse(&value, mode, observed_at).map_err(|_| "invalid_response")?
    } else {
        let url = if mode == QuotaQueryMode::Cpr {
            crate::transport::endpoint_url(&data.base_url, "quota")
        } else {
            super::balance::query_url(&data.base_url, mode)
                .map_err(|_| "invalid_response")?
                .to_string()
        };
        let bytes = fetch_bytes(&client, &url, &data.api_key, deadline, maximum_bytes).await?;
        if mode == QuotaQueryMode::Cpr {
            serde_json::from_slice(&bytes).map_err(|_| "invalid_response")?
        } else {
            let value = serde_json::from_slice(&bytes).map_err(|_| "invalid_response")?;
            super::balance::parse(&value, mode, observed_at).map_err(|_| "invalid_response")?
        }
    };
    if mode != QuotaQueryMode::Cpr {
        for account in &mut response.accounts {
            account.label = loaded.account.name().to_owned();
        }
    }
    response.validate().map_err(|_| "invalid_response")?;
    Ok(response)
}

async fn fetch_bytes(
    client: &Client,
    url: &str,
    api_key: &str,
    deadline: tokio::time::Instant,
    maximum_bytes: usize,
) -> Result<Vec<u8>, &'static str> {
    if deadline <= tokio::time::Instant::now() {
        return Err("timeout");
    }
    let request = client
        .get(url)
        .bearer_auth(api_key)
        .header("Accept", "application/json")
        .send();
    let mut response = tokio::time::timeout_at(deadline, request)
        .await
        .map_err(|_| "timeout")?
        .map_err(|error| {
            if error.is_timeout() {
                "timeout"
            } else {
                "network_error"
            }
        })?;
    let status = response.status();
    let rejection = match status.as_u16() {
        200..=299 => None,
        401 => Some("unauthorized"),
        403 => Some("forbidden"),
        404 | 405 => Some("unsupported"),
        429 => Some("rate_limited"),
        _ => Some("upstream_error"),
    };
    if let Some(code) = rejection
        && status.as_u16() != 403
    {
        return Err(code);
    }
    // 权限拒绝不能被响应体读取失败覆盖，否则撤销权限后旧快照仍可能继续转发
    if response
        .content_length()
        .is_some_and(|size| size > maximum_bytes as u64)
    {
        return Err(rejection.unwrap_or("response_too_large"));
    }
    let mut bytes = Vec::new();
    loop {
        let chunk = tokio::time::timeout_at(deadline, response.chunk())
            .await
            .map_err(|_| rejection.unwrap_or("timeout"))?
            .map_err(|_| rejection.unwrap_or("network_error"))?;
        let Some(chunk) = chunk else { break };
        if bytes.len().saturating_add(chunk.len()) > maximum_bytes {
            return Err(rejection.unwrap_or("response_too_large"));
        }
        bytes.extend_from_slice(&chunk);
    }
    if status.as_u16() == 403 {
        return Err(
            if serde_json::from_slice::<Value>(&bytes)
                .ok()
                .as_ref()
                .and_then(|value| value.pointer("/error/code"))
                .and_then(Value::as_str)
                == Some("quota_sharing_disabled")
            {
                "sharing_disabled"
            } else {
                "forbidden"
            },
        );
    }
    Ok(bytes)
}

async fn fetch_command_code(
    client: &Client,
    api_key: &str,
    deadline: tokio::time::Instant,
) -> Result<Value, &'static str> {
    let whoami = command_code_json(
        client,
        api_key,
        "/alpha/whoami",
        &[("limits", "1")],
        deadline,
    )
    .await?;
    let org_id = whoami.pointer("/org/id").and_then(Value::as_str);
    let org_params = org_id.map(|id| vec![("orgId", id)]).unwrap_or_default();
    let credits = command_code_json(
        client,
        api_key,
        "/alpha/billing/credits",
        &org_params,
        deadline,
    )
    .await?;
    let subscription = command_code_json(
        client,
        api_key,
        "/alpha/billing/subscriptions",
        &org_params,
        deadline,
    )
    .await?;
    let mut summary_params = org_params;
    if let Some(start) = subscription
        .pointer("/data/currentPeriodStart")
        .and_then(Value::as_str)
    {
        summary_params.push(("since", start));
    }
    let summary = command_code_json(
        client,
        api_key,
        "/alpha/usage/summary",
        &summary_params,
        deadline,
    )
    .await?;
    Ok(serde_json::json!({"credits": credits, "subscription": subscription, "summary": summary}))
}

async fn command_code_json(
    client: &Client,
    api_key: &str,
    path: &str,
    params: &[(&str, &str)],
    deadline: tokio::time::Instant,
) -> Result<Value, &'static str> {
    // 控制面固定在官方根域名，不把 /provider 或 /provider/v1 拼进查询路径
    let mut url = url::Url::parse("https://api.commandcode.ai").map_err(|_| "invalid_response")?;
    url.set_path(path);
    url.query_pairs_mut().extend_pairs(params.iter().copied());
    let bytes = fetch_bytes(client, url.as_str(), api_key, deadline, 256 * 1024).await?;
    serde_json::from_slice(&bytes).map_err(|_| "invalid_response")
}

fn store_failure(error: gateway_core::error::StoreError) -> ProviderAdminError {
    ProviderAdminError::new(ProviderAdminErrorKind::Unavailable).with_source(error)
}

fn query_timeout(mode: QuotaQueryMode) -> Duration {
    Duration::from_secs(match mode {
        QuotaQueryMode::Cpr => 5,
        QuotaQueryMode::KimiCoding
        | QuotaQueryMode::KimiCodingGlobal
        | QuotaQueryMode::CommandCode => 15,
        _ => 10,
    })
}

fn invalid_response() -> ProviderAdminError {
    ProviderAdminError::new(ProviderAdminErrorKind::BadGateway)
        .with_public_message("上游额度格式不兼容")
}

fn query_failure(code: &str) -> ProviderAdminError {
    let message = gateway_admin::model::upstream_quota::quota_error_display(code);
    ProviderAdminError::new(ProviderAdminErrorKind::BadGateway).with_public_message(message)
}
