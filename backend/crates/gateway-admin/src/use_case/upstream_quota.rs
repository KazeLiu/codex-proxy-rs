//! 基于认证后账号范围的只读额度导出和管理端子额度查询

use std::sync::Arc;

use crate::model::quota::{MAX_QUOTA_RESPONSE_BYTES, QuotaResponse};
use gateway_core::{
    account::ProviderAccountId,
    policy::{ClientApiKeyId, ClientPolicy},
    routing::ProviderKind,
    runtime::{RuntimeSnapshotHandle, SnapshotControl},
};
use hkdf::Hkdf;
use secrecy::ExposeSecret as _;
use sha2::Sha256;

use crate::{
    model::{
        AdminError, MutationContext,
        provider_credentials::ProviderQuotaRequest,
        upstream_quota::{
            AccountQuotaSnapshot, QuotaExportError, QuotaQuerySettings, QuotaRefreshError,
            QuotaRefreshResult, QuotaSharingSnapshot, UpdateQuotaQuerySettings, UpdateQuotaSharing,
        },
    },
    ports::{
        provider::ProviderAdminRegistry,
        store::{AccountStore, ClientKeyStore},
    },
};

pub struct UpstreamQuotaService {
    keys: Arc<dyn ClientKeyStore>,
    providers: ProviderAdminRegistry,
    accounts: Arc<dyn AccountStore>,
    snapshot: Arc<dyn SnapshotControl>,
    published: RuntimeSnapshotHandle,
}

impl UpstreamQuotaService {
    pub(crate) fn new(
        keys: Arc<dyn ClientKeyStore>,
        providers: ProviderAdminRegistry,
        accounts: Arc<dyn AccountStore>,
        snapshot: Arc<dyn SnapshotControl>,
        published: RuntimeSnapshotHandle,
    ) -> Self {
        Self {
            keys,
            providers,
            accounts,
            snapshot,
            published,
        }
    }

    async fn provider_for_account(
        &self,
        id: &ProviderAccountId,
    ) -> Result<
        (
            crate::model::accounts::AccountRecord,
            Arc<dyn crate::ports::provider::ProviderAdmin>,
        ),
        AdminError,
    > {
        let account =
            super::required_plugin_credential(self.accounts.as_ref(), id, "quota account")
                .await?
                .credential;
        let provider = self
            .providers
            .require(&account.provider_kind)
            .map_err(|error| super::map_provider_error(error, "quota provider"))?;
        Ok((account, provider))
    }

    pub async fn account_snapshot(
        &self,
        id: &ProviderAccountId,
    ) -> Result<AccountQuotaSnapshot, AdminError> {
        let (account, provider) = self.provider_for_account(id).await?;
        let quota = provider
            .quota(ProviderQuotaRequest {
                account_id: id.clone(),
                refresh: false,
                rolling_usage: None,
            })
            .await
            .map_err(|error| super::map_provider_error(error, "quota snapshot"))?;
        let upstream = provider
            .upstream_quota_snapshots(std::slice::from_ref(id))
            .await
            .map_err(|error| super::map_provider_error(error, "upstream quota snapshot"))?
            .remove(id);
        let current =
            super::required_plugin_credential(self.accounts.as_ref(), id, "quota account")
                .await?
                .credential;
        if current.credential_revision != account.credential_revision
            || current.provider_kind != account.provider_kind
        {
            return Err(AdminError::conflict("账号凭据或查询设置已更新，请重新读取"));
        }
        Ok(AccountQuotaSnapshot {
            account_id: id.as_str().to_owned(),
            credential_revision: account.credential_revision,
            name: current.name,
            observed_at: quota.observed_at,
            windows: quota.windows.into_iter().map(Into::into).collect(),
            upstream,
        })
    }

    pub async fn query_settings(
        &self,
        id: &ProviderAccountId,
    ) -> Result<QuotaQuerySettings, AdminError> {
        let (_, provider) = self.provider_for_account(id).await?;
        provider
            .quota_query_settings(id)
            .await
            .map_err(|error| super::map_provider_error(error, "quota query settings"))
    }

    pub async fn set_query_settings(
        &self,
        command: UpdateQuotaQuerySettings,
        context: &MutationContext,
    ) -> Result<QuotaQuerySettings, AdminError> {
        let id = ProviderAccountId::new(command.account_id.clone())
            .map_err(|_| AdminError::invalid("账号 ID 无效"))?;
        let (account, provider) = self.provider_for_account(&id).await?;
        let mode = command.mode;
        let prepared = provider
            .prepare_quota_query_update(command)
            .await
            .map_err(|error| super::map_provider_error(error, "quota query settings"))?;
        super::validate_prepared_rotation_facts(&account, prepared.facts())?;
        let result = super::commit_credential_rotation(
            self.accounts.as_ref(),
            prepared,
            None,
            context,
            "quota query settings",
        )
        .await?;
        super::publish_credentials_and_observe_quota(
            &provider,
            self.snapshot.as_ref(),
            result.config_revision,
            std::slice::from_ref(&id),
            &context.request_id,
        )
        .await?;
        Ok(QuotaQuerySettings {
            account_id: id.as_str().to_owned(),
            credential_revision: result
                .credential_revision
                .ok_or_else(|| AdminError::internal("额度设置保存未返回凭据版本"))?,
            mode,
        })
    }

