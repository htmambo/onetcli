import fs from "node:fs";
import path from "node:path";
import type Database from "better-sqlite3";

export function runMigrations(db: Database.Database, migrationsPath: string) {
  db.exec(`
    CREATE TABLE IF NOT EXISTS _migrations (
      version TEXT PRIMARY KEY,
      applied_at TEXT NOT NULL
    );
  `);

  const files = fs
    .readdirSync(migrationsPath)
    .filter((file) => file.endsWith(".sql"))
    .sort();

  for (const file of files) {
    const applied = db
      .prepare("SELECT COUNT(*) as count FROM _migrations WHERE version = ?")
      .get(file) as { count: number };

    if (applied.count > 0) {
      continue;
    }

    const sql = fs.readFileSync(path.join(migrationsPath, file), "utf8");
    db.exec(sql);
    db.prepare("INSERT INTO _migrations (version, applied_at) VALUES (?, ?)").run(file, new Date().toISOString());
  }
}
