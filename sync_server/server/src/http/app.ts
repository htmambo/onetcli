import cors from "@fastify/cors";
import fastifyStatic from "@fastify/static";
import Fastify from "fastify";
import type { FastifyInstance, FastifyReply, FastifyRequest } from "fastify";
import fs from "node:fs";
import type { ViteDevServer } from "vite";
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

function isBackendRequest(url: string): boolean {
  return url.startsWith("/api/") || url === "/health";
}

function registerDevWebFallbackRoutes(
  app: FastifyInstance,
  handler: (request: FastifyRequest, reply: FastifyReply) => Promise<unknown>,
) {
  app.get("/", handler);
  app.get("/*", handler);
}

async function handleViteMiddleware(
  app: FastifyInstance,
  vite: ViteDevServer,
  request: FastifyRequest,
  reply: FastifyReply,
) {
  const url = request.raw.url ?? "/";
  if (isBackendRequest(url)) {
    return reply.callNotFound();
  }

  reply.hijack();

  try {
    await new Promise<void>((resolve, reject) => {
      vite.middlewares(request.raw, reply.raw, (error?: Error) => {
        if (error) {
          reject(error);
          return;
        }

        resolve();
      });
    });
  } catch (error) {
    const viteError = error instanceof Error ? error : new Error(String(error));
    vite.ssrFixStacktrace(viteError);
    app.log.error(viteError);

    if (!reply.raw.headersSent) {
      reply.raw.statusCode = 500;
      reply.raw.setHeader("content-type", "text/plain; charset=utf-8");
    }

    if (!reply.raw.writableEnded) {
      reply.raw.end("前端开发中间件处理请求失败");
    }
  }
}

async function registerViteWebMiddleware(app: FastifyInstance) {
  const { createServer } = await import("vite");
  const vite = await createServer({
    root: env.webRootPath,
    configFile: env.webViteConfigPath,
    appType: "spa",
    clearScreen: false,
    server: {
      middlewareMode: {
        server: app.server,
      },
      hmr: {
        server: app.server,
      },
    },
  });

  app.log.info("已启用单端口 Vite 开发中间件");

  registerDevWebFallbackRoutes(app, async (request, reply) =>
    handleViteMiddleware(app, vite, request, reply),
  );

  app.addHook("onClose", async () => {
    await vite.close();
  });
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
    origin: Array.from(new Set([env.webOrigin, env.webDevOrigin])),
    credentials: false,
  });

  app.register(authPlugin);
  registerHealthRoutes(app);
  registerAuthRoutes(app);
  registerSyncRoutes(app);
  registerAdminRoutes(app);

  if (env.isSourceRuntime || !fs.existsSync(env.webDistPath)) {
    app.register(async (instance) => {
      await registerViteWebMiddleware(instance);
    });
  } else {
    app.register(fastifyStatic, {
      root: env.webDistPath,
      prefix: "/",
      wildcard: false,
    });

    app.get("/*", async (request, reply) => {
      const url = request.raw.url ?? "/";
      if (isBackendRequest(url)) {
        return reply.callNotFound();
      }

      return reply.sendFile("index.html");
    });
  }

  app.addHook("onClose", async () => {
    database.close();
  });

  return app;
}