    pub async fn refresh_account(
        &self,
        id: &ProviderAccountId,
    ) -> Result<QuotaRefreshResult, AdminError> {
        let (_, provider) = self.provider_for_account(id).await?;
        let error = provider
            .quota(ProviderQuotaRequest {
                account_id: id.clone(),
                refresh: true,
                rolling_usage: None,
            })
            .await
            .err()
            .map(|error| {
                let error = super::map_provider_error(error, "quota refresh");
                QuotaRefreshError {
                    kind: error.kind().as_str().to_owned(),
                    message: error.message().to_owned(),
                }
            });
        Ok(QuotaRefreshResult {
            snapshot: self.account_snapshot(id).await?,
            error,
        })
    }

    pub async fn sharing_settings(
        &self,
        id: &ClientApiKeyId,
    ) -> Result<QuotaSharingSnapshot, AdminError> {
        self.keys
            .quota_sharing_snapshot(id)
            .await
            .map_err(|error| super::map_store_error(error, "quota sharing"))?
            .ok_or_else(|| AdminError::not_found("Client Key 不存在"))
    }

    pub async fn set_sharing_settings(
        &self,
        command: UpdateQuotaSharing,
        context: &MutationContext,
    ) -> Result<QuotaSharingSnapshot, AdminError> {
        ClientApiKeyId::new(command.client_key_id.clone())
            .map_err(|_| AdminError::invalid("Client Key ID 无效"))?;
        let result = self
            .keys
            .update_quota_sharing(command, context)
            .await
            .map_err(|error| super::map_store_error(error, "quota sharing"))?;
        super::publish_committed(self.snapshot.as_ref(), result.config_revision).await?;
        Ok(result)
    }

    pub async fn export_for_key(&self, id: &ClientApiKeyId) -> Result<QuotaResponse, AdminError> {
        let snapshot = self
            .published
            .acquire()
            .map_err(|_| AdminError::unavailable("运行配置暂不可用"))?;
        let policy = snapshot.client_policy(id).ok_or_else(|| {
            AdminError::new(
                crate::model::AdminErrorKind::Unauthorized,
                "Client Key 不可用",
            )
        })?;
        self.export(policy).await.map_err(|error| {
            use crate::model::AdminErrorKind;
            let kind = match error {
                QuotaExportError::Unauthorized => AdminErrorKind::Unauthorized,
                QuotaExportError::SharingDisabled => AdminErrorKind::Forbidden,
                QuotaExportError::Unavailable | QuotaExportError::ResponseTooLarge => {
                    AdminErrorKind::Unavailable
                }
            };
            AdminError::new(kind, error.to_string())
        })
    }

    async fn export(&self, policy: &ClientPolicy) -> Result<QuotaResponse, QuotaExportError> {
        let identity = self
            .keys
            .quota_export_identity(policy.key_id())
            .await
            .map_err(|_| QuotaExportError::Unavailable)?
            .ok_or(QuotaExportError::Unauthorized)?;
        if !identity.enabled {
            return Err(QuotaExportError::Unauthorized);
        }
        if !identity.sharing.enabled {
            return Err(QuotaExportError::SharingDisabled);
        }
        let provider = self
            .providers
            .require(&openai_kind())
            .map_err(|_| QuotaExportError::Unavailable)?;
        let ids = policy
            .account_scope()
            .account_ids()
            .cloned()
            .collect::<Vec<_>>();
        let entries = provider
            .quota_accounts(&ids, identity.sharing.include_models)
            .await
            .map_err(|_| QuotaExportError::Unavailable)?;
        let derivation = Hkdf::<Sha256>::new(
            Some(policy.key_id().as_str().as_bytes()),
            identity.secret.expose_secret().as_bytes(),
        );
        let mut accounts = Vec::with_capacity(entries.len());
        for (source, mut entry) in entries {
            let info = serde_json::to_vec(&(source.as_str(), entry.id.as_str()))
                .map_err(|_| QuotaExportError::Unavailable)?;
            let mut public_id = [0_u8; 32];
            derivation
                .expand(&info, &mut public_id)
                .map_err(|_| QuotaExportError::Unavailable)?;
            entry.id = hex::encode(public_id);
            // 每一跳独立执行当前 Key 的披露策略，名称仅在管理员明确授权时沿用
            if !identity.sharing.include_names {
                entry.label = format!("上游账号 {}", accounts.len() + 1);
            }
            if !identity.sharing.include_models {
                entry.models = None;
            } else if let Some(models) = &mut entry.models {
                models.retain(|model| policy.account_scope().allows_model(&source, model));
            }
            accounts.push(entry);
        }
        let response = QuotaResponse {
            schema_version: 1,
            accounts,
        };
        response
            .validate()
            .map_err(|_| QuotaExportError::Unavailable)?;
        if serde_json::to_vec(&response)
            .map_err(|_| QuotaExportError::Unavailable)?
            .len()
            > MAX_QUOTA_RESPONSE_BYTES
        {
            return Err(QuotaExportError::ResponseTooLarge);
        }
        Ok(response)
    }
}

fn openai_kind() -> ProviderKind {
    ProviderKind::new("openai").expect("内置 Provider 标识有效")
}
