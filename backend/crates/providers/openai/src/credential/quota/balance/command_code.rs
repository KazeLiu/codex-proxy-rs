//! Command Code 五小时、周和账期月额度的已用百分比投影

use super::*;
use gateway_admin::model::quota::QuotaWindowRole;

pub(super) fn parse(value: &Value) -> Result<Vec<QuotaWindow>, ProviderAdminError> {
    let credits_response = &value["credits"];
    let credits = credits_response
        .get("credits")
        .filter(|credits| credits.is_object())
        .ok_or_else(invalid_response)?;
    let subscription = &value["subscription"]["data"];
    let mut entries = Vec::new();
    // 官方响应存在同级及嵌套两种窗口位置，与 CC Switch 使用相同的读取顺序
    let limits = credits_response
        .get("windowLimits")
        .or_else(|| credits_response["credits"].get("windowLimits"));
    if let Some(limits) = limits
        && limits["limited"] == true
    {
        for (key, label, role, seconds) in [
            (
                "fiveHour",
                "5 小时额度",
                QuotaWindowRole::Primary,
                5 * 60 * 60,
            ),
            (
                "weekly",
                "周额度",
                QuotaWindowRole::Secondary,
                7 * 24 * 60 * 60,
            ),
        ] {
            if let Some(window) = limits.get(key)
                && let Some(entry) = window_entry(window, key, label, role, seconds)?
            {
                entries.push(entry);
            }
        }
    }
    // 沿用 CC Switch 的账期额度口径，缺少任一组成值时不把它补成零
    let used_percent = match [
        optional_quota_number(&credits["monthlyCredits"])?,
        optional_quota_number(&credits["purchasedCredits"])?,
        optional_quota_number(&credits["freeCredits"])?,
        optional_quota_number(&value["summary"]["totalCost"])?,
    ] {
        [Some(monthly), Some(purchased), Some(free), Some(spent)] => {
            let spent = spent.max(0.0);
            let pool = spent + monthly.max(0.0) + purchased.max(0.0) + free.max(0.0);
            Some(if pool > 0.0 {
                spent / pool * 100.0
            } else {
                0.0
            })
        }
        _ => None,
    };
    let reset_at = kimi_reset_at(&subscription["currentPeriodEnd"])?;
    let window_seconds = kimi_reset_at(&subscription["currentPeriodStart"])?
        .zip(reset_at)
        .and_then(|(start, end)| (end - start).to_std().ok())
        .map(|duration| duration.as_secs())
        .filter(|seconds| *seconds > 0);
    entries.push(QuotaWindow {
        key: "command_code:monthly".to_owned(),
        group: "longTerm".to_owned(),
        label: "月额度".to_owned(),
        role: Some(QuotaWindowRole::Monthly),
        window_seconds,
        used_percent,
        reset_at,
        limit_reached: used_percent.is_some_and(|used| used >= 100.0),
        limit_id: None,
        limit_name: None,
    });
    Ok(entries)
}

fn window_entry(
    window: &Value,
    key: &str,
    label: &str,
    role: QuotaWindowRole,
    seconds: u64,
) -> Result<Option<QuotaWindow>, ProviderAdminError> {
    let cap = optional_quota_number(&window["cap"])?;
    // cap 为零的窗口不提供有限额度进度，不能把缺少 cap 当成零
    if cap.is_some_and(|cap| cap <= 0.0) {
        return Ok(None);
    }
    let used = optional_quota_number(&window["used"])?;
    let percent = used
        .zip(cap)
        .map(|(used, cap)| (used.max(0.0) / cap * 100.0).clamp(0.0, 100.0));
    Ok(Some(QuotaWindow {
        key: format!("command_code:{key}"),
        group: "shortTerm".to_owned(),
        label: label.to_owned(),
        role: Some(role),
        window_seconds: Some(seconds),
        used_percent: percent,
        reset_at: kimi_reset_at(&window["resetAt"])?,
        limit_reached: percent.is_some_and(|used| used >= 100.0),
        limit_id: None,
        limit_name: None,
    }))
}
