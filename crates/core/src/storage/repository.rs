use anyhow::Result;
use gpui::{App, SharedString};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::crypto;
use crate::storage::connection::SqliteConnection;
use crate::storage::manager::{GlobalStorageState, StorageManager, now};
use crate::storage::models::{
    Certificate, CertificateKind, apply_certificate_to_connection_snapshot,
    detach_certificate_from_connection_snapshot, has_decrypt_failure_in_sensitive_fields,
};
use crate::storage::quick_command::QuickCommandRepository;
use crate::storage::row_mapping::FromSqliteRow;
use crate::storage::sftp_favorite_path::SftpFavoritePathRepository;
use crate::storage::traits::Repository;
use crate::storage::{ConnectionType, StoredConnection, Workspace};

struct ConnectionRow {
    id: i64,
    name: String,
    connection_type: String,
    params: String,
    sort_order: i64,
    workspace_id: Option<i64>,
    selected_databases: Option<String>,
    remark: Option<String>,
    sync_enabled: bool,
    cloud_id: Option<String>,
    last_synced_at: Option<i64>,
    created_at: i64,
    updated_at: i64,
    owner_id: Option<String>,
}

impl FromSqliteRow for ConnectionRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(ConnectionRow {
            id: row.get("id")?,
            name: row.get("name")?,
            connection_type: row.get("connection_type")?,
            params: row.get("params")?,
            sort_order: row.get("sort_order")?,
            workspace_id: row.get("workspace_id")?,
            selected_databases: row.get("selected_databases")?,
            remark: row.get("remark")?,
            sync_enabled: row
                .get::<_, i64>("sync_enabled")
                .map(|v| v != 0)
                .unwrap_or(true),
            cloud_id: row.get("cloud_id")?,
            last_synced_at: row.get("last_synced_at")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
            owner_id: row.get("owner_id").unwrap_or(None),
        })
    }
}

impl From<ConnectionRow> for StoredConnection {
    fn from(row: ConnectionRow) -> Self {
        let mut conn = StoredConnection {
            id: Some(row.id),
            name: row.name,
            connection_type: ConnectionType::from_str(&row.connection_type),
            params: row.params,
            sort_order: Some(row.sort_order),
            workspace_id: row.workspace_id,
            selected_databases: row.selected_databases,
            remark: row.remark,
            sync_enabled: row.sync_enabled,
            cloud_id: row.cloud_id,
            last_synced_at: row.last_synced_at,
            created_at: Some(row.created_at),
            updated_at: Some(row.updated_at),
            owner_id: row.owner_id,
        };
        // 从数据库读取后自动解密敏感字段
        conn.params = conn.decrypt_params();
        conn
    }
}

struct WorkspaceRow {
    id: i64,
    name: String,
    sort_order: i64,
    color: Option<String>,
    icon: Option<String>,
    created_at: i64,
    updated_at: i64,
    cloud_id: Option<String>,
    last_synced_at: Option<i64>,
}

impl FromSqliteRow for WorkspaceRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(WorkspaceRow {
            id: row.get("id")?,
            name: row.get("name")?,
            sort_order: row.get("sort_order")?,
            color: row.get("color")?,
            icon: row.get("icon")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
            cloud_id: row.get("cloud_id")?,
            last_synced_at: row.get("last_synced_at")?,
        })
    }
}

impl From<WorkspaceRow> for Workspace {
    fn from(row: WorkspaceRow) -> Self {
        Workspace {
            id: Some(row.id),
            name: row.name,
            sort_order: Some(row.sort_order),
            color: row.color,
            icon: row.icon,
            created_at: Some(row.created_at),
            updated_at: Some(row.updated_at),
            cloud_id: row.cloud_id,
            last_synced_at: row.last_synced_at,
        }
    }
}

fn decrypt_secret(value: Option<String>) -> Option<String> {
    value.map(|secret| crypto::decrypt_password(&secret))
}

/// 对证书 params 中的敏感字段加密后序列化
fn encrypt_certificate_params(params: &serde_json::Value) -> String {
    let mut encrypted = params.clone();
    if let Some(obj) = encrypted.as_object_mut() {
        for key in ["password", "passphrase"] {
            if let Some(v) = obj.get(key).and_then(|v| v.as_str()) {
                if !v.is_empty() {
                    obj.insert(
                        key.to_string(),
                        serde_json::Value::String(crypto::encrypt_password(v)),
                    );
                }
            }
        }
    }
    serde_json::to_string(&encrypted).unwrap_or_default()
}

fn next_workspace_sort_order(conn: &rusqlite::Connection) -> Result<i64> {
    let sort_order = conn.query_row(
        "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM workspaces",
        [],
        |row| row.get(0),
    )?;
    Ok(sort_order)
}

fn next_connection_sort_order(
    conn: &rusqlite::Connection,
    workspace_id: Option<i64>,
) -> Result<i64> {
    let sort_order = if let Some(workspace_id) = workspace_id {
        conn.query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM connections WHERE workspace_id = ?1",
            params![workspace_id],
            |row| row.get(0),
        )?
    } else {
        conn.query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM connections WHERE workspace_id IS NULL",
            [],
            |row| row.get(0),
        )?
    };
    Ok(sort_order)
}

struct CertificateRow {
    id: i64,
    name: String,
    kind: String,
    params: String,
    remark: Option<String>,
    sync_enabled: bool,
    cloud_id: Option<String>,
    last_synced_at: Option<i64>,
    created_at: i64,
    updated_at: i64,
    owner_id: Option<String>,
}

impl FromSqliteRow for CertificateRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            kind: row.get("kind")?,
            params: row.get("params")?,
            remark: row.get("remark")?,
            sync_enabled: row
                .get::<_, i64>("sync_enabled")
                .map(|v| v != 0)
                .unwrap_or(true),
            cloud_id: row.get("cloud_id")?,
            last_synced_at: row.get("last_synced_at")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
            owner_id: row.get("owner_id").unwrap_or(None),
        })
    }
}

impl From<CertificateRow> for Certificate {
    fn from(row: CertificateRow) -> Self {
        let mut params: serde_json::Value = serde_json::from_str(&row.params)
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        // 对敏感字段解密
        if let Some(obj) = params.as_object_mut() {
            for key in ["password", "passphrase"] {
                if let Some(v) = obj.get(key).and_then(|v| v.as_str()) {
                    if !v.is_empty() {
                        let decrypted = decrypt_secret(Some(v.to_string()));
                        if let Some(d) = decrypted {
                            obj.insert(key.to_string(), serde_json::Value::String(d));
                        }
                    }
                }
            }
        }
        Self {
            id: Some(row.id),
            name: row.name,
            kind: CertificateKind::from_str(&row.kind),
            params,
            remark: row.remark,
            sync_enabled: row.sync_enabled,
            cloud_id: row.cloud_id,
            last_synced_at: row.last_synced_at,
            created_at: Some(row.created_at),
            updated_at: Some(row.updated_at),
            owner_id: row.owner_id,
        }
    }
}

#[derive(Clone)]
pub struct ConnectionRepository {
    conn: SqliteConnection,
}

impl ConnectionRepository {
    pub fn new(conn: SqliteConnection) -> Self {
        Self { conn }
    }

