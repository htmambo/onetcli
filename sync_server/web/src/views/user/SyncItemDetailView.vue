<template>
  <div class="space-y-6">
    <section class="panel hover-card rounded-[28px] p-6">
      <p class="text-xs uppercase tracking-[0.32em] text-[var(--muted)]">同步记录详情</p>
      <div class="mt-4 flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
        <div>
          <h2 class="text-3xl font-semibold text-[var(--text)]">单条同步项详细信息</h2>
          <p class="mt-2 max-w-2xl text-sm leading-7 text-[var(--muted)]">
            这里可以核对同步项的类型、密钥版本、记录版本、校验值、时间戳以及完整加密载荷，方便排查某一条同步记录。
          </p>
        </div>
        <div class="flex flex-wrap gap-3">
          <RouterLink
            to="/app/sync-items"
            class="ghost-button rounded-2xl px-4 py-3 text-sm font-medium"
          >
            返回列表
          </RouterLink>
          <button
            v-if="item && !item.deletedAt"
            class="danger-button rounded-2xl px-4 py-3 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-50"
            :disabled="deleting"
            @click="deleteItem"
          >
            {{ deleting ? "删除中..." : "删除同步项" }}
          </button>
          <button
            v-else-if="item?.deletedAt"
            class="inline-button rounded-2xl px-4 py-3 text-sm font-medium disabled:cursor-not-allowed disabled:opacity-50"
            :disabled="restoring"
            @click="restoreItem"
          >
            {{ restoring ? "恢复中..." : "恢复同步项" }}
          </button>
          <button
            class="ghost-button rounded-2xl px-4 py-3 text-sm font-medium"
            @click="loadItem"
          >
            刷新详情
          </button>
        </div>
      </div>
    </section>

    <p v-if="errorMessage" class="feedback-danger rounded-2xl px-4 py-3 text-sm">
      {{ errorMessage }}
    </p>
    <div v-else-if="loading" class="text-sm text-[var(--muted)]">读取中...</div>
    <template v-else-if="item">
      <p v-if="actionMessage" class="feedback-success rounded-2xl px-4 py-3 text-sm">
        {{ actionMessage }}
      </p>
      <p v-if="actionError" class="feedback-danger rounded-2xl px-4 py-3 text-sm">
        {{ actionError }}
      </p>

      <section class="grid gap-6 xl:grid-cols-[1.1fr_0.9fr]">
        <div class="panel hover-card rounded-[28px] p-6">
          <p class="text-xs uppercase tracking-[0.28em] text-[var(--muted)]">基础信息</p>
          <div class="mt-6 grid gap-4 md:grid-cols-2">
            <article class="hover-card rounded-2xl border border-[var(--line)] bg-[var(--panel-strong)] p-4 md:col-span-2">
              <p class="text-xs uppercase tracking-[0.24em] text-[var(--muted)]">项目名称</p>
              <p class="mt-2 text-base font-semibold text-[var(--text)]">{{ item.name || "未提供名称" }}</p>
            </article>
            <article class="hover-card rounded-2xl border border-[var(--line)] bg-[var(--panel-strong)] p-4">
              <p class="text-xs uppercase tracking-[0.24em] text-[var(--muted)]">数据类型</p>
              <p class="mt-2 text-base font-semibold text-[var(--text)]">{{ getSyncItemTypeLabel(item.dataType) }}</p>
            </article>
            <article class="hover-card rounded-2xl border border-[var(--line)] bg-[var(--panel-strong)] p-4">
              <p class="text-xs uppercase tracking-[0.24em] text-[var(--muted)]">当前状态</p>
              <p class="mt-2 text-base font-semibold" :class="item.deletedAt ? 'text-[var(--danger)]' : 'text-[var(--success)]'">
                {{ item.deletedAt ? "已软删除" : "有效" }}
              </p>
            </article>
            <article class="hover-card rounded-2xl border border-[var(--line)] bg-[var(--panel-strong)] p-4 md:col-span-2">
              <p class="text-xs uppercase tracking-[0.24em] text-[var(--muted)]">同步项 ID</p>
              <p class="mt-2 break-all font-mono text-xs text-[var(--text)]">{{ item.id }}</p>
            </article>
            <article class="hover-card rounded-2xl border border-[var(--line)] bg-[var(--panel-strong)] p-4 md:col-span-2">
              <p class="text-xs uppercase tracking-[0.24em] text-[var(--muted)]">owner_id</p>
              <p class="mt-2 break-all font-mono text-xs text-[var(--text)]">{{ item.ownerId }}</p>
            </article>
            <article class="hover-card rounded-2xl border border-[var(--line)] bg-[var(--panel-strong)] p-4">
              <p class="text-xs uppercase tracking-[0.24em] text-[var(--muted)]">密钥版本</p>
              <p class="mt-2 text-base font-semibold text-[var(--text)]">{{ formatKeyVersion(item.keyVersion) }}</p>
              <p class="mt-2 text-xs leading-6 text-[var(--muted)]">
                表示这条记录是用第几代同步密钥加密的。
              </p>
            </article>
            <article class="hover-card rounded-2xl border border-[var(--line)] bg-[var(--panel-strong)] p-4">
              <p class="text-xs uppercase tracking-[0.24em] text-[var(--muted)]">记录版本</p>
              <p class="mt-2 text-base font-semibold text-[var(--text)]">{{ formatRecordVersion(item.version) }}</p>
              <p class="mt-2 text-xs leading-6 text-[var(--muted)]">
                表示这条记录内容已经更新到第几版，每次修改都会递增。
              </p>
            </article>
            <article class="hover-card rounded-2xl border border-[var(--line)] bg-[var(--panel-strong)] p-4">
              <p class="text-xs uppercase tracking-[0.24em] text-[var(--muted)]">创建时间</p>
              <p class="mt-2 text-sm text-[var(--text)]">{{ formatDate(item.createdAt) }}</p>
            </article>
            <article class="hover-card rounded-2xl border border-[var(--line)] bg-[var(--panel-strong)] p-4">
              <p class="text-xs uppercase tracking-[0.24em] text-[var(--muted)]">更新时间</p>
              <p class="mt-2 text-sm text-[var(--text)]">{{ formatDate(item.updatedAt) }}</p>
            </article>
            <article class="hover-card rounded-2xl border border-[var(--line)] bg-[var(--panel-strong)] p-4 md:col-span-2">
              <p class="text-xs uppercase tracking-[0.24em] text-[var(--muted)]">删除时间</p>
              <p class="mt-2 text-sm text-[var(--text)]">
                {{ item.deletedAt ? formatDate(item.deletedAt) : "未删除" }}
              </p>
            </article>
          </div>
        </div>

        <div class="space-y-6">
          <section class="panel hover-card rounded-[28px] p-6">
            <p class="text-xs uppercase tracking-[0.28em] text-[var(--muted)]">校验值</p>
            <p class="mt-4 break-all font-mono text-xs leading-6 text-[var(--text)]">{{ item.checksum || "空" }}</p>
          </section>

          <section class="panel hover-card rounded-[28px] p-6">
            <div class="flex flex-wrap items-center justify-between gap-3">
              <div>
                <p class="text-xs uppercase tracking-[0.28em] text-[var(--muted)]">本地解密</p>
                <h3 class="mt-2 text-xl font-semibold text-[var(--text)]">手动输入主密钥查看明文</h3>
              </div>
              <span
                class="rounded-full px-3 py-1 text-xs font-medium"
                :class="config ? 'status-success' : 'status-warning'"
              >
                {{ config ? `已读取 key_verification · ${formatKeyVersion(config.keyVersion)}` : "未读取到 key_verification" }}
              </span>
            </div>

            <p class="mt-4 text-sm leading-7 text-[var(--muted)]">
              主密钥只在当前浏览器内存中使用，不会发送到 sync_server。每次需要查看明文时，请手动输入主密钥。
            </p>
            <p v-if="configLoadFailed" class="feedback-warning mt-4 rounded-2xl px-4 py-3 text-sm">
              同步密钥配置读取失败，本次将直接尝试解密密文，无法提前校验主密钥是否正确。
            </p>

            <form class="mt-6 space-y-4" @submit.prevent="decryptItemData">
              <label class="block">
                <span class="mb-2 block text-sm font-medium text-[var(--text)]">主密钥</span>
                <input
                  v-model="masterKey"
                  type="password"
                  autocomplete="off"
                  class="input-shell w-full rounded-2xl border px-4 py-3 outline-none"
                  placeholder="输入主密钥后在本地解密"
                />
              </label>

              <p v-if="decryptMessage" class="feedback-success rounded-2xl px-4 py-3 text-sm">
                {{ decryptMessage }}
              </p>
              <p v-if="decryptError" class="feedback-danger rounded-2xl px-4 py-3 text-sm">
                {{ decryptError }}
              </p>

              <div class="flex flex-wrap gap-3">
                <button
                  class="accent-button rounded-2xl px-4 py-3 text-sm font-semibold text-white"
                  :disabled="decrypting"
                >
                  {{ decrypting ? "解密中..." : "验证并解密" }}
                </button>
                <button
                  type="button"
                  class="ghost-button rounded-2xl px-4 py-3 text-sm font-medium"
                  @click="clearDecryptedData"
                >
                  清空结果
                </button>
              </div>
            </form>

            <div v-if="decryptedPlaintext" class="mt-6">
              <p class="text-xs uppercase tracking-[0.28em] text-[var(--muted)]">解密结果</p>

              <!-- 结构化 table 展示 -->
              <template v-if="decryptedPayloadFields">
                <div class="mt-4 overflow-hidden rounded-2xl border border-[color:rgba(0,217,163,0.22)] bg-[color:rgba(0,217,163,0.04)]">
                  <table class="w-full text-xs">
                    <tbody>
                      <template v-for="f in decryptedPayloadFields" :key="f.label">
                        <!-- 嵌套分组行 -->
                        <template v-if="f.nested">
                          <tr class="border-t border-[color:rgba(0,217,163,0.15)] bg-[color:rgba(0,217,163,0.06)]">
                            <td colspan="2" class="px-4 py-2 font-semibold tracking-[0.2em] uppercase text-[var(--accent)]">{{ f.label }}</td>
                          </tr>
                          <tr
                            v-for="nf in f.nested"
                            :key="nf.label"
                            class="border-t border-[var(--line)] hover:bg-[color:rgba(255,255,255,0.02)]"
                          >
                            <td class="w-36 shrink-0 px-4 py-2 text-[var(--muted)] align-top">{{ nf.label }}</td>
                            <td class="px-4 py-2 break-all align-top" :class="[nf.mono ? 'font-mono' : '', nf.sensitive ? 'text-[var(--warning)]' : 'text-[var(--text)]']">
                              <span v-if="nf.sensitive" class="italic opacity-70">{{ nf.value }}</span>
                              <span v-else>{{ nf.value }}</span>
                            </td>
                          </tr>
                        </template>
                        <!-- 普通字段行 -->
                        <tr
                          v-else
                          class="border-t border-[var(--line)] hover:bg-[color:rgba(255,255,255,0.02)]"
                        >
                          <td class="w-36 shrink-0 px-4 py-2 text-[var(--muted)] align-top">{{ f.label }}</td>
                          <td class="px-4 py-2 break-all align-top" :class="[f.mono ? 'font-mono' : '', f.sensitive ? 'text-[var(--warning)]' : 'text-[var(--text)]']">
                            <span v-if="f.sensitive" class="italic opacity-70">{{ f.value }}</span>
                            <span v-else>{{ f.value }}</span>
                          </td>
                        </tr>
                      </template>
                    </tbody>
                  </table>
                </div>
              </template>

              <!-- 非 JSON 或解析失败时降级为原文 -->
              <pre v-else class="mt-4 max-h-[30rem] overflow-auto rounded-2xl border border-[color:rgba(0,217,163,0.22)] bg-[color:rgba(0,217,163,0.08)] p-4 text-xs leading-6 text-[var(--text)] whitespace-pre-wrap break-all">{{ formattedDecryptedPayload }}</pre>
            </div>
          </section>

          <section class="panel hover-card rounded-[28px] p-6">
            <p class="text-xs uppercase tracking-[0.28em] text-[var(--muted)]">加密数据</p>
            <pre class="mt-4 max-h-[30rem] overflow-auto rounded-2xl border border-[var(--line)] bg-[var(--panel-strong)] p-4 text-xs leading-6 text-[var(--text)] whitespace-pre-wrap break-all">{{ item.encryptedData }}</pre>
          </section>
        </div>
      </section>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRoute } from "vue-router";
