# 额度查询与分享

插件页面先读取账号的查询设置，再显式选择上游类型。宿主使用当前账号凭据和出站代理获取额度、保存快照并负责后台刷新，页面只需读取展示结果。向下游透传由已安装的请求中间件插件提供，宿主不自行响应额度结果

需要 SDK `0.2.0`、宿主 `3.30.1` 或更高版本，RPC 仍为 v5。插件的 `engines.codex-proxy-rs` 应声明 `>=3.30.1, <4.0.0`；这些操作通过现有 `HostClient::service` 调用，在服务器和插件 CLI 的公开服务注册表中提供

## 读取设置并配置查询

```rust,ignore
use gateway_plugin_sdk::call::services::quota::{
    AccountQuotaQuery, GetQuotaQuerySettings, QuotaQueryMode,
    SetQuotaQuerySettings, UpdateQuotaQuerySettings,
};

let current = host.service::<GetQuotaQuerySettings>(AccountQuotaQuery {
    account_id: account_id.clone(),
}).await?;
let saved = host.service::<SetQuotaQuerySettings>(UpdateQuotaQuerySettings {
    account_id: account_id.clone(),
    expected_credential_revision: current.credential_revision,
    mode: QuotaQueryMode::Cpr,
}).await?;
```

`QuotaQueryMode` 的默认值为 `Disabled`，不自动探测上游类型。API Key 账号支持以下模式，OAuth 账号不使用此查询设置

| Rust 值 / JSON 值 | 查询目标 |
| --- | --- |
| `Disabled` / `disabled` | 不查询 |
| `Cpr` / `cpr` | 账号 Base URL 下的 `/quota`，通常为 `/v1/quota` |
| `Kimi` / `kimi` | 国内按量付费，`api.moonshot.cn/v1/users/me/balance` |
| `KimiGlobal` / `kimi_global` | 海外按量付费，`api.moonshot.ai/v1/users/me/balance` |
| `KimiCoding` / `kimi_coding` | 国内编程订阅，`api.kimi.com/coding/v1/usages` |
| `KimiCodingGlobal` / `kimi_coding_global` | 海外编程订阅，`api.kimi.ai/coding/v1/usages` |
| `Deepseek` / `deepseek` | 账号 Base URL 去掉末尾 `/v1` 后的 `/user/balance` |
| `Sub2api` / `sub2api` | 账号 Base URL 去掉末尾 `/v1` 后的 `/v1/usage` |
| `CommandCode` / `command_code` | `api.commandcode.ai/alpha` 下的身份、credits、订阅和用量接口 |

Command Code 沿用 CC Switch 的查询流程：`whoami?limits=1` 取得组织范围，再读取 `billing/credits`、`billing/subscriptions`，按订阅起点及组织范围读取 `usage/summary`。控制面固定使用官方根域名，不拼接账号的 `/provider` 或 `/provider/v1`。五小时和周窗口使用上游明确的 `used/cap`；月额度使用 `totalCost / (totalCost + monthlyCredits + purchasedCredits + freeCredits) * 100`，各组成值缺失时保持未知。返回三类已用百分比窗口，月窗口标记为 `Monthly`，时长与重置时间来自订阅起止，不返回 credits 或费用明细

CPR 与余额查询互斥，分别复用 Provider 的 `fetch_upstream_quota` 和 `balance_query` 配置。保存只修改查询配置，不要求读取或重新提交 Key，不修改账号名称、分组、模型策略、代理或传输设置

保存使用 `expected_credential_revision` 做事务内条件更新，并返回新的 `credential_revision`。凭据或查询配置已变化时返回 `ServiceError.kind = "conflict"`，调用者应重新读取设置后决定是否保存，不自动覆盖并发修改

保存后由宿主后台发起首次查询。服务运行期间，CPR 后续按一分钟间隔、其他模式按五分钟间隔复用额度 worker 刷新；停用账号不主动查询。插件 CLI 不启动周期 worker

## 读取完整展示快照

```rust,ignore
use gateway_plugin_sdk::call::services::quota::{AccountQuotaQuery, GetAccountQuota};

let snapshot = host.service::<GetAccountQuota>(AccountQuotaQuery {
    account_id: account_id.clone(),
}).await?;
```

此操作只读取已保存数据，不访问上游。可读取停用账号的已有快照，账号在读取期间更换凭据时返回 `conflict`