    pub fn insert_from_cloud(&self, item: &mut StoredConnection) -> Result<i64> {
        let name = item.name.clone();
        let connection_type = item.connection_type.to_string();
        let params_str = item.encrypt_params();
        let requested_sort_order = item.sort_order;
        let workspace_id = item.workspace_id;
        let selected_databases = item.selected_databases.clone();
        let remark = item.remark.clone();
        let sync_enabled = if item.sync_enabled { 1i64 } else { 0i64 };
        let cloud_id = item.cloud_id.clone();
        let updated_at = item.updated_at.unwrap_or_else(now);
        let last_synced_at = item.last_synced_at.or(Some(updated_at));
        let owner_id = item.owner_id.clone();
        let created_at = now();
        let computed_sort_order = self.conn.with_connection(|conn| {
            requested_sort_order
                .map(Ok)
                .unwrap_or_else(|| next_connection_sort_order(conn, workspace_id))
        })?;

        let id = self.conn.with_connection(|conn| {
            conn.execute(
                "INSERT INTO connections (name, connection_type, params, sort_order, workspace_id, selected_databases, remark, sync_enabled, cloud_id, last_synced_at, owner_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    name,
                    connection_type,
                    params_str,
                    computed_sort_order,
                    workspace_id,
                    selected_databases,
                    remark,
                    sync_enabled,
                    cloud_id,
                    last_synced_at,
                    owner_id,
                    created_at,
                    updated_at
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })?;

        item.id = Some(id);
        item.sort_order = Some(computed_sort_order);
        item.created_at = Some(created_at);
        item.updated_at = Some(updated_at);
        item.last_synced_at = last_synced_at;

        Ok(id)
    }

    pub fn update_from_cloud(&self, item: &StoredConnection) -> Result<()> {
        let id = item
            .id
            .ok_or_else(|| anyhow::anyhow!("Cannot update without ID"))?;
        let name = item.name.clone();
        let connection_type = item.connection_type.to_string();
        let params_str = item.encrypt_params();
        let requested_sort_order = item.sort_order;
        let workspace_id = item.workspace_id;
        let selected_databases = item.selected_databases.clone();
        let remark = item.remark.clone();
        let sync_enabled = if item.sync_enabled { 1i64 } else { 0i64 };
        let cloud_id = item.cloud_id.clone();
        let updated_at = item.updated_at.unwrap_or_else(now);
        let last_synced_at = item.last_synced_at.or(Some(updated_at));
        let owner_id = item.owner_id.clone();

        let (previous_workspace_id, previous_sort_order) = self.conn.with_connection(|conn| {
            conn.query_row(
                "SELECT workspace_id, sort_order FROM connections WHERE id = ?1",
                params![id],
                |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, i64>(1)?)),
            )
            .map_err(anyhow::Error::from)
        })?;
        let computed_sort_order = self
            .conn
            .with_connection(|conn| match requested_sort_order {
                Some(sort_order) => Ok(sort_order),
                None if previous_workspace_id == workspace_id => Ok(previous_sort_order),
                None => next_connection_sort_order(conn, workspace_id),
            })?;

        self.conn.with_connection(|conn| {
            conn.execute(
                "UPDATE connections SET name = ?1, connection_type = ?2, params = ?3, sort_order = ?4, workspace_id = ?5, selected_databases = ?6, remark = ?7, sync_enabled = ?8, cloud_id = ?9, last_synced_at = ?10, owner_id = ?11, updated_at = ?12 WHERE id = ?13",
                params![
                    name,
                    connection_type,
                    params_str,
                    computed_sort_order,
                    workspace_id,
                    selected_databases,
                    remark,
                    sync_enabled,
                    cloud_id,
                    last_synced_at,
                    owner_id,
                    updated_at,
                    id
                ],
            )?;
            Ok(())
        })
    }

    pub fn reorder_within_workspace(
        &self,
        workspace_id: Option<i64>,
        connection_ids: &[i64],
    ) -> Result<()> {
        if connection_ids.is_empty() {
            return Ok(());
        }

        let ts = now();
        self.conn.with_connection_mut(|conn| {
            let tx = conn.transaction()?;
            apply_connection_orders_in_workspace(&tx, workspace_id, connection_ids, ts)?;
            tx.commit()?;
            Ok(())
        })
    }

    pub fn move_across_workspaces(
        &self,
        connection_id: i64,
        source_workspace_id: Option<i64>,
        target_workspace_id: Option<i64>,
        source_connection_ids: &[i64],
        target_connection_ids: &[i64],
    ) -> Result<()> {
        if source_workspace_id == target_workspace_id {
            return Err(anyhow::anyhow!(
                "源工作区和目标工作区相同，无法跨工作区移动"
            ));
        }
        if source_connection_ids.contains(&connection_id) {
            return Err(anyhow::anyhow!(
                "跨工作区移动后的源工作区排序中仍包含目标连接 {}",
                connection_id
            ));
        }
        if !target_connection_ids.contains(&connection_id) {
            return Err(anyhow::anyhow!(
                "目标工作区排序中缺少被移动连接 {}",
                connection_id
            ));
        }

        let ts = now();
        self.conn.with_connection_mut(|conn| {
            let tx = conn.transaction()?;
            let current_workspace_id = tx
                .query_row(
                    "SELECT workspace_id FROM connections WHERE id = ?1",
                    params![connection_id],
                    |row| row.get::<_, Option<i64>>(0),
                )
                .map_err(anyhow::Error::from)?;
            if current_workspace_id != source_workspace_id {
                return Err(anyhow::anyhow!(
                    "连接 {} 当前工作区已变化，预期 {:?}，实际 {:?}",
                    connection_id,
                    source_workspace_id,
                    current_workspace_id
                ));
            }

            tx.execute(
                "UPDATE connections SET workspace_id = ?1, updated_at = ?2 WHERE id = ?3",
                params![target_workspace_id, ts, connection_id],
            )?;

            apply_connection_orders_in_workspace(
                &tx,
                source_workspace_id,
                source_connection_ids,
                ts,
            )?;
            apply_connection_orders_in_workspace(
                &tx,
                target_workspace_id,
                target_connection_ids,
                ts,
            )?;
            tx.commit()?;
            Ok(())
        })
    }
}

fn apply_connection_orders_in_workspace(
    tx: &rusqlite::Transaction<'_>,
    workspace_id: Option<i64>,
    connection_ids: &[i64],
    ts: i64,
) -> Result<()> {
    for (sort_order, connection_id) in connection_ids.iter().enumerate() {
        let rows = if let Some(workspace_id) = workspace_id {
            tx.execute(
                "UPDATE connections SET sort_order = ?1, updated_at = ?2 WHERE id = ?3 AND workspace_id = ?4",
                params![sort_order as i64, ts, connection_id, workspace_id],
            )?
        } else {
            tx.execute(
                "UPDATE connections SET sort_order = ?1, updated_at = ?2 WHERE id = ?3 AND workspace_id IS NULL",
                params![sort_order as i64, ts, connection_id],
            )?
        };

        if rows == 0 {
            return Err(anyhow::anyhow!(
                "连接 {} 不在目标工作区内，无法重排",
                connection_id
            ));
        }
    }

    Ok(())
}

#[derive(Clone)]
pub struct CertificateRepository {
    conn: SqliteConnection,
}

impl CertificateRepository {
    pub fn new(conn: SqliteConnection) -> Self {
        Self { conn }
    }

    pub fn update_from_cloud(&self, item: &Certificate) -> Result<()> {
        let id = item
            .id
            .ok_or_else(|| anyhow::anyhow!("Cannot update without ID"))?;
        let updated_at = item.updated_at.unwrap_or_else(now);

        let params_str = encrypt_certificate_params(&item.params);

        self.conn.with_connection(|conn| {
            conn.execute(
                "UPDATE certificates SET name = ?1, kind = ?2, params = ?3, remark = ?4, sync_enabled = ?5, cloud_id = ?6, last_synced_at = ?7, owner_id = ?8, updated_at = ?9 WHERE id = ?10",
                params![
                    item.name,
                    item.kind.to_string(),
                    params_str,
                    item.remark,
                    if item.sync_enabled { 1i64 } else { 0i64 },
                    item.cloud_id,
                    item.last_synced_at,
                    item.owner_id,
                    updated_at,
                    id
                ],
            )?;
            Ok(())
        })
    }

    pub fn update_sync_status(
        &self,
        id: i64,
        cloud_id: Option<String>,
        last_synced_at: Option<i64>,
    ) -> Result<()> {
        self.conn.with_connection(|conn| {
            conn.execute(
                "UPDATE certificates SET cloud_id = ?1, last_synced_at = ?2 WHERE id = ?3",
                params![cloud_id, last_synced_at, id],
            )?;
            Ok(())
        })
    }

