import type { FastifyReply } from "fastify";

export function sendError(reply: FastifyReply, statusCode: number, message: string) {
  return reply.status(statusCode).send({
    error: {
      message,
    },
  });
}