| 类型 / 字段 | 展示含义 |
| --- | --- |
| `AccountQuotaSnapshot` | 宿主账号 ID、名称、凭据版本、原生 `observed_at` 与完整 `windows` |
| `upstream: Option<AccountUpstreamQuota>` | API Key 查询的保存结果；未开启查询时为 `None` |
| `upstream.credential_revision` | 该展示快照所属的账号凭据版本，宿主导出时再次核对 |
| `upstream.accounts: Vec<QuotaAccount>` | 上游账号 ID、名称 `label`、已分享模型 `models`、观测状态、观测时间、窗口与余额 |
| `QuotaWindow` | 稳定窗口键、分组、标签、角色、时长、已用百分比、重置时间、是否触顶及额度名称 |
| `QuotaBalance` | `remaining`、`used`、`total`、币种或单位 `unit`、`unlimited`、有效性、重置时间及 `details` |
| `QuotaBalanceDetail` | 明细标签与可为空的十进制金额 |
| `upstream.fetched_at` | 宿主最近成功保存快照的时间 |
| `upstream.last_attempt_at` / `last_error_code` | 最近查询尝试时间及安全错误码，不包含上游原始响应 |
| `upstream.stale` | 最近查询失败、上游标记陈旧，或宿主成功快照已超过十分钟 |
| `upstream.forwarding_blocked` | 上游拒绝认证或权限后暂停向下游分享，仍允许管理页面查看旧快照 |

时间为 UTC RFC 3339 字符串，未知值为 `None`。原生额度的观测时间位于顶层，API Key 上游各账号的观测时间位于 `upstream.accounts[].observed_at`；尚无成功样本时 `fetched_at=None`、`accounts` 为空

金额使用十进制字符串，不补零、不换算为百分比；额度窗口统一使用 `used_percent` 已用口径，与账号管理一致，透传不反转数值。`models=None` 表示未取得模型列表或上游未分享，不等于分享了一个空列表。查询结果不含 Provider 原始文档、凭据或代理认证信息

Kimi、DeepSeek、Sub2API、Command Code 的查询同时复用宿主模型目录服务，按当前账号凭据版本读取缓存，缺失时查询账号模型接口，并将取得的模型 ID 保存到同一额度快照。模型获取与余额请求共用本次查询期限，失败不丢弃已取得的余额。CPR 模式只保留上游额度响应内已分享的模型，不用上游整站模型目录补全各账号

## 手动刷新并读取结果

```rust,ignore
use gateway_plugin_sdk::call::services::quota::{AccountQuotaQuery, RefreshAccountQuota};

let result = host.service::<RefreshAccountQuota>(AccountQuotaQuery {
    account_id: account_id.clone(),
}).await?;
let snapshot = result.snapshot;
if let Some(error) = result.error {
    // 展示安全错误文案，同时保留 snapshot 中可用的旧数据
    eprintln!("{}: {}", error.kind, error.message);
}
```

此操作等待 Provider 的本次刷新结束，再返回完整快照。查询失败不删除可用旧快照；API Key 查询的错误和陈旧标记一并保存。无效账号、快照读取失败等无法提供完整结果的情况直接返回 `ServiceError`

同账号、同凭据版本的并发刷新合并；API Key 主动查询至少间隔十秒，Kimi 编程订阅与 Command Code 至少十五秒，过于频繁时 `result.error.kind = "rate_limited"`，仍返回可用快照。需要保存后立即展示时，调用上例的 `RefreshAccountQuota` 等待或复用首次查询结果；只调用 `GetAccountQuota` 不会等待后台首次查询完成

CPR 请求期限五秒、响应上限 1 MiB；其他模式十秒，Kimi 编程订阅与 Command Code 十五秒，单个响应上限 256 KiB。Command Code 四步查询及模型获取共用十五秒期限。期限覆盖发起请求和读取正文，父调用取消也会结束等待。凭据版本、刷新尝试标识共同隔离迟到响应。操作未返回不代表没有保存，可调用 `GetAccountQuota` 重新读取，不自动重试查询

API Key 查询只更新展示快照，不改变推理调度、凭据健康状态或本地计费。OAuth 原生刷新仍沿用 Provider 的既有观测规则

## 配置 Client Key 分享

