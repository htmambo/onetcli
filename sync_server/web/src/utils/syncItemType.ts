const syncItemTypeAliases: Record<string, string> = {
  connection: "connection",
  workspace: "workspace",
  certificate: "credential",
  credential: "credential",
};

const syncItemTypeLabels: Record<string, string> = {
  connection: "连接项",
  workspace: "工作区",
  credential: "凭证",
};

export function normalizeSyncItemType(dataType: string) {
  if (!dataType) {
    return "";
  }

  return syncItemTypeAliases[dataType] ?? dataType;
}

export function getSyncItemTypeLabel(dataType: string) {
  const normalizedType = normalizeSyncItemType(dataType);
  if (!normalizedType) {
    return "未识别类型";
  }

  return syncItemTypeLabels[normalizedType] ?? `未识别类型（${dataType}）`;
}

export function isSyncItemType(dataType: string, expectedType: string) {
  return normalizeSyncItemType(dataType) === normalizeSyncItemType(expectedType);
}
