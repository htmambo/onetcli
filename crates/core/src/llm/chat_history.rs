use anyhow::Result;
use gpui::SharedString;
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::storage::connection::SqliteConnection;
use crate::storage::now;
use crate::storage::row_mapping::FromSqliteRow;
use crate::storage::traits::Repository;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: i64,
    pub name: String,
    pub provider_id: String,
    pub connection_id: Option<String>,
    pub database_name: Option<String>,
    pub database_type: Option<String>,
    pub schema_name: Option<String>,
    pub title_source: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl FromSqliteRow for ChatSession {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(ChatSession {
            id: row.get("id")?,
            name: row.get("name")?,
            provider_id: row.get("provider_id")?,
            connection_id: row.get("connection_id").ok(),
            database_name: row.get("database_name").ok(),
            database_type: row.get("database_type").ok(),
            schema_name: row.get("schema_name").ok(),
            title_source: row
                .get("title_source")
                .unwrap_or_else(|_| "extracted".to_string()),
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}

impl crate::storage::traits::Entity for ChatSession {
    fn id(&self) -> Option<i64> {
        Some(self.id)
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }

    fn updated_at(&self) -> i64 {
        self.updated_at
    }
}

impl ChatSession {
    pub fn new(name: String, provider_id: String) -> Self {
        let now = now();
        Self {
            id: 0,
            name,
            provider_id,
            connection_id: None,
            database_name: None,
            database_type: None,
            schema_name: None,
            title_source: "extracted".to_string(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_connection_info(
        name: String,
        provider_id: String,
        connection_id: Option<String>,
        database_name: Option<String>,
        database_type: Option<String>,
        schema_name: Option<String>,
    ) -> Self {
        let mut session = Self::new(name, provider_id);
        session.connection_id = connection_id;
        session.database_name = database_name;
        session.database_type = database_type;
        session.schema_name = schema_name;
        session
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: i64,
    pub session_id: i64,
    pub role: String,
    pub content: String,
    pub created_at: i64,
    /// role='tool' 时对应的 tool call id；普通消息为 None。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// role='assistant' 且携带工具调用时，存 Vec<ToolCall> 的 JSON；普通消息为 None。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls_json: Option<String>,
}

impl FromSqliteRow for ChatMessage {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(ChatMessage {
            id: row.get("id")?,
            session_id: row.get("session_id")?,
            role: row.get("role")?,
            content: row.get("content")?,
            created_at: row.get("created_at")?,
            // 既有行（migration 前的数据）这两列为 NULL，get→Option 安全回落
            tool_call_id: row.get("tool_call_id")?,
            tool_calls_json: row.get("tool_calls_json")?,
        })
    }
}

impl crate::storage::traits::Entity for ChatMessage {
    fn id(&self) -> Option<i64> {
        Some(self.id)
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }

    fn updated_at(&self) -> i64 {
        self.created_at
    }
}

impl ChatMessage {
    pub fn new(session_id: i64, role: String, content: String) -> Self {
        Self {
            id: 0,
            session_id,
            role,
            content,
            created_at: now(),
            tool_call_id: None,
            tool_calls_json: None,
        }
    }

    pub fn user(session_id: i64, content: String) -> Self {
        Self::new(session_id, "user".to_string(), content)
    }

    pub fn assistant(session_id: i64, content: String) -> Self {
        Self::new(session_id, "assistant".to_string(), content)
    }

    pub fn system(session_id: i64, content: String) -> Self {
        Self::new(session_id, "system".to_string(), content)
    }

    /// assistant 携带工具调用的消息：content 为文本部分，tool_calls_json 为
    /// `serde_json::to_string(&Vec<ToolCall>)` 的结果。
    pub fn assistant_tool_calls(session_id: i64, content: String, tool_calls_json: String) -> Self {
        Self {
            id: 0,
            session_id,
            role: "assistant".to_string(),
            content,
            created_at: now(),
            tool_call_id: None,
            tool_calls_json: Some(tool_calls_json),
        }
    }

    /// tool 结果消息：tool_call_id 对应 assistant 发起的 call id，content 为结果文本。
    pub fn tool_result(session_id: i64, tool_call_id: String, content: String) -> Self {
        Self {
            id: 0,
            session_id,
            role: "tool".to_string(),
            content,
            created_at: now(),
            tool_call_id: Some(tool_call_id),
            tool_calls_json: None,
        }
    }
}

#[derive(Clone)]
pub struct SessionRepository {
    conn: SqliteConnection,
}

impl SessionRepository {
    pub fn new(conn: SqliteConnection) -> Self {
        Self { conn }
    }
}

impl Repository for SessionRepository {
    type Entity = ChatSession;

    fn entity_type(&self) -> SharedString {
        SharedString::from("ChatSession")
    }

    fn insert(&self, item: &mut Self::Entity) -> Result<i64> {
        let name = item.name.clone();
        let provider_id = item.provider_id.clone();
        let connection_id = item.connection_id.clone();
        let database_name = item.database_name.clone();
        let database_type = item.database_type.clone();
        let schema_name = item.schema_name.clone();
        let title_source = item.title_source.clone();
        let created_at = item.created_at;
        let updated_at = item.updated_at;

        let id = self.conn.with_connection(|conn| {
            conn.execute(
                "INSERT INTO chat_sessions (name, provider_id, connection_id, database_name, database_type, schema_name, title_source, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![name, provider_id, connection_id, database_name, database_type, schema_name, title_source, created_at, updated_at],
            )?;
            Ok(conn.last_insert_rowid())
        })?;

        item.id = id;
        Ok(id)
    }

    fn update(&self, item: &Self::Entity) -> Result<()> {
        let id = item.id;
        let name = item.name.clone();
        let provider_id = item.provider_id.clone();
        let connection_id = item.connection_id.clone();
        let database_name = item.database_name.clone();
        let database_type = item.database_type.clone();
        let schema_name = item.schema_name.clone();
        let title_source = item.title_source.clone();
        let updated_at = now();

        self.conn.with_connection(|conn| {
            conn.execute(
                "UPDATE chat_sessions SET name = ?1, provider_id = ?2, connection_id = ?3, database_name = ?4, database_type = ?5, schema_name = ?6, title_source = ?7, updated_at = ?8 WHERE id = ?9",
                params![name, provider_id, connection_id, database_name, database_type, schema_name, title_source, updated_at, id],
            )?;
            Ok(())
        })
    }

    fn delete(&self, id: i64) -> Result<()> {
        self.conn.with_connection(|conn| {
            conn.execute("DELETE FROM chat_sessions WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    fn get(&self, id: i64) -> Result<Option<Self::Entity>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare("SELECT id, name, provider_id, connection_id, database_name, database_type, schema_name, title_source, created_at, updated_at FROM chat_sessions WHERE id = ?1")?;
            let mut rows = stmt.query(params![id])?;
            if let Some(row) = rows.next()? {
                Ok(Some(ChatSession::from_row(row)?))
            } else {
                Ok(None)
            }
        })
    }

    fn list(&self) -> Result<Vec<Self::Entity>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare("SELECT id, name, provider_id, connection_id, database_name, database_type, schema_name, title_source, created_at, updated_at FROM chat_sessions ORDER BY updated_at DESC")?;
            let rows = stmt.query_map([], |row| ChatSession::from_row(row))?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row?);
            }
            Ok(results)
        })
    }

    fn count(&self) -> Result<i64> {
        self.conn.with_connection(|conn| {
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM chat_sessions", [], |row| row.get(0))?;
            Ok(count)
        })
    }

    fn exists(&self, id: i64) -> Result<bool> {
        self.conn.with_connection(|conn| {
            let exists: i64 = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM chat_sessions WHERE id = ?1)",
                params![id],
                |row| row.get(0),
            )?;
            Ok(exists == 1)
        })
    }
}

