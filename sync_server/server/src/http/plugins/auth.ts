import fp from "fastify-plugin";
import type { FastifyInstance, FastifyReply, FastifyRequest } from "fastify";
import { sendError } from "../../utils/http.js";

export async function requireAuth(request: FastifyRequest, reply: FastifyReply) {
  if (!request.authUser) {
    return sendError(reply, 401, "未登录");
  }
}

export async function requireAdmin(request: FastifyRequest, reply: FastifyReply) {
  if (!request.authUser) {
    return sendError(reply, 401, "未登录");
  }

  if (request.authUser.role !== "admin") {
    return sendError(reply, 403, "无管理员权限");
  }
}

export const authPlugin = fp(async function authPlugin(app: FastifyInstance) {
  app.decorateRequest("authUser", null);
  app.decorateRequest("authTokenHash", null);

  app.addHook("preHandler", async (request) => {
    request.authUser = null;
    request.authTokenHash = null;

    const header = request.headers.authorization;
    if (!header || !header.startsWith("Bearer ")) {
      return;
    }

    const token = header.slice("Bearer ".length).trim();
    if (!token) {
      return;
    }

    const result = app.authService.authenticateBearerToken(token);
    if (!result) {
      return;
    }

    request.authUser = result.user;
    request.authTokenHash = result.tokenHash;
  });
});
