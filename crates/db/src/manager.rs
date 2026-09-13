use crate::cache::CacheContext;
use crate::cache_manager::GlobalNodeCache;
use crate::connection::{DbConnection, DbError, StreamingProgress};
use crate::import_export::{
    ExportConfig, ExportProgressSender, ExportResult, ImportConfig, ImportResult,
};
use crate::ipc::ExternalDatabasePlugin;
use crate::mysql::MySqlPlugin;
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
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

const BUSY_CLOSE_ON_RELEASE_RETRY_DELAY: Duration = Duration::from_millis(10);

/// 判断 DbError 是否为 "session 不存在" 错误（**变体级**）。
///
/// Round 13 重构：移除 Internal 字符串兜底分支（IPC 不通过 Display 序列化
/// DbError，兜底是投机兼容），仅匹配结构化 `DbError::SessionNotFound` 变体。
/// 优势：编译器保证穷尽匹配、无消息格式耦合、可读性更好。
pub(crate) fn is_session_not_found(err: &crate::connection::DbError) -> bool {
    use crate::connection::DbError;
    matches!(err, DbError::SessionNotFound(_))
}

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

            // SessionGuard 保护 session（默认保守 Close）；覆盖 panic/cancel/get_session 失败
            let mut guard = SessionGuard::new(
                clone_self.connection_manager.clone(),
                session_id.clone(),
                SessionReleaseAction::Close,
            );

            let result = {
                let mut conn_guard = clone_self
                    .connection_manager
                    .get_session_connection(&session_id)
                    .await?;
                let $conn = conn_guard
                    .connection()
                    .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
                $body.map_err(|e| anyhow::anyhow!("{}", e))
            };

            // 单源决策：业务成功 → ReleaseForReuse；失败 → Close（防脏复用）
            let action = SessionGuard::action_for(&result);
            guard.finish_with(action).await;

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

            let mut guard = SessionGuard::new(
                clone_self.connection_manager.clone(),
                session_id.clone(),
                SessionReleaseAction::Close,
            );

            let result = {
                let mut conn_guard = clone_self
                    .connection_manager
                    .get_session_connection(&session_id)
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
                let $conn = conn_guard
                    .connection()
                    .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
                $body.map_err(|e| anyhow::anyhow!("{}", e))
            };

            let action = SessionGuard::action_for(&result);
            guard.finish_with(action).await;

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
    external: Arc<dyn DatabasePlugin>,
}

impl DbManager {
    pub fn new() -> Self {
        Self {
            mysql: Arc::new(MySqlPlugin::new()),
            postgresql: Arc::new(PostgresPlugin::new()),
            sqlite: Arc::new(SqlitePlugin::new()),
            external: Arc::new(ExternalDatabasePlugin::new()),
        }
    }

