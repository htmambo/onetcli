use crate::cache::CacheContext;
use crate::cache_manager::GlobalNodeCache;
use crate::clickhouse::ClickHousePlugin;
use crate::connection::{DbConnection, DbError, StreamingProgress};
use crate::duckdb::DuckDbPlugin;
use crate::import_export::{
    ExportConfig, ExportProgressSender, ExportResult, ImportConfig, ImportResult,
};
use crate::ipc::ExternalDatabasePlugin;
use crate::mssql::MsSqlPlugin;
use crate::mysql::MySqlPlugin;
use crate::oracle::OraclePlugin;
use crate::plugin::DatabasePlugin;
use crate::plugin_manifest::DatabaseCapabilities;
use crate::postgresql::PostgresPlugin;
use crate::sqlite::SqlitePlugin;
use crate::{
    DbNode, DbNodeType, ExecOptions, SqlErrorInfo, SqlResult, SqlSource, TableSaveResponse,
};
use dashmap::DashMap;
use gpui::{AppContext, AsyncApp, Global};
use one_core::connection_notifier::{ConnectionDataEvent, GlobalConnectionNotifier};
use one_core::gpui_tokio::Tokio;
use one_core::storage::{DatabaseType, DbConnectionConfig};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Macro to reduce boilerplate for plugin operations with session management
macro_rules! with_plugin_session {
    ($self:expr, $cx:expr, $connection_id:expr, |$plugin:ident, $conn:ident| $body:expr) => {{
        let config = $self.get_config(&$connection_id);
        if config.is_none() {
            error!(
                "with_plugin_session: Connection not found: {}",
                $connection_id
            );
        }
        let config =
            config.ok_or_else(|| anyhow::anyhow!("Connection not found: {}", $connection_id))?;

        let clone_self = $self.clone();
        Tokio::spawn_result($cx, async move {
            let $plugin = clone_self.get_plugin(&config.database_type)?;
            info!(
                "with_plugin_session: creating session for config_id={}",
                config.id
            );
            let session_id = clone_self
                .connection_manager
                .create_session(config.clone(), &clone_self.db_manager)
                .await?;
            info!("with_plugin_session: session created: {}", session_id);

            let result = {
                let mut guard = clone_self
                    .connection_manager
                    .get_session_connection(&session_id)
                    .await?;
                let $conn = guard
                    .connection()
                    .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
                $body.map_err(|e| anyhow::anyhow!("{}", e))
            };

            clone_self
                .connection_manager
                .release_session(&session_id)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            result
        })
        .await
    }};
}

/// Macro with database parameter for PostgreSQL and other databases that require connection-level database selection
macro_rules! with_plugin_session_db {
    ($self:expr, $cx:expr, $connection_id:expr, $database:expr, |$plugin:ident, $conn:ident| $body:expr) => {{
        let config = $self.get_config(&$connection_id);
        if config.is_none() {
            error!(
                "with_plugin_session_db: Connection not found: {}",
                $connection_id
            );
        }
        let mut config = config
            .ok_or_else(|| anyhow::anyhow!("Connection not found: {}", $connection_id))?
            .clone();
        config.database = Some($database.to_string());

        let clone_self = $self.clone();
        Tokio::spawn_result($cx, async move {
            let $plugin = clone_self.get_plugin(&config.database_type)?;
            let session_id = clone_self
                .connection_manager
                .create_session(config, &clone_self.db_manager)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            let result = {
                let mut guard = clone_self
                    .connection_manager
                    .get_session_connection(&session_id)
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
                let $conn = guard
                    .connection()
                    .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
                $body.map_err(|e| anyhow::anyhow!("{}", e))
            };

            clone_self
                .connection_manager
                .release_session(&session_id)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            result
        })
        .await
    }};
}

async fn cached_foreign_keys(
    cx: &mut AsyncApp,
    cache: GlobalNodeCache,
    connection_id: &str,
    database: &str,
    schema: Option<&str>,
    table: &str,
) -> anyhow::Result<Vec<crate::types::ForeignKeyDefinition>> {
    let conn_id = connection_id.to_string();
    let db = database.to_string();
    let sch = schema.map(str::to_string);
    let tbl = table.to_string();
    Tokio::spawn_result(cx, async move {
        cache
            .get_foreign_keys(&conn_id, &db, sch.as_deref(), &tbl)
            .await
            .ok_or_else(|| anyhow::anyhow!("Cache miss"))
    })
    .await
}

/// Database manager - creates database plugins
pub struct DbManager {
    mysql: Arc<dyn DatabasePlugin>,
    postgresql: Arc<dyn DatabasePlugin>,
    sqlite: Arc<dyn DatabasePlugin>,
    duckdb: Arc<dyn DatabasePlugin>,
    clickhouse: Arc<dyn DatabasePlugin>,
    mssql: Arc<dyn DatabasePlugin>,
    oracle: Arc<dyn DatabasePlugin>,
    external: Arc<dyn DatabasePlugin>,
}

impl DbManager {
    pub fn new() -> Self {
        Self {
            mysql: Arc::new(MySqlPlugin::new()),
            postgresql: Arc::new(PostgresPlugin::new()),
            sqlite: Arc::new(SqlitePlugin::new()),
            duckdb: Arc::new(DuckDbPlugin::new()),
            clickhouse: Arc::new(ClickHousePlugin::new()),
            mssql: Arc::new(MsSqlPlugin::new()),
            oracle: Arc::new(OraclePlugin::new()),
            external: Arc::new(ExternalDatabasePlugin::new()),
        }
    }

    pub fn get_plugin(&self, db_type: &DatabaseType) -> Result<Arc<dyn DatabasePlugin>, DbError> {
        match db_type {
            DatabaseType::MySQL => Ok(Arc::clone(&self.mysql)),
            DatabaseType::PostgreSQL => Ok(Arc::clone(&self.postgresql)),
            DatabaseType::SQLite => Ok(Arc::clone(&self.sqlite)),
            DatabaseType::DuckDB => Ok(Arc::clone(&self.duckdb)),
            DatabaseType::ClickHouse => Ok(Arc::clone(&self.clickhouse)),
            DatabaseType::MSSQL => Ok(Arc::clone(&self.mssql)),
            DatabaseType::Oracle => Ok(Arc::clone(&self.oracle)),
            DatabaseType::External => Ok(Arc::clone(&self.external)),
        }
    }
}

impl Default for DbManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for DbManager {
    fn clone(&self) -> Self {
        Self {
            mysql: Arc::clone(&self.mysql),
            postgresql: Arc::clone(&self.postgresql),
            sqlite: Arc::clone(&self.sqlite),
            duckdb: Arc::clone(&self.duckdb),
            clickhouse: Arc::clone(&self.clickhouse),
            mssql: Arc::clone(&self.mssql),
            oracle: Arc::clone(&self.oracle),
            external: Arc::clone(&self.external),
        }
    }
}

/// Lifecycle state for a connection session in the pool.
///
/// - `Active`: session is either idle in the pool or in use; new `get_session_connection` allowed.
/// - `Closing`: release/close/remove in progress; the inner mutex is reserved for the
///   verifier/closer. `get_session_connection` rejects this state.
/// - `Closed`: terminal — the connection has been disconnected and the session is
///   waiting to be dropped (the Arc may still be referenced by external holders).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionState {
    Active,
    Closing,
    Closed,
}

/// Connection session - represents a single database connection
struct ConnectionSession {
    connection: Box<dyn DbConnection + Send + Sync>,
    last_active: Instant,
    created_at: Instant,
    session_id: String,
    in_use: bool,
    state: SessionState,
}

impl ConnectionSession {
    fn new(connection: Box<dyn DbConnection + Send + Sync>, session_id: String) -> Self {
        let now = Instant::now();
        Self {
            connection,
            last_active: now,
            created_at: now,
            session_id,
            in_use: false,
            state: SessionState::Active,
        }
    }

    fn mark_in_use(&mut self) {
        self.in_use = true;
        self.update_last_active();
    }

    fn release(&mut self) {
        self.in_use = false;
        self.update_last_active();
    }

    fn update_last_active(&mut self) {
        self.last_active = Instant::now();
    }

    fn is_expired(&self, timeout: Duration) -> bool {
        if self.in_use || self.state != SessionState::Active {
            return false;
        }
        self.last_active.elapsed() > timeout
    }

    fn is_lifetime_expired(&self, max_lifetime: Duration) -> bool {
        self.state == SessionState::Active && self.created_at.elapsed() > max_lifetime
    }

    /// Check if current database matches config database
    /// Returns Ok(true) if consistent, Ok(false) if updated config, Err if check failed
    async fn verify_and_sync_database(&mut self) -> Result<bool, DbError> {
        // Skip check for databases that don't support switching
        if !self.connection.supports_database_switch() {
            return Ok(true);
        }

        let config_db = self.connection.config().database.clone();
        let current_db = self.connection.current_database().await?;

        if config_db == current_db {
            Ok(true)
        } else {
            // Database changed, update config
            self.connection.set_config_database(current_db.clone());
            info!(
                "Session {} database changed: {:?} -> {:?}",
                self.session_id, config_db, current_db
            );
            Ok(false)
        }
    }

    async fn close(&mut self) {
        if self.state == SessionState::Closed {
            return;
        }
        if let Err(e) = self.connection.disconnect().await {
            error!("Failed to disconnect session {}: {}", self.session_id, e);
        } else {
            info!("Closed session: {}", self.session_id);
        }
        self.state = SessionState::Closed;
    }
}