    pub fn get_by_cloud_id(&self, cloud_id: &str) -> Result<Option<Certificate>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, kind, params, remark, sync_enabled, cloud_id, last_synced_at, created_at, updated_at, owner_id FROM certificates WHERE cloud_id = ?1",
            )?;
            let mut rows = stmt.query(params![cloud_id])?;
            if let Some(row) = rows.next()? {
                Ok(Some(CertificateRow::from_row(row)?.into()))
            } else {
                Ok(None)
            }
        })
    }

    pub fn list_personal(&self) -> Result<Vec<Certificate>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, kind, params, remark, sync_enabled, cloud_id, last_synced_at, created_at, updated_at, owner_id FROM certificates ORDER BY updated_at DESC",
            )?;
            let rows = stmt.query_map([], |row| CertificateRow::from_row(row))?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row?.into());
            }
            Ok(results)
        })
    }
}

impl Repository for CertificateRepository {
    type Entity = Certificate;

    fn entity_type(&self) -> SharedString {
        SharedString::from("Certificate")
    }

    fn insert(&self, item: &mut Self::Entity) -> Result<i64> {
        let ts = now();

        // Serialize params with encryption for sensitive fields
        let params_str = encrypt_certificate_params(&item.params);

        let id = self.conn.with_connection(|conn| {
            conn.execute(
                "INSERT INTO certificates (name, kind, params, remark, sync_enabled, cloud_id, last_synced_at, owner_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    item.name,
                    item.kind.to_string(),
                    params_str,
                    item.remark,
                    if item.sync_enabled { 1i64 } else { 0i64 },
                    item.cloud_id,
                    item.last_synced_at,
                    item.owner_id,
                    ts,
                    ts
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })?;

        item.id = Some(id);
        item.created_at = Some(ts);
        item.updated_at = Some(ts);

        Ok(id)
    }

    fn update(&self, item: &Self::Entity) -> Result<()> {
        let id = item
            .id
            .ok_or_else(|| anyhow::anyhow!("Cannot update without ID"))?;
        let ts = now();

        let params_str = encrypt_certificate_params(&item.params);

        self.conn.with_connection(|conn| {
            conn.execute(
                "UPDATE certificates SET name = ?1, kind = ?2, params = ?3, remark = ?4, sync_enabled = ?5, cloud_id = ?6, last_synced_at = ?7, owner_id = ?8, updated_at = ?9 WHERE id = ?10",
                params![
                    item.name,
                    item.kind.to_string(),
                    params_str,
                    item.remark,
                    if item.sync_enabled { 1i64 } else { 0i64 },
                    item.cloud_id,
                    item.last_synced_at,
                    item.owner_id,
                    ts,
                    id
                ],
            )?;
            Ok(())
        })
    }

    fn delete(&self, id: i64) -> Result<()> {
        self.conn.with_connection(|conn| {
            conn.execute("DELETE FROM certificates WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    fn get(&self, id: i64) -> Result<Option<Self::Entity>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, kind, params, remark, sync_enabled, cloud_id, last_synced_at, created_at, updated_at, owner_id FROM certificates WHERE id = ?1",
            )?;
            let mut rows = stmt.query(params![id])?;
            if let Some(row) = rows.next()? {
                Ok(Some(CertificateRow::from_row(row)?.into()))
            } else {
                Ok(None)
            }
        })
    }

    fn list(&self) -> Result<Vec<Self::Entity>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, kind, params, remark, sync_enabled, cloud_id, last_synced_at, created_at, updated_at, owner_id FROM certificates ORDER BY updated_at DESC",
            )?;
            let rows = stmt.query_map([], |row| CertificateRow::from_row(row))?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row?.into());
            }
            Ok(results)
        })
    }

    fn count(&self) -> Result<i64> {
        self.conn.with_connection(|conn| {
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM certificates", [], |row| row.get(0))?;
            Ok(count)
        })
    }

    fn exists(&self, id: i64) -> Result<bool> {
        self.conn.with_connection(|conn| {
            let exists: i64 = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM certificates WHERE id = ?1)",
                params![id],
                |row| row.get(0),
            )?;
            Ok(exists == 1)
        })
    }
}

impl Repository for ConnectionRepository {
    type Entity = StoredConnection;

    fn entity_type(&self) -> SharedString {
        SharedString::from("Connection")
    }

    fn insert(&self, item: &mut Self::Entity) -> Result<i64> {
        let name = item.name.clone();
        let connection_type = item.connection_type.to_string();
        let params_str = item.encrypt_params();
        let sort_order = item.sort_order;
        let workspace_id = item.workspace_id;
        let selected_databases = item.selected_databases.clone();
        let remark = item.remark.clone();
        let sync_enabled = if item.sync_enabled { 1i64 } else { 0i64 };
        let cloud_id = item.cloud_id.clone();
        let last_synced_at = item.last_synced_at;
        let owner_id = item.owner_id.clone();
        let ts = now();

        let computed_sort_order = self.conn.with_connection(|conn| {
            sort_order
                .map(Ok)
                .unwrap_or_else(|| next_connection_sort_order(conn, workspace_id))
        })?;

        let id = self.conn.with_connection(|conn| {
            conn.execute(
                "INSERT INTO connections (name, connection_type, params, sort_order, workspace_id, selected_databases, remark, sync_enabled, cloud_id, last_synced_at, owner_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![name, connection_type, params_str, computed_sort_order, workspace_id, selected_databases, remark, sync_enabled, cloud_id, last_synced_at, owner_id, ts, ts],
            )?;
            Ok(conn.last_insert_rowid())
        })?;

        item.id = Some(id);
        item.sort_order = Some(computed_sort_order);
        item.created_at = Some(ts);
        item.updated_at = Some(ts);

        Ok(id)
    }

    fn update(&self, item: &Self::Entity) -> Result<()> {
        let id = item
            .id
            .ok_or_else(|| anyhow::anyhow!("Cannot update without ID"))?;
        let name = item.name.clone();
        let connection_type = item.connection_type.to_string();
        let params_str = item.encrypt_params();
        let requested_sort_order = item.sort_order;
        let workspace_id = item.workspace_id;
        let selected_databases = item.selected_databases.clone();
        let remark = item.remark.clone();
        let sync_enabled = if item.sync_enabled { 1i64 } else { 0i64 };
        let cloud_id = item.cloud_id.clone();
        let last_synced_at = item.last_synced_at;
        let owner_id = item.owner_id.clone();
        let ts = now();

        let (previous_workspace_id, previous_sort_order) = self.conn.with_connection(|conn| {
            conn.query_row(
                "SELECT workspace_id, sort_order FROM connections WHERE id = ?1",
                params![id],
                |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, i64>(1)?)),
            )
            .map_err(anyhow::Error::from)
        })?;
        let computed_sort_order = self
            .conn
            .with_connection(|conn| match requested_sort_order {
                Some(sort_order) => Ok(sort_order),
                None if previous_workspace_id == workspace_id => Ok(previous_sort_order),
                None => next_connection_sort_order(conn, workspace_id),
            })?;

        self.conn.with_connection(|conn| {
            conn.execute(
                "UPDATE connections SET name = ?1, connection_type = ?2, params = ?3, sort_order = ?4, workspace_id = ?5, selected_databases = ?6, remark = ?7, sync_enabled = ?8, cloud_id = ?9, last_synced_at = ?10, owner_id = ?11, updated_at = ?12 WHERE id = ?13",
                params![name, connection_type, params_str, computed_sort_order, workspace_id, selected_databases, remark, sync_enabled, cloud_id, last_synced_at, owner_id, ts, id],
            )?;
            Ok(())
        })
    }

    fn delete(&self, id: i64) -> Result<()> {
        self.conn.with_connection(|conn| {
            conn.execute("DELETE FROM connections WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    fn get(&self, id: i64) -> Result<Option<Self::Entity>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, connection_type, params, sort_order, workspace_id, selected_databases, remark, sync_enabled, cloud_id, last_synced_at, created_at, updated_at, owner_id FROM connections WHERE id = ?1",
            )?;
            let mut rows = stmt.query(params![id])?;
            if let Some(row) = rows.next()? {
                Ok(Some(ConnectionRow::from_row(row)?.into()))
            } else {
                Ok(None)
            }
        })
    }

    fn list(&self) -> Result<Vec<Self::Entity>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, connection_type, params, sort_order, workspace_id, selected_databases, remark, sync_enabled, cloud_id, last_synced_at, created_at, updated_at, owner_id FROM connections ORDER BY updated_at DESC",
            )?;
            let rows = stmt.query_map([], |row| ConnectionRow::from_row(row))?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row?.into());
            }
            Ok(results)
        })
    }

    fn count(&self) -> Result<i64> {
        self.conn.with_connection(|conn| {
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM connections", [], |row| row.get(0))?;
            Ok(count)
        })
    }

    fn exists(&self, id: i64) -> Result<bool> {
        self.conn.with_connection(|conn| {
            let exists: i64 = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM connections WHERE id = ?1)",
                params![id],
                |row| row.get(0),
            )?;
            Ok(exists == 1)
        })
    }
}