import { ApiError, api } from "@/services/api";
import { useAuthStore } from "@/stores/auth";
import type { SyncConfig, SyncItem } from "@/types/api";
import {
  SyncCryptoError,
  decryptSyncEncryptedData,
  formatDecryptedPayload,
  verifySyncMasterKey,
} from "@/utils/syncCrypto";
import { getSyncItemTypeLabel, isSyncItemType } from "@/utils/syncItemType";
import { formatKeyVersion, formatRecordVersion } from "@/utils/syncItemVersion";
import { buildPayloadTable } from "@/utils/syncPayloadTable";
import type { PayloadField } from "@/utils/syncPayloadTable";

const auth = useAuthStore();
const route = useRoute();

const loading = ref(false);
const item = ref<SyncItem | null>(null);
const config = ref<SyncConfig | null>(null);
const errorMessage = ref("");
const actionMessage = ref("");
const actionError = ref("");
const configLoadFailed = ref(false);
const masterKey = ref("");
const deleting = ref(false);
const restoring = ref(false);
const decrypting = ref(false);
const decryptMessage = ref("");
const decryptError = ref("");
const decryptedPlaintext = ref("");

const syncItemId = computed(() => String(route.params.id ?? ""));
const formattedDecryptedPayload = computed(() => formatDecryptedPayload(decryptedPlaintext.value));
const decryptedPayloadFields = computed<PayloadField[] | null>(() => {
  if (!decryptedPlaintext.value || !item.value) return null;
  return buildPayloadTable(decryptedPlaintext.value, item.value.dataType);
});