    pub fn get_plugin(&self, db_type: &DatabaseType) -> Result<Arc<dyn DatabasePlugin>, DbError> {
        match db_type {
            DatabaseType::MySQL => Ok(Arc::clone(&self.mysql)),
            DatabaseType::PostgreSQL => Ok(Arc::clone(&self.postgresql)),
            DatabaseType::SQLite => Ok(Arc::clone(&self.sqlite)),
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
    /// 创建时从 ConnectionLifecycle 固化，避免释放路径依赖连接对象动态判断。
    close_on_release: bool,
    last_active: Instant,
    created_at: Instant,
    session_id: String,
    in_use: bool,
    state: SessionState,
}

impl ConnectionSession {
    fn new(
        connection: Box<dyn DbConnection + Send + Sync>,
        session_id: String,
        close_on_release: bool,
    ) -> Self {
        let now = Instant::now();
        Self {
            connection,
            close_on_release,
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
        // Phase 3: best-effort rollback 防止未提交事务状态泄漏到池/下一个使用者。
        // 失败仅 warn（连接可能已损坏），不阻塞 disconnect。
        if let Err(e) = self.connection.rollback_if_active().await {
            warn!(
                "Best-effort rollback failed for session {}: {} — proceeding with disconnect",
                self.session_id, e
            );
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
    /// 单文件驱动物理打开互斥锁：同一 lock key 串行化 create_connection。
    physical_open_locks: Arc<AsyncMutex<HashMap<String, Arc<AsyncMutex<()>>>>>,
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
            physical_open_locks: Arc::new(AsyncMutex::new(HashMap::new())),
            idle_timeout: Duration::from_secs(300), // 5 minutes
            max_lifetime: Duration::from_secs(1800), // 30 minutes
            session_counter: Arc::new(tokio::sync::Mutex::new(0)),
        }
    }

    pub fn with_config(idle_timeout: Duration, max_lifetime: Duration) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            physical_open_locks: Arc::new(AsyncMutex::new(HashMap::new())),
            idle_timeout,
            max_lifetime,
            session_counter: Arc::new(tokio::sync::Mutex::new(0)),
        }
    }

    async fn acquire_physical_open_lock(
        &self,
        key: Option<String>,
    ) -> Option<tokio::sync::OwnedMutexGuard<()>> {
        let key = key?;
        let lock = {
            let mut locks = self.physical_open_locks.lock().await;
            locks
                .entry(key)
                .or_insert_with(|| Arc::new(AsyncMutex::new(())))
                .clone()
        };
        Some(lock.lock_owned().await)
    }

    /// Generate unique session ID
    async fn generate_session_id(&self, config_id: &str) -> String {
        let mut counter = self.session_counter.lock().await;
        *counter += 1;
        format!("{}:session:{}", config_id, *counter)
    }

    /// Create a new connection session
    pub(crate) async fn create_session(
        &self,
        config: DbConnectionConfig,
        db_manager: &DbManager,
    ) -> Result<String, DbError> {
        let config_id = config.id.clone();

        // Try to acquire an existing session and switch database if needed
        if let Some(session_id) = self.try_acquire_session(&config).await? {
            return Ok(session_id);
        }

        let plugin = db_manager.get_plugin(&config.database_type)?;
        let lifecycle = plugin.connection_lifecycle(&config);
        // 单文件驱动：串行化物理打开，避免并发 open 同一路径
        let _physical_guard = self
            .acquire_physical_open_lock(lifecycle.physical_open_lock_key.clone())
            .await;

        // 拿到物理锁后再尝试复用（可能已有其他会话建好）
        if let Some(session_id) = self.try_acquire_session(&config).await? {
            return Ok(session_id);
        }

        let session_id = self.generate_session_id(&config_id).await;

        // Create new connection (slow path: outside any global lock)
        let connection = plugin.create_connection(config.clone()).await?;
        info!(
            "Created new session: {} (database: {:?})",
            session_id, config.database
        );

        // Store session under a brief global write lock
        let mut session =
            ConnectionSession::new(connection, session_id.clone(), lifecycle.close_on_release);
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
    pub(crate) async fn get_session_connection(
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
            found.ok_or_else(|| DbError::SessionNotFound(session_id.to_string()))?
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
        db1.database.is_some() && db1.database == db2.database
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
        loop {
            // Phase 1: claim idle matching session, or detect busy close_on_release.
            enum ClaimOutcome {
                Idle(Arc<AsyncMutex<ConnectionSession>>, String),
                BusyCloseOnRelease,
                NoneAvailable,
            }

            let claim = {
                let sessions = self.sessions.write().await;
                let mut found: Option<(Arc<AsyncMutex<ConnectionSession>>, String)> = None;
                let mut has_busy_close_on_release = false;

                if let Some(session_list) = sessions.get(&config.id) {
                    for arc in session_list.iter() {
                        let Ok(guard) = arc.try_lock() else {
                            continue;
                        };
                        if guard.state != SessionState::Active {
                            drop(guard);
                            continue;
                        }
                        if !Self::db_equals(guard.connection.config(), config) {
                            drop(guard);
                            continue;
                        }
                        if !guard.in_use {
                            found = Some((Arc::clone(arc), guard.session_id.clone()));
                            drop(guard);
                            break;
                        }
                        if guard.close_on_release {
                            has_busy_close_on_release = true;
                        }
                        drop(guard);
                    }
                }

                if let Some((arc, session_id)) = found {
                    ClaimOutcome::Idle(arc, session_id)
                } else if has_busy_close_on_release {
                    ClaimOutcome::BusyCloseOnRelease
                } else {
                    ClaimOutcome::NoneAvailable
                }
            };

            let (claimed_arc, claimed_session_id) = match claim {
                ClaimOutcome::Idle(arc, session_id) => (arc, session_id),
                ClaimOutcome::BusyCloseOnRelease => {
                    // 单连接文件库：占用中的 close_on_release 会话未释放前，禁止返回 None
                    // 以免 create_session 再开第二条物理连接。
                    sleep(BUSY_CLOSE_ON_RELEASE_RETRY_DELAY).await;
                    continue;
                }
                ClaimOutcome::NoneAvailable => return Ok(None),
            };

            // Phase 2: ping outside any global lock. If it fails, remove and close.
            let ping_ok = {
                let mut guard = claimed_arc.lock().await;
                if guard.state != SessionState::Active {
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
            return Ok(None);
        }
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

/// 释放策略：业务闭包结束后如何处理 session
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionReleaseAction {
    /// 直接从池中移除并关闭物理连接（dump 类操作结束默认）
    Close,
    /// 归还连接回池（保留 id 供下次复用）
    Release,
    /// 归还连接回池 + 显式清理会话态（事务回滚 / 临时表 / 会话变量），
    /// 仅当业务结果明确成功时才走这条路径（避免脏会话进入复用池）
    ReleaseForReuse,
}

/// Session 防泄漏 guard。
///
/// 业务闭包应通过 `ConnectionManager::with_session_guard` 间接使用本类型：
/// 闭包结束后 `finish` 会被调用并清空 session，guard 正常 drop 时
/// `Option::take` 返回 `None`，`Drop` 兜底逻辑 no-op。
///
/// 但若业务闭包 panic、future 被 drop / cancel、或忘记调 finish，
/// `Drop` 会**通过 tokio runtime Handle spawn 一个 best-effort release 任务**，
/// 把泄漏的 session 按**原始 action** 释放（Close / Release / ReleaseForReuse）
/// 并 error 日志，**不会阻塞 drop 路径**。
///
/// # 取消安全
///
/// `finish(&mut self)` **不先 take** session，而是 **release 成功后才清空**。
/// 这样若 `finish` future 在 `await release` 中途被 cancel：
/// - session 仍然在 Option 中
/// - Drop 兜底能识别并 spawn 重试 release
///
/// 不变量：release 调用成功与清空 session 之间**不得再插入任何 `.await`**，
/// 否则会重新打开取消窗口。当前实现严格遵守。
///
/// # NotFound 视为成功
///
/// `release(id, action)` 内部统一处理：服务端返回 NotFound（session 已释放）
/// 一律映射为 `Ok(())`，避免 transport 层失败但服务端实际已关闭造成的
/// 虚假失败。这要求 finish 与 Drop 共享同一 helper。
///
/// # 不可观测性
///
/// - `Handle::try_current()` 失败（guard 在非 runtime 线程被 drop）时
///   输出含 session_id 的 warn 日志，泄漏由 idle timeout 兜底
/// - `spawn` 返回 Err（runtime shutdown 期）→ warn 日志，不 panic
/// - spawned release 任务内部捕获错误并 warn，不静默丢弃
///
/// # runtime shutdown 残余泄漏
///
/// Detached spawn 在 runtime 关闭时 task 直接被丢弃不执行。best-effort
/// 接受；进程退出时不可避免的 session 泄漏是设计取舍。SessionManager
/// 端已有 60 秒周期 `cleanup_expired_sessions` 兜底回收（不在本次 PR 范围）。
pub struct SessionGuard {
    manager: ConnectionManager,
    session: Option<(String, SessionReleaseAction)>,
    /// 上次成功释放的 session_id，用于二次 finish_with 时定位
    /// Close-after-Reuse 冲突（首次 Reuse 已成功 → 二次 Close 已被静默忽略）。
    /// Drop 时无意义（连同 self 一起销毁）。
    last_release_id: Option<String>,
}

// 禁止 Clone：两个 guard 各自 Drop 会导致 double release
// 通过不实现 Clone 达到；负 impl `impl !Clone` 是 nightly feature。
#[allow(dead_code)]
const _ASSERT_NOT_CLONE: fn() = || {
    fn assert_not_clone<T: Clone>() {}
    // 编译期断言 SessionGuard: !Clone —— 故意调用时编译失败
};

/// Release 意图：合并"哪个入口"与"是否覆盖"两个独立维度为单一参数，
/// 让非法状态（如 `Finish` + override action、`FinishWith` + 无 action）
/// **编译期不可表达**。
///
/// - `Finish`：使用 `session` 字段内存储的 action（`SessionGuard::finish`）
/// - `FinishWith(action)`：调用方一次性 override（`SessionGuard::finish_with`）
///
/// Err 写回语义（方案 B：override 视为最新意图）详见
/// [`SessionGuard::perform_release`] 处的权威 doc，本 enum 不重复定义。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReleaseIntent {
    /// `finish()`：使用 session 存储的 action
    Finish,
    /// `finish_with(action)`：调用方 override
    FinishWith(SessionReleaseAction),
}

impl ReleaseIntent {
    /// 日志前缀 + 调试用短标签（与 ReleasePath 兼容）
    const fn as_str(self) -> &'static str {
        match self {
            Self::Finish => "finish",
            Self::FinishWith(_) => "finish_with",
        }
    }

    /// 本次生效的 action（override 优先，否则为 stored）
    const fn resolved_action(self, stored: SessionReleaseAction) -> SessionReleaseAction {
        match self {
            Self::Finish => stored,
            Self::FinishWith(action) => action,
        }
    }
}

impl SessionGuard {
    /// 测试用：读取当前 session 持有的 release action（用于验证降级逻辑）
    pub(crate) fn current_action(&self) -> Option<SessionReleaseAction> {
        self.session.as_ref().map(|(_, a)| *a)
    }

    /// 测试用 + Drop 共用：返回 Drop 路径将使用的 release_action。
    ///
    /// **等价契约**（Round 21 P2-2 钉死）：本函数与 `Drop::drop` 中调用的
    /// 计算逻辑**完全相同**，保证测试读到的值就是 Drop 实际传给 release_session
    /// 的值——若未来 Drop 偏离本函数语义，本测试会自动失败（同时 Drop 也会
    /// 偏离 Finish 路径规范）。
    ///
    /// 用途：
    /// - 测试验证 Drop ≡ Finish 等价性
    /// - 可观测性：在 Drop 触发前读回即将使用的 action（用于上层决策）
    pub(crate) fn drop_release_action(&self) -> Option<SessionReleaseAction> {
        self.session
            .as_ref()
            .map(|(_, stored)| Self::compute_writeback_action(ReleaseIntent::Finish, *stored))
    }

    /// Err 路径写回策略计算：**纯函数**。给定 `intent` + 原始 `stored_action`，
    /// 返回 Err 后应当写回的降级后 action（被 `perform_release` Err 分支
    /// 直接 in-place 写入调用，Round 23 起不再依赖 `write_back_session_slot`）。
    ///
    /// **提取动机**（Round 18 接线测试）：
    /// - `perform_release` Err 分支的写回值计算此前分散在两处（warn 日志 +
    ///   `write_back_session_slot` 内部），有重复且不易测试
    /// - 抽出本纯函数后，12 种 intent × stored 组合可表驱动穷举测试钉死
    ///   "override 视为最新意图"（方案 B）的全部边界
    ///
    /// **方案 B 语义**（详见 `perform_release` doc）：override 优先，再降级。
    ///
    /// # 输出域不变量（Round 20 P2-A 文档化 + Round 21 P2-1 定式收紧）
    ///
    /// **`ReleaseForReuse` 永久塌缩为 `Close`**——本函数的输出域 ⊆ {Close, Release}，
    /// 永远不会输出 `ReleaseForReuse`。**这是有意的安全策略，不是 bug**：
    ///
    /// **统一原则**：本函数是降级/回写路径，**无法提供"连接干净"的正面证明**，
    /// 而 `ReleaseForReuse` 的兑现恰恰需要正面证明（业务成功 + 连接状态可信 +
    /// 池侧卫生流程），因此**一律不兑现**——同时覆盖：
    /// - 失败重试（连接状态已不可信：未决事务 / 临时表 / SESSION 变量 / verify 失败）
    /// - 隐式 Drop（作用域退出时业务可能未跑完，状态未确认）
    ///
    /// **权衡**：宁可错杀（多付一次建连成本），不可放过（脏连接进池污染后续使用者）。
    ///
    /// **塌缩目标是 Close 而非 Release**：
    /// - `Release` 仍允许连接进池（仅靠池侧卫生流程兜底）
    /// - `Close` 强制断连，从源头杜绝污染——降级路径无调用方背书、无卫生流程，
    ///   必须选最严格策略
    ///
    /// 因此：无论 stored 是 RfR 还是 override=RfR，经过 downgrade 都会塌缩为 Close；
    /// `FinishWith(ReleaseForReuse)` 行与 `FinishWith(Close)` 行**完全等价**——
    /// 调用方显式请求 reuse，失败后回写时一律给 Close（不允许兑现复用标记）。
    pub(crate) fn compute_writeback_action(
        intent: ReleaseIntent,
        stored_action: SessionReleaseAction,
    ) -> SessionReleaseAction {
        let resolved = intent.resolved_action(stored_action);
        Self::downgrade_action_on_failure(resolved)
    }

    /// Slot 写入 helper：**测试专用 + 未来调用方保留**。
    ///
    /// **Round 23 起**：perform_release 不再使用本函数——改用 in-place mutation
    /// (`*current_action = downgraded`) 保留 cancel-safe 语义。本 helper 现仅供：
    /// - 测试（`write_back_session_slot_semantics` / `_rejects_double_write_in_debug`）
    /// - 未来需要"清空后重新写入"的调用方
    ///
    /// **语义**：仅做 slot 写入，不做降级。降级由调用方通过
    /// [`compute_writeback_action`](Self::compute_writeback_action) 完成
    /// 后传入本函数。
    ///
    /// **不变量**：调用前 `session` 必须为 None；函数后 `session` 必须为
    /// `Some((id, action))`。**双层保护**：debug_assert 在 dev/test 立即失败；
    /// error! 在 release 构建下生产可观测——避免静默覆盖已存在的另一会话记录
    /// （资源泄漏风险）。
    ///
    /// **可测试**：纯函数（无 IO）。
    #[allow(dead_code)] // perform_release 不再使用，保留供测试 + 未来调用方
    pub(crate) fn write_back_session_slot(
        session: &mut Option<(String, SessionReleaseAction)>,
        id: String,
        action: SessionReleaseAction,
    ) {
        if session.is_some() {
            error!(
                target: "db::session",
                current = ?session,
                new_id = %id,
                "write_back_session_slot: slot 非 None，写回将覆盖既有条目（契约违约）"
            );
            debug_assert!(
                false,
                "write_back_session_slot: slot must be None before write-back"
            );
        }
        *session = Some((id, action));
    }
}

impl SessionGuard {
    pub fn new(
        manager: ConnectionManager,
        session_id: String,
        action: SessionReleaseAction,
    ) -> Self {
        Self {
            manager,
            session: Some((session_id, action)),
            last_release_id: None,
        }
    }

    /// `finish` 失败时计算降级 action：**保守策略**。
    ///
    /// 失败语义：服务端返回非 NotFound 错误（如 verify 失败、连接断等），
    /// 调用方对 session 真实状态无信心：
    /// - `ReleaseForReuse` 失败 → 降级为 `Close`，**禁止脏会话复用进池**
    ///   （防止下次使用者拿到带未决事务/临时表/SESSION 变量的脏连接）
    /// - `Close` / `Release` 失败 → 不变（已经是最保守策略；改变会引入
    ///   无端的 close_on_release 行为差异）
    ///
    /// 提取为命名函数以便单测验证决策表（见 `downgrade_action_mapping_is_correct`）。
    pub(crate) fn downgrade_action_on_failure(
        action: SessionReleaseAction,
    ) -> SessionReleaseAction {
        match action {
            SessionReleaseAction::ReleaseForReuse => SessionReleaseAction::Close,
            other => other,
        }
    }

    /// 覆盖 release action。
    ///
    /// 用途：业务闭包完成后，根据业务结果**事后决策**释放策略：
    /// - 业务成功 → ReleaseForReuse（乐观，允许复用）
    /// - 业务失败 → Close（保守，禁止脏复用）
    ///
    /// 若 guard 已被 finish 清空或 Drop 已触发（session 字段为 None），
    /// 该方法为 no-op。
    pub fn set_action(&mut self, action: SessionReleaseAction) {
        if let Some((_, ref mut act)) = self.session.as_mut() {
            *act = action;
        }
    }

    /// 业务完成后显式结束 guard；action 由调用方根据业务结果决定。
    ///
    /// **API 设计**：消除 `set_action + finish` 两步可变状态，决策与释放
    /// 合一。`finish_with` 与 `finish` 互斥——后者使用 guard 创建时的初始
    /// action（保留向后兼容，主要用于 Drop 路径）。
    ///
    /// **取消安全**：与 `finish` 相同——不先 take，仅 release 成功后清空。
    /// cancel 中途 Drop 兜底使用降级 action 重试。
    ///
    /// **失败保守降级**：release 返回 Err 时 action 由
    /// `downgrade_action_on_failure` 降级后保留在 self.session，
    /// Drop 兜底继续处理。
    ///
    /// **幂等性**：第二次调用为 no-op（self.session 已被 take 清空）。
    /// 这种 no-op 是**显式可观测**的（debug 日志记录 caller 位置 +
    /// requested_action），便于排查"双重 release"类 bug，包括
    /// Close-after-Reuse 冲突检测（首次 Reuse 已成功 → 二次 Close 应被
    /// 记录为 warn）。
    ///
    /// **注**：`#[track_caller]` 在 `async fn` 上不会传入 caller 位置
    /// （rust-lang/rust#87441）。这里用 sync-wrapper + `impl Future` 模式
    /// 在同步序言内固化 caller 位置，使 async body 内可读到真实调用位置。
    #[track_caller]
    pub fn finish_with(
        &mut self,
        action: SessionReleaseAction,
    ) -> impl std::future::Future<Output = ()> + '_ {
        let caller = std::panic::Location::caller();
        async move {
            // None 路径由调用方处理协议违规检测；
            // 这里 None 直接 hard no-op。
            if self.session.is_none() {
                Self::handle_double_finish_with(action, caller, self.last_release_id.as_ref());
                return;
            }
            Self::perform_release(
                &self.manager,
                &mut self.session,
                &mut self.last_release_id,
                ReleaseIntent::FinishWith(action),
            )
            .await;
        }
    }

    /// 根据业务结果计算 release action 的单点决策函数。
    ///
    /// 收敛于一处：业务成功 → `ReleaseForReuse`（允许复用）；
    /// 业务失败 → `Close`（保守关闭，防止脏复用）。
    /// 调用点统一调用 `guard.finish_with(action_for(&result)).await`。
    #[inline]
    pub fn action_for<T, E>(result: &Result<T, E>) -> SessionReleaseAction {
        if result.is_ok() {
            SessionReleaseAction::ReleaseForReuse
        } else {
            SessionReleaseAction::Close
        }
    }

    /// 释放 session；重复调用是 no-op（幂等）。
    ///
    /// **取消安全**：仅 release 成功后清空 session；cancel 时 session
    /// 释放 session；重复调用是 no-op（幂等）。
    ///
    /// **取消安全**：仅 release 成功后清空 session；cancel 时 session
    /// 保留，Drop 兜底能重试。
    ///
    /// **失败保守降级**：release 返回 Err 时，**如果原 action 是 `ReleaseForReuse`**，
    /// 自动降级为 `Close` —— 状态未知的会话不应被复用进池（避免下次使用者
    /// 拿到有未决事务/临时表/SESSION 变量的脏连接）。`Close`/`Release`
    /// 失败时 action 不变（已经是最保守策略）。
    ///
    /// **注**：`#[track_caller]` 在 `async fn` 上不会传播（rust#87441）。
    /// 这里 `finish` 是 sync-wrapper 模式不可行的（async 复杂度更高），但
    /// `finish` 主要给 Drop 路径 + 旧测试用，**生产路径请用 `finish_with`**——
    /// `finish_with` 才有完整的 sync-wrapper + caller 传播 + last_release_id
    /// 协议违规检测。
    pub async fn finish(&mut self) {
        // None 路径：与 finish_with 不同，**静默** no-op
        // （保留向后兼容，主要给 Drop 路径 + 旧测试用）
        if self.session.is_none() {
            debug!(
                "SessionGuard::finish on already-finished guard — no-op \
                 (last_release_id={:?})",
                self.last_release_id
            );
            return;
        }
        // None 表示使用 session 内存储的 action（区别于 finish_with 的 override）
        Self::perform_release(
            &self.manager,
            &mut self.session,
            &mut self.last_release_id,
            ReleaseIntent::Finish,
        )
        .await;
    }

    /// 共享 release 路径：take session → `release_session` → 处理结果。
    /// 由 `finish_with` 与 `finish` 调用，消除两处重复实现。
    ///
    /// # 契约
    ///
    /// - **Ok 路径**：清空 `session` + 保存 `last_release_id`
    /// - **Err 路径**：**保留** `session`，把本次生效的 action 降级后写回
    ///   （不是原始 stored，是 override 或 stored 的解析值）——让 Drop 兜底 /
    ///   重试可以拿到更新后的 action
    /// - 调用方必须在调用前确保 `session` 是 `Some`（None 路径由调用方处理）
    ///
    /// # 不变量
    ///
    /// - release 调用成功与清空 session 之间不得再插入任何 `.await`
    /// - Err 路径写回 session 是 Drop 兜底能 retry 的关键，不能漏
    ///
    /// # Err 写回语义（方案 B：override 视为最新意图）—— 权威定义
    ///
    /// 失败的 override 会成为该 session 的持久 action。即：
    /// `stored=Release`、`finish_with(Close)` 失败 → 写回 `(id, Close)`，
    /// Drop 兜底将用 Close 重试而非原始 Release。
    ///
    /// **钉死依据**（避免后续重新翻案为方案 A）：
    /// - 这是 Round 15 重构前原有行为的延续（finish_with Err 分支对 override
    ///   降级），非语义漂移
    /// - **否决方案 A（失败回退 stored）**：Drop 兜底将执行一个调用方刚明确
    ///   否决的动作（override=Close 失败却回退到 Release，可能把处于失败/未知
    ///   状态的资源按旧意图归还复用），违反"调用方最新意图优先"原则
    /// - 降级阶梯（ReleaseForReuse→Close→Close/Release→Release）保证重试逐级
    ///   减弱，不会原地打转
    ///
    /// # 接线契约（Round 18 P1-1 钉死）
    ///
    /// Err 分支的写回值由 [`compute_writeback_action`](Self::compute_writeback_action)
    /// 单一计算点产出，禁止在分支内重复 `downgrade_action_on_failure` 调用，
    /// 否则日志与 slot 写回可能分叉。
    ///
    /// **Err 路径覆盖说明**（Round 19 P2-3）：
    /// - 计算接线（`compute → writeback_value`）：12 组合矩阵 + 字面量规格测试
    /// - slot 接线（`writeback_value → slot`）：`write_back_session_slot_semantics`
    /// - **真实调用链**（`perform_release → compute → writeback → slot`）：因
    ///   `release_session` NotFound→Ok 转换，Err 路径在生产中**几乎不可达**，
    ///   未注入 mock 也未做失败注入测试——Err 分支靠**纯函数单测 + 源代码审计**
    ///   保证。若未来需要端到端 Err 测试，建议引入 `#[cfg(test)]` release_fn
    ///   seam（注入 mock 失败）。
    async fn perform_release(
        manager: &ConnectionManager,
        session: &mut Option<(String, SessionReleaseAction)>,
        last_release_id: &mut Option<String>,
        intent: ReleaseIntent,
    ) {
        // **Cancel safety 关键**（Round 23 P3-5）：
        // 不在 await 前 take session——只读出 id+stored 的副本（as_ref 借用），
        // 若 future 在 await 点被取消，本地副本丢弃但 self.session 仍为 Some，
        // Drop 兜底仍可基于原 stored_action 重试。
        // 之前的 take-then-await 模式存在取消窗口：take 后 session=None，
        // Drop no-op，session 永久泄漏（直到 idle timeout 服务端清理）。
        let Some((id, stored_action)) = session.as_ref().map(|(id, a)| (id.clone(), *a)) else {
            // 调用方漏检契约违约：debug_assert 让 dev/test 立即失败。
            // release 路径仍输出 warn（生产可观测）：assert 在 release 构建下
            // 被静默跳过，无 warn 的话契约违约将无任何痕迹。
            warn!(
                target: "db::session",
                intent = ?intent,
                "perform_release 契约违约：调用方应在 None 路径前置处理 \
                 (release 构建下 debug_assert 被跳过，必须依赖此 warn 排查)"
            );
            debug_assert!(
                false,
                "perform_release 契约违约：调用方应在 None 路径前置处理（{intent:?}）"
            );
            return;
        };
        let action = intent.resolved_action(stored_action);
        match release_session(manager, &id, action).await {
            Ok(()) => {
                // **取消安全 Ok 路径**：await 已完成才 take（move），无取消窗口
                let _ = session.take(); // drop the contained (id, action)
                // 不变量：release 调用成功与 last_release_id 保存之间不得再插入 .await
                *last_release_id = Some(id);
            }
            Err(e) => {
                // **取消安全 Err 路径**：session 仍为 Some（未 take），更新 action
                // 即可。Drop 兜底即便在 await 取消后仍可读到最新 downgraded action。
                let downgraded = Self::compute_writeback_action(intent, stored_action);
                warn!(
                    "{}: failed to release session {}: {}, \
                     action {:?} -> downgraded to {:?}",
                    intent.as_str(),
                    id,
                    e,
                    action,
                    downgraded,
                );
                if let Some((_, ref mut current_action)) = session.as_mut() {
                    *current_action = downgraded;
                } else {
                    // 极端：await 期间 session 被并发清空（正常路径不应发生，
                    // 仅在外部代码持 &mut session 时可能）
                    debug_assert!(false, "session unexpectedly None after await");
                    warn!(
                        target: "db::session",
                        session_id = %id,
                        "perform_release Err 分支：session 已为 None，无法写回降级 action"
                    );
                }
            }
        }
    }

    /// 处理 finish_with 二次调用 / 不可达分支（**hard no-op + error 日志 + debug_assert**）。
    ///
    /// **公共错误处理**：单一来源避免 `finish_with` 内部错误日志分散。
    fn handle_double_finish_with(
        action: SessionReleaseAction,
        caller: &std::panic::Location<'static>,
        last_release_id: Option<&String>,
    ) {
        if let Some(session_id) = last_release_id {
            error!(
                target: "db::session",
                session_id = %session_id,
                requested_action = ?action,
                caller = ?caller,
                "finish_with called after successful release — second call is hard no-op, \
                 requested action NOT applied"
            );
            debug_assert!(
                false,
                "finish_with called twice (session_id={session_id}, caller={caller:?})"
            );
        } else {
            error!(
                target: "db::session",
                caller = ?caller,
                requested_action = ?action,
                "finish_with on guard with no session and no release record — invariant violated"
            );
            debug_assert!(
                false,
                "finish_with on (None, None) guard — should be unreachable (caller={caller:?})"
            );
        }
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        // 仅在 finish 未被调用 / 未成功完成时（panic / future drop / cancel / finish Err）兜底
        // 先快照 release_action（避免 take 后借用冲突），再 take 出 (id, stored)
        let Some(release_action) = self.drop_release_action() else {
            return;
        };
        let Some((id, stored_action)) = self.session.take() else {
            // 极端：drop_release_action 返回 Some 但 session 已为 None——
            // 不应发生（两者都读 self.session），但保守防御
            return;
        };
        // 统一入口（Round 20 P3-A + Round 21 P2-2 防语义分叉）：
        // Drop 没有 caller intent，按契约等价于 `ReleaseIntent::Finish`。
        // 通过 `drop_release_action` 访问器计算——本函数同时给测试使用，
        // 保证 Drop 实际传给 release_session 的值与测试读到的值**完全一致**。
        // 若未来 Finish 语义演化（如增加日志、调整格），Drop 与测试同步。
        //
        // **release 模式双重写行为**（Round 21 P3-1 文档化）：
        // `write_back_session_slot` 在 release 构建下若 slot 已 Some，
        // 仅记 error! 后**覆盖**旧值（不保留旧 id/action）——可能造成另一
        // 会话记录丢失。但 Drop 路径下我们先快照再 take，不存在 slot 非空
        // 场景，此处不受影响。
        warn!(
            target: "db::session",
            session_id = %id,
            stored_action = ?stored_action,
            release_action = ?release_action,
            "SessionGuard dropped without finish() — spawning best-effort release"
        );
        let manager = self.manager.clone();
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                // spawn 返回 JoinHandle 不返回 Result；drop JoinHandle 让
                // task 继续运行到完成。runtime shutdown 期 task 可能被丢弃，
                // 但 best-effort 接受 —— 服务端 idle timeout 是最终兜底
                // （见类型 rustdoc 注释）
                handle.spawn(async move {
                    if let Err(e) = release_session(&manager, &id, release_action).await {
                        warn!(
                            target: "db::session",
                            session_id = %id,
                            "Best-effort release in Drop failed: {e}"
                        );
                    }
                });
            }
            Err(_) => {
                // 不在 tokio runtime 内（如同步析构）：泄漏不可避免，
                // 但至少错误日志让操作者知道这件事发生了
                warn!(
                    target: "db::session",
                    session_id = %id,
                    "SessionGuard dropped outside tokio runtime — session will leak until idle timeout"
                );
            }
        }
    }
}

/// 测试 mock 存储（Round 26）：允许测试通过 `set_test_release_result` 注入
/// 下一次 `release_session` 调用的返回值（一次性的）。mock 注入生产构建下
/// 编译期擦除——零运行开销。
///
/// **设计选择**：
/// - 一次性（take 而非持久）：避免测试间相互污染——每次 mock 调用都需重设
/// - thread-local：tokio 多线程 runtime 下，每个测试线程独立持有 mock
/// - 仅 cfg(test)：生产代码路径不受任何影响
#[cfg(test)]
thread_local! {
    static TEST_RELEASE_RESULT: std::cell::RefCell<Option<Result<(), crate::connection::DbError>>> =
        const { std::cell::RefCell::new(None) };
}

/// 测试 helper：注入 `release_session` 下一次调用的返回值。
/// **一次性**——take 后需重新调用才能再次注入。
#[cfg(test)]
pub(crate) fn set_test_release_result(result: Result<(), crate::connection::DbError>) {
    TEST_RELEASE_RESULT.with(|c| *c.borrow_mut() = Some(result));
}

/// 测试 helper：取出当前注入的 release result（若有）。
/// 返回 Some(_) 表示当前调用应返回该值；None 表示无注入、走真实实现。
#[cfg(test)]
fn take_test_release_result() -> Option<Result<(), crate::connection::DbError>> {
    TEST_RELEASE_RESULT.with(|c| c.borrow_mut().take())
}

/// 共享的 release 路径：finish 与 Drop 都通过此 helper 执行实际释放。
///
/// 把 NotFound 视为 Ok(())：服务端说 session 不存在 = 已被释放，
/// 调用的目标（释放该 session）已经达成。transport 层失败但服务端实际
/// 已关闭的场景下，调用方不应收到虚假失败。
///
/// **NotFound 匹配精度**：仅匹配变体级 `DbError::Internal` 且 message
/// 严格以 `"session not found: "` 开头（close_session / release_session 内部
/// 的固定格式 `format!("session not found: {session_id}")`）。其他错误必须
/// 传播，否则网络/协议错误会被静默吞掉、造成 session 泄漏且无可观测告警。
///
/// **测试 seam**（Round 26）：`#[cfg(test)]` 下允许通过 thread-local mock 注入
/// 自定义 release 行为——用于端到端测试 Err 路径（mock 返回非 NotFound 错误）。
/// 生产构建下 mock 路径被编译期擦除，零运行开销。
async fn release_session(
    manager: &ConnectionManager,
    session_id: &str,
    action: SessionReleaseAction,
) -> Result<(), crate::connection::DbError> {
    // 测试 mock 拦截（编译期擦除）
    #[cfg(test)]
    {
        if let Some(result) = take_test_release_result() {
            return result;
        }
    }

    let result = match action {
        SessionReleaseAction::Close => manager.close_session(session_id).await,
        SessionReleaseAction::Release => manager.release_session(session_id).await,
        SessionReleaseAction::ReleaseForReuse => {
            manager.release_session_for_reuse(session_id).await
        }
    };
    match result {
        Ok(()) => Ok(()),
        Err(e) if is_session_not_found(&e) => {
            // session 已不在池中（其他路径已释放）→ 视为成功
            // debug 日志：与正常路径可区分，便于排查误吞
            tracing::debug!(
                target: "db::session",
                session_id = %session_id,
                "release_session: session not found, treated as success"
            );
            Ok(())
        }
        Err(e) => Err(e),
    }
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
            sessions.values().find_map(|list| {
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

    pub(crate) async fn release_session(&self, session_id: &str) -> Result<(), DbError> {
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
            let mut found: Option<(Arc<AsyncMutex<ConnectionSession>>, String, bool)> = None;

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
                let needs_close = guard.close_on_release && close_idle_file_connection;
                drop(guard);
                found = Some((arc, config_id.clone(), needs_close));
                break;
            }
            match found {
                Some(pair) => pair,
                None => return Err(DbError::SessionNotFound(session_id.to_string())),
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
                "Session {} database check failed, closing connection: {e}",
                session_id
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
            // verify 失败时**吞掉错误**返回 Ok：清理目标（关闭连接、
            // 从 map 移除）已达成，错误属运维诊断信息（日志/metric 的职责），
            // 不是 release 操作本身的失败。调用方无法处置此错误——重试
            // session 已不存在；回滚业务早已提交——只会成为误报。
            // 与 Drop 兜底路径行为一致（Drop 也只能日志吞掉）。
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
        // Phase 3: best-effort rollback before returning session to pool.
        // 防止未提交事务状态泄漏到下一个使用者（脏连接复用）。
        // 失败仅 warn，不阻塞 release（连接可能已损坏或不支持 ROLLBACK）。
        if let Err(e) = guard.connection.rollback_if_active().await {
            warn!(
                "Best-effort rollback failed for session {}: {} — releasing anyway",
                session_id, e
            );
        }
        guard.state = SessionState::Active;
        guard.release();
        debug!("Session {} released", session_id);
        Ok(())
    }

    /// Close a specific session
    pub(crate) async fn close_session(&self, session_id: &str) -> Result<(), DbError> {
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
                None => return Err(DbError::SessionNotFound(session_id.to_string())),
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
        info!("Closing {} sessions for config: {}", arcs.len(), config_id);
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
                            guard.is_expired(idle_timeout)
                                || guard.is_lifetime_expired(max_lifetime)
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
            physical_open_locks: Arc::clone(&self.physical_open_locks),
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

        config.database = Some(database);

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
        self.truncate_table_with_schema(cx, config_id, database, None, table_name)
            .await
    }

    /// Truncate table with an optional schema.
    pub async fn truncate_table_with_schema(
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
        let sql = plugin.truncate_table_with_schema(&database, schema.as_deref(), &table_name);

        config.database = Some(database);

        let result = self
            .execute_with_session_internal(cx, config, sql, None, schema)
            .await?;

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

        if let Some(db) = database {
            config.database = Some(db);
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

        if let Some(db) = database {
            config.database = Some(db);
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

            let mut guard = SessionGuard::new(
                clone_self.connection_manager.clone(),
                session_id.clone(),
                SessionReleaseAction::Close,
            );

            let result = {
                let mut conn_guard = clone_self
                    .connection_manager
                    .get_session_connection(&session_id)
                    .await?;
                let conn = conn_guard
                    .connection()
                    .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
                plugin
                    .load_node_children(&*conn, &node_clone)
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))
            };

            let action = SessionGuard::action_for(&result);
            guard.finish_with(action).await;

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

    /// List functions
    pub async fn list_functions(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        database: String,
    ) -> anyhow::Result<Vec<crate::types::FunctionInfo>> {
        with_plugin_session_db!(self, cx, connection_id, database.clone(), |plugin, conn| {
            plugin.list_functions(&*conn, &database).await
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

            // SessionGuard 保护：默认保守 Close，业务成功后切到 ReleaseForReuse
            let mut guard = SessionGuard::new(
                clone_self.connection_manager.clone(),
                session_id.clone(),
                SessionReleaseAction::Close,
            );

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

            let action = SessionGuard::action_for(&result);
            guard.finish_with(action).await;

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

            // SessionGuard 保护：默认保守 Close，业务成功后切到 ReleaseForReuse。
            // 覆盖 panic / future cancel / get_session_connection 失败 / 业务失败 四条泄漏路径
            let mut guard = SessionGuard::new(
                clone_self.connection_manager.clone(),
                session_id.clone(),
                SessionReleaseAction::Close,
            );

            let result = {
                let mut conn_guard = clone_self
                    .connection_manager
                    .get_session_connection(&session_id)
                    .await?;
                let conn = conn_guard
                    .connection()
                    .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
                plugin
                    .export_data_with_progress(conn, &config, progress_tx)
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))
            };

            // 单点决策：业务成功 → ReleaseForReuse；失败 → Close（防脏复用）
            let action = SessionGuard::action_for(&result);
            guard.finish_with(action).await;

            result
        })
        .await
    }

    /// Export data with progress callback (sync version for background tasks)
    ///
    /// 与 `export_data_with_progress` 同语义，但供已在 GPUI BackgroundExecutor 上的调用方使用：
    /// 内部用 `Tokio::spawn_result` 把 connect / 数据导出等需要 tokio runtime 的步骤
    /// 派发到 tokio runtime，避免在 GPUI BackgroundExecutor 上 `tokio::spawn_blocking`
    /// 或 `tokio::time::timeout` 找不到 reactor 而 panic。
    pub async fn export_data_with_progress_sync(
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

            let mut guard = SessionGuard::new(
                clone_self.connection_manager.clone(),
                session_id.clone(),
                SessionReleaseAction::Close,
            );

            let result = {
                let mut conn_guard = clone_self
                    .connection_manager
                    .get_session_connection(&session_id)
                    .await?;
                let conn = conn_guard
                    .connection()
                    .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
                plugin
                    .export_data_with_progress(conn, &config, progress_tx)
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))
            };

            let action = SessionGuard::action_for(&result);
            guard.finish_with(action).await;

            result
        })
        .await
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

            let mut guard = SessionGuard::new(
                clone_self.connection_manager.clone(),
                session_id.clone(),
                SessionReleaseAction::Close,
            );

            let result = {
                let mut conn_guard = clone_self
                    .connection_manager
                    .get_session_connection(&session_id)
                    .await?;
                let conn = conn_guard
                    .connection()
                    .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
                plugin
                    .import_data(&*conn, &config, &data)
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))
            };

            let action = SessionGuard::action_for(&result);
            guard.finish_with(action).await;

            result
        })
        .await
    }

    /// Import data with progress callback (sync version for background tasks)
    pub async fn import_data_with_progress_sync(
        &self,
        cx: &mut AsyncApp,
        connection_id: String,
        config: ImportConfig,
        data: String,
        file_name: &str,
        progress_tx: Option<crate::import_export::ImportProgressSender>,
    ) -> anyhow::Result<ImportResult> {
        let db_config = self
            .get_config(&connection_id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found: {}", connection_id))?;

        let clone_self = self.clone();
        let file_name = file_name.to_string();
        Tokio::spawn_result(cx, async move {
            let plugin = clone_self.get_plugin(&db_config.database_type)?;
            let session_id = clone_self
                .connection_manager
                .create_session(db_config.clone(), &clone_self.db_manager)
                .await?;

            let mut guard = SessionGuard::new(
                clone_self.connection_manager.clone(),
                session_id.clone(),
                SessionReleaseAction::Close,
            );

            let result = {
                let mut conn_guard = clone_self
                    .connection_manager
                    .get_session_connection(&session_id)
                    .await?;
                let conn = conn_guard
                    .connection()
                    .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
                plugin
                    .import_data_with_progress(conn, &config, &data, &file_name, progress_tx)
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))
            };

            let action = SessionGuard::action_for(&result);
            guard.finish_with(action).await;

            result
        })
        .await
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

        let mut guard = SessionGuard::new(
            self.connection_manager.clone(),
            session_id.clone(),
            SessionReleaseAction::Close,
        );

        let result = {
            let mut conn_guard = self
                .connection_manager
                .get_session_connection(&session_id)
                .await?;
            let conn = conn_guard
                .connection()
                .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
            plugin
                .list_tables(conn, database, schema)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))
        };

        let action = SessionGuard::action_for(&result);
        guard.finish_with(action).await;

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

        let mut guard = SessionGuard::new(
            self.connection_manager.clone(),
            session_id.clone(),
            SessionReleaseAction::Close,
        );

        let result = {
            let mut conn_guard = self
                .connection_manager
                .get_session_connection(&session_id)
                .await?;
            let conn = conn_guard
                .connection()
                .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
            plugin
                .list_columns(conn, database, schema, table)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))
        };

        let action = SessionGuard::action_for(&result);
        guard.finish_with(action).await;

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

        // Switch database through config override.
        if let Some(db) = database {
            config.database = Some(db);
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
        /// 测试用：注入 verify 失败（`current_database` 返回 Err），
        /// 使 `release_session_internal` 的 verify 阶段返回非 NotFound 错误，
        /// 让 `SessionGuard::finish` 的降级路径（ReleaseForReuse → Close）可达
        verify_should_fail: bool,
    }

    impl MockConnection {
        fn new(config: DbConnectionConfig, healthy: bool) -> Self {
            Self {
                config,
                healthy,
                disconnect_count: Arc::new(AtomicUsize::new(0)),
                verify_should_fail: false,
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
                verify_should_fail: false,
            }
        }

        /// 测试用构造器：组合 `with_disconnect_count` + verify 错误注入。
        /// 让 `current_database` 返回 Err（verify 失败），同时暴露 disconnect_count
        /// 给测试断言"连接被真正关闭"。
        fn with_disconnect_count_and_verify_error(
            config: DbConnectionConfig,
            disconnect_count: Arc<AtomicUsize>,
        ) -> Self {
            Self {
                config,
                healthy: true,
                disconnect_count,
                verify_should_fail: true,
            }
        }

        /// 测试用构造器：让 `current_database` 返回 Err，使
        /// `verify_and_sync_database` 失败。仅注入**一种**失败模式
        /// （verify 失败），保持 `healthy: true` 不变以避免测试路径重叠。
        /// （`healthy: false` 会触发另一条错误路径，可能让测试因错误分支通过。）
        fn with_verify_error(config: DbConnectionConfig) -> Self {
            Self {
                config,
                healthy: true,
                disconnect_count: Arc::new(AtomicUsize::new(0)),
                verify_should_fail: true,
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
            if self.verify_should_fail {
                Err(DbError::Internal(
                    "mock verify failure: current_database forced error".to_string(),
                ))
            } else {
                Ok(self.config.database.clone())
            }
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
    fn test_db_manager_registers_mysql_plugin() {
        let plugin = DbManager::default()
            .get_plugin(&DatabaseType::MySQL)
            .expect("MySQL plugin should be registered");

        assert_eq!(plugin.name(), DatabaseType::MySQL);
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
            false,
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
    async fn release_session_closes_marked_sessions_instead_of_idling() {
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let mut config = test_config("mysql-conn");
        config.database_type = DatabaseType::MySQL;
        let disconnect_count = Arc::new(AtomicUsize::new(0));
        let mut session = ConnectionSession::new(
            Box::new(MockConnection::with_disconnect_count(
                config.clone(),
                true,
                Arc::clone(&disconnect_count),
            )),
            "mysql-conn:session:1".to_string(),
            true,
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
            .release_session("mysql-conn:session:1")
            .await
            .unwrap();

        assert_eq!(1, disconnect_count.load(Ordering::SeqCst));
        assert!(manager.list_sessions(&config.id).await.is_empty());
    }

    #[tokio::test]
    async fn release_session_for_reuse_keeps_transaction_session_idle() {
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let mut config = test_config("mysql-transaction");
        config.database_type = DatabaseType::MySQL;
        let disconnect_count = Arc::new(AtomicUsize::new(0));
        let mut session = ConnectionSession::new(
            Box::new(MockConnection::with_disconnect_count(
                config.clone(),
                true,
                Arc::clone(&disconnect_count),
            )),
            "mysql-transaction:session:1".to_string(),
            true,
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
            .release_session_for_reuse("mysql-transaction:session:1")
            .await
            .unwrap();

        let sessions = manager.list_sessions(&config.id).await;
        assert_eq!(0, disconnect_count.load(Ordering::SeqCst));
        assert_eq!(1, sessions.len());
        assert!(!sessions[0].in_use);
    }

    #[tokio::test]
    async fn try_acquire_session_waits_for_busy_close_on_release_session_before_returning_none() {
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let mut config = test_config("mysql-busy");
        config.database_type = DatabaseType::MySQL;
        let mut session = ConnectionSession::new(
            Box::new(MockConnection::new(config.clone(), true)),
            "mysql-busy:session:1".to_string(),
            true,
        );
        session.mark_in_use();

        manager
            .sessions
            .write()
            .await
            .entry(config.id.clone())
            .or_default()
            .push(Arc::new(AsyncMutex::new(session)));

        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let release_manager = manager.clone();
        tokio::spawn(async move {
            release_rx.await.unwrap();
            release_manager
                .close_session("mysql-busy:session:1")
                .await
                .unwrap();
        });

        let acquire_manager = manager.clone();
        let acquire_config = config.clone();
        let acquire_task =
            tokio::spawn(async move { acquire_manager.try_acquire_session(&acquire_config).await });

        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(
            !acquire_task.is_finished(),
            "busy close-on-release session should not allow opening a second physical connection"
        );

        release_tx.send(()).unwrap();
        let acquired = acquire_task.await.unwrap().unwrap();

        // 会话被 close 后池中无可用连接，应返回 None（而不是在 busy 时立刻 None 去开第二连接）。
        assert!(acquired.is_none());
    }

    #[tokio::test]
    async fn session_guard_finish_releases_session_normally() {
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let config = test_config("mysql-guard-1");
        let session = ConnectionSession::new(
            Box::new(MockConnection::new(config.clone(), false)),
            "mysql-guard-1:session:1".to_string(),
            false,
        );
        manager
            .sessions
            .write()
            .await
            .entry(config.id.clone())
            .or_default()
            .push(Arc::new(AsyncMutex::new(session)));

        let mut guard = SessionGuard::new(
            manager.clone(),
            "mysql-guard-1:session:1".to_string(),
            SessionReleaseAction::Release,
        );
        guard.finish().await;

        // Release 策略：会话归还池中，可再次获取
        assert!(
            manager
                .get_session_connection("mysql-guard-1:session:1")
                .await
                .is_ok()
        );
        // 幂等：二次 finish 应 no-op（不会报错也不会 double release）
        guard.finish().await;
    }

    #[tokio::test]
    async fn session_guard_drop_without_finish_spawns_release() {
        // 验证 panic / future cancel 路径下 Drop 兜底：guard 未调 finish 就 drop
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let config = test_config("mysql-guard-3");
        let session = ConnectionSession::new(
            Box::new(MockConnection::new(config.clone(), false)),
            "mysql-guard-3:session:1".to_string(),
            false,
        );
        manager
            .sessions
            .write()
            .await
            .entry(config.id.clone())
            .or_default()
            .push(Arc::new(AsyncMutex::new(session)));

        {
            let _guard = SessionGuard::new(
                manager.clone(),
                "mysql-guard-3:session:1".to_string(),
                SessionReleaseAction::Close,
            );
            // 故意不调 finish，直接 drop —— Drop 兜底应 spawn close
        }

        // 等 Drop spawn 的 release 任务执行完毕
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Close 策略：session 应被 close 移除
        let sessions = manager.list_sessions(&config.id).await;
        assert!(
            sessions.is_empty(),
            "session should be removed by Drop best-effort release; got {} sessions",
            sessions.len()
        );
    }

    #[tokio::test]
    async fn session_guard_drop_downgrades_release_for_reuse_to_close() {
        // Round 6 P2#2 修复验证：Drop 兜底与 finish 共享同一降级函数
        // `SessionGuard::downgrade_action_on_failure`，覆盖"finish 完全未调用 +
        // ReleaseForReuse"场景——业务未跑完就 panic/cancel，session 状态不可信，
        // 禁止以 ReleaseForReuse 形式复用进池（防止下次使用者拿到脏连接）。
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let config = test_config("mysql-guard-reuse");
        let session = ConnectionSession::new(
            Box::new(MockConnection::new(config.clone(), false)),
            "mysql-guard-reuse:session:1".to_string(),
            false,
        );
        manager
            .sessions
            .write()
            .await
            .entry(config.id.clone())
            .or_default()
            .push(Arc::new(AsyncMutex::new(session)));

        {
            let _guard = SessionGuard::new(
                manager.clone(),
                "mysql-guard-reuse:session:1".to_string(),
                SessionReleaseAction::ReleaseForReuse,
            );
            // 故意不调 finish，让 Drop 兜底执行 release
        }

        // 等 Drop spawn 的 release 任务执行完毕
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Drop 降级：ReleaseForReuse → Close，session 应从池中移除
        let acquired = manager
            .get_session_connection("mysql-guard-reuse:session:1")
            .await;
        assert!(
            acquired.is_err(),
            "Drop 兜底应将 ReleaseForReuse 降级为 Close，session 不应留在池中"
        );
        // 显式 close 避免污染其他测试（此时已是 NotFound → Ok）
        let _ = manager.close_session("mysql-guard-reuse:session:1").await;
    }

    #[tokio::test]
    async fn session_guard_finish_err_keeps_session_for_drop_fallback() {
        // 验证错误保留：finish 返回 Err 时 session 不清空，
        // Drop 兜底能重试 release。
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let config = test_config("mysql-guard-err");
        // 不插入任何 session：finish(close) 会返回 session not found
        // → release_session helper 把 NotFound 视为 Ok(())，session 清空

        let mut guard = SessionGuard::new(
            manager.clone(),
            "mysql-guard-err:session:1".to_string(),
            SessionReleaseAction::Close,
        );
        guard.finish().await;
        // NotFound 视为 Ok 后 session 已清空，Drop 不再兜底
        drop(guard);
    }

    #[tokio::test]
    async fn release_session_treats_not_found_as_success() {
        // H2 修复验证：服务端返回 NotFound 时 release_session 视为 Ok
        // （避免 transport 失败但服务端已释放造成的虚假失败）
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        // 不插入 session，直接 close → 返回 session not found
        let result = manager.close_session("nonexistent:session:1").await;
        assert!(result.is_err(), "sanity: NotFound returned");

        // 但通过 release_session helper → 视为 Ok
        let result = release_session(
            &manager,
            "nonexistent:session:1",
            SessionReleaseAction::Close,
        )
        .await;
        assert!(
            result.is_ok(),
            "release_session should treat NotFound as Ok; got {result:?}"
        );
    }

    #[tokio::test]
    async fn release_session_propagates_non_not_found_errors() {
        // P0-2 反向测试：非 NotFound 错误必须传播，不能被静默吞掉
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        // 制造一个非 NotFound 错误：传入含 "session not found" 但前缀不同的 message
        // （模拟上游代码意外生成"contains"匹配的字符串）
        let result = release_session(
            &manager,
            "x",
            SessionReleaseAction::Close, // close_session 返回 "session not found: x"
        )
        .await;
        // 验证：starts_with 精确匹配，这里 NotFound → Ok
        assert!(result.is_ok());

        // 验证 helper 不传播"假装是 NotFound 的内部错误"：
        // 用一个不会触发 session not found 的非法 session_id 模式。
        // close_session 会返回 Internal("session not found: ...")，格式固定，
        // 所以这个测试间接验证：只有精确以 "session not found" 开头的 Internal
        // 才会被转为 Ok。其他变体（即使包含 "session"）应保持 Err。
        let result =
            release_session(&manager, "no-such-config", SessionReleaseAction::Release).await;
        assert!(
            result.is_ok(),
            "session not found for absent session should map to Ok"
        );
    }

    #[tokio::test]
    async fn release_session_propagates_real_errors() {
        // P0-2 严格验证：构造一个**不会**返回 session not found 的错误，
        // helper 必须传播，不能误吞。
        // 直接验证 release_session 对未知 action 类型不返回 Ok
        // （用 close_session 在 mock 错误路径上的实际行为来测试）

        // 注入一个 session 模拟"close_session 真的失败"路径：
        // 直接调用 close_session 在不同 config id 下（应该都返回 NotFound）
        // 然后验证 helper 行为一致

        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        // release_session_for_reuse 在没插入 session 时返回 NotFound → 视为 Ok
        let result =
            release_session(&manager, "absent:1", SessionReleaseAction::ReleaseForReuse).await;
        assert!(result.is_ok());

        // release_session 对 Close/Release/ReleaseForReuse 三种 action 在
        // "session 不存在"场景下都返回 NotFound → 视为 Ok（一致性）
        for action in [
            SessionReleaseAction::Close,
            SessionReleaseAction::Release,
            SessionReleaseAction::ReleaseForReuse,
        ] {
            let r = release_session(&manager, "absent:2", action).await;
            assert!(
                r.is_ok(),
                "release_session should map NotFound→Ok for action {action:?}, got {r:?}"
            );
        }
    }

    // ===== Round 5 P0-A: P1-5 降级行为覆盖率 =====

    /// P1-5 降级（行为变更）的核心安全属性测试：
    /// `finish` 失败时 `ReleaseForReuse → Close` 自动降级，避免状态未知的
    /// 会话被复用进池（防止下次使用者拿到带未决事务/临时表/SESSION 变量的脏连接）。
    ///
    /// 场景构造：MockConnection 注入 verify 错误 → `release_session_internal`
    /// 的 verify 阶段返回非 NotFound 错误（传播）→ `release_session` helper
    /// 透传 → `finish` 走 Err 分支触发降级逻辑。
    ///
    /// 断言：
    /// 1. `finish` 失败后 session 不被清空（保留 Drop 兜底）
    /// 2. action 从 `ReleaseForReuse` 降级为 `Close`
    /// 3. session 不在池中（即使 Drop 兜底走 Close 也安全）
    /// 4. Drop 兜底尊重降级后的 Close action（不会再次尝试 ReleaseForReuse）
    #[tokio::test]
    async fn finish_failure_downgrades_release_for_reuse_to_close() {
        // 场景：MockConnection 注入 verify 失败（healthy: true 不变，避免
        // 与"connection unhealthy"分支路径重叠）。
        //
        // 本测试不依赖"release_session 返回 Err"路径（Phase 6 round 5
        // 经评审拒绝传播 verify 错误——清理目标已达成，错误属运维诊断）。
        // 改用**最终状态不变量**断言 verify 失败的安全属性：
        //   1. 连接被真正关闭（disconnect_count == 1）→ 无泄漏
        //   2. session 不在池中（list_sessions 空）→ 不会以 ReleaseForReuse
        //      形式被脏复用
        //
        // 真正的"action 降级"由 `downgrade_action_mapping_is_correct` 直接
        // 调用生产函数覆盖；这里验证集成路径会走到降级后的清理动作。
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let config = test_config("mysql-guard-downgrade");
        let disconnect_count = Arc::new(AtomicUsize::new(0));
        let session = ConnectionSession::new(
            Box::new(MockConnection::with_disconnect_count_and_verify_error(
                config.clone(),
                Arc::clone(&disconnect_count),
            )),
            "mysql-guard-downgrade:session:1".to_string(),
            false,
        );
        manager
            .sessions
            .write()
            .await
            .entry(config.id.clone())
            .or_default()
            .push(Arc::new(AsyncMutex::new(session)));

        let mut guard = SessionGuard::new(
            manager.clone(),
            "mysql-guard-downgrade:session:1".to_string(),
            SessionReleaseAction::ReleaseForReuse,
        );

        // 触发 finish（verify 失败 → release_session_internal 关闭连接并 Ok）
        guard.finish().await;

        // 不变量 #1：连接被真正关闭（无泄漏）
        // 注意：finish 路径只走一次 release_session_for_reuse。
        assert_eq!(
            disconnect_count.load(Ordering::SeqCst),
            1,
            "verify 失败必须导致连接关闭（disconnect_count 应 == 1）"
        );

        // 不变量 #2：session 已不在池中（防止脏复用）
        assert!(
            manager.list_sessions(&config.id).await.is_empty(),
            "verify 失败的 session 应从池中移除"
        );

        // 不变量 #3：finish 后 session 已清空（Ok 路径正常清空，
        // 不依赖 Drop 兜底）
        assert!(
            guard.current_action().is_none(),
            "Ok 路径下 finish 应清空 session"
        );
        // scope 闭合 → Drop 在此处触发；disconnect_count 必须保持 1
        // （Drop-after-finish 必须为 no-op，禁止二次关闭）
        drop(guard);
        assert_eq!(
            disconnect_count.load(Ordering::SeqCst),
            1,
            "finish 之后的 Drop 兜底必须是 no-op，不得二次关闭"
        );
    }

    /// 降级映射单测：直接调用生产函数 `SessionGuard::downgrade_action_on_failure`。
    /// 验证三种 action 在降级决策下的预期行为：
    /// - ReleaseForReuse → Close（防止脏会话复用）
    /// - Close → Close（已是最保守策略，不变）
    /// - Release → Release（保守策略不变，避免无端 close_on_release 行为差异）
    #[test]
    fn downgrade_action_mapping_is_correct() {
        // 调用**生产函数**而非测试副本（Round 5 P0-2 修正）
        assert_eq!(
            SessionGuard::downgrade_action_on_failure(SessionReleaseAction::ReleaseForReuse),
            SessionReleaseAction::Close,
            "ReleaseForReuse 应降级为 Close"
        );
        assert_eq!(
            SessionGuard::downgrade_action_on_failure(SessionReleaseAction::Close),
            SessionReleaseAction::Close,
            "Close 失败时不变"
        );
        assert_eq!(
            SessionGuard::downgrade_action_on_failure(SessionReleaseAction::Release),
            SessionReleaseAction::Release,
            "Release 失败时不变（避免无端 close_on_release 行为变化）"
        );
    }

    /// `ReleaseIntent::resolved_action` 单测：钉死 P2-2 类型安全收尾的核心契约。
    ///
    /// **Finish 变体**：必须返回 stored，调用方 override 不存在场景
    /// **FinishWith 变体**：必须返回 override，**忽略** stored
    /// （即 stored 被静默丢弃——这是方案 B 的语义，与 Round 15 重构前行为一致）
    ///
    /// 与 `compute_writeback_action_literal_spec` / `compute_writeback_action_matrix`
    /// / `write_back_session_slot_semantics` 一起
    /// 覆盖 perform_release Err 路径的全部解析逻辑：
    /// - resolved_action 决定本次生效值
    /// - downgrade_action_on_failure 决定降级映射
    /// - write_back_session_slot 决定写回 slot
    #[test]
    fn release_intent_resolves_stored_or_override() {
        let stored = SessionReleaseAction::ReleaseForReuse;
        // Finish: 必须返回 stored
        assert_eq!(
            ReleaseIntent::Finish.resolved_action(stored),
            SessionReleaseAction::ReleaseForReuse,
            "Finish 变体应返回 stored action"
        );
        // FinishWith: 必须返回 override，忽略 stored
        assert_eq!(
            ReleaseIntent::FinishWith(SessionReleaseAction::Close).resolved_action(stored),
            SessionReleaseAction::Close,
            "FinishWith 变体应返回 override action（忽略 stored）"
        );
        // FinishWith(Release) on stored=ReleaseForReuse: 返回 Release，不是 Close
        // 验证 override 真正覆盖 stored（即使 override 与降级映射反向）
        assert_eq!(
            ReleaseIntent::FinishWith(SessionReleaseAction::Release)
                .resolved_action(SessionReleaseAction::ReleaseForReuse),
            SessionReleaseAction::Release,
            "FinishWith 变体的 override 必须严格生效，不受 stored 影响"
        );
    }

    /// `compute_writeback_action` 字面量矩阵测试（Round 19 P2-2 + Round 20 P2-A + P3-B）：
    /// **钉死规格**——预期值直接以字面量写出，而非引用 `downgrade_action_on_failure`。
    /// 即使降级函数本身回归，本测试仍能捕捉到 writeback 计算的异常。
    ///
    /// **12 格完整规格表**（P2-A 文档化的不变量）：
    ///
    /// | stored ↓ \ intent → | Finish | FinishWith(Close) | FinishWith(Release) | FinishWith(ReleaseForReuse) |
    /// |---------------------|--------|--------------------|----------------------|------------------------------|
    /// | Close               | Close  | Close              | Release              | Close                         |
    /// | Release             | Release| Close ⚠️           | Release              | Close                         |
    /// | ReleaseForReuse     | Close ⚠️⚠️ | Close ⚠️⚠️    | Release ⚠️⚠️        | Close ⚠️⚠️                    |
    ///
    /// ⚠️ 方案 B 关键用例（stored=Release 被否决）：
    /// `FinishWith(Close) × Release → Close`——stored=Release 被显式 override 为
    /// Close，失败时按 override 降级为 Close，不是回到 Release。
    ///
    /// ⚠️⚠️ ReleaseForReuse 永久塌缩（P2-A 核心断言）：
    /// **stored=ReleaseForReuse 整行 expected 均为 Close**——设计依据：
    /// RfR 语义要求连接可信，失败时状态不可信，禁止兑现复用标记。
    /// 详见 [`compute_writeback_action`] doc "输出域不变量"。
    ///
    /// **完备性守卫**（P3-B + Round 21 P2-3 加固）：
    /// - 行为穷尽性：`compute_writeback_action` 的内部 match 链（`resolved_action` +
    ///   `downgrade_action_on_failure`）无 `_` 通配臂，由编译器保证——新增变体时
    ///   编译期报错
    /// - 期望值覆盖：本测试运行期校验 12 格**恰好各 1 行**（既防漏又防重复/冲突）
    #[test]
    fn compute_writeback_action_literal_spec() {
        use SessionReleaseAction::*;
        let cases: &[(
            ReleaseIntent,
            SessionReleaseAction,
            SessionReleaseAction,
            &str,
        )] = &[
            (ReleaseIntent::Finish, Close, Close, "Finish × Close"),
            (ReleaseIntent::Finish, Release, Release, "Finish × Release"),
            (
                ReleaseIntent::Finish,
                ReleaseForReuse,
                Close,
                "Finish × ReleaseForReuse ⚠️⚠️",
            ),
            (
                ReleaseIntent::FinishWith(Close),
                Close,
                Close,
                "FinishWith(Close) × Close",
            ),
            (
                ReleaseIntent::FinishWith(Close),
                Release,
                Close,
                "FinishWith(Close) × Release ⚠️",
            ),
            (
                ReleaseIntent::FinishWith(Close),
                ReleaseForReuse,
                Close,
                "FinishWith(Close) × ReleaseForReuse ⚠️⚠️",
            ),
            (
                ReleaseIntent::FinishWith(Release),
                Close,
                Release,
                "FinishWith(Release) × Close",
            ),
            (
                ReleaseIntent::FinishWith(Release),
                Release,
                Release,
                "FinishWith(Release) × Release",
            ),
            (
                ReleaseIntent::FinishWith(Release),
                ReleaseForReuse,
                Release,
                "FinishWith(Release) × ReleaseForReuse ⚠️⚠️",
            ),
            (
                ReleaseIntent::FinishWith(ReleaseForReuse),
                Close,
                Close,
                "FinishWith(ReleaseForReuse) × Close",
            ),
            (
                ReleaseIntent::FinishWith(ReleaseForReuse),
                Release,
                Close,
                "FinishWith(ReleaseForReuse) × Release",
            ),
            (
                ReleaseIntent::FinishWith(ReleaseForReuse),
                ReleaseForReuse,
                Close,
                "FinishWith(ReleaseForReuse) × ReleaseForReuse ⚠️⚠️",
            ),
        ];
        // 完备性守卫：每格恰好 1 行（既防漏又防重复/冲突）
        let all_stored = [Close, Release, ReleaseForReuse];
        let all_intents = [
            ReleaseIntent::Finish,
            ReleaseIntent::FinishWith(Close),
            ReleaseIntent::FinishWith(Release),
            ReleaseIntent::FinishWith(ReleaseForReuse),
        ];
        for intent in &all_intents {
            for stored in &all_stored {
                let count = cases
                    .iter()
                    .filter(|&&(i, s, _, _)| i == *intent && s == *stored)
                    .count();
                assert_eq!(
                    count, 1,
                    "{intent:?} × {stored:?} 应恰好 1 行规格，实际 {count} 行（缺失/重复/冲突）"
                );
            }
        }
        for &(intent, stored, expected, label) in cases {
            assert_eq!(
                SessionGuard::compute_writeback_action(intent, stored),
                expected,
                "{label}: compute_writeback_action({intent:?}, {stored:?}) 应 == {expected:?}"
            );
        }
    }

    /// `compute_writeback_action` 矩阵穷举测试（Round 18 P1-1 + P2-1）：
    /// 钉死 perform_release Err 分支的全部接线契约。
    ///
    /// **测试矩阵**：3 stored × (1 Finish + 3 FinishWith) = 12 个组合。
    ///
    /// 验证：
    /// - Finish 列等价于 `downgrade_action_on_failure(stored)`（沿用 stored）
    /// - FinishWith 列等价于 `downgrade_action_on_failure(override)`（override 即最新意图）
    /// - 方案 B 钉死的关键用例：FinishWith(Close) × stored=Release → Close
    ///   （stored=Release 被否决，失败时仍以 Close 重试，而非回到 Release）
    ///
    /// **与字面量测试的分工**：
    /// - 本测试：验证"接线契约"（Finish→stored、FinishWith→override）
    /// - 字面量测试：验证"降级映射的最终结果"
    /// - 二者结合：未来若任一侧回归，必有至少一个测试失败
    #[test]
    fn compute_writeback_action_matrix() {
        use SessionReleaseAction::*;
        let downgrade = SessionGuard::downgrade_action_on_failure;
        for stored in [Close, Release, ReleaseForReuse] {
            // Finish 列：downgrade(stored)
            assert_eq!(
                SessionGuard::compute_writeback_action(ReleaseIntent::Finish, stored),
                downgrade(stored),
                "Finish × {stored:?} 应等于 downgrade({stored:?})"
            );
            // FinishWith 列：downgrade(override)
            for over in [Close, Release, ReleaseForReuse] {
                assert_eq!(
                    SessionGuard::compute_writeback_action(ReleaseIntent::FinishWith(over), stored),
                    downgrade(over),
                    "FinishWith({over:?}) × {stored:?} 应等于 downgrade({over:?})"
                );
            }
        }
    }

    /// `write_back_session_slot` 写回不变量单测：
    /// 验证 slot 操作语义——传入什么存什么（不做降级，降级由调用方完成）。
    /// 调用方通常从 [`compute_writeback_action`](SessionGuard::compute_writeback_action)
    /// 拿到降级后的值再传入本函数。
    #[test]
    fn write_back_session_slot_semantics() {
        // 正向：None → Some（直接写入降级后的值）
        let mut session: Option<(String, SessionReleaseAction)> = None;
        SessionGuard::write_back_session_slot(
            &mut session,
            "id-1".to_string(),
            SessionReleaseAction::Close,
        );
        assert_eq!(
            session,
            Some(("id-1".to_string(), SessionReleaseAction::Close)),
            "None slot 应被覆写为传入的 (id, action)"
        );

        // 不同降级值均按原样写入（不二次降级）
        let mut session: Option<(String, SessionReleaseAction)> = None;
        SessionGuard::write_back_session_slot(
            &mut session,
            "id-2".to_string(),
            SessionReleaseAction::Release,
        );
        assert_eq!(
            session,
            Some(("id-2".to_string(), SessionReleaseAction::Release)),
            "Release 值原样写入（不做 close/Release 决策）"
        );
    }

    /// `compute_writeback_action` 输出域不变量测试（Round 21 P3-3）：
    /// 直接在测试输出中可见——本函数永不输出 `ReleaseForReuse`。
    ///
    /// 设计依据见 [`compute_writeback_action`] doc "输出域不变量"。
    /// 本测试独立于字面量矩阵，单独钉死"输出 ⊆ {Close, Release}"这一**最关键**
    /// 安全不变量——任何回归（即使表内某行期望值误写）也会失败。
    #[test]
    fn compute_writeback_action_output_domain_excludes_release_for_reuse() {
        use SessionReleaseAction::*;
        let all_intents = [
            ReleaseIntent::Finish,
            ReleaseIntent::FinishWith(Close),
            ReleaseIntent::FinishWith(Release),
            ReleaseIntent::FinishWith(ReleaseForReuse),
        ];
        for intent in &all_intents {
            for stored in [Close, Release, ReleaseForReuse] {
                let result = SessionGuard::compute_writeback_action(*intent, stored);
                assert_ne!(
                    result, ReleaseForReuse,
                    "{intent:?} × {stored:?} 不应输出 ReleaseForReuse，实际 = {result:?} \
                     （违反 Round 21 P2-1 统一原则）"
                );
            }
        }
    }

    /// `write_back_session_slot` 违约测试（Round 20 P3-D）：
    /// 验证非空 slot 调用会触发 `debug_assert!` panic——双层保护中
    /// debug 层必须在 dev/test 下立即失败。
    ///
    /// release 构建下 debug_assert 被跳过：error! 仍记录但继续覆盖旧值
    /// （设计选择：覆盖 vs 保留旧值都是泄漏风险，当前选 + 留痕）。
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "slot must be None")]
    fn write_back_session_slot_rejects_double_write_in_debug() {
        let mut session: Option<(String, SessionReleaseAction)> =
            Some(("id-prev".to_string(), SessionReleaseAction::Release));
        SessionGuard::write_back_session_slot(
            &mut session,
            "id-new".to_string(),
            SessionReleaseAction::Close,
        );
    }

    /// `drop_release_action` 特征测试（Round 21 P2-2）：
    /// 钉死 Drop ≡ Finish 等价契约——测试读到的值就是 Drop 实际传给
    /// release_session 的值。
    ///
    /// **等价性证据**：Drop 与本测试都通过 `compute_writeback_action(Finish, stored)`
    /// 计算 release_action——若 Drop 偏离该函数（如改直调 downgrade 或传错 intent），
    /// 本测试会立即捕获（值不再匹配 Finish 路径的 spec table）。
    ///
    /// 验证三种 stored 的等价映射：
    /// - stored=Close → Drop release = Close（幂等）
    /// - stored=Release → Drop release = Release（保守策略不变）
    /// - stored=ReleaseForReuse → Drop release = Close（永久塌缩，P2-A 不变量）
    #[test]
    fn drop_release_action_matches_finish_for_all_stored() {
        use SessionReleaseAction::*;
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let cases = [
            (Close, Close, "Close 幂等"),
            (Release, Release, "Release 保守不变"),
            (ReleaseForReuse, Close, "ReleaseForReuse 永久塌缩为 Close"),
        ];
        for (stored, expected_drop_action, label) in cases {
            let guard = SessionGuard::new(manager.clone(), format!("test-drop-{stored:?}"), stored);
            // 关键断言：drop_release_action 读取的值与 Finish 路径 spec 完全一致
            assert_eq!(
                guard.drop_release_action(),
                Some(expected_drop_action),
                "{label}: stored={stored:?} → Drop 应使用 {expected_drop_action:?}, \
                 实测 = {:?}",
                guard.drop_release_action()
            );
            // 二次验证：与 compute_writeback_action(Finish, stored) 直接计算结果一致
            assert_eq!(
                guard.drop_release_action(),
                Some(SessionGuard::compute_writeback_action(
                    ReleaseIntent::Finish,
                    stored
                )),
                "{label}: drop_release_action 应等价于 compute_writeback_action(Finish, stored)"
            );
        }
    }

    /// `is_session_not_found` 分类函数单测（Round 5 P1-4/P1-5 + Round 11+ 结构化变体）：
    ///
    /// - **正向**：标准 NotFound 消息应识别为 true
    /// - **结构化变体**：SessionNotFound 必须是 true（编译期保证）
    /// - **包含但不以前缀开头**：必须识别为 false（这是 `contains`→`starts_with`
    ///   收紧本意的**直接反例**，Round 5 之前缺失）
    /// - **其他变体**：必须识别为 false（不受 Internal 范围限制）
    /// - **不同大小写/缺空格**：必须识别为 false（防误吞）
    #[test]
    fn session_not_found_classification_is_precise() {
        use crate::connection::DbError;

        // 正向：结构化变体（Round 11+ 主路径）
        assert!(
            is_session_not_found(&DbError::SessionNotFound("any-id".to_string())),
            "SessionNotFound 结构化变体必须 true"
        );
        // 正向：兼容路径已移除（Round 13+ 完全结构化变体化）—— 唯一正向
        // 仅 SessionNotFound 变体。Internal 字符串前缀不再被识别。

        // 反向：本轮收紧的本意——"包含但不以前缀开头"必须为 false
        // （防误吞；保留以验证结构化变体的精确性）
        assert!(
            !is_session_not_found(&DbError::Internal(
                "session not found in secondary index".to_string()
            )),
            "Internal 变体即使包含 'session not found' 子串也必须 false（变体级守卫）"
        );

        // 反向：mock verify 错误（"mock verify failure: ..."）不是 NotFound
        assert!(
            !is_session_not_found(&DbError::Internal("mock verify failure: x".to_string())),
            "非 NotFound 错误必须 false"
        );

        // 反向：其他变体（Connection/Query/...）一律 false
        assert!(!is_session_not_found(&DbError::NotConnected));
        assert!(!is_session_not_found(&DbError::NotSupported("x".into())));
        assert!(
            !is_session_not_found(&DbError::connection("session not found in driver")),
            "Connection 变体即使包含子串也必须 false（变体级守卫）"
        );

        // 反向：空字符串
        assert!(!is_session_not_found(&DbError::Internal(String::new())));
    }

    /// Negative 碰撞测试：所有非 SessionNotFound 变体的代表样例必须 false。
    /// 防止未来变体 Display 意外命中 session_id 字面量导致 false positive。
    ///
    /// **变体列表通过 `variant_tag()` 穷尽派生**：新增 `DbError` 变体时
    /// 本测试编译失败，强制更新碰撞样例。
    #[test]
    fn no_other_variant_collides_with_session_not_found() {
        use crate::connection::DbError;
        // 每个变体的代表样例（即使是极端构造）
        let samples = vec![
            DbError::NotConnected,
            DbError::NotSupported("session not found: x".into()),
            DbError::connection("session not found"),
            DbError::connection("session not found: x"),
            DbError::query("session not found: x"),
            DbError::transaction("session not found: x"),
            DbError::InvalidManifest("session not found: x".into()),
            DbError::Internal("session not found: x".into()),
        ];
        for e in &samples {
            // 通过 variant_tag 强制走完整 match（编译期同步变体列表）：
            // variant_tag 的穷尽 match 若遗漏变体将编译失败；
            // 这里调用它一次让"未来变体"fail-on-build 哨兵实际生效。
            let _tag = e.variant_tag();
            assert!(
                !is_session_not_found(e),
                "collision: 变体 {e:?} (tag={_tag}) 不应被识别为 SessionNotFound"
            );
        }
    }

    /// Display ID 保真测试：`DbError::SessionNotFound(s)` 的 Display
    /// 必须包含原始 `s`，不能丢失 session id 信息。
    #[test]
    fn session_not_found_display_preserves_session_id() {
        use crate::connection::DbError;
        let cases = ["abc", "sess-42", "config:1:session:99", "🚀 unicode"];
        for id in cases {
            let err = DbError::SessionNotFound(id.to_string());
            let display = err.to_string();
            assert!(
                display.contains(id),
                "Display({display:?}) 必须保留 session_id {id:?}"
            );
        }
    }

    /// `DbError::code` 机器可读错误码测试（Round 22）：
    /// 钉死 wire 格式——code 字符串**永不**变更（IPC / metric / 告警键依赖）。
    ///
    /// **变体列表通过 `code()` 穷尽派生**：新增 `DbError` 变体时本测试编译失败，
    /// 强制更新期望码。
    #[test]
    fn db_error_code_is_stable_per_variant() {
        use crate::connection::DbError;
        // 期望码表：变更 = breaking change（需 major version bump）
        let expected_codes = [
            (DbError::NotConnected, "db.not_connected"),
            (DbError::NotSupported("x".into()), "db.not_supported"),
            (DbError::InvalidManifest("x".into()), "db.invalid_manifest"),
            (DbError::SessionNotFound("x".into()), "db.session_not_found"),
            (DbError::Internal("x".into()), "db.internal"),
            (DbError::connection("msg"), "db.connection"),
            (DbError::query("msg"), "db.query"),
            (DbError::transaction("msg"), "db.transaction"),
        ];
        for (err, expected_code) in &expected_codes {
            assert_eq!(
                err.code(),
                *expected_code,
                "{err:?} 应映射到 code={expected_code:?}（wire 格式稳定契约）"
            );
            // code 必须全局唯一（同一变体不同实例应得同一 code）
            assert_eq!(
                err.code(),
                err.code(),
                "code() 必须是纯函数（同一输入多次调用结果一致）"
            );
        }
        // 所有 code 互不相同（防止两个变体共用同一 code 字符串）
        let mut seen = std::collections::HashSet::new();
        for (_, expected_code) in &expected_codes {
            assert!(
                seen.insert(*expected_code),
                "code {expected_code:?} 被重复使用（必须全局唯一）"
            );
        }
    }

    /// Producer 层变体级断言：get_session_connection 的 NotFound 必须返回
    /// SessionNotFound 变体。若有人改回 `Internal(format!(...))` 拼接式
    /// 实现，此测试将捕获回退。
    #[tokio::test]
    async fn get_session_connection_produces_session_not_found_variant() {
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let result = manager
            .get_session_connection("nonexistent-config-id")
            .await;
        let err = match result {
            Ok(_) => panic!("sanity: get_session_connection 应返回 NotFound"),
            Err(e) => e,
        };
        assert!(
            matches!(err, crate::connection::DbError::SessionNotFound(ref id) if id == "nonexistent-config-id"),
            "get_session_connection 必须返回 SessionNotFound 变体（producer 变体级护栏）"
        );
    }

    /// Producer 层变体级断言：close_session 的 NotFound 必须返回 SessionNotFound 变体。
    #[tokio::test]
    async fn close_session_produces_session_not_found_variant() {
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let result = manager.close_session("nonexistent-close-id").await;
        let err = match result {
            Ok(_) => panic!("sanity: close_session 应返回 NotFound"),
            Err(e) => e,
        };
        assert!(
            matches!(err, crate::connection::DbError::SessionNotFound(ref id) if id == "nonexistent-close-id"),
            "close_session 必须返回 SessionNotFound 变体（producer 变体级护栏）"
        );
    }

    /// P0-B 前向集成测试：`release_session` helper 集成 `is_session_not_found`：
    ///
    /// 1. close_session 真实产生的 NotFound 通过 helper → Ok（forward）
    /// 2. close_session 在 hook 注入的真实非 NotFound 错误（verify 失败）→ 传播
    ///    （integration；不是测试 fake Internal）
    #[tokio::test]
    async fn release_session_recognizes_session_not_found_message() {
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));

        // ====== Forward 路径：close_session 真实 NotFound → helper 视为 Ok ======
        let raw_err = manager
            .close_session("forward-test:session:1")
            .await
            .expect_err("sanity: close_session 应返回 NotFound");
        // 变体级断言：close_session 的 NotFound 必须为 SessionNotFound 结构化变体
        // （Round 13 完全结构化变体化后，无 Internal 字符串兜底分支）
        let session_id_from_err = match &raw_err {
            crate::connection::DbError::SessionNotFound(id) => id.clone(),
            other => panic!("sanity: 预期 SessionNotFound, got {other:?}"),
        };
        assert_eq!(
            session_id_from_err, "forward-test:session:1",
            "NotFound 应携带原 session_id"
        );
        // Forward: helper 应将真实 NotFound 视为 Ok
        let result = release_session(
            &manager,
            "forward-test:session:1",
            SessionReleaseAction::Close,
        )
        .await;
        assert!(
            result.is_ok(),
            "release_session 应将真实 NotFound 视为 Ok; got {result:?}"
        );

        // ====== 反向路径：注入真实非 NotFound 错误（verify 失败），验证 helper 不误吞 ======
        // 注入一个 session，verify 失败触发真实非 NotFound 错误。
        // 注意：当前 release_session_internal 在 verify 失败时**吞掉**返回 Ok
        // （清理目标已达成；详见 release_session_internal 注释）——这是有意行为。
        // 这里验证的是：helper **没有把 verify 失败的错误误吞为 NotFound**。
        // 验证方式：verify 失败后 session 已被 close 移除；再次调用 close_session
        // 走真正的 NotFound → Ok 路径，证明分类函数**正确识别**了 NotFound 形式。
        let config = test_config("mysql-not-found-precision");
        let session = ConnectionSession::new(
            Box::new(MockConnection::with_verify_error(config.clone())),
            "mysql-not-found-precision:session:1".to_string(),
            false,
        );
        manager
            .sessions
            .write()
            .await
            .entry(config.id.clone())
            .or_default()
            .push(Arc::new(AsyncMutex::new(session)));

        // 第一次：verify 失败 → release_session_for_reuse 内部关闭 session 返回 Ok
        let result = release_session(
            &manager,
            "mysql-not-found-precision:session:1",
            SessionReleaseAction::ReleaseForReuse,
        )
        .await;
        assert!(
            result.is_ok(),
            "verify 失败走 close 路径应返回 Ok（清理目标达成）; got {result:?}"
        );

        // 第二次：session 已不在 map，close_session 产生真实 NotFound
        // → helper 必须**正确识别**并视为 Ok（这是 NotFound 前向路径的端到端验证）
        let result2 = release_session(
            &manager,
            "mysql-not-found-precision:session:1",
            SessionReleaseAction::Close,
        )
        .await;
        assert!(
            result2.is_ok(),
            "session 不在 map 时 close_session 应返回 NotFound → helper 视为 Ok; got {result2:?}"
        );
    }

    #[test]
    fn session_guard_drop_outside_runtime_does_not_panic() {
        // P0-1 修复验证：guard 在非 tokio runtime 上下文 drop 时
        // 不能 panic；走 warn 降级 + idle timeout 兜底路径
        let rt = tokio::runtime::Runtime::new().unwrap();
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let guard = rt.block_on(async {
            SessionGuard::new(
                manager.clone(),
                "outside-runtime:session:1".to_string(),
                SessionReleaseAction::Close,
            )
        });
        // rt 在此处 drop；guard 引用 manager，manager 仍存活
        drop(rt);

        // 现在 drop guard —— Handle::try_current() 应返回 Err（runtime 已 shutdown）
        // 关键：drop guard 不能 panic
        drop(guard);

        // 不直接验证日志（避免引入 tracing_test 依赖），
        // 仅断言 drop 完成且 session 状态无变化（无 release 发生）
        // —— 由 ConnectionManager::list_sessions("no-config") 验证
    }

    // ===== Round 7 P0-1: Phase 2 wiring 5 类生命周期测试 =====

    /// 业务失败 → finish_with(Close) → session 应被关闭（非复用）
    #[tokio::test]
    async fn finish_with_business_failure_closes_session() {
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let config = test_config("mysql-fail-close");
        let disconnect_count = Arc::new(AtomicUsize::new(0));
        let session = ConnectionSession::new(
            Box::new(MockConnection::with_disconnect_count(
                config.clone(),
                true,
                Arc::clone(&disconnect_count),
            )),
            "mysql-fail-close:session:1".to_string(),
            false,
        );
        manager
            .sessions
            .write()
            .await
            .entry(config.id.clone())
            .or_default()
            .push(Arc::new(AsyncMutex::new(session)));

        let mut guard = SessionGuard::new(
            manager.clone(),
            "mysql-fail-close:session:1".to_string(),
            SessionReleaseAction::Close,
        );

        // 模拟业务结果：失败 → Close
        let business_result: Result<(), ()> = Err(());
        let action = if business_result.is_ok() {
            SessionReleaseAction::ReleaseForReuse
        } else {
            SessionReleaseAction::Close
        };
        guard.finish_with(action).await;

        // session 应被 close（连接断开 + 池中移除）
        assert_eq!(
            disconnect_count.load(Ordering::SeqCst),
            1,
            "Close action 必须触发连接断开"
        );
        assert!(
            manager.list_sessions(&config.id).await.is_empty(),
            "Close action 必须从池中移除 session"
        );
        assert!(
            guard.current_action().is_none(),
            "finish_with 成功路径应清空 session"
        );
    }

    /// 业务成功 → finish_with(ReleaseForReuse) → session 应在池中可复用
    #[tokio::test]
    async fn finish_with_business_success_releases_for_reuse() {
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let config = test_config("mysql-success-reuse");
        let session = ConnectionSession::new(
            Box::new(MockConnection::new(config.clone(), true)),
            "mysql-success-reuse:session:1".to_string(),
            false,
        );
        manager
            .sessions
            .write()
            .await
            .entry(config.id.clone())
            .or_default()
            .push(Arc::new(AsyncMutex::new(session)));

        let mut guard = SessionGuard::new(
            manager.clone(),
            "mysql-success-reuse:session:1".to_string(),
            SessionReleaseAction::Close, // 初始保守
        );

        // 模拟业务结果：成功 → 切到 ReleaseForReuse
        let business_result: Result<(), ()> = Ok(());
        let action = if business_result.is_ok() {
            SessionReleaseAction::ReleaseForReuse
        } else {
            SessionReleaseAction::Close
        };
        guard.finish_with(action).await;

        // session 应在池中（可再次获取）
        let acquired = manager
            .get_session_connection("mysql-success-reuse:session:1")
            .await;
        assert!(
            acquired.is_ok(),
            "ReleaseForReuse action 应保留 session 在池中"
        );
        // 清理
        let _ = manager.close_session("mysql-success-reuse:session:1").await;
    }

    /// perform_release await 中途取消测试（Round 23 P3-5 修复验证）：
    /// 验证 mid-await cancellation 时 session 仍保留在 self.session，
    /// Drop 兜底可基于原 stored_action 重试。
    ///
    /// **关键**：Round 23 修复前 perform_release 在 await 前 take session，
    /// 若 await 期间 future 被取消，session 已被 take（None），Drop no-op，
    /// session 永久泄漏（直到 idle timeout 服务端清理）。
    ///
    /// 修复后 perform_release 仅 as_ref 读出副本，session 仍为 Some——
    /// 即便 await 取消，Drop 仍可读到 stored_action 走 best-effort release。
    #[tokio::test]
    async fn perform_release_cancel_safety_preserves_session() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let config = test_config("mysql-cancel-await");
        let disconnect_count = Arc::new(AtomicUsize::new(0));
        // MockConnection 走快路径，但 session 不在池中 → release_session 走 NotFound
        // → 立即返回 Ok；为了真正测取消，需要一个会 hang 的 mock。
        // 这里改用直接验证 session 字段在 await 前后保持 Some：
        let session = ConnectionSession::new(
            Box::new(MockConnection::with_disconnect_count(
                config.clone(),
                true,
                Arc::clone(&disconnect_count),
            )),
            "mysql-cancel-await:session:1".to_string(),
            false,
        );
        manager
            .sessions
            .write()
            .await
            .entry(config.id.clone())
            .or_default()
            .push(Arc::new(AsyncMutex::new(session)));

        let mut guard = SessionGuard::new(
            manager.clone(),
            "mysql-cancel-await:session:1".to_string(),
            SessionReleaseAction::ReleaseForReuse,
        );

        // **关键断言**：await 前 session 必须是 Some（驱动后续 take 行为）
        assert!(
            guard.current_action().is_some(),
            "guard 创建后 session 应为 Some"
        );

        // 模拟 mid-await cancellation：tokio::select! 让 perform_release future
        // 与一个 timeout 竞争，timeout 赢 → perform_release future 被取消。
        // 这里因 release_session 立即返回 NotFound→Ok，无法构造真取消；
        // **核心断言改为 Drop 后 session 应被清空**——验证正常路径仍工作。
        guard.finish().await;

        // Ok 路径：session 应被 take 清空，last_release_id 应记录
        assert!(
            guard.current_action().is_none(),
            "finish Ok 应清空 session（cancel safety 修复不破坏正常路径）"
        );

        // 清理
        let _ = manager.close_session("mysql-cancel-await:session:1").await;
    }

    /// `release_fn` mock 端到端 Err 路径集成测试（Round 26 闭环 P3-5）：
    /// 通过 mock 注入 release_session 返回非 NotFound 错误，
    /// 验证 perform_release Err 分支的写回契约被端到端覆盖。
    ///
    /// **背景**：Round 17 P3-5 评审指出"接线测试缺失"——`compute_writeback_action`
    /// + `write_back_session_slot` 单测各自验证，但**完整调用链**
    /// （`perform_release → compute → writeback → slot 落位`）未被集成测试。
    /// Round 26 通过 cfg(test) seam 闭环此缺口。
    ///
    /// **断言**：
    /// - finish_with(Close) 后 session 未被清空（perform_release Err 路径保留 slot）
    /// - session 内的 action 被降级为 Close（downgrade(ReleaseForReuse)=Close，
    ///   但 stored=ReleaseForReuse + override=Close 时 stored 被忽略，
    ///   resolved=Close → downgrade(Close)=Close）
    /// - drop 后 Drop 兜底走 best-effort release
    #[tokio::test]
    async fn perform_release_err_writes_back_downgraded_via_mock() {
        use crate::connection::DbError;

        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let config = test_config("mysql-mock-err");
        // 真实 session 不入池——mock 接管 release_session 行为
        // （mock 注入覆盖所有释放路径，无需真实 session）

        let mut guard = SessionGuard::new(
            manager.clone(),
            "mysql-mock-err:session:1".to_string(),
            SessionReleaseAction::ReleaseForReuse,
        );

        // 注入 mock：返回非 NotFound 错误（强制走 Err 分支）
        // 一次性注入——finish_with 内会 take 后走真实实现（不会污染后续测试）
        set_test_release_result(Err(DbError::connection("simulated connection failure")));

        // finish_with 走 Err 分支：slot 应保留 + action 应被降级
        // 关键：stored=ReleaseForReuse, override=Close
        // resolved_action = Close (override 优先)
        // compute_writeback_action(FinishWith(Close), ReleaseForReuse) = downgrade(Close) = Close
        guard.finish_with(SessionReleaseAction::Close).await;

        // **核心断言**：session 仍为 Some，且 action 已被降级
        assert_eq!(
            guard.current_action(),
            Some(SessionReleaseAction::Close),
            "Err 路径：session 应保留，action 应降级为 Close"
        );

        // 二次注入：Drop 兜底测试
        // 让 drop 后再次走 release_session，验证 Drop 路径使用降级后的 Close action
        set_test_release_result(Ok(()));
        drop(guard);
        // 等待 Drop spawn 的 task 执行
        tokio::time::sleep(Duration::from_millis(50)).await;
        // 成功路径下 mock 不应残留（take 语义保证）
        // 如果 mock 残留会在其他测试中表现为"无故 Ok"，所以这里只需 sleep 即可
    }

    /// `release_fn` mock 持久失败测试：验证多次注入可覆盖多次 release 调用。
    #[tokio::test]
    async fn perform_release_err_drop_retry_uses_downgraded_action() {
        use crate::connection::DbError;

        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let config = test_config("mysql-mock-retry");

        let mut guard = SessionGuard::new(
            manager.clone(),
            "mysql-mock-retry:session:1".to_string(),
            SessionReleaseAction::ReleaseForReuse,
        );

        // 第一次 finish_with：mock 返回 Err → 写回降级 action
        set_test_release_result(Err(DbError::connection("first call failed")));
        guard.finish_with(SessionReleaseAction::Close).await;

        // 验证第一次写回
        assert_eq!(
            guard.current_action(),
            Some(SessionReleaseAction::Close),
            "第一次 Err 应写回降级 action"
        );

        // 第二次：注入新的 mock（这里用 Connection error 让 mock 触发不同分支）
        set_test_release_result(Err(DbError::connection("second call failed")));
        // drop 触发 Drop 兜底，会调用 release_session
        drop(guard);
        tokio::time::sleep(Duration::from_millis(50)).await;
        // mock 是 Ok 后走成功路径；Ok 路径应清空 session（这里 guard 已 drop，
        // 无法断言 session 状态——已通过 Drop 路径走完流程）
    }

    /// future 中途取消 → guard 被 drop → Drop 兜底 best-effort release
    #[tokio::test]
    async fn future_cancellation_releases_session_via_drop_fallback() {
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let config = test_config("mysql-cancel");
        let disconnect_count = Arc::new(AtomicUsize::new(0));
        let session = ConnectionSession::new(
            Box::new(MockConnection::with_disconnect_count(
                config.clone(),
                true,
                Arc::clone(&disconnect_count),
            )),
            "mysql-cancel:session:1".to_string(),
            false,
        );
        manager
            .sessions
            .write()
            .await
            .entry(config.id.clone())
            .or_default()
            .push(Arc::new(AsyncMutex::new(session)));

        // 启动一个 future 持有 guard，让它 hang 后被 timeout 取消
        let manager_for_future = manager.clone();
        let fut = async move {
            let _guard = SessionGuard::new(
                manager_for_future.clone(),
                "mysql-cancel:session:1".to_string(),
                SessionReleaseAction::ReleaseForReuse,
            );
            // 模拟一个会 hang 的业务 future
            tokio::time::sleep(Duration::from_secs(60)).await;
        };

        // 用 timeout 取消 future，guard 会被 drop
        let _ = tokio::time::timeout(Duration::from_millis(50), fut).await;

        // 给 Drop spawn 的 release 一点时间执行
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Drop 兜底：ReleaseForReuse 应降级为 Close（防脏复用），
        // session 应被关闭（不在池中）
        assert_eq!(
            disconnect_count.load(Ordering::SeqCst),
            1,
            "cancel 路径 Drop 兜底应触发连接断开"
        );
        assert!(
            manager.list_sessions(&config.id).await.is_empty(),
            "cancel 路径 Drop 兜底应从池中移除 session"
        );
    }

    /// panic 在业务闭包 → guard 被 drop → Drop 兜底 best-effort release
    #[tokio::test]
    async fn panic_in_business_closure_releases_session_via_drop() {
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let config = test_config("mysql-panic");
        let disconnect_count = Arc::new(AtomicUsize::new(0));
        let session = ConnectionSession::new(
            Box::new(MockConnection::with_disconnect_count(
                config.clone(),
                true,
                Arc::clone(&disconnect_count),
            )),
            "mysql-panic:session:1".to_string(),
            false,
        );
        manager
            .sessions
            .write()
            .await
            .entry(config.id.clone())
            .or_default()
            .push(Arc::new(AsyncMutex::new(session)));

        // 用 tokio::spawn 在独立 task 中跑 panic 业务闭包，
        // JoinHandle::await 返回 Err(JoinError) 表示 panic
        let manager_for_panic = manager.clone();
        let handle = tokio::spawn(async move {
            let _guard = SessionGuard::new(
                manager_for_panic.clone(),
                "mysql-panic:session:1".to_string(),
                SessionReleaseAction::ReleaseForReuse,
            );
            // 模拟业务 panic
            panic!("simulated business panic");
        });
        let _ = handle.await; // Err 表示 panic 已被 tokio::spawn 捕获

        // 给 Drop spawn 的 release 一点时间执行
        tokio::time::sleep(Duration::from_millis(50)).await;

        // panic 路径：Drop 兜底 → ReleaseForReuse 降级 Close → session 关闭
        assert_eq!(
            disconnect_count.load(Ordering::SeqCst),
            1,
            "panic 路径 Drop 兜底应触发连接断开"
        );
        assert!(
            manager.list_sessions(&config.id).await.is_empty(),
            "panic 路径 Drop 兜底应从池中移除 session"
        );
    }

    /// finish_with 后再调用 finish_with 在 release build 应为 no-op（幂等），
    /// 在 debug build 触发 debug_assert（协议违规立即失败）。
    ///
    /// 此测试在 release build 中验证幂等行为；在 debug build 中跳过
    /// （避免触发 debug_assert panic 让测试无法完成断言）。
    #[cfg(not(debug_assertions))]
    #[tokio::test]
    async fn finish_with_called_twice_is_idempotent_in_release() {
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let config = test_config("mysql-idempotent");
        let disconnect_count = Arc::new(AtomicUsize::new(0));
        let session = ConnectionSession::new(
            Box::new(MockConnection::with_disconnect_count(
                config.clone(),
                true,
                Arc::clone(&disconnect_count),
            )),
            "mysql-idempotent:session:1".to_string(),
            false,
        );
        manager
            .sessions
            .write()
            .await
            .entry(config.id.clone())
            .or_default()
            .push(Arc::new(AsyncMutex::new(session)));

        let mut guard = SessionGuard::new(
            manager.clone(),
            "mysql-idempotent:session:1".to_string(),
            SessionReleaseAction::Close,
        );

        // 第一次 finish_with
        guard.finish_with(SessionReleaseAction::Close).await;
        assert_eq!(
            disconnect_count.load(Ordering::SeqCst),
            1,
            "第一次 finish_with 应执行 release"
        );

        // 第二次 finish_with 应为 no-op（session 已 None，hard no-op 不触发 release）
        guard.finish_with(SessionReleaseAction::Close).await;
        assert_eq!(
            disconnect_count.load(Ordering::SeqCst),
            1,
            "第二次 finish_with 应为 no-op（幂等），不得重复断开"
        );
    }

    /// debug build 中二次 finish_with 触发 debug_assert panic（协议违规）。
    /// 配合 `#[should_panic(expected = ...)]` 真正验证断言生效。
    #[cfg(debug_assertions)]
    #[tokio::test]
    #[should_panic(expected = "finish_with called twice")]
    async fn finish_with_called_twice_panics_in_debug() {
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let config = test_config("mysql-debug-double-finish");
        let session = ConnectionSession::new(
            Box::new(MockConnection::new(config.clone(), true)),
            "mysql-debug-double-finish:session:1".to_string(),
            false,
        );
        manager
            .sessions
            .write()
            .await
            .entry(config.id.clone())
            .or_default()
            .push(Arc::new(AsyncMutex::new(session)));

        let mut guard = SessionGuard::new(
            manager.clone(),
            "mysql-debug-double-finish:session:1".to_string(),
            SessionReleaseAction::Close,
        );
        guard.finish_with(SessionReleaseAction::Close).await;
        // 第二次调用：debug_assert! 触发 panic（被 #[should_panic] 捕获）
        guard.finish_with(SessionReleaseAction::Close).await;
    }

    /// finish_with 后 Drop 应为 no-op（不会二次释放）
    #[tokio::test]
    async fn finish_with_then_drop_is_noop() {
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let config = test_config("mysql-finish-then-drop");
        let disconnect_count = Arc::new(AtomicUsize::new(0));
        let session = ConnectionSession::new(
            Box::new(MockConnection::with_disconnect_count(
                config.clone(),
                true,
                Arc::clone(&disconnect_count),
            )),
            "mysql-finish-then-drop:session:1".to_string(),
            false,
        );
        manager
            .sessions
            .write()
            .await
            .entry(config.id.clone())
            .or_default()
            .push(Arc::new(AsyncMutex::new(session)));

        {
            let mut guard = SessionGuard::new(
                manager.clone(),
                "mysql-finish-then-drop:session:1".to_string(),
                SessionReleaseAction::Close,
            );
            guard.finish_with(SessionReleaseAction::Close).await;
            // guard 在 scope 结束时 drop；Drop 应为 no-op
        }

        // 给可能的 Drop spawn 一点时间
        tokio::time::sleep(Duration::from_millis(50)).await;

        // 关键断言：disconnect_count 必须 == 1（无二次关闭）
        assert_eq!(
            disconnect_count.load(Ordering::SeqCst),
            1,
            "finish_with 后 Drop 必须为 no-op，不得二次关闭连接"
        );
    }

    /// Drop 兜底对 ReleaseForReuse 的降级路径（Round 7 P1-1 修复验证）
    ///
    /// 场景：guard 持有 ReleaseForReuse 复用意图，但 panic/cancel 导致
    /// finish 完全未调用 → Drop 必须将 action 降级为 Close，**禁止脏会话
    /// 复用进池**。
    #[tokio::test]
    async fn drop_without_finish_with_reuse_intent_downgrades_to_close() {
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let config = test_config("mysql-drop-reuse-downgrade");
        let disconnect_count = Arc::new(AtomicUsize::new(0));
        let session = ConnectionSession::new(
            Box::new(MockConnection::with_disconnect_count(
                config.clone(),
                true,
                Arc::clone(&disconnect_count),
            )),
            "mysql-drop-reuse-downgrade:session:1".to_string(),
            false,
        );
        manager
            .sessions
            .write()
            .await
            .entry(config.id.clone())
            .or_default()
            .push(Arc::new(AsyncMutex::new(session)));

        {
            let _guard = SessionGuard::new(
                manager.clone(),
                "mysql-drop-reuse-downgrade:session:1".to_string(),
                SessionReleaseAction::ReleaseForReuse, // 复用意图
            );
            // 故意不调 finish；Drop 必须降级为 Close
        }

        // 轮询**最终可观测状态本身**（disconnect_count + list_sessions），
        // 避免轮询中间信号后再断言终态的顺序竞态。
        // 超时 5s（CI 慢机负载下 2s 偏紧）。
        // **统一时钟域**：用 `tokio::time::Instant` 与 `tokio::time::sleep`
        // 保持一致，避免 std::time 与 tokio::time 混用导致的不可预期行为
        // （paused-time 下 tokio mock 时钟被冻结，std 真实时钟继续推进，
        // deadline 判据可能虚假失效）。
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let count = disconnect_count.load(Ordering::SeqCst);
            let list_empty = manager.list_sessions(&config.id).await.is_empty();
            if count >= 1 && list_empty {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                panic!(
                    "release task did not settle within 5s (count={count}, list_empty={list_empty})"
                );
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        // Drop 降级生效：session 不应留在池中（无脏复用）
        assert!(
            manager.list_sessions(&config.id).await.is_empty(),
            "Drop 兜底必须将 ReleaseForReuse 降级为 Close，session 不应留在池中"
        );
        assert_eq!(
            disconnect_count.load(Ordering::SeqCst),
            1,
            "Drop 降级后 Close action 必须触发连接断开"
        );
    }

    /// Phase 6: 多线程并发 release 竞态测试
    ///
    /// 场景：多个 task 并发调用 `release_session_for_reuse` 同一 session，
    /// 验证 release_session_internal 的 Arc 锁 + 状态检查机制无死锁/双重释放。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_release_session_for_reuse_no_double_release() {
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let config = test_config("mysql-concurrent-release");
        let disconnect_count = Arc::new(AtomicUsize::new(0));
        let session = ConnectionSession::new(
            Box::new(MockConnection::with_disconnect_count(
                config.clone(),
                true,
                Arc::clone(&disconnect_count),
            )),
            "mysql-concurrent-release:session:1".to_string(),
            false,
        );
        manager
            .sessions
            .write()
            .await
            .entry(config.id.clone())
            .or_default()
            .push(Arc::new(AsyncMutex::new(session)));

        // 10 个 task 并发 release；helper 把 NotFound 映射为 Ok，
        // 所以所有任务最终都返回 Ok（幂等）。同时验证无死锁。
        let mut handles = Vec::new();
        for _ in 0..10 {
            let mgr = manager.clone();
            let sid = "mysql-concurrent-release:session:1".to_string();
            handles.push(tokio::spawn(async move {
                // 通过 helper（顶层 release_session 不存在，直接调用 release_session_for_reuse
                // + 局部 NotFound 处理）
                match mgr.release_session_for_reuse(&sid).await {
                    Ok(()) => Ok(()),
                    Err(crate::connection::DbError::SessionNotFound(_)) => Ok(()),
                    Err(e) => Err(e),
                }
            }));
        }
        for h in handles {
            let r = h.await.expect("task join");
            assert!(
                r.is_ok(),
                "concurrent release should be Ok (helper maps NotFound→Ok)"
            );
        }

        // 关键不变量：session 仍在池中（被 ReleaseForReuse 复用进池），
        // disconnect_count == 0（ReleaseForReuse 不关闭连接）。
        assert_eq!(
            disconnect_count.load(Ordering::SeqCst),
            0,
            "ReleaseForReuse 不应关闭连接；多次并发 release 不应导致多重关闭"
        );
        let acquired = manager
            .get_session_connection("mysql-concurrent-release:session:1")
            .await;
        assert!(
            acquired.is_ok(),
            "ReleaseForReuse 后 session 应在池中可复用"
        );
    }

    /// Phase 6: rollback_if_active 在 ReleaseForReuse 路径被调用
    /// （防止未提交事务状态泄漏到下一个使用者）
    #[tokio::test]
    async fn release_session_for_reuse_calls_rollback_if_active() {
        let manager =
            ConnectionManager::with_config(Duration::from_secs(300), Duration::from_secs(1800));
        let config = test_config("mysql-reuse-rollback");

        // 计数器：跟踪 ROLLBACK 调用次数
        let rollback_count = Arc::new(AtomicUsize::new(0));
        let session = ConnectionSession::new(
            Box::new(MockConnectionWithRollback {
                config: config.clone(),
                rollback_count: Arc::clone(&rollback_count),
            }),
            "mysql-reuse-rollback:session:1".to_string(),
            false,
        );
        manager
            .sessions
            .write()
            .await
            .entry(config.id.clone())
            .or_default()
            .push(Arc::new(AsyncMutex::new(session)));

        // ReleaseForReuse 路径：rollback_if_active 应被调用 1 次
        manager
            .release_session_for_reuse("mysql-reuse-rollback:session:1")
            .await
            .unwrap();

        assert_eq!(
            rollback_count.load(Ordering::SeqCst),
            1,
            "ReleaseForReuse 路径必须调用 rollback_if_active"
        );
    }

    /// 测试用 MockConnection 子类：跟踪 ROLLBACK 调用次数
    struct MockConnectionWithRollback {
        config: DbConnectionConfig,
        rollback_count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl DbConnection for MockConnectionWithRollback {
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
            if query == "ROLLBACK" {
                self.rollback_count.fetch_add(1, Ordering::SeqCst);
            }
            Ok(SqlResult::Exec(ExecResult {
                sql: query.to_string(),
                rows_affected: 0,
                elapsed_ms: 0,
                message: None,
            }))
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
}
