import type { PublicUser } from "./models.js";

declare module "fastify" {
  interface FastifyRequest {
    authUser: PublicUser | null;
    authTokenHash: string | null;
  }
}

export {};
