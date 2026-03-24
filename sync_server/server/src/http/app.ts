import cors from "@fastify/cors";
import fastifyHttpProxy from "@fastify/http-proxy";
import fastifyStatic from "@fastify/static";
import Fastify from "fastify";
import fs from "node:fs";
import { env } from "../config/env.js";
import { DatabaseClient } from "../db/database.js";
import { AuthService } from "../services/auth.js";
import { authPlugin } from "./plugins/auth.js";
import { registerAdminRoutes } from "./routes/admin.js";
import { registerAuthRoutes } from "./routes/auth.js";
import { registerHealthRoutes } from "./routes/health.js";
import { registerSyncRoutes } from "./routes/sync.js";

declare module "fastify" {
  interface FastifyInstance {
    database: DatabaseClient;
    authService: AuthService;
  }
}

export function createApp() {
  const app = Fastify({
    logger: true,
  });

  const database = new DatabaseClient(env.dbPath, env.migrationsPath);
  const authService = new AuthService(database, env.sessionTtlHours);

  app.decorate("database", database);
  app.decorate("authService", authService);

  app.register(cors, {
    origin: [env.webOrigin],
    credentials: false,
  });

  app.register(authPlugin);
  registerHealthRoutes(app);
  registerAuthRoutes(app);
  registerSyncRoutes(app);
  registerAdminRoutes(app);

  if (fs.existsSync(env.webDistPath)) {
    app.register(fastifyStatic, {
      root: env.webDistPath,
      prefix: "/",
      wildcard: false,
    });

    app.get("/*", async (request, reply) => {
      const url = request.raw.url ?? "/";
      if (url.startsWith("/api/") || url === "/health") {
        return reply.callNotFound();
      }

      return reply.sendFile("index.html");
    });
  } else {
    app.register(fastifyHttpProxy, {
      upstream: env.webDevUrl,
      prefix: "/",
      rewritePrefix: "/",
      websocket: true,
      http2: false,
      replyOptions: {
        rewriteRequestHeaders: (request, headers) => ({
          ...headers,
          host: new URL(env.webDevUrl).host,
        }),
      },
      preHandler(request, reply, done) {
        const url = request.raw.url ?? "/";
        if (url.startsWith("/api/") || url === "/health") {
          done();
          return;
        }

        done();
      },
    });
  }

  app.addHook("onClose", async () => {
    database.close();
  });

  return app;
}
