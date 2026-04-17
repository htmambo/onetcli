import type { FastifyInstance } from "fastify";
import { z } from "zod";
import { requireAuth } from "../plugins/auth.js";
import { sendError } from "../../utils/http.js";

const registerSchema = z.object({
  email: z.string().email(),
  password: z.string().min(8).max(128),
});

const loginSchema = registerSchema;

const changePasswordSchema = z.object({
  currentPassword: z.string().min(8).max(128),
  nextPassword: z.string().min(8).max(128),
});

const updateProfileSchema = z.object({
  nickname: z.string().trim().min(1).max(255),
});

const refreshSchema = z.object({
  refreshToken: z.string().min(1),
});

export async function registerAuthRoutes(app: FastifyInstance) {
  app.post("/api/v1/auth/register", async (request, reply) => {
    const parsed = registerSchema.safeParse(request.body);
    if (!parsed.success) {
      return sendError(reply, 400, parsed.error.issues[0]?.message ?? "请求参数错误");
    }

    try {
      const result = app.authService.register(parsed.data.email, parsed.data.password);
      return {
        data: result,
      };
    } catch (error) {
      return sendError(reply, 400, error instanceof Error ? error.message : "注册失败");
    }
  });

  app.post("/api/v1/auth/login", async (request, reply) => {
    const parsed = loginSchema.safeParse(request.body);
    if (!parsed.success) {
      return sendError(reply, 400, parsed.error.issues[0]?.message ?? "请求参数错误");
    }

    try {
      const result = app.authService.login(parsed.data.email, parsed.data.password);
      return {
        data: result,
      };
    } catch (error) {
      return sendError(reply, 400, error instanceof Error ? error.message : "登录失败");
    }
  });

  app.get("/api/v1/auth/me", { preHandler: requireAuth }, async (request) => {
    return {
      data: request.authUser,
    };
  });

  app.post("/api/v1/auth/refresh", async (request, reply) => {
    const parsed = refreshSchema.safeParse(request.body);
    if (!parsed.success) {
      return sendError(reply, 400, parsed.error.issues[0]?.message ?? "请求参数错误");
    }

    try {
      const result = app.authService.refresh(parsed.data.refreshToken);
      return {
        data: result,
      };
    } catch (error) {
      return sendError(reply, 401, error instanceof Error ? error.message : "刷新会话失败");
    }
  });

  app.post("/api/v1/auth/logout", { preHandler: requireAuth }, async (request) => {
    if (request.authTokenHash) {
      app.authService.revokeSession(request.authTokenHash);
    }

    return {
      data: {
        success: true,
      },
    };
  });

  app.post("/api/v1/auth/change-password", { preHandler: requireAuth }, async (request, reply) => {
    const parsed = changePasswordSchema.safeParse(request.body);
    if (!parsed.success) {
      return sendError(reply, 400, parsed.error.issues[0]?.message ?? "请求参数错误");
    }

    try {
      app.authService.changePassword(request.authUser!.id, parsed.data.currentPassword, parsed.data.nextPassword);
      return {
        data: {
          success: true,
        },
      };
    } catch (error) {
      return sendError(reply, 400, error instanceof Error ? error.message : "修改密码失败");
    }
  });

  app.patch("/api/v1/auth/profile", { preHandler: requireAuth }, async (request, reply) => {
    const parsed = updateProfileSchema.safeParse(request.body);
    if (!parsed.success) {
      return sendError(reply, 400, parsed.error.issues[0]?.message ?? "请求参数错误");
    }

    try {
      return {
        data: app.authService.updateProfile(request.authUser!.id, parsed.data.nickname),
      };
    } catch (error) {
      return sendError(reply, 400, error instanceof Error ? error.message : "更新资料失败");
    }
  });
}
