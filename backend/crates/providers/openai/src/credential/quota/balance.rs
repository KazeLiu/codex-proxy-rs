//! Kimi、DeepSeek 与 Sub2API 余额响应的非秘密投影，不负责网络或持久化

use chrono::{DateTime, Utc};
use gateway_admin::model::quota::{
    QuotaAccount, QuotaBalance as AccountBalanceEntry, QuotaBalanceDetail as AccountBalanceDetail,
    QuotaObservationStatus, QuotaResponse, QuotaWindow,
};
use gateway_admin::model::upstream_quota::QuotaQueryMode;
use gateway_admin::ports::provider::{ProviderAdminError, ProviderAdminErrorKind};
use serde_json::{Number, Value};

pub(crate) fn parse(
    value: &Value,
    mode: QuotaQueryMode,
    observed_at: DateTime<Utc>,
) -> Result<QuotaResponse, ProviderAdminError> {
    let entries = match mode {
        QuotaQueryMode::Kimi => parse_kimi(value, "CNY")?,
        QuotaQueryMode::KimiGlobal => parse_kimi(value, "USD")?,
        QuotaQueryMode::KimiCoding | QuotaQueryMode::KimiCodingGlobal => parse_kimi_coding(value)?,
        QuotaQueryMode::Deepseek => parse_deepseek(value)?,
        QuotaQueryMode::Sub2api => parse_sub2api(value)?,
        QuotaQueryMode::Disabled | QuotaQueryMode::Cpr => return Err(invalid_response()),
    };
    let mut account = QuotaAccount {
        id: "balance".to_owned(),
        label: "上游额度".to_owned(),
        models: None,
        status: QuotaObservationStatus::Observed,
        observed_at: Some(observed_at),
        windows: Vec::new(),
        balances: Vec::new(),
    };
    for (index, entry) in entries.into_iter().enumerate() {
        if entry.unit.as_deref() != Some("%") {
            account.balances.push(entry);
            continue;
        }
        let used_percent = entry
            .used
            .as_deref()
            .map(|value| value.parse::<f64>().map_err(|_| invalid_response()))
            .transpose()?;
        account.windows.push(QuotaWindow {
            key: format!("balance:{index}"),
            group: "shortTerm".to_owned(),
            label: entry.label,
            role: None,
            window_seconds: entry.window_seconds,
            used_percent,
            reset_at: entry.reset_at,
            limit_reached: used_percent.is_some_and(|value| value >= 100.0),
            limit_id: None,
            limit_name: None,
        });
    }
    let response = QuotaResponse {
        schema_version: 1,
        accounts: vec![account],
    };
    response.validate().map_err(|_| invalid_response())?;
    Ok(response)
}

pub(crate) fn query_url(
    base_url: &str,
    mode: QuotaQueryMode,
) -> Result<url::Url, ProviderAdminError> {
    let (base_url, suffix) = match mode {
        QuotaQueryMode::Kimi => ("https://api.moonshot.cn", "/v1/users/me/balance"),
        QuotaQueryMode::KimiGlobal => ("https://api.moonshot.ai", "/v1/users/me/balance"),
        QuotaQueryMode::KimiCoding => ("https://api.kimi.com", "/coding/v1/usages"),
        QuotaQueryMode::KimiCodingGlobal => ("https://api.kimi.ai", "/coding/v1/usages"),
        QuotaQueryMode::Deepseek => (base_url, "/user/balance"),
        QuotaQueryMode::Sub2api => (base_url, "/v1/usage"),
        QuotaQueryMode::Disabled | QuotaQueryMode::Cpr => return Err(invalid_response()),
    };
    let mut url =
        crate::transport::parse_upstream_base_url(base_url).ok_or_else(invalid_response)?;
    let path = url.path().trim_end_matches('/');
    let root = path.strip_suffix("/v1").unwrap_or(path);
    url.set_path(&format!("{root}{suffix}"));
    Ok(url)
}

fn invalid_response() -> ProviderAdminError {
    ProviderAdminError::new(ProviderAdminErrorKind::BadGateway)
        .with_public_message("上游余额响应格式不符合所选查询类型")
}

fn amount(value: &Value) -> Result<String, ProviderAdminError> {
    let text = match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        _ => return Err(invalid_response()),
    };
    if text.len() > 128 || text.parse::<Number>().is_err() {
        return Err(invalid_response());
    }
    Ok(text)
}

