import { normalizeSyncItemType } from "@/utils/syncItemType";

export interface PayloadField {
  label: string;
  value: string;
  sensitive?: boolean;
  mono?: boolean;
  nested?: PayloadField[];
}

function field(
  label: string,
  value: unknown,
  options?: { sensitive?: boolean; mono?: boolean },
): PayloadField | null {
  if (value === undefined || value === null || value === "") return null;
  return {
    label,
    value: typeof value === "object" ? JSON.stringify(value, null, 2) : String(value),
    sensitive: options?.sensitive,
    mono: options?.mono ?? (typeof value === "object"),
  };
}

function objectToFields(obj: Record<string, unknown>, sensitiveKeys: Set<string>): PayloadField[] {
  return Object.entries(obj)
    .filter(([, v]) => v !== undefined && v !== null && v !== "")
    .map(([k, v]) => ({
      label: k,
      value: typeof v === "object" ? JSON.stringify(v, null, 2) : String(v),
      sensitive: sensitiveKeys.has(k),
      mono: typeof v === "object" || sensitiveKeys.has(k),
    }));
}

const CONNECTION_SENSITIVE_PARAMS = new Set([
  "password",
  "secret",
  "private_key",
  "passphrase",
  "token",
  "auth_token",
  "access_key",
  "secret_key",
]);

function parseConnection(data: Record<string, unknown>): PayloadField[] {
  const fields: PayloadField[] = [];

  const top: Array<[string, string, Partial<{ sensitive: boolean; mono: boolean }>]> = [
    ["名称", "name", {}],
    ["连接类型", "connection_type", {}],
    ["工作区 ID", "workspace_cloud_id", { mono: true }],
    ["已选数据库", "selected_databases", { mono: true }],
    ["备注", "remark", {}],
    ["创建者 ID", "owner_id", { mono: true }],
  ];

  for (const [label, key, opts] of top) {
    const f = field(label, data[key], opts);
    if (f) fields.push(f);
  }

  // params 作为嵌套表格展开
  if (data.params && typeof data.params === "object") {
    const paramFields = objectToFields(
      data.params as Record<string, unknown>,
      CONNECTION_SENSITIVE_PARAMS,
    );
    if (paramFields.length > 0) {
      fields.push({
        label: "连接参数",
        value: "",
        nested: paramFields,
      });
    }
  }

  return fields;
}

function parseWorkspace(data: Record<string, unknown>): PayloadField[] {
  const rows: Array<[string, string]> = [
    ["名称", "name"],
    ["颜色", "color"],
    ["图标", "icon"],
  ];
  return rows.flatMap(([label, key]) => {
    const f = field(label, data[key]);
    return f ? [f] : [];
  });
}

function parseCredential(data: Record<string, unknown>): PayloadField[] {
  const rows: Array<[string, string, Partial<{ sensitive: boolean }>]> = [
    ["名称", "name", {}],
    ["类型", "kind", {}],
    ["用户名", "username", {}],
    ["密码", "password", { sensitive: true }],
    ["私钥路径", "key_path", {}],
    ["私钥口令", "passphrase", { sensitive: true }],
    ["备注", "remark", {}],
    ["创建者 ID", "owner_id", {}],
  ];
  return rows.flatMap(([label, key, opts]) => {
    const f = field(label, data[key], opts);
    return f ? [f] : [];
  });
}

function parseGeneric(data: Record<string, unknown>): PayloadField[] {
  return objectToFields(data, new Set());
}

export function buildPayloadTable(
  plaintext: string,
  dataType: string,
): PayloadField[] | null {
  if (!plaintext) return null;

  let data: Record<string, unknown>;
  try {
    data = JSON.parse(plaintext);
  } catch {
    return null; // 非 JSON，调用方直接显示原文
  }

  if (typeof data !== "object" || Array.isArray(data)) return null;

  const type = normalizeSyncItemType(dataType);
  switch (type) {
    case "connection":
      return parseConnection(data);
    case "workspace":
      return parseWorkspace(data);
    case "credential":
      return parseCredential(data);
    default:
      return parseGeneric(data);
  }
}