impl ConnectionRepository {
    pub fn list_by_workspace(&self, workspace_id: Option<i64>) -> Result<Vec<StoredConnection>> {
        self.conn.with_connection(|conn| {
            let sql = if workspace_id.is_some() {
                "SELECT id, name, connection_type, params, sort_order, workspace_id, selected_databases, remark, sync_enabled, cloud_id, last_synced_at, created_at, updated_at, owner_id FROM connections WHERE workspace_id = ?1 ORDER BY updated_at DESC"
            } else {
                "SELECT id, name, connection_type, params, sort_order, workspace_id, selected_databases, remark, sync_enabled, cloud_id, last_synced_at, created_at, updated_at, owner_id FROM connections WHERE workspace_id IS NULL ORDER BY updated_at DESC"
            };
            let mut stmt = conn.prepare(sql)?;

            let mut results = Vec::new();
            if let Some(wid) = workspace_id {
                let rows = stmt.query_map(params![wid], |row| ConnectionRow::from_row(row))?;
                for row in rows {
                    results.push(row?.into());
                }
            } else {
                let rows = stmt.query_map([], |row| ConnectionRow::from_row(row))?;
                for row in rows {
                    results.push(row?.into());
                }
            }
            Ok(results)
        })
    }

    /// 更新连接的同步状态
    ///
    /// 同步成功后调用，设置 cloud_id 和 last_synced_at
    pub fn update_sync_status(
        &self,
        id: i64,
        cloud_id: Option<String>,
        last_synced_at: Option<i64>,
    ) -> Result<()> {
        self.conn.with_connection(|conn| {
            conn.execute(
                "UPDATE connections SET cloud_id = ?1, last_synced_at = ?2 WHERE id = ?3",
                params![cloud_id, last_synced_at, id],
            )?;
            Ok(())
        })
    }

    /// 查询需要同步的连接（sync_enabled=true 且 cloud_id 为空或 updated_at > last_synced_at）
    pub fn list_pending_sync(&self) -> Result<Vec<StoredConnection>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, connection_type, params, sort_order, workspace_id, selected_databases, remark, sync_enabled, cloud_id, last_synced_at, created_at, updated_at,  owner_id
                 FROM connections
                 WHERE sync_enabled = 1 AND (cloud_id IS NULL OR updated_at > COALESCE(last_synced_at, 0))
                 ORDER BY updated_at DESC",
            )?;
            let rows = stmt.query_map([], |row| ConnectionRow::from_row(row))?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row?.into());
            }
            Ok(results)
        })
    }

    /// 根据 cloud_id 查询连接
    pub fn get_by_cloud_id(&self, cloud_id: &str) -> Result<Option<StoredConnection>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, connection_type, params, sort_order, workspace_id, selected_databases, remark, sync_enabled, cloud_id, last_synced_at, created_at, updated_at,  owner_id
                 FROM connections WHERE cloud_id = ?1",
            )?;
            let mut rows = stmt.query(params![cloud_id])?;
            if let Some(row) = rows.next()? {
                Ok(Some(ConnectionRow::from_row(row)?.into()))
            } else {
                Ok(None)
            }
        })
    }

    /// 检测启用同步的连接中是否存在解密失败的数据。
    ///
    /// 返回值为 (id, name) 列表，便于上层记录日志与阻断同步。
    pub fn list_sync_decrypt_failures(&self) -> Result<Vec<(i64, String)>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, params FROM connections WHERE sync_enabled = 1 ORDER BY updated_at DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                let id: i64 = row.get("id")?;
                let name: String = row.get("name")?;
                let params: String = row.get("params")?;
                Ok((id, name, params))
            })?;

            let mut failures = Vec::new();
            for row in rows {
                let (id, name, params) = row?;
                if has_decrypt_failure_in_sensitive_fields(&params) {
                    failures.push((id, name));
                }
            }
            Ok(failures)
        })
    }

    /// 查询个人连接
    pub fn list_personal(&self) -> Result<Vec<StoredConnection>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, connection_type, params, sort_order, workspace_id, selected_databases, remark, sync_enabled, cloud_id, last_synced_at, created_at, updated_at, owner_id FROM connections ORDER BY updated_at DESC",
            )?;
            let rows = stmt.query_map([], |row| ConnectionRow::from_row(row))?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row?.into());
            }
            Ok(results)
        })
    }
}

#[derive(Clone)]
pub struct WorkspaceRepository {
    conn: SqliteConnection,
}

impl WorkspaceRepository {
    pub fn new(conn: SqliteConnection) -> Self {
        Self { conn }
    }

    pub fn update_from_cloud(&self, item: &Workspace) -> Result<()> {
        let id = item
            .id
            .ok_or_else(|| anyhow::anyhow!("Cannot update without ID"))?;
        let name = item.name.clone();
        let requested_sort_order = item.sort_order;
        let color = item.color.clone();
        let icon = item.icon.clone();
        let cloud_id = item.cloud_id.clone();
        let updated_at = item.updated_at.unwrap_or_else(now);
        let last_synced_at = item.last_synced_at.or(Some(updated_at));

        let previous_sort_order = self.conn.with_connection(|conn| {
            conn.query_row(
                "SELECT sort_order FROM workspaces WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .map_err(anyhow::Error::from)
        })?;
        let computed_sort_order = requested_sort_order.unwrap_or(previous_sort_order);

        self.conn.with_connection(|conn| {
            conn.execute(
                "UPDATE workspaces SET name = ?1, sort_order = ?2, color = ?3, icon = ?4, cloud_id = ?5, last_synced_at = ?6, updated_at = ?7 WHERE id = ?8",
                params![name, computed_sort_order, color, icon, cloud_id, last_synced_at, updated_at, id],
            )?;
            Ok(())
        })
    }

    /// 更新工作空间的云端同步状态
    pub fn update_sync_status(
        &self,
        local_id: i64,
        cloud_id: Option<String>,
        last_synced_at: Option<i64>,
    ) -> Result<()> {
        self.conn.with_connection(|conn| {
            conn.execute(
                "UPDATE workspaces SET cloud_id = ?1, last_synced_at = ?2 WHERE id = ?3",
                params![cloud_id, last_synced_at, local_id],
            )?;
            Ok(())
        })
    }

    pub fn get_by_cloud_id(&self, cloud_id: &str) -> Result<Option<Workspace>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, sort_order, color, icon, created_at, updated_at, cloud_id, last_synced_at FROM workspaces WHERE cloud_id = ?1",
            )?;
            let mut rows = stmt.query(params![cloud_id])?;
            if let Some(row) = rows.next()? {
                Ok(Some(WorkspaceRow::from_row(row)?.into()))
            } else {
                Ok(None)
            }
        })
    }

    pub fn reorder(&self, workspace_ids: &[i64]) -> Result<()> {
        if workspace_ids.is_empty() {
            return Ok(());
        }

        let ts = now();
        self.conn.with_connection_mut(|conn| {
            let tx = conn.transaction()?;
            for (sort_order, workspace_id) in workspace_ids.iter().enumerate() {
                let rows = tx.execute(
                    "UPDATE workspaces SET sort_order = ?1, last_synced_at = NULL, updated_at = ?2 WHERE id = ?3",
                    params![sort_order as i64, ts, workspace_id],
                )?;
                if rows == 0 {
                    return Err(anyhow::anyhow!("工作区 {} 不存在，无法重排", workspace_id));
                }
            }
            tx.commit()?;
            Ok(())
        })
    }
}