impl SessionRepository {
    pub fn list_by_provider(&self, provider_id: &str) -> Result<Vec<ChatSession>> {
        let provider_id = provider_id.to_string();
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare("SELECT id, name, provider_id, connection_id, database_name, database_type, schema_name, title_source, created_at, updated_at FROM chat_sessions WHERE provider_id = ?1 ORDER BY updated_at DESC")?;
            let rows = stmt.query_map(params![provider_id], |row| ChatSession::from_row(row))?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row?);
            }
            Ok(results)
        })
    }
}

#[derive(Clone)]
pub struct MessageRepository {
    conn: SqliteConnection,
}

impl MessageRepository {
    pub fn new(conn: SqliteConnection) -> Self {
        Self { conn }
    }
}

impl Repository for MessageRepository {
    type Entity = ChatMessage;

    fn entity_type(&self) -> SharedString {
        SharedString::from("ChatMessage")
    }

    fn insert(&self, item: &mut Self::Entity) -> Result<i64> {
        let session_id = item.session_id;
        let role = item.role.clone();
        let content = item.content.clone();
        let created_at = item.created_at;
        let tool_call_id = item.tool_call_id.clone();
        let tool_calls_json = item.tool_calls_json.clone();

        let id = self.conn.with_connection(|conn| {
            conn.execute(
                "INSERT INTO chat_messages (session_id, role, content, created_at, tool_call_id, tool_calls_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![session_id, role, content, created_at, tool_call_id, tool_calls_json],
            )?;
            Ok(conn.last_insert_rowid())
        })?;

        item.id = id;
        Ok(id)
    }