/// Connection manager - manages database connections for a client application
pub struct ConnectionManager {
    /// config_id -> list of sessions for that config
    ///
    /// Each session is wrapped in an `Arc<AsyncMutex<…>>` so that DB operations
    /// can hold the per-session lock across `.await` without blocking other
    /// sessions or pool mutations. The outer `RwLock` only protects the map
    /// structure and never spans an `.await`.
    sessions: Arc<RwLock<HashMap<String, Vec<Arc<AsyncMutex<ConnectionSession>>>>>>,
    /// Connection idle timeout (default: 5 minutes)
    idle_timeout: Duration,
    /// Maximum connection lifetime (default: 30 minutes)
    max_lifetime: Duration,
    /// Session counter for generating unique IDs
    session_counter: Arc<tokio::sync::Mutex<u64>>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            idle_timeout: Duration::from_secs(300), // 5 minutes
            max_lifetime: Duration::from_secs(1800), // 30 minutes
            session_counter: Arc::new(tokio::sync::Mutex::new(0)),
        }
    }

    pub fn with_config(idle_timeout: Duration, max_lifetime: Duration) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            idle_timeout,
            max_lifetime,
            session_counter: Arc::new(tokio::sync::Mutex::new(0)),
        }
    }

    /// Generate unique session ID
    async fn generate_session_id(&self, config_id: &str) -> String {
        let mut counter = self.session_counter.lock().await;
        *counter += 1;
        format!("{}:session:{}", config_id, *counter)
    }

    /// Create a new connection session
    pub async fn create_session(
        &self,
        config: DbConnectionConfig,
        db_manager: &DbManager,
    ) -> Result<String, DbError> {
        let config_id = config.id.clone();

        // Try to acquire an existing session and switch database if needed
        if let Some(session_id) = self.try_acquire_session(&config).await? {
            return Ok(session_id);
        }

        let session_id = self.generate_session_id(&config_id).await;

        // Create new connection (slow path: outside any global lock)
        let plugin = db_manager.get_plugin(&config.database_type)?;
        let connection = plugin.create_connection(config.clone()).await?;
        info!(
            "Created new session: {} (database: {:?})",
            session_id, config.database
        );

        // Store session under a brief global write lock
        let mut session = ConnectionSession::new(connection, session_id.clone());
        session.mark_in_use();
        let new_arc = Arc::new(AsyncMutex::new(session));

        let mut sessions = self.sessions.write().await;
        sessions
            .entry(config_id)
            .or_insert_with(Vec::new)
            .push(new_arc);

        Ok(session_id)
    }

    /// Get mutable access to a session's connection.
    ///
    /// Clones the session `Arc` under a read lock, drops the lock, then locks
    /// the per-session mutex. Rejects if the session is not `Active`.
    pub async fn get_session_connection(
        &self,
        session_id: &str,
    ) -> Result<SessionConnectionGuard, DbError> {
        // Phase 1: locate and clone the Arc under a read lock.
        // We use a non-blocking try_lock to peek at session_id/state without
        // holding the inner mutex across the global lock scope. The lock guard
        // is dropped before the Arc is cloned so the returned Arc is not
        // associated with a held lock.
        let arc = {
            let sessions = self.sessions.read().await;
            let mut found: Option<Arc<AsyncMutex<ConnectionSession>>> = None;
            'outer: for list in sessions.values() {
                for arc in list.iter() {
                    if let Ok(guard) = arc.try_lock() {
                        let matches =
                            guard.session_id == session_id && guard.state == SessionState::Active;
                        drop(guard);
                        if matches {
                            found = Some(Arc::clone(arc));
                            break 'outer;
                        }
                    }
                }
            }
            found.ok_or_else(|| {
                DbError::Internal(format!("session not found: {}", session_id))
            })?
        };
        // Phase 2: acquire the inner mutex and confirm state under the lock.
        let guard = arc.lock_owned().await;
        if guard.state != SessionState::Active {
            return Err(DbError::Internal(format!(
                "session not active: {}",
                session_id
            )));
        }
        Ok(SessionConnectionGuard { inner: guard })
    }

    fn db_equals(db1: &DbConnectionConfig, db2: &DbConnectionConfig) -> bool {
        match db1.database_type {
            DatabaseType::Oracle => {
                (db1.sid.is_some() && db1.sid == db2.sid)
                    || (db1.service_name.is_some() && db1.service_name == db2.service_name)
            }
            _ => db1.database.is_some() && db1.database == db2.database,
        }
    }

    /// Try to acquire an existing idle session with matching database.
    ///
    /// Uses a non-async claim under the global write lock (`try_lock` on each
    /// session's inner mutex). The slow ping runs only after the global lock
    /// has been released. On ping failure, the session is re-removed and
    /// closed outside any lock.
    async fn try_acquire_session(
        &self,
        config: &DbConnectionConfig,
    ) -> Result<Option<String>, DbError> {
        // Phase 1: claim a matching idle session under the global write lock.
        let (claimed_arc, claimed_session_id) = {
            let sessions = self.sessions.write().await;
            let mut found: Option<(Arc<AsyncMutex<ConnectionSession>>, String)> = None;

            if let Some(session_list) = sessions.get(&config.id) {
                for arc in session_list.iter() {
                    // Non-blocking inspection: only proceed if we can grab the
                    // inner lock without awaiting.
                    let Ok(guard) = arc.try_lock() else {
                        continue;
                    };
                    let matches = guard.state == SessionState::Active
                        && !guard.in_use
                        && Self::db_equals(guard.connection.config(), config);
                    if matches {
                        found = Some((Arc::clone(arc), guard.session_id.clone()));
                    }
                    drop(guard);
                    if found.is_some() {
                        break;
                    }
                }
            }
            // found already holds clones; global lock is dropped at end of block
            match found {
                Some(pair) => pair,
                None => return Ok(None),
            }
        };
        // Global write lock dropped here.

        // Phase 2: ping outside any global lock. If it fails, remove and close.
        let ping_ok = {
            let mut guard = claimed_arc.lock().await;
            if guard.state != SessionState::Active {
                // Someone else closed/released it concurrently.
                drop(guard);
                return Ok(None);
            }
            match guard.connection.ping().await {
                Ok(()) => {
                    guard.mark_in_use();
                    true
                }
                Err(error) => {
                    warn!(
                        "Discarding stale session {} before reuse: {}",
                        guard.session_id, error
                    );
                    guard.state = SessionState::Closing;
                    false
                }
            }
        };

        if ping_ok {
            debug!(
                "Reusing session: {} (database: {:?})",
                claimed_session_id, config.database
            );
            return Ok(Some(claimed_session_id));
        }

        // Ping failed: remove the now-closing session from the pool and close it.
        {
            let mut sessions = self.sessions.write().await;
            if let Some(list) = sessions.get_mut(&config.id) {
                list.retain(|s| {
                    s.try_lock()
                        .map(|g| g.session_id != claimed_session_id)
                        .unwrap_or(true)
                });
            }
        }
        {
            let mut guard = claimed_arc.lock().await;
            guard.close().await;
        }
        Ok(None)
    }
}

/// Guard that holds the per-session mutex and provides access to its connection.
///
/// Obtained from `ConnectionManager::get_session_connection`. The guard owns
/// the inner `MutexGuard`, so dropping it releases the per-session lock. The
/// global pool lock is never held by this guard.
pub struct SessionConnectionGuard {
    inner: tokio::sync::OwnedMutexGuard<ConnectionSession>,
}

impl SessionConnectionGuard {
    /// Get mutable reference to the connection and update last active time.
    ///
    /// Returns `None` if the underlying session has been concurrently marked
    /// `Closed`. Callers should treat this as a session-loss error and let
    /// the outer release/close path clean up.
    pub fn connection(&mut self) -> Option<&mut (dyn DbConnection + Send + Sync)> {
        if self.inner.state == SessionState::Closed {
            return None;
        }
        self.inner.mark_in_use();
        Some(&mut *self.inner.connection)
    }
}

impl ConnectionManager {
    /// Get session config
    pub async fn get_session_config(&self, session_id: &str) -> Option<DbConnectionConfig> {
        // Phase 1: find Arc under read lock.
        let arc = {
            let sessions = self.sessions.read().await;
            sessions
                .values()
                .find_map(|list| {
                    list.iter()
                        .find(|s| {
                            s.try_lock()
                                .map(|g| g.session_id == session_id)
                                .unwrap_or(false)
                        })
                        .cloned()
                })
        }?;
        // Phase 2: read config under inner lock.
        let guard = arc.lock().await;
        Some(guard.connection.config().clone())
    }

    pub async fn release_session(&self, session_id: &str) -> Result<(), DbError> {
        self.release_session_internal(session_id, true).await
    }

    async fn release_session_for_reuse(&self, session_id: &str) -> Result<(), DbError> {
        self.release_session_internal(session_id, false).await
    }

