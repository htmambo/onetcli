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

            <div v-if="formattedDecryptedPayload" class="mt-6">
              <p class="text-xs uppercase tracking-[0.28em] text-[var(--muted)]">解密结果</p>
              <pre class="mt-4 max-h-[30rem] overflow-auto rounded-2xl border border-[color:rgba(0,217,163,0.22)] bg-[color:rgba(0,217,163,0.08)] p-4 text-xs leading-6 text-[var(--text)] whitespace-pre-wrap break-all">{{ formattedDecryptedPayload }}</pre>
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
import { getSyncItemTypeLabel } from "@/utils/syncItemType";
import { formatKeyVersion, formatRecordVersion } from "@/utils/syncItemVersion";

const auth = useAuthStore();
const route = useRoute();

const loading = ref(false);
const item = ref<SyncItem | null>(null);
const config = ref<SyncConfig | null>(null);
const errorMessage = ref("");
const configLoadFailed = ref(false);
const masterKey = ref("");
const decrypting = ref(false);
const decryptMessage = ref("");
const decryptError = ref("");
const decryptedPlaintext = ref("");

const syncItemId = computed(() => String(route.params.id ?? ""));
const formattedDecryptedPayload = computed(() => formatDecryptedPayload(decryptedPlaintext.value));

function formatDate(value: string) {
  return new Date(value).toLocaleString("zh-CN");
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
    void loadItem();
  },
  { immediate: true },
);
</script>