function formatDate(value: string) {
  return new Date(value).toLocaleString("zh-CN");
}

function getItemDisplayName(currentItem: SyncItem) {
  return currentItem.name.trim() || currentItem.id;
}

function buildDeletePrompt(currentItem: SyncItem) {
  const lines = [
    `确认要删除${getSyncItemTypeLabel(currentItem.dataType)}「${getItemDisplayName(currentItem)}」吗？`,
    "这会把当前远端记录标记为已软删除。",
  ];

  if (isSyncItemType(currentItem.dataType, "workspace")) {
    lines.push("删除工作区时只会解除子级连接关联，不会删除子连接本身。");
  }

  lines.push("如果这条记录已被其他设备更新，则更新优先，本次删除会被拒绝。");

  return lines.join("\n");
}

function buildRestorePrompt(currentItem: SyncItem) {
  const lines = [
    `确认要恢复${getSyncItemTypeLabel(currentItem.dataType)}「${getItemDisplayName(currentItem)}」吗？`,
    "这会清除当前远端记录的软删除标记。",
  ];

  if (isSyncItemType(currentItem.dataType, "workspace")) {
    lines.push("恢复工作区时只会恢复工作区记录本身，不会自动重新关联此前解绑的子连接。");
  }

  lines.push("如果这条记录已被其他设备更新，则会按最新版本校验恢复请求。");

  return lines.join("\n");
}

