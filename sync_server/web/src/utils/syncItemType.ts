const syncItemTypeLabels: Record<string, string> = {
  connection: "连接项",
  workspace: "工作区",
  app_settings: "应用设置",
};

export function getSyncItemTypeLabel(dataType: string) {
  if (!dataType) {
    return "未识别类型";
  }

  return syncItemTypeLabels[dataType] ?? `未识别类型（${dataType}）`;
}