    fn update(&self, item: &Self::Entity) -> Result<()> {
        let id = item.id;
        let session_id = item.session_id;
        let role = item.role.clone();
        let content = item.content.clone();
        let tool_call_id = item.tool_call_id.clone();
        let tool_calls_json = item.tool_calls_json.clone();

        self.conn.with_connection(|conn| {
            conn.execute(
                "UPDATE chat_messages SET session_id = ?1, role = ?2, content = ?3, tool_call_id = ?4, tool_calls_json = ?5 WHERE id = ?6",
                params![session_id, role, content, tool_call_id, tool_calls_json, id],
            )?;
            Ok(())
        })
    }

    fn delete(&self, id: i64) -> Result<()> {
        self.conn.with_connection(|conn| {
            conn.execute("DELETE FROM chat_messages WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    fn get(&self, id: i64) -> Result<Option<Self::Entity>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, role, content, created_at, tool_call_id, tool_calls_json FROM chat_messages WHERE id = ?1",
            )?;
            let mut rows = stmt.query(params![id])?;
            if let Some(row) = rows.next()? {
                Ok(Some(ChatMessage::from_row(row)?))
            } else {
                Ok(None)
            }
        })
    }

    fn list(&self) -> Result<Vec<Self::Entity>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare("SELECT id, session_id, role, content, created_at, tool_call_id, tool_calls_json FROM chat_messages ORDER BY created_at ASC")?;
            let rows = stmt.query_map([], |row| ChatMessage::from_row(row))?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row?);
            }
            Ok(results)
        })
    }

    fn count(&self) -> Result<i64> {
        self.conn.with_connection(|conn| {
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM chat_messages", [], |row| row.get(0))?;
            Ok(count)
        })
    }

    fn exists(&self, id: i64) -> Result<bool> {
        self.conn.with_connection(|conn| {
            let exists: i64 = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM chat_messages WHERE id = ?1)",
                params![id],
                |row| row.get(0),
            )?;
            Ok(exists == 1)
        })
    }
}

impl MessageRepository {
    pub fn list_by_session(&self, session_id: i64) -> Result<Vec<ChatMessage>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare("SELECT id, session_id, role, content, created_at, tool_call_id, tool_calls_json FROM chat_messages WHERE session_id = ?1 ORDER BY created_at ASC, id ASC")?;
            let rows = stmt.query_map(params![session_id], |row| ChatMessage::from_row(row))?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row?);
            }
            Ok(results)
        })
    }

    pub fn list_recent(&self, limit: i32) -> Result<Vec<ChatMessage>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare("SELECT id, session_id, role, content, created_at, tool_call_id, tool_calls_json FROM chat_messages ORDER BY created_at DESC, id DESC LIMIT ?1")?;
            let rows = stmt.query_map(params![limit], |row| ChatMessage::from_row(row))?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row?);
            }
            Ok(results)
        })
    }

    pub fn delete_by_session(&self, session_id: i64) -> Result<()> {
        self.conn.with_connection(|conn| {
            conn.execute(
                "DELETE FROM chat_messages WHERE session_id = ?1",
                params![session_id],
            )?;
            Ok(())
        })
    }
}