function clearDecryptedData() {
  masterKey.value = "";
  decryptMessage.value = "";
  decryptError.value = "";
  decryptedPlaintext.value = "";
}

async function decryptItemData() {
  if (!item.value) {
    decryptError.value = "当前没有可解密的同步项";
    return;
  }

  if (!masterKey.value.trim()) {
    decryptMessage.value = "";
    decryptError.value = "请输入主密钥";
    decryptedPlaintext.value = "";
    return;
  }

  decrypting.value = true;
  decryptMessage.value = "";
  decryptError.value = "";

  try {
    if (config.value?.keyVerification) {
      const isValid = await verifySyncMasterKey(masterKey.value, config.value.keyVerification);
      if (!isValid) {
        decryptError.value = "主密钥错误，无法通过当前账号的 key_verification 校验";
        decryptedPlaintext.value = "";
        return;
      }
    }

    decryptedPlaintext.value = await decryptSyncEncryptedData(item.value.encryptedData, masterKey.value);
    decryptMessage.value = config.value?.keyVerification
      ? "主密钥校验通过，已在浏览器本地完成解密"
      : "已在浏览器本地完成解密";
    masterKey.value = "";
  } catch (error) {
    decryptedPlaintext.value = "";
    decryptError.value = error instanceof SyncCryptoError ? error.message : "解密失败";
  } finally {
    decrypting.value = false;
  }
}