    async fn release_session_internal(
        &self,
        session_id: &str,
        close_idle_file_connection: bool,
    ) -> Result<(), DbError> {
        // Phase 1: locate the Arc and mark the session as Closing under the
        // global write lock. Closing blocks new get_session_connection callers
        // and prevents try_acquire_session from claiming a stale connection
        // before verify_and_sync_database has run.
        let (arc, config_id, should_close) = {
            let mut sessions = self.sessions.write().await;
            let mut found: Option<(
                Arc<AsyncMutex<ConnectionSession>>,
                String,
                bool,
            )> = None;

            for (config_id, list) in sessions.iter_mut() {
                let Some(pos) = list.iter().position(|s| {
                    s.try_lock()
                        .map(|g| g.session_id == session_id)
                        .unwrap_or(false)
                }) else {
                    continue;
                };
                let arc = Arc::clone(&list[pos]);
                // A second try_lock on the same Arc from the same task is
                // guaranteed to succeed (no reentrancy on tokio Mutex and we
                // already dropped the first peek guard). If it fails the
                // session is being concurrently released/closed — treat as
                // idempotent and let the closer win.
                let mut guard = match arc.try_lock() {
                    Ok(g) => g,
                    Err(_) => return Ok(()),
                };
                if guard.state != SessionState::Active {
                    // Already closing/closed: idempotent release.
                    drop(guard);
                    return Ok(());
                }
                guard.state = SessionState::Closing;
                let needs_close = guard.connection.close_on_release()
                    && close_idle_file_connection;
                drop(guard);
                found = Some((arc, config_id.clone(), needs_close));
                break;
            }
            match found {
                Some(pair) => pair,
                None => {
                    return Err(DbError::Internal(format!(
                        "session not found: {}",
                        session_id
                    )));
                }
            }
        };
        // Global write lock dropped here.

        // Phase 2: run verify_and_sync_database outside any global lock.
        let verify_result = {
            let mut guard = arc.lock().await;
            guard.verify_and_sync_database().await
        };

        // Phase 3: decide close vs reuse, update the map.
        if let Err(e) = verify_result {
            warn!(
                "Session {} database check failed: {}, closing connection",
                session_id, e
            );
            // Remove the now-closing session from the map (if still present)
            // and disconnect it. Other sessions in the same config stay.
            {
                let mut sessions = self.sessions.write().await;
                if let Some(list) = sessions.get_mut(&config_id) {
                    list.retain(|s| {
                        s.try_lock()
                            .map(|g| g.session_id != session_id)
                            .unwrap_or(true)
                    });
                }
            }
            let mut guard = arc.lock().await;
            guard.close().await;
            return Ok(());
        }

        if should_close {
            // Remove from the map and close the connection.
            {
                let mut sessions = self.sessions.write().await;
                if let Some(list) = sessions.get_mut(&config_id) {
                    list.retain(|s| {
                        s.try_lock()
                            .map(|g| g.session_id != session_id)
                            .unwrap_or(true)
                    });
                }
            }
            let mut guard = arc.lock().await;
            guard.close().await;
            return Ok(());
        }

        // Reuse: re-activate and mark idle. If the session was already
        // removed (e.g. by concurrent cleanup), close it instead.
        {
            let sessions = self.sessions.read().await;
            let still_present = sessions
                .get(&config_id)
                .map(|list| {
                    list.iter().any(|s| {
                        s.try_lock()
                            .map(|g| g.session_id == session_id)
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            if !still_present {
                drop(sessions);
                let mut guard = arc.lock().await;
                guard.close().await;
                return Ok(());
            }
        }
        let mut guard = arc.lock().await;
        guard.state = SessionState::Active;
        guard.release();
        debug!("Session {} released", session_id);
        Ok(())
    }

    /// Close a specific session
    pub async fn close_session(&self, session_id: &str) -> Result<(), DbError> {
        // Phase 1: locate the Arc and mark Closing under the global write lock.
        let (arc, config_id) = {
            let mut sessions = self.sessions.write().await;
            let mut found: Option<(Arc<AsyncMutex<ConnectionSession>>, String)> = None;
            for (config_id, list) in sessions.iter_mut() {
                let Some(pos) = list.iter().position(|s| {
                    s.try_lock()
                        .map(|g| g.session_id == session_id)
                        .unwrap_or(false)
                }) else {
                    continue;
                };
                let arc = Arc::clone(&list[pos]);
                if let Ok(mut guard) = arc.try_lock() {
                    if guard.state == SessionState::Closed {
                        drop(guard);
                        return Ok(());
                    }
                    guard.state = SessionState::Closing;
                }
                // Whether or not try_lock succeeded, the session must be
                // removed from the map so no new caller can claim it.
                list.remove(pos);
                found = Some((arc, config_id.clone()));
                break;
            }
            match found {
                Some(pair) => pair,
                None => {
                    return Err(DbError::Internal(format!(
                        "session not found: {}",
                        session_id
                    )));
                }
            }
        };

        // Phase 2: drop the global lock, then close outside.
        let mut guard = arc.lock().await;
        guard.close().await;
        // Cleanup empty config entry.
        {
            let mut sessions = self.sessions.write().await;
            if let Some(list) = sessions.get(&config_id) {
                if list.is_empty() {
                    sessions.remove(&config_id);
                }
            }
        }
        Ok(())
    }

    /// Remove all sessions for a connection config
    pub async fn remove_all_sessions(&self, config_id: &str) {
        // Phase 1: take the whole Vec of Arcs out of the map.
        let arcs: Vec<Arc<AsyncMutex<ConnectionSession>>> = {
            let mut sessions = self.sessions.write().await;
            sessions.remove(config_id).unwrap_or_default()
        };
        if arcs.is_empty() {
            return;
        }
        info!(
            "Closing {} sessions for config: {}",
            arcs.len(),
            config_id
        );
        // Phase 2: mark each as Closing, then close outside the global lock.
        for arc in &arcs {
            if let Ok(mut g) = arc.try_lock() {
                if g.state == SessionState::Active {
                    g.state = SessionState::Closing;
                }
            }
        }
        for arc in arcs {
            let mut guard = arc.lock().await;
            guard.close().await;
        }
    }

    /// Clean up expired sessions
    async fn cleanup_expired_sessions(&self) {
        let idle_timeout = self.idle_timeout;
        let max_lifetime = self.max_lifetime;

        // Phase 1: collect expired Arcs and remove them from the map.
        let to_close: Vec<(String, Arc<AsyncMutex<ConnectionSession>>)> = {
            let mut sessions = self.sessions.write().await;
            let mut collected: Vec<(String, Arc<AsyncMutex<ConnectionSession>>)> = Vec::new();
            for (config_id, list) in sessions.iter_mut() {
                let mut i = 0;
                while i < list.len() {
                    let expired = {
                        if let Ok(guard) = list[i].try_lock() {
                            guard.is_expired(idle_timeout) || guard.is_lifetime_expired(max_lifetime)
                        } else {
                            false
                        }
                    };
                    if expired {
                        let arc = list.remove(i);
                        if let Ok(mut g) = arc.try_lock() {
                            g.state = SessionState::Closing;
                        }
                        collected.push((config_id.clone(), arc));
                    } else {
                        i += 1;
                    }
                }
            }
            sessions.retain(|_, list| !list.is_empty());
            collected
        };

        // Phase 2: close each outside the global lock.
        for (config_id, arc) in to_close {
            let mut guard = arc.lock().await;
            warn!(
                "Closing expired session {} for config {} (in_use: {}, idle: {}s, lifetime: {}s)",
                guard.session_id,
                config_id,
                guard.in_use,
                guard.last_active.elapsed().as_secs(),
                guard.created_at.elapsed().as_secs()
            );
            guard.close().await;
        }

        // 记录当前剩余会话数，便于监控资源占用
        let sessions = self.sessions.read().await;
        let total: usize = sessions.values().map(|l| l.len()).sum();
        let in_use: usize = sessions
            .values()
            .flat_map(|l| l.iter())
            .filter_map(|s| s.try_lock().ok())
            .filter(|g| g.in_use)
            .count();
        tracing::debug!(
            "连接池清理完成：剩余会话 {}（其中使用中 {}）",
            total,
            in_use
        );
    }

    /// Get connection statistics
    pub async fn stats(&self) -> ConnectionStats {
        let sessions = self.sessions.read().await;
        let mut total = 0;
        let mut in_use_count = 0;

        for session_list in sessions.values() {
            for arc in session_list.iter() {
                total += 1;
                if let Ok(guard) = arc.try_lock() {
                    if guard.in_use {
                        in_use_count += 1;
                    }
                }
            }
        }

        ConnectionStats {
            total_sessions: total,
            active_sessions: in_use_count,
            configs_with_sessions: sessions.len(),
        }
    }

    /// List all sessions for a config
    pub async fn list_sessions(&self, config_id: &str) -> Vec<SessionInfo> {
        let sessions = self.sessions.read().await;
        let Some(list) = sessions.get(config_id) else {
            return Vec::new();
        };
        list.iter()
            .map(|s| {
                if let Ok(g) = s.try_lock() {
                    SessionInfo {
                        session_id: g.session_id.clone(),
                        database: g.connection.config().database.clone(),
                        in_use: g.in_use,
                        idle_time: g.last_active.elapsed(),
                        lifetime: g.created_at.elapsed(),
                    }
                } else {
                    SessionInfo {
                        session_id: String::new(),
                        database: None,
                        in_use: true,
                        idle_time: Duration::ZERO,
                        lifetime: Duration::ZERO,
                    }
                }
            })
            .collect()
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ConnectionManager {
    fn clone(&self) -> Self {
        Self {
            sessions: Arc::clone(&self.sessions),
            idle_timeout: self.idle_timeout,
            max_lifetime: self.max_lifetime,
            session_counter: Arc::clone(&self.session_counter),
        }
    }
}

/// Connection statistics
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub total_sessions: usize,
    pub active_sessions: usize,
    pub configs_with_sessions: usize,
}

/// Session information
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub session_id: String,
    pub database: Option<String>,
    pub in_use: bool,
    pub idle_time: Duration,
    pub lifetime: Duration,
}

/// Connection pool compatibility layer
#[derive(Clone)]
pub struct ConnectionPool {
    db_manager: DbManager,
}

impl ConnectionPool {
    pub fn new(db_manager: DbManager) -> Self {
        Self { db_manager }
    }

    pub async fn get_connection(
        &self,
        config: DbConnectionConfig,
        _db_manager: &DbManager,
    ) -> anyhow::Result<Arc<RwLock<Box<dyn DbConnection + Send + Sync>>>> {
        let plugin = self.db_manager.get_plugin(&config.database_type)?;
        let connection = plugin.create_connection(config).await?;
        Ok(Arc::new(RwLock::new(connection)))
    }
}

/// Global database state - stores DbManager and ConnectionManager
///
/// 所有字段均已 Arc 包装，`Clone` 仅增加引用计数，不会复制底层数据。
#[derive(Clone)]
pub struct GlobalDbState {
    pub db_manager: DbManager,
    pub connection_manager: ConnectionManager,
    pub connection_pool: ConnectionPool,
    /// connection_id -> config mapping
    connections: Arc<DashMap<String, DbConnectionConfig>>,
}

impl GlobalDbState {
    pub fn new() -> Self {
        let manager = ConnectionManager::new();
        let db_manager = DbManager::new();

        Self {
            db_manager: db_manager.clone(),
            connection_manager: manager,
            connection_pool: ConnectionPool::new(db_manager),
            connections: Arc::new(DashMap::new()),
        }
    }

    /// Start the cleanup task (should be called after Tokio runtime is available)
    pub fn start_cleanup_task<C>(&self, cx: &mut C)
    where
        C: AppContext,
    {
        let manager = Arc::new(self.connection_manager.clone());
        // 使用 .detach() 保持任务运行，避免 Task 被 drop 时立即 abort
        Tokio::spawn(cx, async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                manager.cleanup_expired_sessions().await;
            }
        })
        .detach();
    }

    /// Internal method for get_config
    pub fn get_config(&self, connection_id: &str) -> Option<DbConnectionConfig> {
        let config_ref = self.connections.get(connection_id);
        if let Some(config) = config_ref {
            return Some(config.value().clone());
        }
        None
    }

    pub fn get_plugin(
        &self,
        database_type: &DatabaseType,
    ) -> Result<Arc<dyn DatabasePlugin>, DbError> {
        self.db_manager.get_plugin(database_type)
    }

    fn wrapper_result(result: Vec<SqlResult>) -> anyhow::Result<SqlResult> {
        match result.into_iter().next() {
            Some(re) => Ok(re),
            None => Err(anyhow::anyhow!("No result returned")),
        }
    }

    pub async fn drop_database(
        &self,
        cx: &mut AsyncApp,
        config_id: String,
        database_name: String,
    ) -> anyhow::Result<SqlResult> {
        let config = self
            .get_config(&config_id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found: {}", config_id))?;
        let plugin = self.get_plugin(&config.database_type)?;
        let sql = plugin.drop_database(&database_name);

        let result = self.execute_with_session(cx, config, sql, None).await?;

        Self::wrapper_result(result)
    }

    /// Drop table
    pub async fn drop_table(
        &self,
        cx: &mut AsyncApp,
        config_id: String,
        database: String,
        schema: Option<String>,
        table_name: String,
    ) -> anyhow::Result<SqlResult> {
        let mut config = self
            .get_config(&config_id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found: {}", config_id))?;
        let plugin = self.get_plugin(&config.database_type)?;
        let sql = plugin.drop_table(&database, schema.as_deref(), &table_name);

        // For non-Oracle databases, modify config.database to switch database
        if config.database_type != DatabaseType::Oracle {
            config.database = Some(database);
        }

        // Pass schema to switch before executing
        let result = self
            .execute_with_session_internal(cx, config, sql, None, schema)
            .await?;

        Self::wrapper_result(result)
    }

    /// Truncate table
    pub async fn truncate_table(
        &self,
        cx: &mut AsyncApp,
        config_id: String,
        database: String,
        table_name: String,
    ) -> anyhow::Result<SqlResult> {
        let config = self
            .get_config(&config_id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found: {}", config_id))?;
        let plugin = self.get_plugin(&config.database_type)?;
        let sql = plugin.truncate_table(&database, &table_name);

        let result = self.execute_with_session(cx, config, sql, None).await?;

        Self::wrapper_result(result)
    }

    /// Rename table
    pub async fn rename_table(
        &self,
        cx: &mut AsyncApp,
        config_id: String,
        database: String,
        old_name: String,
        new_name: String,
    ) -> anyhow::Result<SqlResult> {
        let config = self
            .get_config(&config_id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found: {}", config_id))?;
        let plugin = self.get_plugin(&config.database_type)?;
        let sql = plugin.rename_table(&database, &old_name, &new_name);

        let result = self.execute_with_session(cx, config, sql, None).await?;

        Self::wrapper_result(result)
    }

    /// Drop view
    pub async fn drop_view(
        &self,
        cx: &mut AsyncApp,
        config_id: String,
        database: String,
        view_name: String,
    ) -> anyhow::Result<SqlResult> {
        let config = self
            .get_config(&config_id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found: {}", config_id))?;
        let plugin = self.get_plugin(&config.database_type)?;
        let sql = plugin.drop_view(&database, &view_name);

        let result = self.execute_with_session(cx, config, sql, None).await?;

        Self::wrapper_result(result)
    }

    /// Register a connection configuration
    pub fn register_connection(&mut self, config: DbConnectionConfig) {
        self.connections.insert(config.id.clone(), config);
    }

    pub async fn update_connection(
        &mut self,
        cx: &mut AsyncApp,
        config: DbConnectionConfig,
    ) -> anyhow::Result<()> {
        self.unregister_connection(cx, config.id.clone()).await?;
        self.register_connection(config);
        Ok(())
    }

    /// Unregister a connection configuration
    pub async fn unregister_connection(
        &mut self,
        cx: &mut AsyncApp,
        connection_id: String,
    ) -> anyhow::Result<()> {
        self.connections.remove(&connection_id);
        let clone_self = self.clone();
        // Remove from registry
        Tokio::spawn_result(cx, async move {
            // Close all sessions for this connection
            clone_self
                .connection_manager
                .remove_all_sessions(&connection_id)
                .await;
            Ok(())
        })
        .await
    }

    /// Create a new session for executing queries
    pub async fn create_session(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        database: Option<String>,
    ) -> anyhow::Result<String> {
        let clone_self = self.clone();
        let mut config = self
            .get_config(&connection_id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found: {}", connection_id))?;

        // Override database if specified
        if let Some(db) = database {
            config.database = Some(db);
        }
        Tokio::spawn_result(cx, async move {
            clone_self
                .connection_manager
                .create_session(config, &clone_self.db_manager)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))
        })
        .await
    }

    /// Execute SQL  (simplified - creates session per execution)
    pub async fn execute_single(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        script: String,
        database: Option<String>,
        opts: Option<ExecOptions>,
    ) -> anyhow::Result<SqlResult> {
        let result = self
            .execute_script(cx, connection_id, script, database, None, opts)
            .await?;
        Self::wrapper_result(result)
    }

    /// Execute SQL script (simplified - creates session per execution)
    pub async fn execute_script(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        script: String,
        database: Option<String>,
        schema: Option<String>,
        opts: Option<ExecOptions>,
    ) -> anyhow::Result<Vec<SqlResult>> {
        //  Get config
        let mut config = self
            .get_config(&connection_id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found: {}", connection_id))?;

        // Schema to switch before executing
        let schema_to_switch = schema;

        // For non-Oracle databases, modify config.database to switch database
        if config.database_type != DatabaseType::Oracle {
            if let Some(db) = database {
                config.database = Some(db);
            }
        }

        self.execute_with_session_internal(cx, config, script, opts, schema_to_switch)
            .await
    }

    /// Execute script with existing session (for transaction scenarios)
    pub async fn execute_with_session(
        &self,
        cx: &mut AsyncApp,
        config: DbConnectionConfig,
        script: String,
        opts: Option<ExecOptions>,
    ) -> anyhow::Result<Vec<SqlResult>> {
        self.execute_with_session_internal(cx, config, script, opts, None)
            .await
    }

    async fn execute_with_session_internal(
        &self,
        cx: &mut AsyncApp,
        config: DbConnectionConfig,
        script: String,
        opts: Option<ExecOptions>,
        schema_to_switch: Option<String>,
    ) -> anyhow::Result<Vec<SqlResult>> {
        // 获取缓存实例用于 DDL 失效
        let cache = cx.update(|cx| cx.try_global::<GlobalNodeCache>().cloned());

        let cache_ctx = cx.update(|cx| {
            cx.try_global::<GlobalDbState>()
                .and_then(|state| state.get_config(&config.id))
                .map(|cfg| CacheContext::from_config(&cfg))
        });

        let notifier = cx.update(|cx| cx.try_global::<GlobalConnectionNotifier>().cloned());

        let clone_self = self.clone();
        let config_id = config.id.clone();
        let current_database = config.database.clone().unwrap_or_default();
        let current_schema = schema_to_switch.clone();
        let script_for_ddl = script.clone();

        let result = Tokio::spawn_result(cx, async move {
            // Create session
            let session_id = clone_self
                .connection_manager
                .create_session(config.clone(), &clone_self.db_manager)
                .await?;

            // Execute query on session
            let opts = opts.unwrap_or_default();
            let is_transactional = opts.transactional;

            let plugin = clone_self.get_plugin(&config.database_type)?;

            let mut result = {
                let mut guard = clone_self
                    .connection_manager
                    .get_session_connection(&session_id)
                    .await?;
                let conn = guard
                    .connection()
                    .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;

                // Switch schema before executing
                if let Some(schema) = &schema_to_switch {
                    conn.switch_schema(schema)
                        .await
                        .map_err(|e| anyhow::anyhow!("Failed to switch schema: {}", e))?;
                }

                conn.execute(plugin.as_ref(), &script, opts.clone()).await?
            };

            opts.truncate_results(&mut result);

            // Determine if session should stay open based on script content
            let upper_script = script.to_uppercase();
            let has_begin =
                upper_script.contains("BEGIN") || upper_script.contains("START TRANSACTION");
            let has_commit = upper_script.contains("COMMIT");
            let has_rollback = upper_script.contains("ROLLBACK");

            // Keep session open if: in transactional mode, or has BEGIN without COMMIT/ROLLBACK
            let keep_session = is_transactional || (has_begin && !has_commit && !has_rollback);

            if keep_session {
                // Release but don't close - session can be reused later
                clone_self
                    .connection_manager
                    .release_session_for_reuse(&session_id)
                    .await?;
            } else {
                // Close session completely
                clone_self
                    .connection_manager
                    .close_session(&session_id)
                    .await?;
            }

            Ok(result)
        })
        .await?;

        // 执行成功后，处理 DDL 缓存失效
        if let Some(cache) = cache {
            let ddl_info = Tokio::spawn_result(cx, async move {
                Ok(cache
                    .process_sql_for_invalidation(
                        &config_id,
                        &script_for_ddl,
                        &current_database,
                        current_schema.as_deref(),
                        cache_ctx.as_ref(),
                    )
                    .await)
            })
            .await;

            // 如果检测到 DDL 变更，发射 SchemaChanged 事件
            if let Ok(Some((conn_id, database, schema))) = ddl_info {
                if let Some(notifier) = notifier {
                    cx.update(|cx| {
                        notifier.0.update(cx, |_, cx| {
                            cx.emit(ConnectionDataEvent::SchemaChanged {
                                connection_id: conn_id,
                                database,
                                schema,
                            });
                        });
                    });
                }
            }
        }

        Ok(result)
    }

    /// Execute SQL with streaming progress (supports both script string and file)
    /// Returns a receiver that will receive progress updates for each statement
    /// For file source, the file is read incrementally to avoid loading the entire file into memory
    pub fn execute_streaming(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        source: SqlSource,
        database: Option<String>,
        schema: Option<String>,
        opts: Option<ExecOptions>,
    ) -> anyhow::Result<mpsc::Receiver<StreamingProgress>> {
        let (tx, rx) = mpsc::channel::<StreamingProgress>(100);
        let mut config = self
            .get_config(&connection_id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found: {}", connection_id))?;

        let schema_to_switch = schema;

        if config.database_type != DatabaseType::Oracle {
            if let Some(db) = database {
                config.database = Some(db);
            }
        }

        let mut opts = opts.unwrap_or_default();
        if source.is_file() {
            opts.streaming = true;
        }

        let clone_self = self.clone();
        Tokio::spawn(cx, async move {
            let plugin = match clone_self.get_plugin(&config.database_type) {
                Ok(c) => c,
                Err(_) => return,
            };

            let session_result = clone_self
                .connection_manager
                .create_session(config.clone(), &clone_self.db_manager)
                .await;

            let session_id = match session_result {
                Ok(id) => id,
                Err(e) => {
                    let total_size = source.file_size().unwrap_or(0);
                    let progress = StreamingProgress::with_file_progress(
                        0,
                        SqlResult::Error(SqlErrorInfo {
                            sql: String::new(),
                            message: format!("Failed to create session: {}", e),
                        }),
                        0,
                        total_size,
                    );
                    let _ = tx.send(progress).await;
                    return;
                }
            };

            let exec_result = async {
                let mut guard = clone_self
                    .connection_manager
                    .get_session_connection(&session_id)
                    .await?;
                let conn = guard
                    .connection()
                    .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;

                if let Some(schema) = &schema_to_switch {
                    conn.switch_schema(schema)
                        .await
                        .map_err(|e| anyhow::anyhow!("Failed to switch schema: {}", e))?;
                }

                conn.execute_streaming(plugin.as_ref(), source, opts, tx)
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
                Ok::<_, anyhow::Error>(())
            }
            .await;

            let _ = clone_self
                .connection_manager
                .close_session(&session_id)
                .await;

            if let Err(e) = exec_result {
                error!("Streaming execution error: {}", e);
            }
        })
        .detach();

        Ok(rx)
    }

    pub async fn with_session_connection<R, F>(
        &self,
        cx: &mut AsyncApp,
        config: DbConnectionConfig,
        f: F,
    ) -> anyhow::Result<R>
    where
        R: Send + 'static,
        F: FnOnce(&dyn DatabasePlugin, &mut (dyn DbConnection + Send + Sync)) -> anyhow::Result<R>
            + Send
            + 'static,
    {
        let clone_self = self.clone();
        Tokio::spawn_result(cx, async move {
            let plugin = clone_self.get_plugin(&config.database_type)?;
            let session_id = clone_self
                .connection_manager
                .create_session(config.clone(), &clone_self.db_manager)
                .await?;

            let result = {
                let mut guard = clone_self
                    .connection_manager
                    .get_session_connection(&session_id)
                    .await?;
                let conn = guard
                    .connection()
                    .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
                f(&*plugin, conn)
            };

            clone_self
                .connection_manager
                .close_session(&session_id)
                .await?;

            result
        })
        .await
    }

    /// Get connection statistics
    pub async fn stats(&self, cx: &mut AsyncApp) -> anyhow::Result<ConnectionStats> {
        let clone_self = self.clone();
        Tokio::spawn_result(cx, async move {
            Ok(clone_self.connection_manager.stats().await)
        })
        .await
    }

    /// List all sessions for a connection
    pub async fn list_sessions(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
    ) -> anyhow::Result<Vec<SessionInfo>> {
        let clone_self = self.clone();
        Tokio::spawn_result(cx, async move {
            Ok(clone_self
                .connection_manager
                .list_sessions(&connection_id)
                .await)
        })
        .await
    }

    /// Close a specific session
    pub async fn close_session(&self, cx: &mut AsyncApp, session_id: String) -> anyhow::Result<()> {
        let clone_self = self.clone();
        Tokio::spawn_result(cx, async move {
            clone_self
                .connection_manager
                .close_session(&session_id)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))
        })
        .await
    }

    /// Disconnect all sessions for a connection
    pub async fn disconnect_all(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
    ) -> anyhow::Result<()> {
        let clone_self = self.clone();
        Tokio::spawn_result(cx, async move {
            clone_self
                .connection_manager
                .remove_all_sessions(&connection_id)
                .await;
            Ok(())
        })
        .await
    }

    /// Query table data
    pub async fn query_table_data(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        request: crate::types::TableDataRequest,
    ) -> anyhow::Result<crate::types::TableDataResponse> {
        info!("query_table_data: connection_id={}", connection_id);
        let database = request.database.clone();
        with_plugin_session_db!(self, cx, connection_id, database, |plugin, conn| {
            plugin.query_table_data(&*conn, request).await
        })
    }

    fn cached_children_ready(cached: &DbNode) -> bool {
        cached.children_loaded
    }

    /// Load node children for tree view
    pub async fn load_node_children(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        node: DbNode,
    ) -> anyhow::Result<Vec<DbNode>> {
        // 获取连接配置
        let mut config = self
            .get_config(&connection_id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found: {}", connection_id))?
            .clone();

        // 创建缓存上下文
        let cache_ctx = crate::CacheContext::from_config(&config);

        // 获取缓存实例
        let cache = cx.update(|cx| cx.try_global::<crate::GlobalNodeCache>().cloned());

        // For Database and Schema nodes, we need to connect to the specific database
        // This is especially important for PostgreSQL which doesn't support database switching
        let target_database = node.get_database_name();

        if let Some(db) = target_database {
            config.database = Some(db);
        }

        let clone_self = self.clone();
        let node_clone = node.clone();

        Tokio::spawn_result(cx, async move {
            // 尝试从缓存获取
            if let Some(ref cache) = cache {
                if let Some(cached) = cache.get_node(&cache_ctx, &node_clone.id).await {
                    if Self::cached_children_ready(&cached) {
                        tracing::debug!("Cache hit for node: {}", node_clone.id);
                        return Ok(cached.children);
                    }
                }
            }

            // 缓存未命中，从数据库加载
            tracing::debug!(
                "Cache miss for node: {}, loading from database",
                node_clone.id
            );

            let plugin = clone_self.get_plugin(&config.database_type)?;
            let session_id = clone_self
                .connection_manager
                .create_session(config.clone(), &clone_self.db_manager)
                .await?;

            let result = {
                let mut guard = clone_self
                    .connection_manager
                    .get_session_connection(&session_id)
                    .await?;
                let conn = guard
                    .connection()
                    .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
                plugin
                    .load_node_children(&*conn, &node_clone)
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))
            };

            if let Err(e) = clone_self
                .connection_manager
                .release_session(&session_id)
                .await
            {
                warn!("Failed to release session {}: {}", session_id, e);
            }

            // 如果加载成功，缓存结果
            if let Ok(ref children) = result {
                if let Some(ref cache) = cache {
                    let mut node_with_children = node_clone.clone();
                    node_with_children.children = children.clone();
                    node_with_children.children_loaded = true;

                    cache
                        .cache_node(&cache_ctx, &node_with_children.id, &node_with_children)
                        .await;
                    tracing::debug!(
                        "Cached node: {} with {} children",
                        node_with_children.id,
                        children.len()
                    );
                }
            }

            result
        })
        .await
    }

    /// Apply table changes
    pub async fn apply_table_changes(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        request: crate::types::TableSaveRequest,
    ) -> anyhow::Result<TableSaveResponse> {
        let database = request.database.clone();
        with_plugin_session_db!(self, cx, connection_id, database, |plugin, conn| {
            let mut success_count = 0;
            let mut errors = Vec::new();

            for change in &request.changes {
                let Some(sql) = plugin.build_table_change_sql(&request, change) else {
                    continue;
                };

                match conn
                    .execute(plugin.as_ref(), &sql, ExecOptions::default())
                    .await
                {
                    Ok(results) => {
                        for result in results {
                            match result {
                                SqlResult::Exec(_) => {
                                    success_count += 1;
                                }
                                SqlResult::Error(err) => {
                                    errors.push(err.message);
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(e) => {
                        errors.push(e.to_string());
                    }
                }
            }

            anyhow::Ok(TableSaveResponse {
                success_count,
                errors,
            })
        })
    }

    /// List databases (with caching)
    pub async fn list_databases(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
    ) -> anyhow::Result<Vec<String>> {
        // 获取缓存实例
        let cache = cx.update(|cx| cx.try_global::<GlobalNodeCache>().cloned());

        // 尝试从缓存获取
        if let Some(cache) = cache.clone() {
            let conn_id = connection_id.clone();
            let result = Tokio::spawn_result(cx, async move {
                if let Some(databases) = cache.get_databases(&conn_id).await {
                    tracing::debug!("Cache hit for databases: {}", conn_id);
                    return Ok(databases);
                }
                Err(anyhow::anyhow!("Cache miss"))
            })
            .await;

            if let Ok(databases) = result {
                return Ok(databases);
            }
        }

        // 缓存未命中，从数据库查询
        let conn_id = connection_id.clone();
        let databases = with_plugin_session!(self, cx, connection_id, |plugin, conn| {
            plugin.list_databases(&*conn).await
        })?;

        // 写入缓存
        if let Some(cache) = cache {
            let databases_clone = databases.clone();
            Tokio::spawn(cx, async move {
                cache.cache_databases(&conn_id, databases_clone).await;
                tracing::debug!("Cached databases for: {}", conn_id);
            })
            .detach();
        }

        Ok(databases)
    }

    /// List databases view
    pub async fn list_databases_view(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
    ) -> anyhow::Result<crate::types::ObjectView> {
        with_plugin_session!(self, cx, connection_id, |plugin, conn| {
            plugin.list_databases_view(&*conn).await
        })
    }

    pub fn capabilities(&self, database_type: &DatabaseType) -> DatabaseCapabilities {
        self.db_manager
            .get_plugin(database_type)
            .map(|plugin| plugin.capabilities())
            .unwrap_or_default()
    }

    /// List schemas in a database (with caching)
    pub async fn list_schemas(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        database: String,
    ) -> anyhow::Result<Vec<String>> {
        // 获取缓存实例
        let cache = cx.update(|cx| cx.try_global::<GlobalNodeCache>().cloned());

        // 尝试从缓存获取
        if let Some(cache) = cache.clone() {
            let conn_id = connection_id.clone();
            let db = database.clone();
            let result = Tokio::spawn_result(cx, async move {
                if let Some(schemas) = cache.get_schemas(&conn_id, &db).await {
                    tracing::debug!("Cache hit for schemas: {}:{}", conn_id, db);
                    return Ok(schemas);
                }
                Err(anyhow::anyhow!("Cache miss"))
            })
            .await;

            if let Ok(schemas) = result {
                return Ok(schemas);
            }
        }

        // 缓存未命中，从数据库查询
        let conn_id = connection_id.clone();
        let db = database.clone();
        let schemas =
            with_plugin_session_db!(self, cx, connection_id, database.clone(), |plugin, conn| {
                plugin.list_schemas(&*conn, &database).await
            })?;

        // 写入缓存
        if let Some(cache) = cache {
            let schemas_clone = schemas.clone();
            Tokio::spawn(cx, async move {
                cache.cache_schemas(&conn_id, &db, schemas_clone).await;
                tracing::debug!("Cached schemas for: {}:{}", conn_id, db);
            })
            .detach();
        }

        Ok(schemas)
    }

    /// List tables (with caching)
    pub async fn list_tables(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        database: String,
        schema: Option<String>,
    ) -> anyhow::Result<Vec<crate::types::TableInfo>> {
        // 获取缓存实例
        let cache = cx.update(|cx| cx.try_global::<GlobalNodeCache>().cloned());

        // 尝试从缓存获取
        if let Some(cache) = cache.clone() {
            let conn_id = connection_id.clone();
            let db = database.clone();
            let sch = schema.clone();
            let result = Tokio::spawn_result(cx, async move {
                if let Some(tables) = cache.get_tables(&conn_id, &db, sch.as_deref()).await {
                    tracing::debug!("Cache hit for tables: {}:{}:{:?}", conn_id, db, sch);
                    return Ok(tables);
                }
                Err(anyhow::anyhow!("Cache miss"))
            })
            .await;

            if let Ok(tables) = result {
                return Ok(tables);
            }
        }

        // 缓存未命中，从数据库查询
        let conn_id = connection_id.clone();
        let db = database.clone();
        let sch = schema.clone();
        let tables =
            with_plugin_session_db!(self, cx, connection_id, database.clone(), |plugin, conn| {
                plugin.list_tables(&*conn, &database, schema).await
            })?;

        // 写入缓存
        if let Some(cache) = cache {
            let tables_clone = tables.clone();
            Tokio::spawn(cx, async move {
                cache
                    .cache_tables(&conn_id, &db, sch.as_deref(), tables_clone)
                    .await;
                tracing::debug!("Cached tables for: {}:{}:{:?}", conn_id, db, sch);
            })
            .detach();
        }

        Ok(tables)
    }

    /// List tables view
    pub async fn list_tables_view(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        database: String,
        schema: Option<String>,
    ) -> anyhow::Result<crate::types::ObjectView> {
        with_plugin_session_db!(self, cx, connection_id, database.clone(), |plugin, conn| {
            plugin.list_tables_view(&*conn, &database, schema).await
        })
    }

    /// List columns (with caching)
    pub async fn list_columns(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        database: String,
        schema: Option<String>,
        table: String,
    ) -> anyhow::Result<Vec<crate::types::ColumnInfo>> {
        // 获取缓存实例
        let cache = cx.update(|cx| cx.try_global::<GlobalNodeCache>().cloned());

        // 尝试从缓存获取
        if let Some(cache) = cache.clone() {
            let conn_id = connection_id.clone();
            let db = database.clone();
            let sch = schema.clone();
            let tbl = table.clone();
            let result = Tokio::spawn_result(cx, async move {
                if let Some(columns) = cache.get_columns(&conn_id, &db, sch.as_deref(), &tbl).await
                {
                    tracing::debug!(
                        "Cache hit for columns: {}:{}:{:?}:{}",
                        conn_id,
                        db,
                        sch,
                        tbl
                    );
                    return Ok(columns);
                }
                Err(anyhow::anyhow!("Cache miss"))
            })
            .await;

            if let Ok(columns) = result {
                return Ok(columns);
            }
        }

        // 缓存未命中，从数据库查询
        let conn_id = connection_id.clone();
        let db = database.clone();
        let sch = schema.clone();
        let tbl = table.clone();
        let columns =
            with_plugin_session_db!(self, cx, connection_id, database.clone(), |plugin, conn| {
                plugin.list_columns(&*conn, &database, schema, &table).await
            })?;

        // 写入缓存
        if let Some(cache) = cache {
            let columns_clone = columns.clone();
            Tokio::spawn(cx, async move {
                cache
                    .cache_columns(&conn_id, &db, sch.as_deref(), &tbl, columns_clone)
                    .await;
                tracing::debug!("Cached columns for: {}:{}:{:?}:{}", conn_id, db, sch, tbl);
            })
            .detach();
        }

        Ok(columns)
    }

    /// List columns view
    pub async fn list_columns_view(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        database: String,
        schema: Option<String>,
        table: String,
    ) -> anyhow::Result<crate::types::ObjectView> {
        with_plugin_session_db!(self, cx, connection_id, database.clone(), |plugin, conn| {
            plugin
                .list_columns_view(&*conn, &database, schema, &table)
                .await
        })
    }

    /// List indexes (with caching)
    pub async fn list_indexes(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        database: String,
        schema: Option<String>,
        table: String,
    ) -> anyhow::Result<Vec<crate::types::IndexInfo>> {
        // 获取缓存实例
        let cache = cx.update(|cx| cx.try_global::<GlobalNodeCache>().cloned());

        // 尝试从缓存获取
        if let Some(cache) = cache.clone() {
            let conn_id = connection_id.clone();
            let db = database.clone();
            let sch = schema.clone();
            let tbl = table.clone();
            let result = Tokio::spawn_result(cx, async move {
                if let Some(indexes) = cache.get_indexes(&conn_id, &db, sch.as_deref(), &tbl).await
                {
                    tracing::debug!(
                        "Cache hit for indexes: {}:{}:{:?}:{}",
                        conn_id,
                        db,
                        sch,
                        tbl
                    );
                    return Ok(indexes);
                }
                Err(anyhow::anyhow!("Cache miss"))
            })
            .await;

            if let Ok(indexes) = result {
                return Ok(indexes);
            }
        }

        // 缓存未命中，从数据库查询
        let conn_id = connection_id.clone();
        let db = database.clone();
        let sch = schema.clone();
        let tbl = table.clone();
        let indexes =
            with_plugin_session_db!(self, cx, connection_id, database.clone(), |plugin, conn| {
                plugin.list_indexes(&*conn, &database, schema, &table).await
            })?;

        // 写入缓存
        if let Some(cache) = cache {
            let indexes_clone = indexes.clone();
            Tokio::spawn(cx, async move {
                cache
                    .cache_indexes(&conn_id, &db, sch.as_deref(), &tbl, indexes_clone)
                    .await;
                tracing::debug!("Cached indexes for: {}:{}:{:?}:{}", conn_id, db, sch, tbl);
            })
            .detach();
        }

        Ok(indexes)
    }

    /// List foreign keys (with caching)
    pub async fn list_foreign_keys(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        database: String,
        schema: Option<String>,
        table: String,
    ) -> anyhow::Result<Vec<crate::types::ForeignKeyDefinition>> {
        let cache = cx.update(|cx| cx.try_global::<GlobalNodeCache>().cloned());
        if let Some(cache) = cache.clone() {
            let result = cached_foreign_keys(
                cx,
                cache,
                &connection_id,
                &database,
                schema.as_deref(),
                &table,
            )
            .await;
            if let Ok(foreign_keys) = result {
                return Ok(foreign_keys);
            }
        }

        let conn_id = connection_id.clone();
        let db = database.clone();
        let sch = schema.clone();
        let tbl = table.clone();
        let foreign_keys =
            with_plugin_session_db!(self, cx, connection_id, database.clone(), |plugin, conn| {
                plugin
                    .list_foreign_keys(&*conn, &database, schema, &table)
                    .await
            })?;

        if let Some(cache) = cache {
            let foreign_keys_clone = foreign_keys.clone();
            Tokio::spawn(cx, async move {
                cache
                    .cache_foreign_keys(&conn_id, &db, sch.as_deref(), &tbl, foreign_keys_clone)
                    .await;
            })
            .detach();
        }

        Ok(foreign_keys)
    }

    /// List views
    pub async fn list_views_view(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        database: String,
    ) -> anyhow::Result<crate::types::ObjectView> {
        with_plugin_session_db!(self, cx, connection_id, database.clone(), |plugin, conn| {
            plugin.list_views_view(&*conn, &database).await
        })
    }

    /// List functions view
    pub async fn list_functions_view(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        database: String,
    ) -> anyhow::Result<crate::types::ObjectView> {
        with_plugin_session_db!(self, cx, connection_id, database.clone(), |plugin, conn| {
            plugin.list_functions_view(&*conn, &database).await
        })
    }

    /// List procedures view
    pub async fn list_procedures_view(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        database: String,
    ) -> anyhow::Result<crate::types::ObjectView> {
        with_plugin_session_db!(self, cx, connection_id, database.clone(), |plugin, conn| {
            plugin.list_procedures_view(&*conn, &database).await
        })
    }

    /// List triggers view
    pub async fn list_triggers_view(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        database: String,
    ) -> anyhow::Result<crate::types::ObjectView> {
        with_plugin_session_db!(self, cx, connection_id, database.clone(), |plugin, conn| {
            plugin.list_triggers_view(&*conn, &database).await
        })
    }

    /// List sequences view
    pub async fn list_sequences_view(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        database: String,
    ) -> anyhow::Result<crate::types::ObjectView> {
        with_plugin_session_db!(self, cx, connection_id, database.clone(), |plugin, conn| {
            plugin.list_sequences_view(&*conn, &database).await
        })
    }

    /// List schemas view
    pub async fn list_schemas_view(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        database: String,
    ) -> anyhow::Result<crate::types::ObjectView> {
        with_plugin_session_db!(self, cx, connection_id, database.clone(), |plugin, conn| {
            plugin.list_schemas_view(&*conn, &database).await
        })
    }

    /// Load object view based on node type
    pub async fn load_object_view(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        node: DbNode,
    ) -> anyhow::Result<Option<crate::types::ObjectView>> {
        if node.node_type == DbNodeType::Connection && !node.children_loaded {
            info!(
                "[DB][Timing] load_object_view skipped connection_id={} node_id={} reason=connection_children_not_loaded",
                connection_id, node.id
            );
            return Ok(None);
        }

        let mut config = self
            .get_config(&connection_id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found: {}", connection_id))?
            .clone();

        let target_database = node.get_database_name();
        if let Some(db) = target_database {
            config.database = Some(db);
        }

        let database = config.database.clone().unwrap_or_default();
        let schema = node.get_schema_name();
        let table = node.get_table_name().unwrap_or_default();
        let clone_self = self.clone();
        Tokio::spawn_result(cx, async move {
            let plugin = clone_self.get_plugin(&config.database_type)?;
            let session_id = clone_self
                .connection_manager
                .create_session(config.clone(), &clone_self.db_manager)
                .await?;

            let result = {
                let mut guard = clone_self
                    .connection_manager
                    .get_session_connection(&session_id)
                    .await?;
                let conn = guard
                    .connection()
                    .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
                let view = match node.node_type {
                    DbNodeType::Connection => {
                        if node.children_loaded {
                            if plugin.capabilities().uses_schema_as_database {
                                plugin.list_schemas_view(&*conn, &database).await.ok()
                            } else {
                                plugin.list_databases_view(&*conn).await.ok()
                            }
                        } else {
                            None
                        }
                    }
                    DbNodeType::Database => {
                        if plugin.capabilities().supports_schema {
                            plugin.list_schemas_view(&*conn, &database).await.ok()
                        } else {
                            plugin.list_tables_view(&*conn, &database, None).await.ok()
                        }
                    }
                    DbNodeType::TablesFolder => plugin
                        .list_tables_view(&*conn, &database, schema)
                        .await
                        .ok(),
                    DbNodeType::Schema => plugin
                        .list_tables_view(&*conn, &database, schema)
                        .await
                        .ok(),
                    DbNodeType::Table | DbNodeType::ColumnsFolder => plugin
                        .list_columns_view(&*conn, &database, schema, &table)
                        .await
                        .ok(),
                    DbNodeType::ViewsFolder => plugin.list_views_view(&*conn, &database).await.ok(),
                    DbNodeType::FunctionsFolder => {
                        plugin.list_functions_view(&*conn, &database).await.ok()
                    }
                    DbNodeType::ProceduresFolder => {
                        plugin.list_procedures_view(&*conn, &database).await.ok()
                    }
                    DbNodeType::TriggersFolder => {
                        plugin.list_triggers_view(&*conn, &database).await.ok()
                    }
                    DbNodeType::SequencesFolder => {
                        plugin.list_sequences_view(&*conn, &database).await.ok()
                    }
                    _ => None,
                };
                Ok::<_, anyhow::Error>(view)
            };

            if let Err(e) = clone_self
                .connection_manager
                .release_session(&session_id)
                .await
            {
                warn!("Failed to release session {}: {}", session_id, e);
            }

            result
        })
        .await
    }

    /// Get completion info
    pub fn get_completion_info(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
    ) -> anyhow::Result<crate::plugin::SqlCompletionInfo> {
        let _ = cx;
        if let Some(config) = self.get_config(&connection_id) {
            match self.get_plugin(&config.database_type) {
                Ok(plugin) => Ok(plugin.get_completion_info()),
                Err(_) => Ok(crate::plugin::SqlCompletionInfo::default()),
            }
        } else {
            Ok(crate::plugin::SqlCompletionInfo::default())
        }
    }

    /// Export data
    pub async fn export_data(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        config: ExportConfig,
    ) -> anyhow::Result<ExportResult> {
        self.export_data_with_progress(cx, connection_id, config, None)
            .await
    }

    /// Export data with progress callback
    pub async fn export_data_with_progress(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        config: ExportConfig,
        progress_tx: Option<ExportProgressSender>,
    ) -> anyhow::Result<ExportResult> {
        let db_config = self
            .get_config(&connection_id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found: {}", connection_id))?;

        let clone_self = self.clone();
        Tokio::spawn_result(cx, async move {
            let plugin = clone_self.get_plugin(&db_config.database_type)?;
            let session_id = clone_self
                .connection_manager
                .create_session(db_config.clone(), &clone_self.db_manager)
                .await?;

            let result = {
                let mut guard = clone_self
                    .connection_manager
                    .get_session_connection(&session_id)
                    .await?;
                let conn = guard
                    .connection()
                    .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
                plugin
                    .export_data_with_progress(conn, &config, progress_tx)
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))
            };

            clone_self
                .connection_manager
                .release_session(&session_id)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            result
        })
        .await
    }

    /// Export data with progress callback (sync version for background tasks)
    pub async fn export_data_with_progress_sync(
        &self,
        connection_id: String,
        config: ExportConfig,
        progress_tx: Option<ExportProgressSender>,
    ) -> anyhow::Result<ExportResult> {
        let db_config = self
            .get_config(&connection_id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found: {}", connection_id))?;

        let plugin = self.get_plugin(&db_config.database_type)?;
        let session_id = self
            .connection_manager
            .create_session(db_config.clone(), &self.db_manager)
            .await?;

        let result = {
            let mut guard = self
                .connection_manager
                .get_session_connection(&session_id)
                .await?;
            let conn = guard
                .connection()
                .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
            plugin
                .export_data_with_progress(conn, &config, progress_tx)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))
        };

        self.connection_manager
            .release_session(&session_id)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        result
    }

    /// Import data
    pub async fn import_data(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        config: ImportConfig,
        data: String,
    ) -> anyhow::Result<ImportResult> {
        let db_config = self
            .get_config(&connection_id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found: {}", connection_id))?;

        let clone_self = self.clone();
        Tokio::spawn_result(cx, async move {
            let session_id = clone_self
                .connection_manager
                .create_session(db_config.clone(), &clone_self.db_manager)
                .await?;

            let plugin = clone_self.get_plugin(&db_config.database_type)?;

            let result = {
                let mut guard = clone_self
                    .connection_manager
                    .get_session_connection(&session_id)
                    .await?;
                let conn = guard
                    .connection()
                    .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
                plugin
                    .import_data(&*conn, &config, &data)
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))
            };

            clone_self
                .connection_manager
                .release_session(&session_id)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            result
        })
        .await
    }

    /// Import data with progress callback (sync version for background tasks)
    pub async fn import_data_with_progress_sync(
        &self,
        connection_id: String,
        config: ImportConfig,
        data: String,
        file_name: &str,
        progress_tx: Option<crate::import_export::ImportProgressSender>,
    ) -> anyhow::Result<ImportResult> {
        let db_config = self
            .get_config(&connection_id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found: {}", connection_id))?;

        let plugin = self.get_plugin(&db_config.database_type)?;
        let session_id = self
            .connection_manager
            .create_session(db_config.clone(), &self.db_manager)
            .await?;

        let result = {
            let mut guard = self
                .connection_manager
                .get_session_connection(&session_id)
                .await?;
            let conn = guard
                .connection()
                .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
            plugin
                .import_data_with_progress(conn, &config, &data, file_name, progress_tx)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))
        };

        self.connection_manager
            .release_session(&session_id)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        result
    }

    /// Pure async version of `list_tables` — can be called from any tokio context
    /// without `AsyncApp`. Skips `GlobalNodeCache`.
    pub async fn list_tables_direct(
        &self,
        connection_id: &str,
        database: &str,
        schema: Option<String>,
    ) -> anyhow::Result<Vec<crate::types::TableInfo>> {
        let config = self
            .get_config(connection_id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found: {}", connection_id))?;
        let mut config = config.clone();
        config.database = Some(database.to_string());

        let plugin = self.get_plugin(&config.database_type)?;
        let session_id = self
            .connection_manager
            .create_session(config, &self.db_manager)
            .await?;

        let result = {
            let mut guard = self
                .connection_manager
                .get_session_connection(&session_id)
                .await?;
            let conn = guard
                .connection()
                .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
            plugin
                .list_tables(conn, database, schema)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))
        };

        self.connection_manager
            .release_session(&session_id)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        result
    }

    /// Pure async version of `list_columns` — can be called from any tokio context
    /// without `AsyncApp`. Skips `GlobalNodeCache`.
    pub async fn list_columns_direct(
        &self,
        connection_id: &str,
        database: &str,
        schema: Option<String>,
        table: &str,
    ) -> anyhow::Result<Vec<crate::types::ColumnInfo>> {
        let config = self
            .get_config(connection_id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found: {}", connection_id))?;
        let mut config = config.clone();
        config.database = Some(database.to_string());

        let plugin = self.get_plugin(&config.database_type)?;
        let session_id = self
            .connection_manager
            .create_session(config, &self.db_manager)
            .await?;

        let result = {
            let mut guard = self
                .connection_manager
                .get_session_connection(&session_id)
                .await?;
            let conn = guard
                .connection()
                .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
            plugin
                .list_columns(conn, database, schema, table)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))
        };

        self.connection_manager
            .release_session(&session_id)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        result
    }

    /// Pure async SQL execution version — can be called from any tokio context
    /// without `AsyncApp`. Skips cache invalidation and notifier side effects.
    pub async fn execute_script_direct(
        &self,
        connection_id: &str,
        script: &str,
        database: Option<String>,
        schema: Option<String>,
        opts: Option<ExecOptions>,
    ) -> anyhow::Result<Vec<SqlResult>> {
        let mut config = self
            .get_config(connection_id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found: {}", connection_id))?
            .clone();

        // For non-Oracle databases, switch database through config override.
        if config.database_type != DatabaseType::Oracle {
            if let Some(db) = database {
                config.database = Some(db);
            }
        }

        let plugin = self.get_plugin(&config.database_type)?;
        let session_id = self
            .connection_manager
            .create_session(config, &self.db_manager)
            .await?;

        let opts = opts.unwrap_or_default();
        let result = {
            let mut guard = self
                .connection_manager
                .get_session_connection(&session_id)
                .await?;
            let conn = guard
                .connection()
                .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;

            if let Some(schema) = &schema {
                conn.switch_schema(schema)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to switch schema: {}", e))?;
            }

            conn.execute(plugin.as_ref(), script, opts.clone()).await
        };

        self.connection_manager.close_session(&session_id).await?;
        result
            .map(|mut r| {
                opts.truncate_results(&mut r);
                r
            })
            .map_err(|e| anyhow::anyhow!("{}", e))
    }
}

impl Default for GlobalDbState {
    fn default() -> Self {
        Self::new()
    }
}

impl Global for GlobalDbState {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::DbConnection;
    use crate::executor::{ExecOptions, ExecResult, SqlErrorInfo, SqlSource};
    use async_trait::async_trait;
    use one_core::storage::DatabaseType;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::sync::mpsc;

    struct MockConnection {
        config: DbConnectionConfig,
        healthy: bool,
        disconnect_count: Arc<AtomicUsize>,
    }

    impl MockConnection {
        fn new(config: DbConnectionConfig, healthy: bool) -> Self {
            Self {
                config,
                healthy,
                disconnect_count: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn with_disconnect_count(
            config: DbConnectionConfig,
            healthy: bool,
            disconnect_count: Arc<AtomicUsize>,
        ) -> Self {
            Self {
                config,
                healthy,
                disconnect_count,
            }
        }
    }

    #[async_trait]
    impl DbConnection for MockConnection {
        fn config(&self) -> &DbConnectionConfig {
            &self.config
        }

        fn set_config_database(&mut self, database: Option<String>) {
            self.config.database = database;
        }

        async fn connect(&mut self) -> Result<(), DbError> {
            Ok(())
        }

        async fn disconnect(&mut self) -> Result<(), DbError> {
            self.disconnect_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn execute(
            &self,
            _plugin: &dyn DatabasePlugin,
            _script: &str,
            _options: ExecOptions,
        ) -> Result<Vec<SqlResult>, DbError> {
            Ok(Vec::new())
        }

        async fn query(&self, query: &str) -> Result<SqlResult, DbError> {
            if self.healthy {
                Ok(SqlResult::Exec(ExecResult {
                    sql: query.to_string(),
                    rows_affected: 0,
                    elapsed_ms: 0,
                    message: None,
                }))
            } else {
                Ok(SqlResult::Error(SqlErrorInfo {
                    sql: query.to_string(),
                    message: "connection closed".to_string(),
                }))
            }
        }

        async fn current_database(&self) -> Result<Option<String>, DbError> {
            Ok(self.config.database.clone())
        }

        async fn switch_database(&self, _database: &str) -> Result<(), DbError> {
            Ok(())
        }

        async fn execute_streaming(
            &self,
            _plugin: &dyn DatabasePlugin,
            _source: SqlSource,
            _options: ExecOptions,
            _sender: mpsc::Sender<StreamingProgress>,
        ) -> Result<(), DbError> {
            Ok(())
        }
    }

    fn test_config(id: &str) -> DbConnectionConfig {
        DbConnectionConfig {
            id: id.to_string(),
            database_type: DatabaseType::PostgreSQL,
            name: "test".to_string(),
            host: "localhost".to_string(),
            port: 5432,
            username: "user".to_string(),
            password: "password".to_string(),
            database: Some("postgres".to_string()),
            service_name: None,
            sid: None,
            credential_ref: None,
            ssh_tunnel_credential_ref: None,
            workspace_id: None,
            extra_params: Default::default(),
        }
    }

    #[test]
    fn test_db_manager_registers_duckdb_plugin() {
        let plugin = DbManager::default()
            .get_plugin(&DatabaseType::DuckDB)
            .expect("DuckDB plugin should be registered");

        assert_eq!(plugin.name(), DatabaseType::DuckDB);
    }

    #[test]
    fn test_cached_children_ready_allows_empty_children() {
        let node = DbNode::new(
            "node-id",
            "node",
            DbNodeType::Table,
            "conn-id".to_string(),
            DatabaseType::SQLite,
        )
        .with_children_loaded(true);

        assert!(GlobalDbState::cached_children_ready(&node));
    }

    #[test]
    fn test_cached_children_ready_blocks_unloaded_children() {
        let node = DbNode::new(
            "node-id",
            "node",
            DbNodeType::Table,
            "conn-id".to_string(),
            DatabaseType::SQLite,
        );

        assert!(!GlobalDbState::cached_children_ready(&node));
    }

    #[tokio::test]
    async fn ping_returns_error_for_sql_error_result() {
        let connection = MockConnection::new(test_config("conn1"), false);

        assert!(connection.ping().await.is_err());
    }

    #[tokio::test]
    async fn try_acquire_session_discards_idle_session_when_ping_fails() {
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let config = test_config("conn1");
        let session = ConnectionSession::new(
            Box::new(MockConnection::new(config.clone(), false)),
            "conn1:session:1".to_string(),
        );

        manager
            .sessions
            .write()
            .await
            .entry(config.id.clone())
            .or_default()
            .push(Arc::new(AsyncMutex::new(session)));

        let acquired = manager.try_acquire_session(&config).await.unwrap();
        let remaining = manager.list_sessions(&config.id).await;

        assert!(acquired.is_none());
        assert!(remaining.is_empty());
    }

    #[tokio::test]
    async fn release_session_closes_duckdb_sessions_instead_of_idling() {
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let mut config = test_config("duckdb-conn");
        config.database_type = DatabaseType::DuckDB;
        let disconnect_count = Arc::new(AtomicUsize::new(0));
        let mut session = ConnectionSession::new(
            Box::new(MockConnection::with_disconnect_count(
                config.clone(),
                true,
                Arc::clone(&disconnect_count),
            )),
            "duckdb-conn:session:1".to_string(),
        );
        session.mark_in_use();

        manager
            .sessions
            .write()
            .await
            .entry(config.id.clone())
            .or_default()
            .push(Arc::new(AsyncMutex::new(session)));

        manager
            .release_session("duckdb-conn:session:1")
            .await
            .unwrap();

        assert_eq!(1, disconnect_count.load(Ordering::SeqCst));
        assert!(manager.list_sessions(&config.id).await.is_empty());
    }

    #[tokio::test]
    async fn release_session_for_reuse_keeps_duckdb_transaction_session_idle() {
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let mut config = test_config("duckdb-transaction");
        config.database_type = DatabaseType::DuckDB;
        let disconnect_count = Arc::new(AtomicUsize::new(0));
        let mut session = ConnectionSession::new(
            Box::new(MockConnection::with_disconnect_count(
                config.clone(),
                true,
                Arc::clone(&disconnect_count),
            )),
            "duckdb-transaction:session:1".to_string(),
        );
        session.mark_in_use();

        manager
            .sessions
            .write()
            .await
            .entry(config.id.clone())
            .or_default()
            .push(Arc::new(AsyncMutex::new(session)));

        manager
            .release_session_for_reuse("duckdb-transaction:session:1")
            .await
            .unwrap();

        let sessions = manager.list_sessions(&config.id).await;
        assert_eq!(0, disconnect_count.load(Ordering::SeqCst));
        assert_eq!(1, sessions.len());
        assert!(!sessions[0].in_use);
    }
}
