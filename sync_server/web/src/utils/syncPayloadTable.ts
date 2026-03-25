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

// SSH auth_method 枚举展开
function parseSshAuthMethod(authMethod: unknown): PayloadField[] {
  if (!authMethod || typeof authMethod !== "object") {
    return authMethod != null ? [{ label: "认证方式", value: String(authMethod) }] : [];
  }
  const obj = authMethod as Record<string, unknown>;

  // { "Password": { "password": "..." } }
  if ("Password" in obj) {
    const inner = obj.Password as Record<string, unknown>;
    return [
      { label: "认证方式", value: "密码认证" },
      { label: "密码", value: String(inner?.password ?? ""), sensitive: true, mono: true },
    ];
  }
  // { "PrivateKey": { "key_path": "...", "passphrase": "..." } }
  if ("PrivateKey" in obj) {
    const inner = obj.PrivateKey as Record<string, unknown>;
    const fields: PayloadField[] = [{ label: "认证方式", value: "私钥认证" }];
    if (inner?.key_path) fields.push({ label: "私钥路径", value: String(inner.key_path), mono: true });
    if (inner?.passphrase) fields.push({ label: "私钥口令", value: String(inner.passphrase), sensitive: true, mono: true });
    return fields;
  }
  // { "Agent": null } or "Agent"
  if ("Agent" in obj || obj === null) {
    return [{ label: "认证方式", value: "SSH Agent" }];
  }
  // 未知格式降级
  return [{ label: "认证方式", value: JSON.stringify(authMethod), mono: true }];
}