async function deleteItem() {
  if (!item.value || item.value.deletedAt || deleting.value) {
    return;
  }

  const currentItem = item.value;
  const confirmed = window.confirm(buildDeletePrompt(currentItem));
  if (!confirmed) {
    return;
  }

  actionMessage.value = "";
  actionError.value = "";
  deleting.value = true;

  try {
    const result = await api.deleteSyncItem(auth.token!, currentItem.id, currentItem.version);
    item.value = {
      ...currentItem,
      version: result.item.version,
      updatedAt: result.item.updatedAt,
      deletedAt: result.item.deletedAt,
    };
    actionMessage.value = isSyncItemType(currentItem.dataType, "workspace")
      ? "工作区同步项已软删除，子连接会在后续同步中解除关联。"
      : "同步项已软删除。";
  } catch (error) {
    if (error instanceof ApiError && error.status === 409) {
      actionError.value = "删除未生效：该记录已被更新或状态已变化，已刷新为最新数据。";
      await loadItem();
    } else {
      actionError.value = error instanceof ApiError ? error.message : "删除同步项失败";
    }
  } finally {
    deleting.value = false;
  }
}

async function restoreItem() {
  if (!item.value || !item.value.deletedAt || restoring.value) {
    return;
  }

  const currentItem = item.value;
  const confirmed = window.confirm(buildRestorePrompt(currentItem));
  if (!confirmed) {
    return;
  }

  actionMessage.value = "";
  actionError.value = "";
  restoring.value = true;

  try {
    item.value = await api.restoreSyncItem(auth.token!, currentItem);
    actionMessage.value = isSyncItemType(currentItem.dataType, "workspace")
      ? "工作区同步项已恢复。注意：此前解绑的子连接不会自动重新关联。"
      : "同步项已恢复。";
  } catch (error) {
    if (error instanceof ApiError && error.status === 409) {
      actionError.value = "恢复未生效：该记录已被更新或状态已变化，已刷新为最新数据。";
      await loadItem();
    } else {
      actionError.value = error instanceof ApiError ? error.message : "恢复同步项失败";
    }
  } finally {
    restoring.value = false;
  }
}

async function loadItem() {
  if (!syncItemId.value) {
    item.value = null;
    config.value = null;
    errorMessage.value = "同步项 ID 无效";
    clearDecryptedData();
    return;
  }

  loading.value = true;
  errorMessage.value = "";
  configLoadFailed.value = false;
  clearDecryptedData();

  try {
    const [nextItem, nextConfig] = await Promise.all([
      api.getSyncItem(auth.token!, syncItemId.value),
      api.getSyncConfig(auth.token!).catch(() => {
        configLoadFailed.value = true;
        return null;
      }),
    ]);
    item.value = nextItem;
    config.value = nextConfig;
  } catch (error) {
    item.value = null;
    config.value = null;
    errorMessage.value = error instanceof ApiError ? error.message : "读取同步项详情失败";
  } finally {
    loading.value = false;
  }
}

watch(
  () => route.params.id,
  () => {
    actionMessage.value = "";
    actionError.value = "";
    void loadItem();
  },
  { immediate: true },
);
</script>
