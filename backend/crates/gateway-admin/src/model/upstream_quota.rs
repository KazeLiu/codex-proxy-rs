//! 额度展示快照与只读导出的管理合同，密钥材料不进入调试输出

use crate::model::quota::QuotaAccount;
use chrono::{DateTime, Utc};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};

use super::Revision;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountUpstreamQuota {
    pub credential_revision: u64,
    pub fetched_at: Option<DateTime<Utc>>,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub last_error_code: Option<String>,
    pub forwarding_blocked: bool,
    pub stale: bool,
    pub accounts: Vec<QuotaAccount>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaQueryMode {
    #[default]
    Disabled,
    Cpr,
    Kimi,
    KimiGlobal,
    KimiCoding,
    KimiCodingGlobal,
    Deepseek,
    Sub2api,
    CommandCode,
}

impl QuotaQueryMode {
    pub const fn refresh_interval(self) -> std::time::Duration {
        std::time::Duration::from_secs(if matches!(self, Self::Cpr) { 60 } else { 300 })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountQuotaQuery {
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaQuerySettings {
    pub account_id: String,
    pub credential_revision: Revision,
    pub mode: QuotaQueryMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateQuotaQuerySettings {
    pub account_id: String,
    pub expected_credential_revision: Revision,
    pub mode: QuotaQueryMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountQuotaSnapshot {
    pub account_id: String,
    pub credential_revision: Revision,
    pub name: String,
    pub observed_at: Option<DateTime<Utc>>,
    pub windows: Vec<crate::model::quota::QuotaWindow>,
    pub upstream: Option<AccountUpstreamQuota>,
}

impl From<super::provider_credentials::ProviderQuotaWindow> for crate::model::quota::QuotaWindow {
    fn from(window: super::provider_credentials::ProviderQuotaWindow) -> Self {
        use super::provider_credentials::ProviderQuotaWindowRole;
        use crate::model::quota::QuotaWindowRole;
        Self {
            key: window.key,
            group: window.group,
            label: window.label,
            role: window.role.map(|role| match role {
                ProviderQuotaWindowRole::Primary => QuotaWindowRole::Primary,
                ProviderQuotaWindowRole::Secondary => QuotaWindowRole::Secondary,
                ProviderQuotaWindowRole::Monthly => QuotaWindowRole::Monthly,
            }),
            window_seconds: window.window_seconds,
            used_percent: window.used_percent,
            reset_at: window.reset_at,
            limit_reached: window.limit_reached,
            limit_id: window.limit_id,
            limit_name: window.limit_name,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaRefreshError {
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaRefreshResult {
    pub snapshot: AccountQuotaSnapshot,
    pub error: Option<QuotaRefreshError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaSharingQuery {
    pub client_key_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaSharingSnapshot {
    pub client_key_id: String,
    pub config_revision: Revision,
    pub settings: QuotaSharingSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateQuotaSharing {
    pub client_key_id: String,
    pub expected_config_revision: Revision,
    pub settings: QuotaSharingSettings,
}

pub struct QuotaExportIdentity {
    pub enabled: bool,
    pub sharing: QuotaSharingSettings,
    pub secret: SecretString,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaSharingSettings {
    pub enabled: bool,
    pub include_names: bool,
    pub include_models: bool,
}

impl Default for QuotaSharingSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            include_names: false,
            include_models: false,
        }
    }
}

#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum QuotaExportError {
    #[error("Client key is not available")]
    Unauthorized,
    #[error("Quota sharing is not enabled for this key")]
    SharingDisabled,
    #[error("Quota is temporarily unavailable")]
    Unavailable,
    #[error("Quota response exceeds the supported size")]
    ResponseTooLarge,
}

pub fn quota_error_display(code: &str) -> &'static str {
    match code {
        "unsupported" => "上游不支持额度查询",
        "sharing_disabled" => "上游未开放额度共享",
        "unauthorized" => "上游额度查询认证失败",
        "forbidden" => "没有上游额度查询权限",
        "timeout" => "上游额度查询超时",
        "invalid_response" => "上游额度格式不兼容",
        "response_too_large" => "上游额度响应过大",
        "rate_limited" => "上游额度查询频率受限",
        _ => "上游额度获取失败",
    }
}
