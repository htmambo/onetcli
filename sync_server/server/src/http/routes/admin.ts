import type { FastifyInstance } from "fastify";
import { z } from "zod";
import { requireAdmin } from "../plugins/auth.js";
import { sendError } from "../../utils/http.js";

const updateUserSchema = z
  .object({
    role: z.enum(["admin", "user"]).optional(),
    status: z.enum(["active", "disabled"]).optional(),
  })
  .refine((value) => value.role !== undefined || value.status !== undefined, {
    message: "至少提供一个更新字段",
  });

export async function registerAdminRoutes(app: FastifyInstance) {
  app.get("/api/v1/admin/overview", { preHandler: requireAdmin }, async () => {
    return {
      data: app.database.countOverview(),
    };
  });

  app.get("/api/v1/admin/users", { preHandler: requireAdmin }, async () => {
    return {
      data: app.database.listUsers(),
    };
  });

  app.patch("/api/v1/admin/users/:id", { preHandler: requireAdmin }, async (request, reply) => {
    const parsed = updateUserSchema.safeParse(request.body);
    if (!parsed.success) {
      return sendError(reply, 400, parsed.error.issues[0]?.message ?? "请求参数错误");
    }

    const params = request.params as { id: string };
    const user = app.database.updateUser(params.id, parsed.data);
    if (!user) {
      return sendError(reply, 404, "用户不存在");
    }

    return {
      data: user,
    };
  });

  app.delete("/api/v1/admin/users/:id/data", { preHandler: requireAdmin }, async (request, reply) => {
    const params = request.params as { id: string };
    const user = app.database.findUserById(params.id);
    if (!user) {
      return sendError(reply, 404, "用户不存在");
    }

    app.database.purgeUserData(params.id);

    return {
      data: {
        success: true,
      },
    };
  });
}
