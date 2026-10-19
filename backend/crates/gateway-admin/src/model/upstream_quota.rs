//! 额度展示快照与只读导出的管理合同，密钥材料不进入调试输出

use chrono::{DateTime, Utc};
use gateway_protocol::quota::QuotaAccount;
use secrecy::SecretString;

#[derive(Debug, Clone)]
pub struct AccountUpstreamQuota {
    pub fetched_at: Option<DateTime<Utc>>,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub last_error_code: Option<String>,
    pub forwarding_blocked: bool,
    pub accounts: Vec<QuotaAccount>,
}

pub struct QuotaExportIdentity {
    pub enabled: bool,
    pub sharing_enabled: bool,
    pub secret: SecretString,
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
