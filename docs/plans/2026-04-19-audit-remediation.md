# Audit Remediation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Land a focused batch of fixes for the audit findings covering SSH host key verification, SSH session reuse correctness, SSH config de-duplication, and DB import/export cleanup.

**Architecture:** Reuse `russh` known-host utilities for TOFU-based host verification, centralize SSH config conversion in `one-core`, fix the session manager’s cold-start race by serializing connector creation, and add shared DB import/export helpers so transaction and schema handling are consistent across formats.

**Tech Stack:** Rust, russh, ssh, sftp, one-core, db

---

### Task 1: Add SSH host key verification helpers

**Files:**
- Modify: `crates/ssh/src/ssh.rs`
- Modify: `crates/ssh/src/lib.rs`
- Modify: `crates/sftp/src/russh_impl.rs`

- [ ] Add a reusable host key verification helper backed by `known_hosts`
- [ ] Make SSH terminal and SFTP handlers both use it
- [ ] Add unit tests for first-use learning and key-change rejection

### Task 2: Fix shared SSH session cold-start race

**Files:**
- Modify: `crates/ssh/src/session_manager.rs`

- [ ] Serialize connector creation so concurrent `client()` calls reuse one connection
- [ ] Keep existing reuse/invalidate/disconnect behavior intact
- [ ] Add a concurrent access regression test

### Task 3: Centralize SSH config construction

**Files:**
- Modify: `crates/core/Cargo.toml`
- Modify: `crates/core/src/storage/models.rs`
- Modify: `crates/terminal/src/terminal.rs`
- Modify: `crates/sftp_view/src/lib.rs`
- Modify: `crates/terminal_view/src/ssh_form_window.rs`

- [ ] Add conversion helpers on `SshParams`
- [ ] Replace duplicated builders in terminal, SFTP view, and SSH form
- [ ] Keep call sites focused on their own UI/business logic

### Task 4: Make DB import transaction and export SQL helpers real

**Files:**
- Modify: `crates/db/src/import_export/formats/mod.rs`
- Modify: `crates/db/src/import_export/formats/csv.rs`
- Modify: `crates/db/src/import_export/formats/json.rs`
- Modify: `crates/db/src/import_export/formats/txt.rs`
- Modify: `crates/db/src/import_export/formats/xml.rs`
- Modify: `crates/db/src/import_export/formats/sql.rs`

- [ ] Add shared helpers for export `SELECT` SQL construction
- [ ] Make helper include `schema` and `limit` consistently
- [ ] Batch CSV/JSON/TXT import statements through `connection.execute()` so `use_transaction` is honored
- [ ] Reuse helper logic to reduce duplicated SQL string assembly

### Task 5: Verify targeted regressions

- [ ] Run `cargo test -p ssh`
- [ ] Run `cargo test -p db import_export`
- [ ] Run `cargo check -p terminal -p sftp_view -p terminal_view -p db`

用户已要求“制作实施方案后全自动实施落地”，因此本计划保存后直接在当前会话执行，不等待额外选择。