impl Repository for WorkspaceRepository {
    type Entity = Workspace;

    fn entity_type(&self) -> SharedString {
        SharedString::from("Workspace")
    }

    fn insert(&self, item: &mut Self::Entity) -> Result<i64> {
        let name = item.name.clone();
        let sort_order = item.sort_order;
        let color = item.color.clone();
        let icon = item.icon.clone();
        let cloud_id = item.cloud_id.clone();
        let last_synced_at = item.last_synced_at;
        let ts = now();
        let updated_at = item.updated_at.unwrap_or(ts);

        let computed_sort_order = self.conn.with_connection(|conn| {
            sort_order
                .map(Ok)
                .unwrap_or_else(|| next_workspace_sort_order(conn))
        })?;

        let id = self.conn.with_connection(|conn| {
            conn.execute(
                "INSERT INTO workspaces (name, sort_order, color, icon, cloud_id, last_synced_at, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![name, computed_sort_order, color, icon, cloud_id, last_synced_at, ts, updated_at],
            )?;
            Ok(conn.last_insert_rowid())
        })?;

        item.id = Some(id);
        item.sort_order = Some(computed_sort_order);
        item.created_at = Some(ts);
        item.updated_at = Some(updated_at);

        Ok(id)
    }

    fn update(&self, item: &Self::Entity) -> Result<()> {
        let id = item
            .id
            .ok_or_else(|| anyhow::anyhow!("Cannot update without ID"))?;
        let name = item.name.clone();
        let sort_order = item.sort_order;
        let color = item.color.clone();
        let icon = item.icon.clone();
        let cloud_id = item.cloud_id.clone();
        let ts = now();

        let computed_sort_order = self.conn.with_connection(|conn| match sort_order {
            Some(sort_order) => Ok(sort_order),
            None => conn
                .query_row(
                    "SELECT sort_order FROM workspaces WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .map_err(anyhow::Error::from),
        })?;

        self.conn.with_connection(|conn| {
            conn.execute(
                "UPDATE workspaces SET name = ?1, sort_order = ?2, color = ?3, icon = ?4, cloud_id = ?5, last_synced_at = NULL, updated_at = ?6 WHERE id = ?7",
                params![name, computed_sort_order, color, icon, cloud_id, ts, id],
            )?;
            Ok(())
        })
    }

    fn delete(&self, id: i64) -> Result<()> {
        let ts = now();
        self.conn.with_connection(|conn| {
            conn.execute(
                "UPDATE connections SET workspace_id = NULL, updated_at = ?2 WHERE workspace_id = ?1",
                params![id, ts],
            )?;
            conn.execute("DELETE FROM workspaces WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    fn get(&self, id: i64) -> Result<Option<Self::Entity>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare("SELECT id, name, sort_order, color, icon, created_at, updated_at, cloud_id, last_synced_at FROM workspaces WHERE id = ?1")?;
            let mut rows = stmt.query(params![id])?;
            if let Some(row) = rows.next()? {
                Ok(Some(WorkspaceRow::from_row(row)?.into()))
            } else {
                Ok(None)
            }
        })
    }

    fn list(&self) -> Result<Vec<Self::Entity>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare("SELECT id, name, sort_order, color, icon, created_at, updated_at, cloud_id, last_synced_at FROM workspaces ORDER BY updated_at DESC")?;
            let rows = stmt.query_map([], |row| WorkspaceRow::from_row(row))?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row?.into());
            }
            Ok(results)
        })
    }

    fn count(&self) -> Result<i64> {
        self.conn.with_connection(|conn| {
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM workspaces", [], |row| row.get(0))?;
            Ok(count)
        })
    }

    fn exists(&self, id: i64) -> Result<bool> {
        self.conn.with_connection(|conn| {
            let exists: i64 = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = ?1)",
                params![id],
                |row| row.get(0),
            )?;
            Ok(exists == 1)
        })
    }
}

/// 待删除云端记录
#[derive(Debug, Clone)]
pub struct PendingCloudDeletion {
    pub id: Option<i64>,
    pub cloud_id: String,
    pub entity_type: String,
    pub base_last_synced_at: Option<i64>,
    pub metadata: Option<PendingCloudDeletionMetadata>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingCloudDeletionMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_local_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affected_connection_ids: Vec<i64>,
}

/// 待删除云端记录仓库
#[derive(Clone)]
pub struct PendingCloudDeletionRepository {
    conn: SqliteConnection,
}

impl PendingCloudDeletionRepository {
    pub fn new(conn: SqliteConnection) -> Self {
        Self { conn }
    }

    /// 添加待删除记录
    pub fn add(&self, cloud_id: &str, entity_type: &str) -> Result<()> {
        self.add_with_context(cloud_id, entity_type, None, None)
    }

    pub fn add_with_context(
        &self,
        cloud_id: &str,
        entity_type: &str,
        base_last_synced_at: Option<i64>,
        metadata: Option<&PendingCloudDeletionMetadata>,
    ) -> Result<()> {
        let ts = now();
        let metadata = metadata
            .map(serde_json::to_string)
            .transpose()?
            .filter(|value| !value.is_empty());
        self.conn.with_connection(|conn| {
            conn.execute(
                "INSERT OR IGNORE INTO pending_cloud_deletions (cloud_id, entity_type, base_last_synced_at, metadata, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![cloud_id, entity_type, base_last_synced_at, metadata, ts],
            )?;
            Ok(())
        })
    }

    pub fn list_by_entity_type(&self, entity_type: &str) -> Result<Vec<PendingCloudDeletion>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, cloud_id, entity_type, base_last_synced_at, metadata, created_at FROM pending_cloud_deletions WHERE entity_type = ?1",
            )?;
            let rows = stmt.query_map(params![entity_type], |row| {
                let metadata = row.get::<_, Option<String>>(4)?;
                Ok(PendingCloudDeletion {
                    id: row.get(0)?,
                    cloud_id: row.get(1)?,
                    entity_type: row.get(2)?,
                    base_last_synced_at: row.get(3)?,
                    metadata: metadata
                        .as_deref()
                        .map(serde_json::from_str::<PendingCloudDeletionMetadata>)
                        .transpose()
                        .map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                4,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?,
                    created_at: row.get(5)?,
                })
            })?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row?);
            }
            Ok(results)
        })
    }

    /// 获取所有待删除的连接
    pub fn list_connections(&self) -> Result<Vec<PendingCloudDeletion>> {
        self.list_by_entity_type("connection")
    }

    /// 获取所有待删除的工作空间
    pub fn list_workspaces(&self) -> Result<Vec<PendingCloudDeletion>> {
        self.list_by_entity_type("workspace")
    }

    /// 获取所有待删除的证书
    pub fn list_certificates(&self) -> Result<Vec<PendingCloudDeletion>> {
        self.list_by_entity_type("certificate")
    }

    /// 删除记录（同步成功后调用）
    pub fn remove(&self, cloud_id: &str) -> Result<()> {
        self.conn.with_connection(|conn| {
            conn.execute(
                "DELETE FROM pending_cloud_deletions WHERE cloud_id = ?1",
                params![cloud_id],
            )?;
            Ok(())
        })
    }

    /// 检查 cloud_id 是否在待删除列表中
    pub fn is_pending(&self, cloud_id: &str) -> Result<bool> {
        self.conn.with_connection(|conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM pending_cloud_deletions WHERE cloud_id = ?1",
                params![cloud_id],
                |row| row.get(0),
            )?;
            Ok(count > 0)
        })
    }
}