```rust,ignore
use gateway_plugin_sdk::call::services::quota::{
    GetQuotaSharing, QuotaSharingQuery, QuotaSharingSettings,
    SetQuotaSharing, UpdateQuotaSharing,
};

let current = host.service::<GetQuotaSharing>(QuotaSharingQuery {
    client_key_id: client_key_id.clone(),
}).await?;
let saved = host.service::<SetQuotaSharing>(UpdateQuotaSharing {
    client_key_id,
    expected_config_revision: current.config_revision,
    settings: QuotaSharingSettings {
        enabled: true,
        include_names: false,
        include_models: false,
    },
}).await?;
```

分享默认关闭，名称和模型默认不分享。开启后固定提供授权账号的额度，不存在单独关闭账号额度的选项。保存只替换这三个分享字段，保留 Key 的分组、模型策略、预算和其他配置；读取不返回明文 Key 或匿名标识密钥

`expected_config_revision` 为宿主配置版本，不是账号凭据版本。事务内比较版本，冲突返回 `ServiceError.kind = "conflict"`；保存返回当前分享设置和配置版本，相同设置不递增版本

下游以自己的 Client Key 查询 `GET /v1/quota`。宿主先完成数据面鉴权，再进入已有 request 中间件链；没有插件接管时返回 `404 quota_plugin_unavailable`。插件停用、卸载或其 binding 不覆盖当前请求时，不提供透传

## 在插件中提供额度透传

请求中间件检查 `call.request.head.endpoint == "/v1/quota"` 后，使用宿主签发的 `client_key_id` 导出额度并返回 JSON 短路响应；其他请求继续调用 `next`。不重新接收原始 Key，不自行解释账号范围

```rust,ignore
use gateway_plugin_sdk::call::services::quota::{ExportQuota, QuotaSharingQuery};

let quota = call.host.service::<ExportQuota>(QuotaSharingQuery {
    client_key_id: call.request.head.client_key_id.clone(),
}).await?;
let body = serde_json::to_vec(&quota)?;
```

`ExportQuota` 仅读取保存结果，按指定 Key 的当前发布策略重新限制账号范围和模型，检查 Key 启用与分享设置，并独立匿名化账号 ID。未知金额和币种保持为空。失败的 `ServiceError.kind` 为 `unauthorized`、`forbidden` 或 `unavailable`；插件应映射为 401、403 或 503，并设置 `Cache-Control: no-store`

每一跳重新应用名称和模型披露规则。上游返回 401、403 或关闭分享后，旧快照停止下传，直到同一凭据版本下成功查询后恢复。该服务不执行上游查询、不进入推理调度或计费

账号白名单与 Client Key 访问范围仍会过滤已取得的模型目录，不将白名单本身当成可用模型列表。白名单中的 ID 未出现在目录中时，不会仅因勾选而补入分享结果

## 权限与操作目录

这些是管理级公开服务：安装者须完整信任插件，插件可在有效宿主调用内操作指定账号或 Key，不借用浏览器管理 Cookie。现行完整信任模型不要求额外的权限或 capability 声明；`management`、`command_line` 等现有贡献项只决定插件从哪里接收调用，不改变服务授权

所有操作都经过正式服务注册表和 `service` 中间件；写入审计主体及调用标识由宿主签发。下游 Client Key 的权限只约束 `/v1/quota` 的导出范围，不为插件提供管理员身份

| Operation | 稳定名称 | 输入 → 输出 |
| --- | --- | --- |
| `GetAccountQuota` | `quota.account_snapshot` | `AccountQuotaQuery` → `AccountQuotaSnapshot` |
| `GetQuotaQuerySettings` | `quota.query_settings` | `AccountQuotaQuery` → `QuotaQuerySettings` |
| `SetQuotaQuerySettings` | `quota.set_query_settings` | `UpdateQuotaQuerySettings` → `QuotaQuerySettings` |
| `RefreshAccountQuota` | `quota.refresh_account` | `AccountQuotaQuery` → `QuotaRefreshResult` |
| `GetQuotaSharing` | `quota.sharing_settings` | `QuotaSharingQuery` → `QuotaSharingSnapshot` |
| `SetQuotaSharing` | `quota.set_sharing_settings` | `UpdateQuotaSharing` → `QuotaSharingSnapshot` |
| `ExportQuota` | `quota.export` | `QuotaSharingQuery` → `QuotaResponse` |

类型位于 `call::services::quota`，调用需要 `io` feature。现有 `call::data::QuotaFacts`、`quota_facts` 与 `refresh_account_quota` 保持原有基础窗口合同，调用它们不需要先配置 API Key 查询；不支持主动查询的 API Key 账号仍返回原有错误
