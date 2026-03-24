import dotenv from "dotenv";
import path from "node:path";
import { fileURLToPath } from "node:url";

dotenv.config({
  path: path.resolve(process.cwd(), ".env"),
});

const currentDir = path.dirname(fileURLToPath(import.meta.url));
const serverRoot = path.resolve(currentDir, "..", "..");
const projectRoot = path.resolve(serverRoot, "..");

function getEnv(name: string, fallback: string): string {
  const value = process.env[name];
  if (!value || value.trim() === "") {
    return fallback;
  }
  return value;
}

function getOptionalEnv(name: string): string | undefined {
  const value = process.env[name];
  if (!value || value.trim() === "") {
    return undefined;
  }
  return value;
}

function resolveProjectPath(input: string): string {
  if (path.isAbsolute(input)) {
    return input;
  }

  return path.resolve(projectRoot, input);
}

export const env = {
  serverRoot,
  projectRoot,
  host: getEnv("SYNC_SERVER_HOST", "0.0.0.0"),
  port: Number(getEnv("SYNC_SERVER_PORT", "8787")),
  webOrigin: getEnv("SYNC_SERVER_WEB_ORIGIN", "http://localhost:5173"),
  webDevUrl: getEnv("SYNC_SERVER_WEB_DEV_URL", "http://127.0.0.1:5173"),
  dbPath: resolveProjectPath(getEnv("SYNC_SERVER_DB_PATH", "./data/sync_server.db")),
  sessionTtlHours: Number(getEnv("SYNC_SERVER_SESSION_TTL_HOURS", "168")),
  adminEmail: getOptionalEnv("SYNC_SERVER_ADMIN_EMAIL"),
  adminPassword: getOptionalEnv("SYNC_SERVER_ADMIN_PASSWORD"),
  webDistPath: path.resolve(projectRoot, "web", "dist"),
  migrationsPath: path.resolve(serverRoot, "migrations"),
};