pub fn sync_connections_for_certificate(
    storage: &StorageManager,
    certificate: &Certificate,
) -> Result<Vec<StoredConnection>> {
    let connection_repo = storage
        .get::<ConnectionRepository>()
        .ok_or_else(|| anyhow::anyhow!("ConnectionRepository not found"))?;

    let mut changed_connections = Vec::new();
    for mut connection in connection_repo.list()? {
        if apply_certificate_to_connection_snapshot(&mut connection, certificate) {
            connection_repo.update(&connection)?;
            changed_connections.push(connection);
        }
    }

    Ok(changed_connections)
}

pub fn detach_connections_for_certificate(
    storage: &StorageManager,
    local_id: Option<i64>,
    cloud_id: Option<&str>,
) -> Result<Vec<StoredConnection>> {
    let connection_repo = storage
        .get::<ConnectionRepository>()
        .ok_or_else(|| anyhow::anyhow!("ConnectionRepository not found"))?;

    let mut changed_connections = Vec::new();
    for mut connection in connection_repo.list()? {
        if detach_certificate_from_connection_snapshot(&mut connection, local_id, cloud_id) {
            connection_repo.update(&connection)?;
            changed_connections.push(connection);
        }
    }

    Ok(changed_connections)
}

#[derive(Clone)]
pub struct KeyValueRepository {
    conn: SqliteConnection,
}

impl KeyValueRepository {
    pub fn new(conn: SqliteConnection) -> Self {
        Self { conn }
    }

    pub fn get_by_key(&self, key: &str) -> Result<Option<String>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare("SELECT value FROM key_values WHERE key = ?1")?;
            let mut rows = stmt.query(params![key])?;
            if let Some(row) = rows.next()? {
                Ok(Some(row.get(0)?))
            } else {
                Ok(None)
            }
        })
    }

    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        let ts = now();
        self.conn.with_connection(|conn| {
            conn.execute(
                "INSERT INTO key_values (key, value, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = ?4",
                params![key, value, ts, ts],
            )?;
            Ok(())
        })
    }

    pub fn delete(&self, key: &str) -> Result<()> {
        self.conn.with_connection(|conn| {
            conn.execute("DELETE FROM key_values WHERE key = ?1", params![key])?;
            Ok(())
        })
    }
}

