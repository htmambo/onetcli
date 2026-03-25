import type { FastifyInstance } from "fastify";
import { z } from "zod";
import { requireAuth } from "../plugins/auth.js";
import type { SyncDataRecord } from "../../types/models.js";
import { sendError } from "../../utils/http.js";
import { parseSince } from "../../utils/time.js";

const configSchema = z.object({
  keyVerification: z.string().min(1),
  keyVersion: z.number().int().min(1),
});

const createSyncItemSchema = z.object({
  id: z.string().uuid().optional(),
  dataType: z.string().min(1).max(50),
  encryptedData: z.string().min(1),
  keyVersion: z.number().int().min(1),
  checksum: z.string().default(""),
});

const updateSyncItemSchema = z.object({
  encryptedData: z.string().min(1),
  keyVersion: z.number().int().min(1),
  checksum: z.string().default(""),
  version: z.number().int().min(1),
  deletedAt: z.string().datetime().nullable().optional(),
});

function toSyncItemResponse(item: SyncDataRecord) {
  return {
    id: item.id,
    ownerId: item.owner_id,
    dataType: item.data_type,
    encryptedData: item.encrypted_data,
    keyVersion: item.key_version,
    checksum: item.checksum,
    version: item.version,
    createdAt: item.created_at,
    updatedAt: item.updated_at,
    deletedAt: item.deleted_at,
  };
}

export async function registerSyncRoutes(app: FastifyInstance) {
  app.get("/api/v1/sync/config", { preHandler: requireAuth }, async (request) => {
    const config = app.database.getUserConfig(request.authUser!.id);
    return {
      data: config
        ? {
            keyVerification: config.key_verification,
            keyVersion: config.key_version,
            updatedAt: config.updated_at,
          }
        : null,
    };
  });

  app.put("/api/v1/sync/config", { preHandler: requireAuth }, async (request, reply) => {
    const parsed = configSchema.safeParse(request.body);
    if (!parsed.success) {
      return sendError(reply, 400, parsed.error.issues[0]?.message ?? "请求参数错误");
    }

    const config = app.database.upsertUserConfig(
      request.authUser!.id,
      parsed.data.keyVerification,
      parsed.data.keyVersion,
    );

    return {
      data: {
        keyVerification: config.key_verification,
        keyVersion: config.key_version,
        updatedAt: config.updated_at,
      },
    };
  });

  app.get("/api/v1/sync/items", { preHandler: requireAuth }, async (request) => {
    const query = request.query as Record<string, string | undefined>;
    const items = app.database.listSyncItems({
      ownerId: request.authUser!.id,
      dataType: query.dataType,
      since: parseSince(query.since),
      includeDeleted: query.includeDeleted !== "false",
    });

    return {
      data: items.map(toSyncItemResponse),
    };
  });

  app.get("/api/v1/sync/items/:id", { preHandler: requireAuth }, async (request, reply) => {
    const params = request.params as { id: string };
    const item = app.database.getSyncItem(request.authUser!.id, params.id);

    if (!item) {
      return sendError(reply, 404, "同步项不存在");
    }

    return {
      data: toSyncItemResponse(item),
    };
  });

  app.post("/api/v1/sync/items", { preHandler: requireAuth }, async (request, reply) => {
    const parsed = createSyncItemSchema.safeParse(request.body);
    if (!parsed.success) {
      return sendError(reply, 400, parsed.error.issues[0]?.message ?? "请求参数错误");
    }

    try {
      const item = app.database.createSyncItem({
        id: parsed.data.id,
        ownerId: request.authUser!.id,
        dataType: parsed.data.dataType,
        encryptedData: parsed.data.encryptedData,
        keyVersion: parsed.data.keyVersion,
        checksum: parsed.data.checksum,
      });

      return reply.status(201).send({
        data: toSyncItemResponse(item),
      });
    } catch (error) {
      return sendError(reply, 400, error instanceof Error ? error.message : "创建同步项失败");
    }
  });

  app.put("/api/v1/sync/items/:id", { preHandler: requireAuth }, async (request, reply) => {
    const parsed = updateSyncItemSchema.safeParse(request.body);
    if (!parsed.success) {
      return sendError(reply, 400, parsed.error.issues[0]?.message ?? "请求参数错误");
    }

    const params = request.params as { id: string };
    const item = app.database.updateSyncItem({
      id: params.id,
      ownerId: request.authUser!.id,
      encryptedData: parsed.data.encryptedData,
      keyVersion: parsed.data.keyVersion,
      checksum: parsed.data.checksum,
      version: parsed.data.version,
      deletedAt: parsed.data.deletedAt,
    });

    if (!item) {
      return sendError(reply, 409, "版本冲突或同步项不存在");
    }

    return {
      data: toSyncItemResponse(item),
    };
  });

  app.delete("/api/v1/sync/items/:id", { preHandler: requireAuth }, async (request, reply) => {
    const params = request.params as { id: string };
    const query = request.query as { version?: string };
    const version =
      query.version && /^\d+$/.test(query.version) ? Number(query.version) : undefined;

    const item = app.database.softDeleteSyncItem(request.authUser!.id, params.id, version);
    if (!item) {
      return sendError(reply, 409, "版本冲突或同步项不存在");
    }

    return {
      data: {
        success: true,
        item: {
          id: item.id,
          version: item.version,
          deletedAt: item.deleted_at,
          updatedAt: item.updated_at,
        },
      },
    };
  });

  app.delete("/api/v1/sync/items", { preHandler: requireAuth }, async (request) => {
    app.database.clearSyncData(request.authUser!.id);
    app.database.clearUserConfig(request.authUser!.id);

    return {
      data: {
        success: true,
      },
    };
  });
}