fn optional_amount(value: &Value) -> Result<Option<String>, ProviderAdminError> {
    if value.is_null() {
        Ok(None)
    } else {
        amount(value).map(Some)
    }
}

fn entry(label: &str, unit: Option<&str>) -> AccountBalanceEntry {
    AccountBalanceEntry {
        label: label.to_owned(),
        remaining: None,
        used: None,
        total: None,
        unit: unit.map(str::to_owned),
        unlimited: false,
        is_valid: true,
        invalid_message: None,
        window_seconds: None,
        reset_at: None,
        details: Vec::new(),
    }
}

fn parse_kimi(value: &Value, unit: &str) -> Result<Vec<AccountBalanceEntry>, ProviderAdminError> {
    if value["status"] != true || value["code"] != 0 {
        return Err(invalid_response());
    }
    let mut result = entry("可用余额", Some(unit));
    result.remaining = optional_amount(&value["data"]["available_balance"])?;
    for (key, label) in [
        ("cash_balance", "现金余额"),
        ("voucher_balance", "代金券余额"),
    ] {
        result.details.push(AccountBalanceDetail {
            label: label.to_owned(),
            amount: optional_amount(&value["data"][key])?,
        });
    }
    Ok(vec![result])
}

fn parse_kimi_coding(value: &Value) -> Result<Vec<AccountBalanceEntry>, ProviderAdminError> {
    let mut entries = Vec::new();
    // 与 CC Switch 相同，先展示短周期，再展示订阅周额度
    if let Some(limits) = value["limits"].as_array() {
        for limit in limits {
            if let Some(detail) = limit.get("detail") {
                entries.push(kimi_quota_entry(detail, "5 小时额度", 5 * 60 * 60)?);
            }
        }
    }
    if let Some(usage) = value.get("usage") {
        entries.push(kimi_quota_entry(usage, "周额度", 7 * 24 * 60 * 60)?);
    }
    if entries.is_empty() {
        return Err(invalid_response());
    }
    Ok(entries)
}

fn kimi_quota_entry(
    detail: &Value,
    label: &str,
    window_seconds: u64,
) -> Result<AccountBalanceEntry, ProviderAdminError> {
    let limit = optional_quota_number(&detail["limit"])?;
    if limit.is_some_and(|value| value <= 0.0) {
        return Err(invalid_response());
    }
    let used = if detail["remaining"].is_null() {
        // 上游也会只返回 used，不能将缺失的 remaining 当成零剩余
        optional_quota_number(&detail["used"])?
    } else {
        limit
            .zip(optional_quota_number(&detail["remaining"])?)
            .map(|(limit, remaining)| limit - remaining)
    };
    let used_percent = used
        .zip(limit)
        .map(|(used, limit)| (used.max(0.0) / limit * 100.0).clamp(0.0, 100.0));
    let mut result = entry(label, Some("%"));
    result.remaining = used_percent.map(|used| kimi_percentage(100.0 - used));
    result.used = used_percent.map(kimi_percentage);
    result.total = limit.map(|_| "100".to_owned());
    result.window_seconds = Some(window_seconds);
    result.reset_at = kimi_reset_at(&detail["resetTime"])?;
    Ok(result)
}

fn optional_quota_number(value: &Value) -> Result<Option<f64>, ProviderAdminError> {
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        .filter(|number| number.is_finite())
        .map(Some)
        .ok_or_else(invalid_response)
}

