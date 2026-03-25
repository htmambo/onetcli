export interface PublicUser {
  id: string;
  email: string;
  nickname: string;
  role: "admin" | "user";
  status: "active" | "disabled";
  createdAt: string;
  updatedAt: string;
  lastLoginAt: string | null;
}

export interface AuthPayload {
  token: string;
  user: PublicUser;
}

export interface SyncConfig {
  keyVerification: string;
  keyVersion: number;
  updatedAt: string;
}

export interface SyncItem {
  id: string;
  ownerId: string;
  dataType: string;
  encryptedData: string;
  keyVersion: number;
  checksum: string;
  version: number;
  createdAt: string;
  updatedAt: string;
  deletedAt: string | null;
}

export interface AdminOverview {
  users: number;
  activeUsers: number;
  syncItems: number;
  syncConfigs: number;
}

export interface AdminUserSummary extends PublicUser {
  syncItemCount: number;
  hasSyncConfig: boolean;
}
