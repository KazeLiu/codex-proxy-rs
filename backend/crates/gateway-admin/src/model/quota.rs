//! 额度展示与跨网关分享的非秘密值合同及输入校验

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};

pub const MAX_QUOTA_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaResponse {
    pub schema_version: u32,
    pub accounts: Vec<QuotaAccount>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaAccount {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<String>>,
    pub status: QuotaObservationStatus,
    #[serde(deserialize_with = "required_nullable")]
    pub observed_at: Option<DateTime<Utc>>,
    pub windows: Vec<QuotaWindow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub balances: Vec<QuotaBalance>,
}

/// 金额与数量保留上游十进制文本，不将金额转换为百分比
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    pub reset_at: Option<DateTime<Utc>>,
    pub details: Vec<QuotaBalanceDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaBalanceDetail {
    pub label: String,
    pub amount: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaObservationStatus {
    Observed,
    Unknown,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    pub key: String,
    pub group: String,
    pub label: String,
    #[serde(deserialize_with = "required_nullable")]
    pub role: Option<QuotaWindowRole>,
    #[serde(deserialize_with = "required_nullable")]
    pub window_seconds: Option<u64>,
    #[serde(deserialize_with = "required_nullable")]
    pub used_percent: Option<f64>,
    #[serde(deserialize_with = "required_nullable")]
    pub reset_at: Option<DateTime<Utc>>,
    pub limit_reached: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaWindowRole {
    Primary,
    Secondary,
    Monthly,
}

impl QuotaResponse {
    pub fn validate(&self) -> Result<(), InvalidQuotaResponse> {
        if self.schema_version != 1 {
            return Err(InvalidQuotaResponse);
        }
        let mut ids = BTreeSet::new();
        for account in &self.accounts {
            if !valid_text(&account.id)
                || !valid_text(&account.label)
                || !ids.insert(&account.id)
                || match account.status {
                    QuotaObservationStatus::Unknown => {
                        account.observed_at.is_some()
                            || !account.windows.is_empty()
                            || !account.balances.is_empty()
                    }
                    QuotaObservationStatus::Observed | QuotaObservationStatus::Stale => {
                        account.observed_at.is_none()
                    }
                }
            {
                return Err(InvalidQuotaResponse);
            }
            if let Some(models) = &account.models {
                let mut seen = BTreeSet::new();
                if models
                    .iter()
                    .any(|model| !valid_text(model) || !seen.insert(model))
                {
                    return Err(InvalidQuotaResponse);
                }
            }
            let mut keys = BTreeSet::new();
            for balance in &account.balances {
                if !valid_text(&balance.label)
                    || balance
                        .unit
                        .as_deref()
                        .is_some_and(|unit| !valid_text(unit))
                    || balance.window_seconds == Some(0)
                    || balance
                        .invalid_message
                        .as_deref()
                        .is_some_and(|value| !valid_text(value))
                    || [&balance.remaining, &balance.used, &balance.total]
                        .into_iter()
                        .flatten()
                        .any(|value| !valid_amount(value))
                    || balance.details.iter().any(|detail| {
                        !valid_text(&detail.label)
                            || detail
                                .amount
                                .as_deref()
                                .is_some_and(|value| !valid_amount(value))
                    })
                {
                    return Err(InvalidQuotaResponse);
                }
            }
            for window in &account.windows {
                if !valid_text(&window.key)
                    || !valid_text(&window.group)
                    || !valid_text(&window.label)
                    || !keys.insert(&window.key)
                    || window.window_seconds == Some(0)
                    || window
                        .used_percent
                        .is_some_and(|value| !value.is_finite() || !(0.0..=100.0).contains(&value))
                    || window
                        .limit_id
                        .as_deref()
                        .is_some_and(|value| !valid_text(value))
                    || window
                        .limit_name
                        .as_deref()
                        .is_some_and(|value| !valid_text(value))
                {
                    return Err(InvalidQuotaResponse);
                }
            }
        }
        Ok(())
    }
}

impl QuotaAccount {
    pub fn unknown(id: String, label: String) -> Self {
        Self {
            id,
            label,
            models: None,
            status: QuotaObservationStatus::Unknown,
            observed_at: None,
            windows: Vec::new(),
            balances: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("upstream quota response is invalid")]
pub struct InvalidQuotaResponse;

fn valid_text(value: &str) -> bool {
    !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
}

fn valid_amount(value: &str) -> bool {
    value.len() <= 128 && value.parse::<serde_json::Number>().is_ok()
}

fn required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}
