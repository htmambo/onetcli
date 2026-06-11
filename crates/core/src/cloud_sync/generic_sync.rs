//! 通用同步流程
//!
//! 为实现了 `SyncTypeHandler` 的数据类型提供统一的同步逻辑，
//! 涵盖：待删除处理 → 数据拉取 → 软删除 → 同步计划 → 操作队列 → 执行。

use crate::cloud_sync::engine::SyncEngine;
use crate::cloud_sync::models::{CloudSyncData, SyncResult};
use crate::cloud_sync::queue::SyncOperation;
use crate::cloud_sync::service::SyncError;
use crate::cloud_sync::sync_type::{
    GenericSyncPlan, PendingDeletionDecision, SyncTypeHandler, SyncableItem,
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkedSyncAction {
    None,
    UpdateCloud,
    UpdateLocal,
}

#[derive(Debug, Default)]
struct PendingDeletionOutcome {
    handled_ids: Vec<String>,
    refetch_cloud: bool,
}

/// 通用同步入口
///
/// 按统一流程同步指定类型的数据：
/// 1. 处理待删除列表
/// 2. 获取本地 / 云端数据
/// 3. 过滤未解锁团队数据
/// 4. 构建名称映射
/// 5. 处理云端软删除
/// 6. 计算同步计划
/// 7. 构建并执行操作队列
pub(crate) async fn generic_sync<H: SyncTypeHandler>(
    engine: &SyncEngine,
    handler: &H,
) -> Result<SyncResult, SyncError> {
    let type_name = handler.display_name();
    let mut result = SyncResult::default();

    // ========== 1. 获取云端数据（按 data_type 过滤） ==========
    let mut cloud_sync_data = fetch_cloud_sync_data(engine, handler).await?;
    tracing::info!("[{}] 云端同步数据: {} 个", type_name, cloud_sync_data.len());

    // ========== 2. 处理待删除列表 ==========
    let deletion_outcome = process_pending_deletions(engine, handler, &cloud_sync_data).await;
    tracing::info!(
        "[{}] 处理待删除列表完成: {} 个",
        type_name,
        deletion_outcome.handled_ids.len()
    );
    if deletion_outcome.refetch_cloud {
        cloud_sync_data = fetch_cloud_sync_data(engine, handler).await?;
        tracing::info!(
            "[{}] 待删除处理后重新获取云端数据: {} 个",
            type_name,
            cloud_sync_data.len()
        );
    }

    // ========== 3. 获取本地数据 ==========
    let local_items = handler.list_local(engine)?;
    tracing::info!("[{}] 本地数据: {} 个", type_name, local_items.len());

    // ========== 4. 构建名称映射 ==========
    let cloud_name_map = build_name_map(engine, handler, &cloud_sync_data);

    // ========== 5. 处理云端软删除 ==========
    let soft_deleted = process_soft_deletions(engine, handler, &cloud_sync_data, &local_items)?;
    if soft_deleted > 0 {
        tracing::info!(
            "[{}] 处理云端软删除: 删除了 {} 个本地数据",
            type_name,
            soft_deleted
        );
        result.deleted += soft_deleted;
    }

    // 过滤出活跃数据
    let active_cloud_data: Vec<_> = cloud_sync_data
        .into_iter()
        .filter(|d| d.deleted_at.is_none())
        .collect();
    tracing::info!(
        "[{}] 活跃云端数据: {} 个",
        type_name,
        active_cloud_data.len()
    );

    // ========== 6. 计算同步计划 ==========
    let pending_cloud_ids = get_pending_cloud_ids(engine, handler);
    let plan = calculate_sync_plan(
        &pending_cloud_ids,
        handler,
        &local_items,
        &active_cloud_data,
        &cloud_name_map,
    )?;
    tracing::info!(
        "[{}计划] 上传: {}, 更新云端: {}, 下载: {}, 更新本地: {}",
        type_name,
        plan.to_upload.len(),
        plan.to_update_cloud.len(),
        plan.to_download.len(),
        plan.to_update_local.len()
    );

    // ========== 7. 构建操作队列 ==========
    let local_item_map: HashMap<i64, H::Item> = local_items
        .iter()
        .filter_map(|item| item.local_id().map(|id| (id, item.clone())))
        .collect();
    let cloud_data_map: HashMap<String, CloudSyncData> = active_cloud_data
        .iter()
        .map(|d| (d.id.clone(), d.clone()))
        .collect();

    let mut operations = Vec::new();
    for item in &plan.to_upload {
        if let Some(local_id) = item.local_id() {
            operations.push(SyncOperation::Upload { local_id });
        } else {
            result.errors.push(format!(
                "上传{}失败 {}: 缺少本地 ID",
                type_name,
                item.item_name()
            ));
        }
    }

    for (item, cloud_data) in &plan.to_update_cloud {
        if let Some(local_id) = item.local_id() {
            operations.push(SyncOperation::UpdateCloud {
                local_id,
                cloud_id: cloud_data.id.clone(),
            });
        } else {
            result.errors.push(format!(
                "更新云端{}失败 {}: 缺少本地 ID",
                type_name,
                item.item_name()
            ));
        }
    }

    for cloud_data in &plan.to_download {
        operations.push(SyncOperation::Download(cloud_data.id.clone()));
    }

    for (cloud_data, item) in &plan.to_update_local {
        if let Some(local_id) = item.local_id() {
            operations.push(SyncOperation::UpdateLocal {
                local_id,
                cloud_id: cloud_data.id.clone(),
            });
        } else {
            let name = cloud_name_map
                .get(&cloud_data.id)
                .cloned()
                .unwrap_or_else(|| cloud_data.id.clone());
            result
                .errors
                .push(format!("更新本地{}失败 {}: 缺少本地 ID", type_name, name));
        }
    }

    // ========== 9. 执行操作 ==========
    let mut queue = engine.take_operation_queue(handler.queue_key())?;
    queue.enqueue_all(operations);
    queue.retry_failed();

    while let Some(queued_operation) = queue.dequeue() {
        let operation = queued_operation.operation.clone();
        match operation {
            SyncOperation::Upload { local_id } => {
                let Some(local_item) = local_item_map.get(&local_id) else {
                    result.errors.push(format!(
                        "上传{}失败 {}: 本地数据不存在",
                        type_name, local_id
                    ));
                    continue;
                };

                match upload_item(engine, handler, local_item).await {
                    Ok(cloud_id) => match handler.on_uploaded(engine, local_id, &cloud_id) {
                        Ok(()) => {
                            result.uploaded += 1;
                            tracing::info!("[上传{}] 成功: {}", type_name, local_item.item_name());
                        }
                        Err(e) => {
                            let msg =
                                format!("上传{}失败 {}: {}", type_name, local_item.item_name(), e);
                            result.errors.push(msg.clone());
                            queue.mark_failed(queued_operation, msg);
                        }
                    },
                    Err(e) => {
                        let msg =
                            format!("上传{}失败 {}: {}", type_name, local_item.item_name(), e);
                        result.errors.push(msg.clone());
                        queue.mark_failed(queued_operation, msg);
                    }
                }
            }
            SyncOperation::UpdateCloud { local_id, cloud_id } => {
                let Some(local_item) = local_item_map.get(&local_id) else {
                    result.errors.push(format!(
                        "更新云端{}失败 {}: 本地数据不存在",
                        type_name, local_id
                    ));
                    continue;
                };
                let Some(cloud_data) = cloud_data_map.get(&cloud_id) else {
                    result.errors.push(format!(
                        "更新云端{}失败 {}: 云端数据不存在",
                        type_name, cloud_id
                    ));
                    continue;
                };

                match update_cloud_item(engine, handler, local_item, cloud_data).await {
                    Ok(()) => match handler.on_uploaded(engine, local_id, &cloud_id) {
                        Ok(()) => {
                            result.uploaded += 1;
                            tracing::info!(
                                "[更新云端{}] 成功: {}",
                                type_name,
                                local_item.item_name()
                            );
                        }
                        Err(e) => {
                            let msg = format!(
                                "更新云端{}失败 {}: {}",
                                type_name,
                                local_item.item_name(),
                                e
                            );
                            result.errors.push(msg.clone());
                            queue.mark_failed(queued_operation, msg);
                        }
                    },
                    Err(e) => {
                        let msg = format!(
                            "更新云端{}失败 {}: {}",
                            type_name,
                            local_item.item_name(),
                            e
                        );
                        result.errors.push(msg.clone());
                        queue.mark_failed(queued_operation, msg);
                    }
                }
            }
            SyncOperation::UpdateLocal { local_id, cloud_id } => {
                let Some(local_item) = local_item_map.get(&local_id) else {
                    result.errors.push(format!(
                        "更新本地{}失败 {}: 本地数据不存在",
                        type_name, local_id
                    ));
                    continue;
                };
                let Some(cloud_data) = cloud_data_map.get(&cloud_id) else {
                    result.errors.push(format!(
                        "更新本地{}失败 {}: 云端数据不存在",
                        type_name, cloud_id
                    ));
                    continue;
                };

                let name = cloud_name_map
                    .get(&cloud_data.id)
                    .cloned()
                    .unwrap_or_else(|| cloud_data.id.clone());
                match download_and_update_item(engine, handler, cloud_data, local_item).await {
                    Ok(()) => {
                        result.downloaded += 1;
                        tracing::info!("[更新本地{}] 成功: {}", type_name, name);
                    }
                    Err(e) => {
                        let msg = format!("更新本地{}失败 {}: {}", type_name, name, e);
                        result.errors.push(msg.clone());
                        queue.mark_failed(queued_operation, msg);
                    }
                }
            }
            SyncOperation::Download(cloud_id) => {
                let Some(cloud_data) = cloud_data_map.get(&cloud_id) else {
                    result.errors.push(format!(
                        "下载{}失败 {}: 云端数据不存在",
                        type_name, cloud_id
                    ));
                    continue;
                };

                let name = cloud_name_map
                    .get(&cloud_data.id)
                    .cloned()
                    .unwrap_or_else(|| cloud_data.id.clone());
                match download_item(engine, handler, cloud_data).await {
                    Ok(()) => {
                        result.downloaded += 1;
                        tracing::info!("[下载{}] 成功: {}", type_name, name);
                    }
                    Err(e) => {
                        let msg = format!("下载{}失败 {}: {}", type_name, name, e);
                        result.errors.push(msg.clone());
                        queue.mark_failed(queued_operation, msg);
                    }
                }
            }
            SyncOperation::DeleteCloud(cloud_id) => {
                match engine
                    .cloud_client
                    .delete_sync_data(&cloud_id)
                    .await
                    .map_err(|e| SyncError::NetworkError(e.to_string()))
                {
                    Ok(()) => {
                        result.deleted += 1;
                        tracing::info!("[删除云端{}] 成功: {}", type_name, cloud_id);
                    }
                    Err(e) => {
                        let msg = format!("删除云端{}失败 {}: {}", type_name, cloud_id, e);
                        result.errors.push(msg.clone());
                        queue.mark_failed(queued_operation, msg);
                    }
                }
            }
            SyncOperation::DeleteLocal(local_id) => match handler.delete_local(engine, local_id) {
                Ok(()) => {
                    result.deleted += 1;
                    tracing::info!("[删除本地{}] 成功: {}", type_name, local_id);
                }
                Err(e) => {
                    let msg = format!("删除本地{}失败 {}: {}", type_name, local_id, e);
                    result.errors.push(msg.clone());
                    queue.mark_failed(queued_operation, msg);
                }
            },
        }
    }

    // ========== 10. 保存队列 ==========
    engine.store_operation_queue(handler.queue_key(), queue)?;

    Ok(result)
}

// ============================================================================
// 内部辅助函数
// ============================================================================

async fn fetch_cloud_sync_data<H: SyncTypeHandler>(
    engine: &SyncEngine,
    handler: &H,
) -> Result<Vec<CloudSyncData>, SyncError> {
    let type_name = handler.display_name();
    let cloud_sync_data = engine
        .cloud_client
        .list_sync_data(Some(handler.data_type()), None)
        .await
        .map_err(|e| SyncError::NetworkError(e.to_string()))?;

    tracing::info!(
        "[{}] 可处理的云端数据: {} 个",
        type_name,
        cloud_sync_data.len()
    );

    Ok(cloud_sync_data)
}

/// 处理待删除列表
async fn process_pending_deletions<H: SyncTypeHandler>(
    engine: &SyncEngine,
    handler: &H,
    cloud_data_list: &[CloudSyncData],
) -> PendingDeletionOutcome {
    let mut outcome = PendingDeletionOutcome::default();
    let pending_list = handler.list_pending_deletions(engine);
    let cloud_map: HashMap<&str, &CloudSyncData> = cloud_data_list
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();

    for pending in pending_list {
        tracing::info!(
            "[同步] 处理待删除云端{}: {}",
            handler.display_name(),
            pending.cloud_id
        );
        let current_cloud = cloud_map.get(pending.cloud_id.as_str()).copied();
        match handler.decide_pending_deletion(engine, &pending, current_cloud) {
            Ok(PendingDeletionDecision::DropPending) => {
                if let Err(e) = handler.remove_pending_deletion(engine, &pending.cloud_id) {
                    tracing::error!("[同步] 移除待删除记录失败: {}", e);
                } else {
                    outcome.handled_ids.push(pending.cloud_id);
                }
                continue;
            }
            Ok(PendingDeletionDecision::DeleteCloud) => {}
            Err(error) => {
                tracing::warn!(
                    "[同步] 处理待删除{}决策失败: {} - {}（保留在待删除列表）",
                    handler.display_name(),
                    pending.cloud_id,
                    error
                );
                continue;
            }
        }

        match engine
            .cloud_client
            .delete_sync_data(&pending.cloud_id)
            .await
        {
            Ok(_) => {
                tracing::info!(
                    "[同步] 云端{}删除成功: {}",
                    handler.display_name(),
                    pending.cloud_id
                );
                if let Err(e) = handler.remove_pending_deletion(engine, &pending.cloud_id) {
                    tracing::error!("[同步] 移除待删除记录失败: {}", e);
                } else {
                    outcome.handled_ids.push(pending.cloud_id);
                    outcome.refetch_cloud = true;
                }
            }
            Err(e) => {
                let error_str = e.to_string();
                if error_str.contains("404") || error_str.contains("not found") {
                    tracing::info!(
                        "[同步] 云端{}已不存在，移除待删除记录: {}",
                        handler.display_name(),
                        pending.cloud_id
                    );
                    if let Err(e) = handler.remove_pending_deletion(engine, &pending.cloud_id) {
                        tracing::error!("[同步] 移除待删除记录失败: {}", e);
                    } else {
                        outcome.handled_ids.push(pending.cloud_id);
                    }
                } else {
                    tracing::warn!(
                        "[同步] 删除云端{}失败: {} - {}（保留在待删除列表）",
                        handler.display_name(),
                        pending.cloud_id,
                        e
                    );
                }
            }
        }
    }

    outcome
}

/// 构建 cloud_id → name 映射
fn build_name_map<H: SyncTypeHandler>(
    engine: &SyncEngine,
    handler: &H,
    cloud_data_list: &[CloudSyncData],
) -> HashMap<String, String> {
    // Pre-allocate: at most one entry per cloud data.
    let mut map = HashMap::with_capacity(cloud_data_list.len());
    // Acquire the read guard once for the whole batch. `decrypt_name` only
    // reads from the crypto service, so a single read lock suffices.
    let service = match engine.crypto_service.read() {
        Ok(s) => s,
        Err(_) => return map,
    };
    for data in cloud_data_list {
        if data.has_resolved_name() {
            map.insert(data.id.clone(), data.name.clone());
            continue;
        }

        if let Some(name) = handler.decrypt_name(&service, data) {
            map.insert(data.id.clone(), name);
        }
    }
    map
}

/// 处理云端软删除
fn process_soft_deletions<H: SyncTypeHandler>(
    engine: &SyncEngine,
    handler: &H,
    cloud_data_list: &[CloudSyncData],
    local_items: &[H::Item],
) -> Result<usize, SyncError> {
    let mut deleted_count = 0;

    for cloud_data in cloud_data_list {
        if cloud_data.deleted_at.is_some() {
            if let Some(local_item) = local_items
                .iter()
                .find(|item| item.cloud_id() == Some(cloud_data.id.as_str()))
            {
                if let Some(local_id) = local_item.local_id() {
                    if should_keep_local_item_on_cloud_delete(local_item) {
                        tracing::info!(
                            "[软删除] 跳过删除本地{} {}，因为存在未同步的本地更新",
                            handler.display_name(),
                            local_id
                        );
                        continue;
                    }
                    tracing::info!(
                        "[软删除] 云端{} {} 已被删除，删除对应的本地数据 {}",
                        handler.display_name(),
                        cloud_data.id,
                        local_id
                    );
                    match handler.delete_local(engine, local_id) {
                        Ok(()) => deleted_count += 1,
                        Err(e) => {
                            tracing::error!("[软删除] 删除本地数据失败: {} - {}", local_id, e);
                        }
                    }
                }
            }
        }
    }

    Ok(deleted_count)
}

/// 计算同步计划（通用版，按 updated_at 比较，无冲突检测）
///
/// 入参 `pending_cloud_ids` 是本地已登记的待删除云端 ID 集合；该函数与
/// `SyncEngine` 解耦，便于单测覆盖「云端有同名但本地禁用按名称回链」等分支。
fn calculate_sync_plan<H: SyncTypeHandler>(
    pending_cloud_ids: &HashSet<String>,
    handler: &H,
    local_items: &[H::Item],
    cloud_data_list: &[CloudSyncData],
    cloud_name_map: &HashMap<String, String>,
) -> Result<GenericSyncPlan<H::Item>, SyncError> {
    let mut plan = GenericSyncPlan::default();

    let cloud_map: HashMap<&str, &CloudSyncData> =
        cloud_data_list.iter().map(|d| (d.id.as_str(), d)).collect();

    // 仅启用同步的本地项参与双向同步；未启用的项保留在本地，
    // 不会被上传，也不会被云端同名项匹配覆盖。
    let sync_enabled_locals: Vec<&H::Item> = local_items
        .iter()
        .filter(|item| item.sync_enabled())
        .collect();

    let local_cloud_ids: HashSet<String> = local_items
        .iter()
        .filter_map(|item| item.cloud_id().map(|s| s.to_string()))
        .collect();

    // 按名称回链候选：仅保留 handler 显式允许（默认全部允许）的本地项，
    // 用于在云端已有同名条目时建立 cloud_id 关联。
    let local_unlinked_by_name: HashMap<&str, &H::Item> = sync_enabled_locals
        .iter()
        .filter(|item| item.cloud_id().is_none())
        .filter(|item| handler.should_link_unlinked_local_by_name(item))
        .map(|item| (item.item_name(), *item))
        .collect();

    // 兜底映射：仅用于"本地已有同 cloud_id 但当前未关联"场景；
    // 也只覆盖允许按名称链接的本地项，避免被云端旧值覆盖。
    let local_all_by_name: HashMap<&str, &H::Item> = sync_enabled_locals
        .iter()
        .filter(|item| handler.should_link_unlinked_local_by_name(item))
        .map(|item| (item.item_name(), *item))
        .collect();

    // 处理本地数据
    for local_item in sync_enabled_locals.iter().copied() {
        match local_item.cloud_id() {
            Some(cloud_id) => {
                if let Some(cloud_data) = cloud_map.get(cloud_id) {
                    if cloud_data.needs_name_backfill() && !local_item.item_name().trim().is_empty()
                    {
                        plan.to_update_cloud
                            .push((local_item.clone(), (*cloud_data).clone()));
                        continue;
                    }

                    let local_updated = local_item.updated_at().unwrap_or(0);
                    let cloud_updated = cloud_data.updated_at / 1000;

                    match decide_linked_sync_action(
                        local_item.uses_sync_state(),
                        local_updated,
                        local_item.last_synced_at(),
                        cloud_updated,
                    ) {
                        LinkedSyncAction::UpdateCloud => {
                            plan.to_update_cloud
                                .push((local_item.clone(), (*cloud_data).clone()));
                        }
                        LinkedSyncAction::UpdateLocal => {
                            plan.to_update_local
                                .push(((*cloud_data).clone(), local_item.clone()));
                        }
                        LinkedSyncAction::None => {}
                    }
                } else {
                    tracing::info!(
                        "[同步计划] {} '{}' 的云端记录 {} 不存在，重新加入上传计划",
                        handler.display_name(),
                        local_item.item_name(),
                        cloud_id
                    );
                    plan.to_upload.push(local_item.clone());
                }
            }
            None => {
                let has_cloud_match = cloud_name_map
                    .values()
                    .any(|name| name == local_item.item_name());
                if !has_cloud_match {
                    plan.to_upload.push(local_item.clone());
                } else if handler.should_link_unlinked_local_by_name(local_item) {
                    // 允许按名称回链：云端有同名条目，留给云端处理循环做关联。
                    tracing::debug!(
                        "[同步计划] {} '{}' 与云端已有同名项匹配，等待回链",
                        handler.display_name(),
                        local_item.item_name()
                    );
                } else {
                    // 不允许按名称回链：云端已有同名但本地是新建，优先保留本地数据，
                    // 不应被云端旧值通过 `update_from_cloud` 覆盖；将本地项推入上传队列，
                    // 由云端去重策略（多端冲突由用户在 UI 中处理）兜底。
                    tracing::info!(
                        "[同步计划] {} '{}' 不与云端同名项合并，标记为新建上传",
                        handler.display_name(),
                        local_item.item_name()
                    );
                    plan.to_upload.push(local_item.clone());
                }
            }
        }
    }

    // 处理云端新增数据
    for cloud_data in cloud_data_list {
        if !local_cloud_ids.contains(&cloud_data.id) {
            if pending_cloud_ids.contains(&cloud_data.id) {
                let name = cloud_name_map
                    .get(&cloud_data.id)
                    .cloned()
                    .unwrap_or_else(|| cloud_data.id.clone());
                tracing::info!(
                    "[同步计划] 跳过待删除的云端{}: {}",
                    handler.display_name(),
                    name
                );
                continue;
            }

            let cloud_name = cloud_name_map
                .get(&cloud_data.id)
                .cloned()
                .unwrap_or_else(|| cloud_data.id.clone());

            if let Some(local_item) = local_unlinked_by_name.get(cloud_name.as_str()) {
                tracing::info!(
                    "[同步计划] 按名称匹配{}: {} (云端 {} -> 本地 {:?})",
                    handler.display_name(),
                    cloud_name,
                    cloud_data.id,
                    local_item.local_id()
                );
                plan.to_update_local
                    .push((cloud_data.clone(), (*local_item).clone()));
            } else if let Some(local_item) = local_all_by_name.get(cloud_name.as_str()) {
                // 检查本地已有同名数据是否有有效的 cloud_id
                let local_cloud_id_is_valid = local_item
                    .cloud_id()
                    .is_some_and(|cid| cloud_map.contains_key(cid));
                if !local_cloud_id_is_valid {
                    plan.to_update_local
                        .push((cloud_data.clone(), (*local_item).clone()));
                }
            } else {
                plan.to_download.push(cloud_data.clone());
            }
        }
    }

    Ok(plan)
}

fn should_keep_local_item_on_cloud_delete<T: SyncableItem>(item: &T) -> bool {
    if !item.uses_sync_state() {
        return false;
    }

    let local_updated = item.updated_at().unwrap_or(0);
    let last_synced_at = item.last_synced_at().unwrap_or(0);
    local_updated > last_synced_at
}

fn decide_linked_sync_action(
    uses_sync_state: bool,
    local_updated: i64,
    last_synced_at: Option<i64>,
    cloud_updated: i64,
) -> LinkedSyncAction {
    if !uses_sync_state {
        return if local_updated > cloud_updated {
            LinkedSyncAction::UpdateCloud
        } else if cloud_updated > local_updated {
            LinkedSyncAction::UpdateLocal
        } else {
            LinkedSyncAction::None
        };
    }

    match last_synced_at {
        Some(last_synced_at) => {
            let local_changed = local_updated > last_synced_at;
            let cloud_changed = cloud_updated > last_synced_at;

            match (local_changed, cloud_changed) {
                (true, false) => LinkedSyncAction::UpdateCloud,
                (false, true) => LinkedSyncAction::UpdateLocal,
                (true, true) => {
                    if local_updated >= cloud_updated {
                        LinkedSyncAction::UpdateCloud
                    } else {
                        LinkedSyncAction::UpdateLocal
                    }
                }
                (false, false) => LinkedSyncAction::None,
            }
        }
        None => {
            if local_updated >= cloud_updated {
                LinkedSyncAction::UpdateCloud
            } else {
                LinkedSyncAction::UpdateLocal
            }
        }
    }
}

/// 获取待删除的云端 ID 集合
fn get_pending_cloud_ids<H: SyncTypeHandler>(engine: &SyncEngine, handler: &H) -> HashSet<String> {
    handler
        .list_pending_deletions(engine)
        .into_iter()
        .map(|p| p.cloud_id)
        .collect()
}

/// 上传数据项到云端
async fn upload_item<H: SyncTypeHandler>(
    engine: &SyncEngine,
    handler: &H,
    item: &H::Item,
) -> Result<String, SyncError> {
    let cloud_data = {
        let service = engine
            .crypto_service
            .read()
            .map_err(|_| SyncError::StorageError("同步服务锁获取失败".to_string()))?;
        handler.encrypt(&service, item)?
    };

    let created = engine
        .cloud_client
        .create_sync_data(&cloud_data)
        .await
        .map_err(|e| SyncError::NetworkError(e.to_string()))?;

    Ok(created.id)
}

/// 更新云端数据项
async fn update_cloud_item<H: SyncTypeHandler>(
    engine: &SyncEngine,
    handler: &H,
    item: &H::Item,
    cloud_data: &CloudSyncData,
) -> Result<(), SyncError> {
    let updated_data = {
        let service = engine
            .crypto_service
            .read()
            .map_err(|_| SyncError::StorageError("同步服务锁获取失败".to_string()))?;
        let mut data = handler.encrypt(&service, item)?;
        data.id = cloud_data.id.clone();
        data.version = cloud_data.version;
        data
    };

    engine
        .cloud_client
        .update_sync_data(&updated_data)
        .await
        .map_err(|e| SyncError::NetworkError(e.to_string()))?;

    Ok(())
}

/// 下载云端数据项并创建本地记录
async fn download_item<H: SyncTypeHandler>(
    engine: &SyncEngine,
    handler: &H,
    cloud_data: &CloudSyncData,
) -> Result<(), SyncError> {
    let service = engine
        .crypto_service
        .read()
        .map_err(|_| SyncError::StorageError("同步服务锁获取失败".to_string()))?;

    let mut local_item = handler.decrypt(&service, cloud_data)?;
    local_item.set_local_id(None);
    local_item.set_cloud_id(Some(cloud_data.id.clone()));
    drop(service);

    handler.insert_local(engine, &mut local_item)
}

/// 下载云端数据项并更新已有本地记录
async fn download_and_update_item<H: SyncTypeHandler>(
    engine: &SyncEngine,
    handler: &H,
    cloud_data: &CloudSyncData,
    existing: &H::Item,
) -> Result<(), SyncError> {
    let service = engine
        .crypto_service
        .read()
        .map_err(|_| SyncError::StorageError("同步服务锁获取失败".to_string()))?;

    let mut updated = handler.decrypt(&service, cloud_data)?;
    updated.set_local_id(existing.local_id());
    updated.set_cloud_id(Some(cloud_data.id.clone()));
    drop(service);

    handler.update_local_item(engine, &updated)
}

#[cfg(test)]
mod tests {
    use super::super::llm_provider_sync::LlmProviderSyncType;
    use super::{
        LinkedSyncAction, calculate_sync_plan, decide_linked_sync_action,
        should_keep_local_item_on_cloud_delete,
    };
    use crate::cloud_sync::models::CloudSyncData;
    use crate::llm::types::{ProviderConfig, ProviderType};
    use crate::storage::Workspace;
    use std::collections::{HashMap, HashSet};

    /// 回归用例：用户自建 LLM 提供商（OpenAI）本地新增，云端已有同名记录。
    ///
    /// 期望（依据 `should_link_unlinked_local_by_name = false` 策略）：
    /// - 本项进入 `to_upload`（保留本地，被云端去重策略兜底）。
    /// - `to_update_local` 为空（核心：本地新建**不**会被云端旧值通过
    ///   `update_from_cloud` 静默覆盖，这正是该 bug 修复的关键断言）。
    /// - `to_download` 可能非空（云端同名条目仍会被下载到本地，标记为
    ///   已知 trade-off：可能产生重复云端记录，由云端去重或用户在 UI 中
    ///   处理）。
    #[test]
    fn calculate_sync_plan_preserves_user_provider_when_cloud_has_same_name() {
        let handler = LlmProviderSyncType;

        let mut local = ProviderConfig::default();
        local.id = 1;
        local.name = "user-openai".to_string();
        local.provider_type = ProviderType::OpenAI;
        local.cloud_id = None;
        local.updated_at = 1_700_000_000;
        let local_items = vec![local.clone()];

        let cloud = CloudSyncData {
            id: "cloud-uuid-1".to_string(),
            owner_id: "owner".to_string(),
            data_type: "llm_provider".to_string(),
            name: "user-openai".to_string(),
            encrypted_data: String::new(),
            key_version: 1,
            checksum: String::new(),
            version: 1,
            updated_at: 1_699_000_000_000,
            deleted_at: None,
        };
        let cloud_data_list = vec![cloud.clone()];

        let mut cloud_name_map = HashMap::new();
        cloud_name_map.insert(cloud.id.clone(), cloud.name.clone());

        let pending_cloud_ids = HashSet::new();

        let plan = calculate_sync_plan(
            &pending_cloud_ids,
            &handler,
            &local_items,
            &cloud_data_list,
            &cloud_name_map,
        )
        .expect("计算同步计划应当成功");

        // 核心断言：本地用户项必须被推入上传队列
        assert_eq!(plan.to_upload.len(), 1, "本地项应当被推入上传队列");
        assert_eq!(plan.to_upload[0].id, local.id, "上传目标必须是本地用户项");

        // 核心断言：本地新建不得被云端旧值通过 update_from_cloud 覆盖
        // （这是 bug 修复的关键：`to_update_local` 必须为空）
        assert!(
            plan.to_update_local.is_empty(),
            "云端同名项不得通过 update_from_cloud 覆盖本地新建"
        );

        // 本地无 cloud_id，不应走 to_update_cloud 分支
        assert!(
            plan.to_update_cloud.is_empty(),
            "本地无 cloud_id，不应走 to_update_cloud 分支"
        );
        // 已知 trade-off：to_download 可能非空，云端同名项被下载后由
        // 云端去重/用户 UI 处理
    }

    /// 回归用例：内置 OnetCli 提供商本地新增，云端已有同名记录。
    ///
    /// 期望：本地项**不**进入 `to_upload`（允许按名称回链），云端条目也
    /// **不**进入 `to_download`（将被云端处理循环关联）。
    #[test]
    fn calculate_sync_plan_keeps_builtin_provider_unlinked_for_name_backfill() {
        let handler = LlmProviderSyncType;

        let mut local = ProviderConfig::default();
        local.id = 2;
        local.name = "OnetCli AI".to_string();
        local.provider_type = ProviderType::OnetCli;
        local.cloud_id = None;
        let local_items = vec![local.clone()];

        let cloud = CloudSyncData {
            id: "cloud-uuid-2".to_string(),
            owner_id: "owner".to_string(),
            data_type: "llm_provider".to_string(),
            name: "OnetCli AI".to_string(),
            encrypted_data: String::new(),
            key_version: 1,
            checksum: String::new(),
            version: 1,
            updated_at: 1_699_000_000_000,
            deleted_at: None,
        };
        let cloud_data_list = vec![cloud.clone()];

        let mut cloud_name_map = HashMap::new();
        cloud_name_map.insert(cloud.id.clone(), cloud.name.clone());

        let pending_cloud_ids = HashSet::new();

        let plan = calculate_sync_plan(
            &pending_cloud_ids,
            &handler,
            &local_items,
            &cloud_data_list,
            &cloud_name_map,
        )
        .expect("计算同步计划应当成功");

        assert!(
            plan.to_upload.is_empty(),
            "内置项应允许按名称回链，不应被推入上传队列"
        );
        assert!(
            plan.to_download.is_empty(),
            "云端同名项应留给回链处理，不应被下载"
        );
    }

    /// 回归用例：本地已有 cloud_id 但关闭同步的 provider 不参与上传/更新，
    /// 同时也不能让同一个云端条目被误判为新增并重复下载。
    #[test]
    fn calculate_sync_plan_skips_download_for_sync_disabled_linked_provider() {
        let handler = LlmProviderSyncType;

        let mut local = ProviderConfig::default();
        local.id = 3;
        local.name = "disabled-openai".to_string();
        local.provider_type = ProviderType::OpenAI;
        local.cloud_id = Some("cloud-uuid-disabled".to_string());
        local.sync_enabled = false;
        local.updated_at = 1_700_000_000;
        let local_items = vec![local];

        let cloud = CloudSyncData {
            id: "cloud-uuid-disabled".to_string(),
            owner_id: "owner".to_string(),
            data_type: "llm_provider".to_string(),
            name: "disabled-openai".to_string(),
            encrypted_data: String::new(),
            key_version: 1,
            checksum: String::new(),
            version: 1,
            updated_at: 1_701_000_000_000,
            deleted_at: None,
        };
        let cloud_data_list = vec![cloud.clone()];

        let mut cloud_name_map = HashMap::new();
        cloud_name_map.insert(cloud.id.clone(), cloud.name.clone());

        let pending_cloud_ids = HashSet::new();

        let plan = calculate_sync_plan(
            &pending_cloud_ids,
            &handler,
            &local_items,
            &cloud_data_list,
            &cloud_name_map,
        )
        .expect("计算同步计划应当成功");

        assert!(
            plan.to_upload.is_empty(),
            "关闭同步的本地 provider 不应上传"
        );
        assert!(
            plan.to_update_cloud.is_empty(),
            "关闭同步的本地 provider 不应更新云端"
        );
        assert!(
            plan.to_update_local.is_empty(),
            "关闭同步的本地 provider 不应被云端覆盖"
        );
        assert!(
            plan.to_download.is_empty(),
            "已存在于本地但关闭同步的 cloud_id 不应被重复下载"
        );
    }

    #[test]
    fn non_sync_state_items_keep_original_timestamp_comparison() {
        assert_eq!(
            decide_linked_sync_action(false, 100, Some(90), 100),
            LinkedSyncAction::None
        );
    }

    #[test]
    fn sync_state_items_prefer_local_when_unsynced_timestamps_are_equal() {
        assert_eq!(
            decide_linked_sync_action(true, 100, None, 100),
            LinkedSyncAction::UpdateCloud
        );
    }

    #[test]
    fn sync_state_items_detect_cloud_side_changes_since_last_sync() {
        assert_eq!(
            decide_linked_sync_action(true, 100, Some(100), 101),
            LinkedSyncAction::UpdateLocal
        );
    }

    #[test]
    fn sync_state_items_stay_idle_when_nothing_changed_since_last_sync() {
        assert_eq!(
            decide_linked_sync_action(true, 100, Some(100), 100),
            LinkedSyncAction::None
        );
    }

    #[test]
    fn cloud_delete_should_preserve_local_sync_state_item_when_local_has_unsynced_update() {
        let mut workspace = Workspace::new("本地已更新工作区".to_string());
        workspace.updated_at = Some(200);
        workspace.last_synced_at = Some(100);

        assert!(should_keep_local_item_on_cloud_delete(&workspace));
    }

    #[test]
    fn cloud_delete_should_remove_local_sync_state_item_when_local_is_clean() {
        let mut workspace = Workspace::new("本地已同步工作区".to_string());
        workspace.updated_at = Some(100);
        workspace.last_synced_at = Some(100);

        assert!(!should_keep_local_item_on_cloud_delete(&workspace));
    }

    // ============================================================================
    // T3：calculate_sync_plan 端到端分支
    // ============================================================================

    /// 双向未变（uses_sync_state=false，时间戳完全相等）→ 4 个队列全部为空。
    /// 守护：未来若有人改坏 `decide_linked_sync_action` 的相等时间戳语义，
    /// 此用例可第一时间暴露回归。
    #[test]
    fn calculate_sync_plan_idle_when_both_unchanged() {
        let handler = LlmProviderSyncType;

        let mut local = ProviderConfig::default();
        local.id = 10;
        local.name = "stable".to_string();
        local.provider_type = ProviderType::OnetCli;
        local.cloud_id = Some("cloud-stable".to_string());
        local.updated_at = 1_700_000_000;
        let local_items = vec![local];

        let cloud = CloudSyncData {
            id: "cloud-stable".to_string(),
            owner_id: "owner".to_string(),
            data_type: "llm_provider".to_string(),
            name: "stable".to_string(),
            encrypted_data: String::new(),
            key_version: 1,
            checksum: String::new(),
            version: 1,
            updated_at: 1_700_000_000_000, // ms，与 local 同一秒
            deleted_at: None,
        };
        let cloud_data_list = vec![cloud.clone()];

        let mut cloud_name_map = HashMap::new();
        cloud_name_map.insert(cloud.id.clone(), cloud.name.clone());

        let pending_cloud_ids = HashSet::new();
        let plan = calculate_sync_plan(
            &pending_cloud_ids,
            &handler,
            &local_items,
            &cloud_data_list,
            &cloud_name_map,
        )
        .expect("计算同步计划应当成功");

        assert!(plan.to_upload.is_empty(), "双向无变化，不应上传");
        assert!(
            plan.to_update_cloud.is_empty(),
            "双向无变化，不应更新云端"
        );
        assert!(plan.to_download.is_empty(), "双向无变化，不应下载");
        assert!(
            plan.to_update_local.is_empty(),
            "双向无变化，不应更新本地"
        );
    }

    /// 双向都已变化（uses_sync_state=false, last_synced_at=None），本地秒级时间戳
    /// 晚于云端毫秒级时间戳 → UpdateCloud。
    /// 守护：uses_sync_state=false 时直接比较时间戳的语义（不应用 last_synced_at）。
    #[test]
    fn calculate_sync_plan_dual_change_picks_higher_timestamp() {
        let handler = LlmProviderSyncType;

        let mut local = ProviderConfig::default();
        local.id = 11;
        local.name = "race".to_string();
        local.provider_type = ProviderType::OpenAI;
        local.cloud_id = Some("cloud-race".to_string());
        local.updated_at = 1_700_000_200; // 秒，更晚
        let local_items = vec![local.clone()];

        let cloud = CloudSyncData {
            id: "cloud-race".to_string(),
            owner_id: "owner".to_string(),
            data_type: "llm_provider".to_string(),
            name: "race".to_string(),
            encrypted_data: String::new(),
            key_version: 1,
            checksum: String::new(),
            version: 1,
            updated_at: 1_700_000_100_000, // ms，更早
            deleted_at: None,
        };
        let cloud_data_list = vec![cloud.clone()];

        let mut cloud_name_map = HashMap::new();
        cloud_name_map.insert(cloud.id.clone(), cloud.name.clone());

        let pending_cloud_ids = HashSet::new();
        let plan = calculate_sync_plan(
            &pending_cloud_ids,
            &handler,
            &local_items,
            &cloud_data_list,
            &cloud_name_map,
        )
        .expect("计算同步计划应当成功");

        assert!(
            plan.to_update_local.is_empty(),
            "本地秒级时间戳晚于云端毫秒级时，不应更新本地"
        );
        assert_eq!(plan.to_update_cloud.len(), 1, "应推入更新云端队列");
        assert_eq!(plan.to_update_cloud[0].0.id, local.id, "更新目标是本地项");
    }

    /// cloud 列表里出现的 cloud_id 在 local 端**没有任何匹配项**，且本地存在一个
    /// 与云端条目**名称不同**的本地项 → 新云端条目应被推入 `to_download`，
    /// 且不会被误关联到本地（无 `to_update_local`）。
    /// 守护：`to_download` 路径不被 `local_cloud_ids` 误过滤。
    /// 注：若 local 完全为空，本地默认项 name="" 也会被算作"无云端匹配"而推入
    /// `to_upload`，这是首登场景的预期行为（与 821098c2 trade-off 一致），
    /// 故本用例构造 1 个无关本地项以隔离纯下载路径。
    #[test]
    fn calculate_sync_plan_downloads_unseen_cloud_item() {
        let handler = LlmProviderSyncType;

        let mut local = ProviderConfig::default();
        local.id = 13;
        local.name = "preexisting-local".to_string();
        local.provider_type = ProviderType::OnetCli;
        local.cloud_id = Some("cloud-preexisting".to_string());
        local.updated_at = 1_700_000_000;
        let local_items = vec![local];

        let preexisting_cloud = CloudSyncData {
            id: "cloud-preexisting".to_string(),
            owner_id: "owner".to_string(),
            data_type: "llm_provider".to_string(),
            name: "preexisting-local".to_string(),
            encrypted_data: String::new(),
            key_version: 1,
            checksum: String::new(),
            version: 1,
            updated_at: 1_700_000_000_000,
            deleted_at: None,
        };
        let unseen_cloud = CloudSyncData {
            id: "cloud-brand-new".to_string(),
            owner_id: "owner".to_string(),
            data_type: "llm_provider".to_string(),
            name: "brand-new".to_string(),
            encrypted_data: String::new(),
            key_version: 1,
            checksum: String::new(),
            version: 1,
            updated_at: 1_700_000_000_000,
            deleted_at: None,
        };
        let cloud_data_list = vec![preexisting_cloud.clone(), unseen_cloud.clone()];

        let mut cloud_name_map = HashMap::new();
        cloud_name_map.insert(preexisting_cloud.id.clone(), preexisting_cloud.name.clone());
        cloud_name_map.insert(unseen_cloud.id.clone(), unseen_cloud.name.clone());

        let pending_cloud_ids = HashSet::new();
        let plan = calculate_sync_plan(
            &pending_cloud_ids,
            &handler,
            &local_items,
            &cloud_data_list,
            &cloud_name_map,
        )
        .expect("计算同步计划应当成功");

        assert_eq!(
            plan.to_download.len(),
            1,
            "未见过的云端条目应进入下载队列"
        );
        assert_eq!(plan.to_download[0].id, "cloud-brand-new");
        assert!(
            plan.to_update_local.is_empty(),
            "未与本地项匹配的云端条目不应被关联到本地"
        );
    }

    /// 关闭同步的本地 provider 拥有 cloud_id 时，**不应**被推入任何同步队列。
    /// 守护：eb7f59a8 修复的核心——`local_cloud_ids` 含所有本地项，
    /// 防止关闭同步的 cloud_id 被误判为云端新增。
    #[test]
    fn calculate_sync_plan_sync_disabled_linked_item_is_idle() {
        let handler = LlmProviderSyncType;

        let mut local = ProviderConfig::default();
        local.id = 12;
        local.name = "paused".to_string();
        local.provider_type = ProviderType::OpenAI;
        local.cloud_id = Some("cloud-paused".to_string());
        local.sync_enabled = false;
        local.updated_at = 1_700_000_500;
        let local_items = vec![local];

        let cloud = CloudSyncData {
            id: "cloud-paused".to_string(),
            owner_id: "owner".to_string(),
            data_type: "llm_provider".to_string(),
            name: "paused".to_string(),
            encrypted_data: String::new(),
            key_version: 1,
            checksum: String::new(),
            version: 1,
            updated_at: 1_700_000_400_000,
            deleted_at: None,
        };
        let cloud_data_list = vec![cloud.clone()];

        let mut cloud_name_map = HashMap::new();
        cloud_name_map.insert(cloud.id.clone(), cloud.name.clone());

        let pending_cloud_ids = HashSet::new();
        let plan = calculate_sync_plan(
            &pending_cloud_ids,
            &handler,
            &local_items,
            &cloud_data_list,
            &cloud_name_map,
        )
        .expect("计算同步计划应当成功");

        assert!(plan.to_upload.is_empty());
        assert!(plan.to_update_cloud.is_empty());
        assert!(plan.to_update_local.is_empty());
        assert!(
            plan.to_download.is_empty(),
            "关闭同步的 cloud_id 不应被误判为云端新增并重复下载"
        );
    }

    // ============================================================================
    // T4：decide_linked_sync_action 边界
    // ============================================================================

    /// uses_sync_state=false 且双向时间戳完全相等 → None（idle）。
    /// 守护：双向无变化应被识别为空闲，避免不必要的上传/下载。
    #[test]
    fn decide_action_non_sync_state_equal_timestamps_is_none() {
        assert_eq!(
            decide_linked_sync_action(false, 1_700_000_000, None, 1_700_000_000),
            LinkedSyncAction::None
        );
    }

    /// uses_sync_state=false（last_synced_at 被忽略）且云端时间戳更大 → UpdateLocal。
    /// 守护：uses_sync_state=false 的分支不应读 last_synced_at。
    #[test]
    fn decide_action_non_sync_state_cloud_ahead_updates_local() {
        assert_eq!(
            decide_linked_sync_action(false, 1_700_000_000, Some(2_000_000_000), 1_700_000_500),
            LinkedSyncAction::UpdateLocal
        );
    }

    /// uses_sync_state=true，last_synced_at=None，且云端时间戳更大 → UpdateLocal。
    /// 守护：从未同步过（last_synced_at=None）且 uses_sync_state=true 的语义与
    /// uses_sync_state=false 行为一致（按时间戳直接比较）。
    #[test]
    fn decide_action_sync_state_never_synced_cloud_ahead_updates_local() {
        assert_eq!(
            decide_linked_sync_action(true, 1_700_000_000, None, 1_700_000_500),
            LinkedSyncAction::UpdateLocal
        );
    }

    /// uses_sync_state=true，last_synced_at=None，且本地时间戳更大 → UpdateCloud。
    #[test]
    fn decide_action_sync_state_never_synced_local_ahead_updates_cloud() {
        assert_eq!(
            decide_linked_sync_action(true, 1_700_000_500, None, 1_700_000_000),
            LinkedSyncAction::UpdateCloud
        );
    }
}