fn kimi_percentage(value: f64) -> String {
    format!("{value:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn kimi_reset_at(value: &Value) -> Result<Option<DateTime<Utc>>, ProviderAdminError> {
    if value.is_null() {
        return Ok(None);
    }
    if let Some(text) = value.as_str() {
        return DateTime::parse_from_rfc3339(text)
            .map(|time| Some(time.with_timezone(&Utc)))
            .map_err(|_| invalid_response());
    }
    let timestamp = value.as_i64().ok_or_else(invalid_response)?;
    if timestamp <= 0 {
        return Ok(None);
    }
    // 沿用 CC Switch 的秒、毫秒时间戳分界
    let time = if timestamp < 1_000_000_000_000 {
        DateTime::from_timestamp(timestamp, 0)
    } else {
        DateTime::from_timestamp_millis(timestamp)
    };
    time.map(Some).ok_or_else(invalid_response)
}

fn parse_deepseek(value: &Value) -> Result<Vec<AccountBalanceEntry>, ProviderAdminError> {
    let available = value["is_available"]
        .as_bool()
        .ok_or_else(invalid_response)?;
    let infos = value["balance_infos"]
        .as_array()
        .ok_or_else(invalid_response)?;
    if infos.is_empty() {
        return Err(invalid_response());
    }
    infos
        .iter()
        .map(|info| {
            let currency = match &info["currency"] {
                Value::Null => None,
                Value::String(value) if matches!(value.as_str(), "CNY" | "USD") => {
                    Some(value.as_str())
                }
                _ => return Err(invalid_response()),
            };
            let mut result = entry("可用余额", currency);
            result.remaining = optional_amount(&info["total_balance"])?;
            result.is_valid = available;
            result.invalid_message = (!available).then(|| "余额不足".to_owned());
            for (key, label) in [
                ("granted_balance", "赠送余额"),
                ("topped_up_balance", "充值余额"),
            ] {
                result.details.push(AccountBalanceDetail {
                    label: label.to_owned(),
                    amount: optional_amount(&info[key])?,
                });
            }
            Ok(result)
        })
        .collect()
}

fn parse_sub2api(value: &Value) -> Result<Vec<AccountBalanceEntry>, ProviderAdminError> {
    // Sub2API 的 CC Switch 模板允许缺少 isValid，明确 false 时才显示无效
    if !value.is_object() || !value["error"].is_null() {
        return Err(invalid_response());
    }
    let valid = value["isValid"].as_bool().unwrap_or(true);
    let unit = value["unit"]
        .as_str()
        .or_else(|| value["quota"]["unit"].as_str());
    let mut result = entry(value["planName"].as_str().unwrap_or("上游额度"), unit);
    result.is_valid = valid;
    result.invalid_message = (!valid).then(|| "上游 Key 不可用".to_owned());
    let remaining = optional_amount(&value["remaining"])?;
    result.unlimited = value["subscription"].is_object() && remaining.as_deref() == Some("-1");
    result.remaining = remaining.filter(|_| !result.unlimited);
    if value["quota"]["limit"]
        .as_f64()
        .is_some_and(|limit| limit > 0.0)
    {
        result.label = "Key 总额度".to_owned();
        result.used = optional_amount(&value["quota"]["used"])?;
        result.total = Some(amount(&value["quota"]["limit"])?);
    }
    let mut entries = vec![result];
    if value["subscription"].is_object() {
        for (prefix, label) in [
            ("daily", "订阅日额度"),
            ("weekly", "订阅周额度"),
            ("monthly", "订阅月额度"),
        ] {
            let limit = &value["subscription"][format!("{prefix}_limit_usd")];
            if limit.as_f64().is_some_and(|limit| limit > 0.0) {
                let mut window = entry(label, Some("USD"));
                window.total = Some(amount(limit)?);
                window.used =
                    optional_amount(&value["subscription"][format!("{prefix}_usage_usd")])?;
                entries.push(window);
            }
        }
    }
    if let Some(windows) = value["rate_limits"].as_array() {
        for window in windows {
            let label = match window["window"].as_str() {
                Some("5h") => "Key 5 小时额度",
                Some("1d") => "Key 日额度",
                Some("7d") => "Key 周额度",
                _ => return Err(invalid_response()),
            };
            let mut item = entry(label, unit);
            item.remaining = optional_amount(&window["remaining"])?;
            item.used = optional_amount(&window["used"])?;
            item.total = optional_amount(&window["limit"])?;
            item.window_seconds = match window["window"].as_str() {
                Some("5h") => Some(5 * 60 * 60),
                Some("1d") => Some(24 * 60 * 60),
                Some("7d") => Some(7 * 24 * 60 * 60),
                _ => None,
            };
            item.reset_at = window["reset_at"]
                .as_str()
                .map(|time| DateTime::parse_from_rfc3339(time).map(|time| time.with_timezone(&Utc)))
                .transpose()
                .map_err(|_| invalid_response())?;
            entries.push(item);
        }
    }
    if entries[0].remaining.is_none()
        && entries[0].total.is_none()
        && !entries[0].unlimited
        && valid
        && value["planName"].as_str().is_none()
    {
        if entries.len() == 1 {
            return Err(invalid_response());
        }
        entries.remove(0);
    }
    Ok(entries)
}
