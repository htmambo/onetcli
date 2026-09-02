export interface RateLimiterOptions {
  /** 窗口内允许的最大请求数 */
  max: number;
  /** 窗口时长（毫秒） */
  windowMs: number;
}

export interface RateLimitDecision {
  allowed: boolean;
  /** 被限流时，距离窗口重置的秒数（用于 Retry-After 响应头） */
  retryAfterSeconds: number;
}

interface WindowEntry {
  count: number;
  resetAt: number;
}

/** 触发惰性清理的 Map 容量阈值，避免每次检查都全表扫描 */
const SWEEP_THRESHOLD = 1024;

/**
 * 创建进程内固定窗口限流器（按 key 计数，如客户端 IP）。
 *
 * 单实例 SQLite 部署足够；若未来多实例部署，需替换为共享存储实现。
 */
export function createRateLimiter(options: RateLimiterOptions) {
  const hits = new Map<string, WindowEntry>();

  function sweepExpired(now: number) {
    if (hits.size < SWEEP_THRESHOLD) {
      return;
    }
    for (const [key, entry] of hits) {
      if (entry.resetAt <= now) {
        hits.delete(key);
      }
    }
  }

  return {
    /** 记录一次请求并返回是否放行 */
    check(key: string, now: number = Date.now()): RateLimitDecision {
      sweepExpired(now);

      const entry = hits.get(key);
      if (!entry || entry.resetAt <= now) {
        hits.set(key, { count: 1, resetAt: now + options.windowMs });
        return { allowed: true, retryAfterSeconds: 0 };
      }

      entry.count += 1;
      if (entry.count > options.max) {
        return {
          allowed: false,
          retryAfterSeconds: Math.max(1, Math.ceil((entry.resetAt - now) / 1000)),
        };
      }
      return { allowed: true, retryAfterSeconds: 0 };
    },
  };
}
