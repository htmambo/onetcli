export interface UserRecord {
  id: string;
  email: string;
  password_hash: string;
  role: "admin" | "user";
  status: "active" | "disabled";
  created_at: string;
  updated_at: string;
  last_login_at: string | null;
}

export interface SessionRecord {
  id: string;
  user_id: string;
  token_hash: string;
  expires_at: string;
  created_at: string;
  last_used_at: string;
}

export interface UserConfigRecord {
  id: number;
  user_id: string;
  key_verification: string;
  key_version: number;
  updated_at: string;
}

export interface SyncDataRecord {
  id: string;
  owner_id: string;
  data_type: string;
  encrypted_data: string;
  key_version: number;
  checksum: string;
  version: number;
  created_at: string;
  updated_at: string;
  deleted_at: string | null;
}

export interface PublicUser {
  id: string;
  email: string;
  role: "admin" | "user";
  status: "active" | "disabled";
  createdAt: string;
  updatedAt: string;
  lastLoginAt: string | null;
}

export interface AdminUserSummary extends PublicUser {
  syncItemCount: number;
  hasSyncConfig: boolean;
}
