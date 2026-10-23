//! 额度快照、查询设置、主动刷新与 Client Key 分享的公开服务登记

use std::sync::Arc;

use gateway_core::{account::ProviderAccountId, policy::ClientApiKeyId};

use crate::{
    model::{AdminError, upstream_quota::*},
    use_case::upstream_quota::UpstreamQuotaService,
};

pub(super) fn register(
    registry: &mut super::Registry,
    service: &Arc<UpstreamQuotaService>,
) -> Result<(), AdminError> {
    let target = service.clone();
    registry.register("quota.export", move |query: QuotaSharingQuery| {
        let service = target.clone();
        Box::pin(async move {
            let id = ClientApiKeyId::new(query.client_key_id)
                .map_err(|_| AdminError::invalid("Client Key ID 无效"))?;
            service.export_for_key(&id).await
        })
    })?;
    let target = service.clone();
    registry.register("quota.account_snapshot", move |query: AccountQuotaQuery| {
        let service = target.clone();
        Box::pin(async move {
            let id = ProviderAccountId::new(query.account_id)
                .map_err(|_| AdminError::invalid("账号 ID 无效"))?;
            service.account_snapshot(&id).await
        })
    })?;
    let target = service.clone();
    registry.register("quota.query_settings", move |query: AccountQuotaQuery| {
        let service = target.clone();
        Box::pin(async move {
            let id = ProviderAccountId::new(query.account_id)
                .map_err(|_| AdminError::invalid("账号 ID 无效"))?;
            service.query_settings(&id).await
        })
    })?;
    let target = service.clone();
    registry.register(
        "quota.set_query_settings",
        move |command: UpdateQuotaQuerySettings| {
            let service = target.clone();
            Box::pin(async move {
                service
                    .set_query_settings(command, &super::actor::mutation_context()?)
                    .await
            })
        },
    )?;
    let target = service.clone();
    registry.register("quota.refresh_account", move |query: AccountQuotaQuery| {
        let service = target.clone();
        Box::pin(async move {
            let id = ProviderAccountId::new(query.account_id)
                .map_err(|_| AdminError::invalid("账号 ID 无效"))?;
            service.refresh_account(&id).await
        })
    })?;
    let target = service.clone();
    registry.register("quota.sharing_settings", move |query: QuotaSharingQuery| {
        let service = target.clone();
        Box::pin(async move {
            let id = ClientApiKeyId::new(query.client_key_id)
                .map_err(|_| AdminError::invalid("Client Key ID 无效"))?;
            service.sharing_settings(&id).await
        })
    })?;
    let target = service.clone();
    registry.register(
        "quota.set_sharing_settings",
        move |command: UpdateQuotaSharing| {
            let service = target.clone();
            Box::pin(async move {
                service
                    .set_sharing_settings(command, &super::actor::mutation_context()?)
                    .await
            })
        },
    )?;
    Ok(())
}