pub fn init(cx: &mut App) {
    let storage_state = cx.global::<GlobalStorageState>();
    let storage = storage_state.storage.clone();

    let conn = storage.connection();
    let certificate_repo = CertificateRepository::new(conn.clone());
    let conn_repo = ConnectionRepository::new(conn.clone());
    let workspace_repo = WorkspaceRepository::new(conn.clone());
    let quick_cmd_repo = QuickCommandRepository::new(conn.clone());
    let sftp_favorite_path_repo = SftpFavoritePathRepository::new(conn.clone());
    let pending_deletion_repo = PendingCloudDeletionRepository::new(conn.clone());
    let kv_repo = KeyValueRepository::new(conn.clone());

    storage.register(certificate_repo);
    storage.register(workspace_repo);
    storage.register(conn_repo);
    storage.register(quick_cmd_repo);
    storage.register(sftp_favorite_path_repo);
    storage.register(pending_deletion_repo);
    storage.register(kv_repo);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::migration::run_migrations;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn create_test_sqlite_connection() -> SqliteConnection {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("系统时间不应回退")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("one-core-repository-test-{unique_id}.db"));
        let conn = SqliteConnection::open(&path).unwrap();
        conn.with_connection(|db| {
            run_migrations(db)?;
            Ok(())
        })
        .unwrap();
        conn
    }

    #[test]
    fn workspace_repository_can_lookup_by_cloud_id() {
        let conn = create_test_sqlite_connection();
        let repo = WorkspaceRepository::new(conn);

        let mut workspace = Workspace::new("测试工作区".to_string());
        workspace.cloud_id = Some("workspace-cloud-1".to_string());
        repo.insert(&mut workspace).unwrap();

        let found = repo.get_by_cloud_id("workspace-cloud-1").unwrap().unwrap();
        assert_eq!(found.id, workspace.id);
        assert_eq!(found.name, "测试工作区");
        assert_eq!(found.cloud_id.as_deref(), Some("workspace-cloud-1"));
    }

    #[test]
    fn workspace_repository_update_sync_status_persists_last_synced_at() {
        let conn = create_test_sqlite_connection();
        let repo = WorkspaceRepository::new(conn);

        let mut workspace = Workspace::new("同步工作区".to_string());
        repo.insert(&mut workspace).unwrap();

        let workspace_id = workspace.id.expect("插入后应生成工作区 ID");
        repo.update_sync_status(
            workspace_id,
            Some("workspace-cloud-sync".to_string()),
            Some(456),
        )
        .unwrap();

        let found = repo.get(workspace_id).unwrap().unwrap();
        assert_eq!(found.cloud_id.as_deref(), Some("workspace-cloud-sync"));
        assert_eq!(found.last_synced_at, Some(456));
    }

    #[test]
    fn workspace_repository_local_update_clears_last_synced_at() {
        let conn = create_test_sqlite_connection();
        let repo = WorkspaceRepository::new(conn);

        let mut workspace = Workspace::new("本地修改工作区".to_string());
        workspace.cloud_id = Some("workspace-cloud-local".to_string());
        workspace.last_synced_at = Some(789);
        repo.insert(&mut workspace).unwrap();

        workspace.name = "本地修改工作区-已更新".to_string();
        repo.update(&workspace).unwrap();

        let workspace_id = workspace.id.expect("插入后应生成工作区 ID");
        let found = repo.get(workspace_id).unwrap().unwrap();
        assert_eq!(found.name, "本地修改工作区-已更新");
        assert_eq!(found.cloud_id.as_deref(), Some("workspace-cloud-local"));
        assert_eq!(found.last_synced_at, None);
    }

    #[test]
    fn workspace_repository_delete_unlinks_connections_and_refreshes_timestamp() {
        let conn = create_test_sqlite_connection();
        let workspace_repo = WorkspaceRepository::new(conn.clone());
        let connection_repo = ConnectionRepository::new(conn);

        let mut workspace = Workspace::new("删除联动工作区".to_string());
        workspace_repo.insert(&mut workspace).unwrap();
        let workspace_id = workspace.id.expect("插入后应生成工作区 ID");

        let mut connection = StoredConnection::new_ssh(
            "删除联动连接".to_string(),
            crate::storage::models::SshParams {
                host: "127.0.0.1".to_string(),
                port: 22,
                username: "tester".to_string(),
                auth_method: crate::storage::models::SshAuthMethod::Agent,
                credential_ref: None,
                connect_timeout: None,
                keepalive_interval: None,
                keepalive_max: None,
                enable_legacy_kex: false,
                default_directory: None,
                init_script: None,
                disable_shell_integration: None,
                sftp_local_directory: None,
                sftp_remote_directory: None,
                jump_server: None,
                proxy: None,
            },
            Some(workspace_id),
        );
        connection.last_synced_at = Some(123);
        connection_repo.insert(&mut connection).unwrap();
        let connection_id = connection.id.expect("插入后应生成连接 ID");
        let original_updated_at = connection.updated_at.expect("插入后应生成更新时间");

        workspace_repo.delete(workspace_id).unwrap();

        let found = connection_repo.get(connection_id).unwrap().unwrap();
        assert_eq!(found.workspace_id, None);
        assert!(found.updated_at.expect("删除后应保留更新时间") >= original_updated_at);
        assert_eq!(found.last_synced_at, Some(123));
    }

    #[test]
    fn connection_repository_update_from_cloud_preserves_sync_baseline() {
        let conn = create_test_sqlite_connection();
        let repo = ConnectionRepository::new(conn);

        let mut connection = StoredConnection::new_ssh(
            "云端回写连接".to_string(),
            crate::storage::models::SshParams {
                host: "127.0.0.1".to_string(),
                port: 22,
                username: "tester".to_string(),
                auth_method: crate::storage::models::SshAuthMethod::Agent,
                credential_ref: None,
                connect_timeout: None,
                keepalive_interval: None,
                keepalive_max: None,
                enable_legacy_kex: false,
                default_directory: None,
                init_script: None,
                disable_shell_integration: None,
                sftp_local_directory: None,
                sftp_remote_directory: None,
                jump_server: None,
                proxy: None,
            },
            None,
        );
        repo.insert(&mut connection).unwrap();

        let connection_id = connection.id.expect("插入后应生成连接 ID");
        connection.name = "云端回写连接-已更新".to_string();
        connection.cloud_id = Some("cloud-connection-sync".to_string());
        connection.updated_at = Some(456);
        connection.last_synced_at = Some(456);

        repo.update_from_cloud(&connection).unwrap();

        let found = repo.get(connection_id).unwrap().unwrap();
        assert_eq!(found.name, "云端回写连接-已更新");
        assert_eq!(found.cloud_id.as_deref(), Some("cloud-connection-sync"));
        assert_eq!(found.updated_at, Some(456));
        assert_eq!(found.last_synced_at, Some(456));
    }

    #[test]
    fn pending_cloud_deletion_repository_persists_context_fields() {
        let conn = create_test_sqlite_connection();
        let repo = PendingCloudDeletionRepository::new(conn);

        let metadata = PendingCloudDeletionMetadata {
            workspace_local_id: Some(9),
            affected_connection_ids: vec![1, 2, 3],
        };

        repo.add_with_context(
            "workspace-cloud-ctx",
            "workspace",
            Some(456),
            Some(&metadata),
        )
        .unwrap();

        let pending = repo
            .list_workspaces()
            .unwrap()
            .into_iter()
            .find(|item| item.cloud_id == "workspace-cloud-ctx")
            .expect("应能查到待删除工作区");

        assert_eq!(pending.base_last_synced_at, Some(456));
        assert_eq!(pending.metadata, Some(metadata));
    }

    #[test]
    fn workspace_repository_reorder_updates_sort_order() {
        let conn = create_test_sqlite_connection();
        let repo = WorkspaceRepository::new(conn);

        let mut first = Workspace::new("工作区-A".to_string());
        let mut second = Workspace::new("工作区-B".to_string());
        let mut third = Workspace::new("工作区-C".to_string());
        repo.insert(&mut first).unwrap();
        repo.insert(&mut second).unwrap();
        repo.insert(&mut third).unwrap();

        repo.reorder(&[
            third.id.expect("第三个工作区应有 ID"),
            first.id.expect("第一个工作区应有 ID"),
            second.id.expect("第二个工作区应有 ID"),
        ])
        .unwrap();

        let reloaded = repo.list().unwrap();
        let third = reloaded
            .iter()
            .find(|workspace| workspace.id == third.id)
            .expect("应能重新读到第三个工作区");
        let first = reloaded
            .iter()
            .find(|workspace| workspace.id == first.id)
            .expect("应能重新读到第一个工作区");
        let second = reloaded
            .iter()
            .find(|workspace| workspace.id == second.id)
            .expect("应能重新读到第二个工作区");

        assert_eq!(third.sort_order, Some(0));
        assert_eq!(first.sort_order, Some(1));
        assert_eq!(second.sort_order, Some(2));
    }

    #[test]
    fn connection_repository_reorder_within_workspace_only_changes_target_group() {
        let conn = create_test_sqlite_connection();
        let workspace_repo = WorkspaceRepository::new(conn.clone());
        let connection_repo = ConnectionRepository::new(conn);

        let mut workspace = Workspace::new("目标工作区".to_string());
        let mut other_workspace = Workspace::new("其他工作区".to_string());
        workspace_repo.insert(&mut workspace).unwrap();
        workspace_repo.insert(&mut other_workspace).unwrap();

        let workspace_id = workspace.id.expect("目标工作区应有 ID");
        let other_workspace_id = other_workspace.id.expect("其他工作区应有 ID");

        let mut first = StoredConnection::new_ssh(
            "连接-A".to_string(),
            crate::storage::models::SshParams {
                host: "127.0.0.1".to_string(),
                port: 22,
                username: "tester".to_string(),
                auth_method: crate::storage::models::SshAuthMethod::Agent,
                credential_ref: None,
                connect_timeout: None,
                keepalive_interval: None,
                keepalive_max: None,
                enable_legacy_kex: false,
                default_directory: None,
                init_script: None,
                disable_shell_integration: None,
                sftp_local_directory: None,
                sftp_remote_directory: None,
                jump_server: None,
                proxy: None,
            },
            Some(workspace_id),
        );
        let mut second = StoredConnection::new_ssh(
            "连接-B".to_string(),
            crate::storage::models::SshParams {
                host: "127.0.0.2".to_string(),
                port: 22,
                username: "tester".to_string(),
                auth_method: crate::storage::models::SshAuthMethod::Agent,
                credential_ref: None,
                connect_timeout: None,
                keepalive_interval: None,
                keepalive_max: None,
                enable_legacy_kex: false,
                default_directory: None,
                init_script: None,
                disable_shell_integration: None,
                sftp_local_directory: None,
                sftp_remote_directory: None,
                jump_server: None,
                proxy: None,
            },
            Some(workspace_id),
        );
        let mut third = StoredConnection::new_ssh(
            "连接-C".to_string(),
            crate::storage::models::SshParams {
                host: "127.0.0.3".to_string(),
                port: 22,
                username: "tester".to_string(),
                auth_method: crate::storage::models::SshAuthMethod::Agent,
                credential_ref: None,
                connect_timeout: None,
                keepalive_interval: None,
                keepalive_max: None,
                enable_legacy_kex: false,
                default_directory: None,
                init_script: None,
                disable_shell_integration: None,
                sftp_local_directory: None,
                sftp_remote_directory: None,
                jump_server: None,
                proxy: None,
            },
            Some(workspace_id),
        );
        let mut other = StoredConnection::new_ssh(
            "其他连接".to_string(),
            crate::storage::models::SshParams {
                host: "127.0.0.4".to_string(),
                port: 22,
                username: "tester".to_string(),
                auth_method: crate::storage::models::SshAuthMethod::Agent,
                credential_ref: None,
                connect_timeout: None,
                keepalive_interval: None,
                keepalive_max: None,
                enable_legacy_kex: false,
                default_directory: None,
                init_script: None,
                disable_shell_integration: None,
                sftp_local_directory: None,
                sftp_remote_directory: None,
                jump_server: None,
                proxy: None,
            },
            Some(other_workspace_id),
        );
        connection_repo.insert(&mut first).unwrap();
        connection_repo.insert(&mut second).unwrap();
        connection_repo.insert(&mut third).unwrap();
        connection_repo.insert(&mut other).unwrap();

        connection_repo
            .reorder_within_workspace(
                Some(workspace_id),
                &[
                    third.id.expect("第三个连接应有 ID"),
                    first.id.expect("第一个连接应有 ID"),
                    second.id.expect("第二个连接应有 ID"),
                ],
            )
            .unwrap();

        let first = connection_repo
            .get(first.id.expect("第一个连接应有 ID"))
            .unwrap()
            .unwrap();
        let second = connection_repo
            .get(second.id.expect("第二个连接应有 ID"))
            .unwrap()
            .unwrap();
        let third = connection_repo
            .get(third.id.expect("第三个连接应有 ID"))
            .unwrap()
            .unwrap();
        let other = connection_repo
            .get(other.id.expect("其他连接应有 ID"))
            .unwrap()
            .unwrap();

        assert_eq!(third.sort_order, Some(0));
        assert_eq!(first.sort_order, Some(1));
        assert_eq!(second.sort_order, Some(2));
        assert_eq!(other.sort_order, Some(0));
    }

    #[test]
    fn connection_repository_update_assigns_end_order_when_workspace_changes() {
        let conn = create_test_sqlite_connection();
        let workspace_repo = WorkspaceRepository::new(conn.clone());
        let connection_repo = ConnectionRepository::new(conn);

        let mut workspace = Workspace::new("迁移目标工作区".to_string());
        workspace_repo.insert(&mut workspace).unwrap();
        let workspace_id = workspace.id.expect("工作区应有 ID");

        let mut existing = StoredConnection::new_ssh(
            "现有连接".to_string(),
            crate::storage::models::SshParams {
                host: "127.0.0.10".to_string(),
                port: 22,
                username: "tester".to_string(),
                auth_method: crate::storage::models::SshAuthMethod::Agent,
                credential_ref: None,
                connect_timeout: None,
                keepalive_interval: None,
                keepalive_max: None,
                enable_legacy_kex: false,
                default_directory: None,
                init_script: None,
                disable_shell_integration: None,
                sftp_local_directory: None,
                sftp_remote_directory: None,
                jump_server: None,
                proxy: None,
            },
            Some(workspace_id),
        );
        connection_repo.insert(&mut existing).unwrap();

        let mut moving = StoredConnection::new_ssh(
            "待移动连接".to_string(),
            crate::storage::models::SshParams {
                host: "127.0.0.11".to_string(),
                port: 22,
                username: "tester".to_string(),
                auth_method: crate::storage::models::SshAuthMethod::Agent,
                credential_ref: None,
                connect_timeout: None,
                keepalive_interval: None,
                keepalive_max: None,
                enable_legacy_kex: false,
                default_directory: None,
                init_script: None,
                disable_shell_integration: None,
                sftp_local_directory: None,
                sftp_remote_directory: None,
                jump_server: None,
                proxy: None,
            },
            None,
        );
        connection_repo.insert(&mut moving).unwrap();

        let moving_id = moving.id.expect("待移动连接应有 ID");
        let mut updated = connection_repo.get(moving_id).unwrap().unwrap();
        updated.workspace_id = Some(workspace_id);
        updated.sort_order = None;
        connection_repo.update(&updated).unwrap();

        let moved = connection_repo.get(moving_id).unwrap().unwrap();
        assert_eq!(moved.workspace_id, Some(workspace_id));
        assert_eq!(moved.sort_order, Some(1));
    }

    #[test]
    fn connection_repository_move_across_workspaces_reorders_source_and_target() {
        let conn = create_test_sqlite_connection();
        let workspace_repo = WorkspaceRepository::new(conn.clone());
        let connection_repo = ConnectionRepository::new(conn);

        let mut source_workspace = Workspace::new("源工作区".to_string());
        workspace_repo.insert(&mut source_workspace).unwrap();
        let source_workspace_id = source_workspace.id.expect("源工作区应有 ID");

        let mut target_workspace = Workspace::new("目标工作区".to_string());
        workspace_repo.insert(&mut target_workspace).unwrap();
        let target_workspace_id = target_workspace.id.expect("目标工作区应有 ID");

        let mut source_first = StoredConnection::new_ssh(
            "源连接-A".to_string(),
            crate::storage::models::SshParams {
                host: "127.0.1.1".to_string(),
                port: 22,
                username: "tester".to_string(),
                auth_method: crate::storage::models::SshAuthMethod::Agent,
                credential_ref: None,
                connect_timeout: None,
                keepalive_interval: None,
                keepalive_max: None,
                enable_legacy_kex: false,
                default_directory: None,
                init_script: None,
                disable_shell_integration: None,
                sftp_local_directory: None,
                sftp_remote_directory: None,
                jump_server: None,
                proxy: None,
            },
            Some(source_workspace_id),
        );
        let mut moving = StoredConnection::new_ssh(
            "待移动连接".to_string(),
            crate::storage::models::SshParams {
                host: "127.0.1.2".to_string(),
                port: 22,
                username: "tester".to_string(),
                auth_method: crate::storage::models::SshAuthMethod::Agent,
                credential_ref: None,
                connect_timeout: None,
                keepalive_interval: None,
                keepalive_max: None,
                enable_legacy_kex: false,
                default_directory: None,
                init_script: None,
                disable_shell_integration: None,
                sftp_local_directory: None,
                sftp_remote_directory: None,
                jump_server: None,
                proxy: None,
            },
            Some(source_workspace_id),
        );
        let mut source_last = StoredConnection::new_ssh(
            "源连接-B".to_string(),
            crate::storage::models::SshParams {
                host: "127.0.1.3".to_string(),
                port: 22,
                username: "tester".to_string(),
                auth_method: crate::storage::models::SshAuthMethod::Agent,
                credential_ref: None,
                connect_timeout: None,
                keepalive_interval: None,
                keepalive_max: None,
                enable_legacy_kex: false,
                default_directory: None,
                init_script: None,
                disable_shell_integration: None,
                sftp_local_directory: None,
                sftp_remote_directory: None,
                jump_server: None,
                proxy: None,
            },
            Some(source_workspace_id),
        );
        let mut target_first = StoredConnection::new_ssh(
            "目标连接-A".to_string(),
            crate::storage::models::SshParams {
                host: "127.0.2.1".to_string(),
                port: 22,
                username: "tester".to_string(),
                auth_method: crate::storage::models::SshAuthMethod::Agent,
                credential_ref: None,
                connect_timeout: None,
                keepalive_interval: None,
                keepalive_max: None,
                enable_legacy_kex: false,
                default_directory: None,
                init_script: None,
                disable_shell_integration: None,
                sftp_local_directory: None,
                sftp_remote_directory: None,
                jump_server: None,
                proxy: None,
            },
            Some(target_workspace_id),
        );
        let mut target_last = StoredConnection::new_ssh(
            "目标连接-B".to_string(),
            crate::storage::models::SshParams {
                host: "127.0.2.2".to_string(),
                port: 22,
                username: "tester".to_string(),
                auth_method: crate::storage::models::SshAuthMethod::Agent,
                credential_ref: None,
                connect_timeout: None,
                keepalive_interval: None,
                keepalive_max: None,
                enable_legacy_kex: false,
                default_directory: None,
                init_script: None,
                disable_shell_integration: None,
                sftp_local_directory: None,
                sftp_remote_directory: None,
                jump_server: None,
                proxy: None,
            },
            Some(target_workspace_id),
        );

        connection_repo.insert(&mut source_first).unwrap();
        connection_repo.insert(&mut moving).unwrap();
        connection_repo.insert(&mut source_last).unwrap();
        connection_repo.insert(&mut target_first).unwrap();
        connection_repo.insert(&mut target_last).unwrap();

        let moving_id = moving.id.expect("待移动连接应有 ID");
        connection_repo
            .move_across_workspaces(
                moving_id,
                Some(source_workspace_id),
                Some(target_workspace_id),
                &[
                    source_first.id.expect("源连接-A 应有 ID"),
                    source_last.id.expect("源连接-B 应有 ID"),
                ],
                &[
                    target_first.id.expect("目标连接-A 应有 ID"),
                    moving_id,
                    target_last.id.expect("目标连接-B 应有 ID"),
                ],
            )
            .unwrap();

        let source_first = connection_repo
            .get(source_first.id.expect("源连接-A 应有 ID"))
            .unwrap()
            .unwrap();
        let moving = connection_repo.get(moving_id).unwrap().unwrap();
        let source_last = connection_repo
            .get(source_last.id.expect("源连接-B 应有 ID"))
            .unwrap()
            .unwrap();
        let target_first = connection_repo
            .get(target_first.id.expect("目标连接-A 应有 ID"))
            .unwrap()
            .unwrap();
        let target_last = connection_repo
            .get(target_last.id.expect("目标连接-B 应有 ID"))
            .unwrap()
            .unwrap();

        assert_eq!(source_first.workspace_id, Some(source_workspace_id));
        assert_eq!(source_first.sort_order, Some(0));
        assert_eq!(source_last.workspace_id, Some(source_workspace_id));
        assert_eq!(source_last.sort_order, Some(1));

        assert_eq!(target_first.workspace_id, Some(target_workspace_id));
        assert_eq!(target_first.sort_order, Some(0));
        assert_eq!(moving.workspace_id, Some(target_workspace_id));
        assert_eq!(moving.sort_order, Some(1));
        assert_eq!(target_last.workspace_id, Some(target_workspace_id));
        assert_eq!(target_last.sort_order, Some(2));
    }
}