// SSH params 专项解析
function parseSshParams(params: Record<string, unknown>): PayloadField[] {
  const fields: PayloadField[] = [];

  const simple: Array<[string, string, Partial<{ sensitive: boolean; mono: boolean }>]> = [
    ["主机", "host", {}],
    ["端口", "port", {}],
    ["用户名", "username", {}],
    ["连接超时（秒）", "connect_timeout", {}],
    ["心跳间隔（秒）", "keepalive_interval", {}],
    ["最大心跳失败次数", "keepalive_max", {}],
    ["默认工作目录", "default_directory", { mono: true }],
    ["初始化脚本", "init_script", { mono: true }],
  ];
  for (const [label, key, opts] of simple) {
    const f = field(label, params[key], opts);
    if (f) fields.push(f);
  }

  // auth_method 枚举展开
  if (params.auth_method !== undefined) {
    fields.push(...parseSshAuthMethod(params.auth_method));
  }

  // 跳板机
  if (params.jump_server && typeof params.jump_server === "object") {
    const js = params.jump_server as Record<string, unknown>;
    const jumpFields: PayloadField[] = [];
    for (const [label, key] of [["主机", "host"], ["端口", "port"], ["用户名", "username"]] as [string, string][]) {
      const f = field(label, js[key]);
      if (f) jumpFields.push(f);
    }
    if (js.auth_method !== undefined) jumpFields.push(...parseSshAuthMethod(js.auth_method));
    if (jumpFields.length > 0) fields.push({ label: "跳板机", value: "", nested: jumpFields });
  }

  // 代理
  if (params.proxy && typeof params.proxy === "object") {
    const px = params.proxy as Record<string, unknown>;
    const proxyFields: PayloadField[] = [];
    for (const [label, key, opts] of [
      ["类型", "proxy_type", {}],
      ["主机", "host", {}],
      ["端口", "port", {}],
      ["用户名", "username", {}],
      ["密码", "password", { sensitive: true }],
    ] as [string, string, Partial<{ sensitive: boolean }>][]) {
      const f = field(label, px[key], opts);
      if (f) proxyFields.push(f);
    }
    if (proxyFields.length > 0) fields.push({ label: "代理", value: "", nested: proxyFields });
  }

  return fields;
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

// Redis 模式枚举
function redisModeLabel(mode: unknown): string {
  if (mode === "Standalone") return "单机";
  if (mode === "Sentinel") return "哨兵";
  if (mode === "Cluster") return "集群";
  return String(mode ?? "单机");
}

// Redis params 专项解析
function parseRedisParams(params: Record<string, unknown>): PayloadField[] {
  const fields: PayloadField[] = [];
  const simple: Array<[string, string, Partial<{ sensitive: boolean; mono: boolean }>]> = [
    ["主机", "host", {}],
    ["端口", "port", {}],
    ["用户名", "username", {}],
    ["密码", "password", { sensitive: true }],
    ["数据库索引", "db_index", {}],
    ["连接模式", "mode", {}],
    ["启用 TLS", "use_tls", {}],
    ["连接超时（秒）", "connect_timeout", {}],
  ];
  for (const [label, key, opts] of simple) {
    let f: PayloadField | null;
    if (key === "mode") {
      f = params[key] != null ? { label, value: redisModeLabel(params[key]) } : null;
    } else {
      f = field(label, params[key], opts);
    }
    if (f) fields.push(f);
  }

  // 哨兵配置
  if (params.sentinel && typeof params.sentinel === "object") {
    const s = params.sentinel as Record<string, unknown>;
    const sFields: PayloadField[] = [];
    for (const [label, key] of [["主节点名", "master_name"], ["密码", "password"]] as [string, string][]) {
      const f = field(label, s[key], { sensitive: key === "password" });
      if (f) sFields.push(f);
    }
    if (s.hosts && Array.isArray(s.hosts)) {
      sFields.push({ label: "节点列表", value: s.hosts.join(", "), mono: true });
    }
    if (sFields.length > 0) fields.push({ label: "哨兵配置", value: "", nested: sFields });
  }

  // 集群配置
  if (params.cluster && typeof params.cluster === "object") {
    const c = params.cluster as Record<string, unknown>;
    if (c.nodes && Array.isArray(c.nodes)) {
      fields.push({ label: "集群节点", value: c.nodes.join(", "), mono: true });
    }
  }

  return fields;
}

// MongoDB params 专项解析
function parseMongoParams(params: Record<string, unknown>): PayloadField[] {
  const fields: PayloadField[] = [];
  const simple: Array<[string, string, Partial<{ sensitive: boolean; mono: boolean }>]> = [
    ["连接字符串", "connection_string", { mono: true }],
    ["主机", "host", {}],
    ["端口", "port", {}],
    ["数据库", "database", {}],
    ["用户名", "username", {}],
    ["密码", "password", { sensitive: true }],
    ["认证源", "auth_source", {}],
    ["副本集", "replica_set", {}],
    ["读偏好", "read_preference", {}],
    ["使用 SRV 记录", "use_srv_record", {}],
    ["直连模式", "direct_connection", {}],
    ["启用 TLS", "use_tls", {}],
    ["连接超时（秒）", "connect_timeout_seconds", {}],
    ["应用名", "application_name", {}],
  ];
  for (const [label, key, opts] of simple) {
    const f = field(label, params[key], opts);
    if (f) fields.push(f);
  }
  return fields;
}

// Serial params 专项解析
function parseSerialParams(params: Record<string, unknown>): PayloadField[] {
  const fields: PayloadField[] = [];
  const simple: Array<[string, string]> = [
    ["设备路径", "port_name"],
    ["波特率", "baud_rate"],
    ["数据位", "data_bits"],
    ["停止位", "stop_bits"],
    ["校验位", "parity"],
    ["流控", "flow_control"],
  ];
  for (const [label, key] of simple) {
    const f = field(label, params[key]);
    if (f) fields.push(f);
  }
  return fields;
}

// 通用数据库 params 解析（MySQL/PostgreSQL/SQLite/SQL Server/Oracle/ClickHouse）
function parseDbParams(params: Record<string, unknown>): PayloadField[] {
  const fields: PayloadField[] = [];
  const simple: Array<[string, string, Partial<{ sensitive: boolean; mono: boolean }>]> = [
    ["主机", "host", {}],
    ["端口", "port", {}],
    ["用户名", "username", {}],
    ["密码", "password", { sensitive: true }],
    ["数据库", "database", {}],
    ["服务名", "service_name", {}],
    ["SID", "sid", {}],
  ];
  for (const [label, key, opts] of simple) {
    const f = field(label, params[key], opts);
    if (f) fields.push(f);
  }

  // extra_params 作为嵌套
  if (params.extra_params && typeof params.extra_params === "object") {
    const extra = params.extra_params as Record<string, unknown>;
    const extraFields = Object.entries(extra)
      .filter(([, v]) => v !== undefined && v !== null && v !== "")
      .map(([k, v]) => ({
        label: k,
        value: String(v),
        mono: true,
      }));
    if (extraFields.length > 0) {
      fields.push({ label: "扩展参数", value: "", nested: extraFields });
    }
  }

  return fields;
}

// 根据连接类型选择解析器
function parseConnectionParams(
  params: Record<string, unknown>,
  connectionType: string,
): PayloadField[] {
  const type = connectionType.toLowerCase();
  switch (type) {
    case "ssh":
    case "sftp":
      return parseSshParams(params);
    case "redis":
      return parseRedisParams(params);
    case "mongodb":
    case "mongo":
      return parseMongoParams(params);
    case "serial":
      return parseSerialParams(params);
    case "mysql":
    case "postgres":
    case "postgresql":
    case "sqlite":
    case "sqlserver":
    case "oracle":
    case "clickhouse":
      return parseDbParams(params);
    default:
      // 未知类型降级为通用解析，但尝试识别 auth_method
      if ("auth_method" in params) {
        return parseSshParams(params);
      }
      return objectToFields(params, CONNECTION_SENSITIVE_PARAMS);
  }
}

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

  if (data.params && typeof data.params === "object") {
    const params = data.params as Record<string, unknown>;
    const connType = String(data.connection_type ?? "");
    const paramFields = parseConnectionParams(params, connType);
    if (paramFields.length > 0) {
      fields.push({ label: "连接参数", value: "", nested: paramFields });
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
