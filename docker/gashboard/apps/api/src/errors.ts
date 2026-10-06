import type { Request, Response, NextFunction } from "express";
import { logger } from "./logger.js";

export class AppError extends Error {
  readonly status: number;
  readonly code: string;
  constructor(status: number, code: string, message: string) {
    super(message);
    this.status = status;
    this.code = code;
  }
}

export const unauthorized = (code = "unauthorized") =>
  new AppError(401, code, "Authentication required.");
export const forbidden = (code = "forbidden") =>
  new AppError(403, code, "Access denied.");
export const badRequest = (code = "bad_request", message = "Invalid request.") =>
  new AppError(400, code, message);
export const upstreamUnavailable = () =>
  new AppError(503, "upstream_unavailable", "Upstream temporarily unavailable.");

export function errorHandler(err: unknown, _req: Request, res: Response, _next: NextFunction) {
  if (res.headersSent) return;
  if (err instanceof AppError) {
    res.status(err.status).json({ error: { code: err.code, message: err.message } });
    return;
  }
  logger.error({ err }, "unhandled_error");
  res.status(500).json({ error: { code: "internal_error", message: "Something went wrong." } });
}
