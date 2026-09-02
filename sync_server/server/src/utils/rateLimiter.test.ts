import assert from "node:assert/strict";
import { test } from "node:test";
import { createRateLimiter } from "./rateLimiter.js";

test("allows up to max requests within window", () => {
  const limiter = createRateLimiter({ max: 3, windowMs: 1000 });
  for (let i = 0; i < 3; i++) {
    assert.equal(limiter.check("ip-1", 0).allowed, true);
  }
  const blocked = limiter.check("ip-1", 1);
  assert.equal(blocked.allowed, false);
  assert.ok(blocked.retryAfterSeconds >= 1);
});

test("resets after window expires", () => {
  const limiter = createRateLimiter({ max: 2, windowMs: 1000 });
  limiter.check("ip-1", 0);
  limiter.check("ip-1", 0);
  assert.equal(limiter.check("ip-1", 0).allowed, false);
  assert.equal(limiter.check("ip-1", 1001).allowed, true);
});

test("keys are isolated", () => {
  const limiter = createRateLimiter({ max: 1, windowMs: 1000 });
  assert.equal(limiter.check("ip-1", 0).allowed, true);
  assert.equal(limiter.check("ip-1", 0).allowed, false);
  assert.equal(limiter.check("ip-2", 0).allowed, true);
});

test("sweeps expired entries beyond threshold", () => {
  const limiter = createRateLimiter({ max: 1, windowMs: 1000 });
  for (let i = 0; i < 1100; i++) {
    limiter.check(`ip-${i}`, 0);
  }
  // 超过清理阈值后仍正常工作，新 key 可用
  assert.equal(limiter.check("fresh", 2000).allowed, true);
});
