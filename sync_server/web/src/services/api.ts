import type { AdminOverview, AdminUserSummary, AuthPayload, PublicUser, SyncConfig, SyncItem } from "@/types/api";

const API_BASE = import.meta.env.VITE_API_BASE_URL ?? "";

export class ApiError extends Error {
  constructor(message: string, public readonly status: number) {
    super(message);
  }
}

type RequestOptions = RequestInit & {
  token?: string;
};

async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const headers = new Headers(options.headers ?? {});

  if (!headers.has("Content-Type") && options.body) {
    headers.set("Content-Type", "application/json");
  }

  if (options.token) {
    headers.set("Authorization", `Bearer ${options.token}`);
  }

  const response = await fetch(`${API_BASE}${path}`, {
    ...options,
    headers,
  });

  const payload = (await response.json().catch(() => null)) as
    | { data?: T; error?: { message?: string } }
    | null;

  if (!response.ok) {
    throw new ApiError(payload?.error?.message ?? "请求失败", response.status);
  }

  return payload?.data as T;
}

export const api = {
  health() {
    return request<{ status: string; service: string; timestamp: string }>("/health");
  },

  register(email: string, password: string) {
    return request<AuthPayload>("/api/v1/auth/register", {
      method: "POST",
      body: JSON.stringify({ email, password }),
    });
  },

  login(email: string, password: string) {
    return request<AuthPayload>("/api/v1/auth/login", {
      method: "POST",
      body: JSON.stringify({ email, password }),
    });
  },

  me(token: string) {
    return request<PublicUser>("/api/v1/auth/me", {
      token,
    });
  },

  logout(token: string) {
    return request<{ success: boolean }>("/api/v1/auth/logout", {
      method: "POST",
      token,
    });
  },

  changePassword(token: string, currentPassword: string, nextPassword: string) {
    return request<{ success: boolean }>("/api/v1/auth/change-password", {
      method: "POST",
      token,
      body: JSON.stringify({
        currentPassword,
        nextPassword,
      }),
    });
  },

  getSyncConfig(token: string) {
    return request<SyncConfig | null>("/api/v1/sync/config", {
      token,
    });
  },

  putSyncConfig(token: string, keyVerification: string, keyVersion: number) {
    return request<SyncConfig>("/api/v1/sync/config", {
      method: "PUT",
      token,
      body: JSON.stringify({
        keyVerification,
        keyVersion,
      }),
    });
  },

  listSyncItems(token: string) {
    return request<SyncItem[]>("/api/v1/sync/items", {
      token,
    });
  },

  clearSyncData(token: string) {
    return request<{ success: boolean }>("/api/v1/sync/items", {
      method: "DELETE",
      token,
    });
  },

  getAdminOverview(token: string) {
    return request<AdminOverview>("/api/v1/admin/overview", {
      token,
    });
  },

  listAdminUsers(token: string) {
    return request<AdminUserSummary[]>("/api/v1/admin/users", {
      token,
    });
  },

  updateAdminUser(token: string, userId: string, patch: Partial<Pick<AdminUserSummary, "role" | "status">>) {
    return request<AdminUserSummary>(`/api/v1/admin/users/${userId}`, {
      method: "PATCH",
      token,
      body: JSON.stringify(patch),
    });
  },

  purgeAdminUserData(token: string, userId: string) {
    return request<{ success: boolean }>(`/api/v1/admin/users/${userId}/data`, {
      method: "DELETE",
      token,
    });
  },
};
