//! 非秘密额度快照、Provider 查询设置与 Client Key 分享的公开服务合同

use serde::{Deserialize, Serialize};

use super::Operation;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountQuotaQuery {
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaQuerySettings {
    pub account_id: String,
    pub credential_revision: u64,
    pub mode: QuotaQueryMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateQuotaQuerySettings {
    pub account_id: String,
    pub expected_credential_revision: u64,
    pub mode: QuotaQueryMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountQuotaSnapshot {
    pub account_id: String,
    pub credential_revision: u64,
    pub name: String,
    pub observed_at: Option<String>,
    pub windows: Vec<QuotaWindow>,
    pub upstream: Option<AccountUpstreamQuota>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountUpstreamQuota {
    pub credential_revision: u64,
    pub fetched_at: Option<String>,
    pub last_attempt_at: Option<String>,
    pub last_error_code: Option<String>,
    pub forwarding_blocked: bool,
    pub stale: bool,
    pub accounts: Vec<QuotaAccount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaAccount {
    pub id: String,
    pub label: String,
    pub models: Option<Vec<String>>,
    pub status: QuotaObservationStatus,
    pub observed_at: Option<String>,
    pub windows: Vec<QuotaWindow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub balances: Vec<QuotaBalance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaObservationStatus {
    Observed,
    Unknown,
    Stale,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    pub key: String,
    pub group: String,
    pub label: String,
    pub role: Option<QuotaWindowRole>,
    pub window_seconds: Option<u64>,
    pub used_percent: Option<f64>,
    pub reset_at: Option<String>,
    pub limit_reached: bool,
    pub limit_id: Option<String>,
    pub limit_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaWindowRole {
    Primary,
    Secondary,
    Monthly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaBalance {
    pub label: String,
    pub remaining: Option<String>,
    pub used: Option<String>,
    pub total: Option<String>,
    pub unit: Option<String>,
    pub unlimited: bool,
    pub is_valid: bool,
    pub invalid_message: Option<String>,
    pub window_seconds: Option<u64>,
    pub reset_at: Option<String>,
    pub details: Vec<QuotaBalanceDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaBalanceDetail {
    pub label: String,
    pub amount: Option<String>,
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

/// 按指定 Client Key 的当前访问范围和分享设置过滤后的非秘密导出
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaResponse {
    pub schema_version: u32,
    pub accounts: Vec<QuotaAccount>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaSharingSettings {
    pub enabled: bool,
    pub include_names: bool,
    pub include_models: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaSharingSnapshot {
    pub client_key_id: String,
    pub config_revision: u64,
    pub settings: QuotaSharingSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateQuotaSharing {
    pub client_key_id: String,
    pub expected_config_revision: u64,
    pub settings: QuotaSharingSettings,
}

pub struct GetAccountQuota;
impl Operation for GetAccountQuota {
    const NAME: &'static str = "quota.account_snapshot";
    type Input = AccountQuotaQuery;
    type Output = AccountQuotaSnapshot;
}

pub struct GetQuotaQuerySettings;
impl Operation for GetQuotaQuerySettings {
    const NAME: &'static str = "quota.query_settings";
    type Input = AccountQuotaQuery;
    type Output = QuotaQuerySettings;
}

pub struct SetQuotaQuerySettings;
impl Operation for SetQuotaQuerySettings {
    const NAME: &'static str = "quota.set_query_settings";
    type Input = UpdateQuotaQuerySettings;
    type Output = QuotaQuerySettings;
}

pub struct RefreshAccountQuota;
impl Operation for RefreshAccountQuota {
    const NAME: &'static str = "quota.refresh_account";
    type Input = AccountQuotaQuery;
    type Output = QuotaRefreshResult;
}

pub struct GetQuotaSharing;
impl Operation for GetQuotaSharing {
    const NAME: &'static str = "quota.sharing_settings";
    type Input = QuotaSharingQuery;
    type Output = QuotaSharingSnapshot;
}

pub struct SetQuotaSharing;
impl Operation for SetQuotaSharing {
    const NAME: &'static str = "quota.set_sharing_settings";
    type Input = UpdateQuotaSharing;
    type Output = QuotaSharingSnapshot;
}

pub struct ExportQuota;
impl Operation for ExportQuota {
    const NAME: &'static str = "quota.export";
    type Input = QuotaSharingQuery;
    type Output = QuotaResponse;
}
