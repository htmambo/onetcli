import { env } from "./config/env.js";
import { createApp } from "./http/app.js";
import { hashPassword } from "./utils/crypto.js";

async function main() {
  const app = createApp();

  if (env.adminEmail && env.adminPassword) {
    app.database.ensureAdmin(env.adminEmail, hashPassword(env.adminPassword));
    app.log.info(`管理员账号已就绪: ${env.adminEmail}`);
  }

  await app.listen({
    host: env.host,
    port: env.port,
  });

  app.log.info(`sync_server 已启动: http://${env.host}:${env.port}`);
}

main().catch((error) => {
  console.error("sync_server 启动失败", error);
  process.exit(1);
});
