import dotenv from "dotenv";
import path from "node:path";
import { fileURLToPath } from "node:url";

const currentDir = path.dirname(fileURLToPath(import.meta.url));
const serverRoot = path.resolve(currentDir, "..", "..");
const projectRoot = path.resolve(serverRoot, "..");
const runtimeDirName = path.basename(path.resolve(currentDir, ".."));

dotenv.config({
  path: path.resolve(projectRoot, ".env"),
});

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

function getOrigin(url: string): string {
  return new URL(url).origin;
}

const port = Number(getEnv("SYNC_SERVER_PORT", "8787"));
const webDevUrl = getEnv("SYNC_SERVER_WEB_DEV_URL", "http://localhost:5173");

export const env = {
  serverRoot,
  projectRoot,
  isSourceRuntime: runtimeDirName === "src",
  host: getEnv("SYNC_SERVER_HOST", "0.0.0.0"),
  port,
  webOrigin: getEnv("SYNC_SERVER_WEB_ORIGIN", `http://localhost:${port}`),
  webDevUrl,
  webDevOrigin: getOrigin(webDevUrl),
  dbPath: resolveProjectPath(getEnv("SYNC_SERVER_DB_PATH", "./data/sync_server.db")),
  sessionTtlHours: Number(getEnv("SYNC_SERVER_SESSION_TTL_HOURS", "168")),
  adminEmail: getOptionalEnv("SYNC_SERVER_ADMIN_EMAIL"),
  adminPassword: getOptionalEnv("SYNC_SERVER_ADMIN_PASSWORD"),
  webRootPath: path.resolve(projectRoot, "web"),
  webDistPath: path.resolve(projectRoot, "web", "dist"),
  webViteConfigPath: path.resolve(projectRoot, "web", "vite.config.ts"),
  migrationsPath: path.resolve(serverRoot, "migrations"),
};
