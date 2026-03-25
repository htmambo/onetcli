<template>
  <div class="space-y-6">
    <section class="panel rounded-[28px] p-6">
      <p class="text-xs uppercase tracking-[0.32em] text-[var(--muted)]">同步概览</p>
      <div class="mt-4 flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
        <div>
          <h2 class="text-3xl font-semibold text-[var(--text)]">同一账号，一套同步状态</h2>
          <p class="mt-2 max-w-2xl text-sm leading-7 text-[var(--muted)]">
            这里可以查看当前账号的同步密钥配置和同步数据概况。默认不做设备授权，同一账号下的同步内容保持一致。
          </p>
        </div>
        <button
          class="rounded-2xl border border-[var(--line)] bg-white/70 px-4 py-3 text-sm font-medium text-[var(--text)] transition hover:bg-white"
          @click="loadData"
        >
          刷新数据
        </button>
      </div>
    </section>

    <section class="grid gap-4 md:grid-cols-2 xl:grid-cols-5">
      <StatCard label="有效同步项" :value="stats.total" tone="success" tone-text="当前生效" />
      <StatCard label="连接项" :value="stats.connection" />
      <StatCard label="工作区" :value="stats.workspace" />
      <StatCard label="凭证" :value="stats.credential" />
      <StatCard label="应用设置" :value="stats.appSettings" />
    </section>

    <section class="grid gap-6 xl:grid-cols-[1fr_1.1fr]">
      <div class="panel rounded-[28px] p-6">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-xs uppercase tracking-[0.28em] text-[var(--muted)]">同步密钥配置</p>
            <h3 class="mt-2 text-xl font-semibold text-[var(--text)]">管理同步密钥配置</h3>
          </div>
          <span
            class="rounded-full px-3 py-1 text-xs font-medium"
            :class="config ? 'bg-emerald-100 text-emerald-700' : 'bg-amber-100 text-amber-700'"
          >
            {{ config ? "已配置" : "未配置" }}
          </span>
        </div>

        <div class="mt-6 rounded-3xl border border-[var(--line)] bg-white/60 p-5">
          <p class="text-sm font-semibold text-[var(--text)]">字段说明</p>
          <p class="mt-2 text-sm leading-7 text-[var(--muted)]">
            <b>密钥校验串（key_verification）</b>用于校验当前主密钥是否匹配，不会直接保存你的明文密钥。
          </p>
          <p class="mt-2 text-sm leading-7 text-[var(--muted)]">
            <b>密钥版本（key_version）</b>表示当前账号正在使用第几代同步密钥；只有更换主密钥或重建密钥配置时才应该递增。
          </p>
          <p class="mt-2 text-sm leading-7 text-[var(--muted)]">
            同步记录里的<b>记录版本（record_version）</b>则表示某一条记录已经更新到第几版，它和密钥版本是两回事。
          </p>
        </div>

        <div v-if="config" class="mt-4 rounded-3xl border border-emerald-200 bg-emerald-50/80 p-5">
          <p class="text-sm font-semibold text-emerald-800">当前生效配置</p>
          <div class="mt-3 flex flex-wrap gap-3 text-sm text-emerald-700">
            <span>密钥版本：{{ formatKeyVersion(config.keyVersion) }}</span>
            <span>最近更新：{{ formatDate(config.updatedAt) }}</span>
          </div>
        </div>

        <form class="mt-6 space-y-4" @submit.prevent="saveConfig">
          <label class="block">
            <span class="mb-2 block text-sm font-medium text-[var(--text)]">密钥校验串（key_verification）</span>
            <span class="mb-2 block text-xs leading-6 text-[var(--muted)]">
              用来验证当前主密钥是否正确，不是主密钥本身。
            </span>
            <textarea
              v-model="keyVerification"
              rows="5"
              class="w-full rounded-2xl border border-[var(--line)] bg-white/80 px-4 py-3 outline-none transition focus:border-[var(--accent)] focus:accent-ring"
              placeholder="输入或粘贴同步密钥校验串"
            />
          </label>
          <label class="block">
            <span class="mb-2 block text-sm font-medium text-[var(--text)]">密钥版本（key_version）</span>
            <span class="mb-2 block text-xs leading-6 text-[var(--muted)]">
              表示当前账号使用的是第几代同步密钥。正常修改同步内容时不需要改它。
            </span>
            <input
              v-model.number="keyVersion"
              type="number"
              min="1"
              class="w-full rounded-2xl border border-[var(--line)] bg-white/80 px-4 py-3 outline-none transition focus:border-[var(--accent)] focus:accent-ring"
            />
          </label>

          <p v-if="configMessage" class="rounded-2xl bg-emerald-50 px-4 py-3 text-sm text-emerald-700">
            {{ configMessage }}
          </p>
          <p v-if="configError" class="rounded-2xl bg-rose-50 px-4 py-3 text-sm text-rose-700">
            {{ configError }}
          </p>

          <button
            class="accent-button rounded-2xl px-4 py-3 text-sm font-semibold text-white"
            :disabled="savingConfig"
          >
            {{ savingConfig ? "保存中..." : "保存同步配置" }}
          </button>
        </form>
      </div>

      <div class="panel rounded-[28px] p-6">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-xs uppercase tracking-[0.28em] text-[var(--muted)]">最近同步项</p>
            <h3 class="mt-2 text-xl font-semibold text-[var(--text)]">账号级同步数据</h3>
          </div>
          <div class="flex items-center gap-3">
            <RouterLink
              to="/app/sync-items"
              class="rounded-2xl border border-[var(--line)] bg-white/70 px-4 py-2 text-sm font-medium text-[var(--text)] transition hover:bg-white"
            >
              查看全部同步项
            </RouterLink>
            <span class="rounded-full bg-white/70 px-3 py-1 text-xs font-medium text-[var(--muted)]">
              有效 {{ activeItems.length }} 条
            </span>
            <span
              v-if="deletedItems.length > 0"
              class="rounded-full bg-rose-50 px-3 py-1 text-xs font-medium text-rose-700"
            >
              已软删除 {{ deletedItems.length }} 条
            </span>
          </div>
        </div>

        <p class="mt-3 text-sm leading-6 text-[var(--muted)]">
          远端删除采用软删除保留审计痕迹。上方统计和下方预览只计算当前有效项；已软删除项可在完整列表中查看。
        </p>

        <div v-if="loading" class="mt-6 text-sm text-[var(--muted)]">读取中...</div>
        <div v-else-if="activeItems.length === 0" class="mt-6 rounded-2xl bg-white/60 p-4 text-sm text-[var(--muted)]">
          当前没有同步数据。
        </div>
        <div v-else class="mt-6 space-y-3">
          <article
            v-for="item in previewItems"
            :key="item.id"
            class="rounded-2xl border border-[var(--line)] bg-white/60 p-4"
          >
            <div class="flex items-start justify-between gap-4">
              <div class="min-w-0">
                <p class="text-sm font-semibold text-[var(--text)]">{{ getSyncItemTypeLabel(item.dataType) }}</p>
                <p class="mt-1 truncate text-sm text-[var(--text)]">{{ item.name || "未提供名称" }}</p>
                <p class="mt-1 truncate text-xs text-[var(--muted)]">{{ item.id }}</p>
              </div>
              <span class="rounded-full bg-white px-3 py-1 text-xs font-medium text-[var(--muted)]">
                记录{{ formatRecordVersion(item.version) }}
              </span>
            </div>
            <div class="mt-3 flex flex-wrap gap-2 text-xs text-[var(--muted)]">
              <span>密钥版本：{{ formatKeyVersion(item.keyVersion) }}</span>
              <span>最后更新：{{ formatDate(item.updatedAt) }}</span>
              <span v-if="item.deletedAt" class="text-rose-700">已软删除</span>
            </div>
            <div class="mt-4">
              <RouterLink
                :to="{ name: 'sync-item-detail', params: { id: item.id } }"
                class="inline-flex rounded-xl border border-[var(--line)] bg-white px-3 py-2 text-xs font-medium text-[var(--text)] transition hover:bg-[var(--accent-soft)]"
              >
                查看详情
              </RouterLink>
            </div>
          </article>
        </div>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import StatCard from "@/components/StatCard.vue";
import { ApiError, api } from "@/services/api";
import { useAuthStore } from "@/stores/auth";
import type { SyncConfig, SyncItem } from "@/types/api";
import { getSyncItemTypeLabel, isSyncItemType } from "@/utils/syncItemType";
import { formatKeyVersion, formatRecordVersion } from "@/utils/syncItemVersion";

const auth = useAuthStore();

const loading = ref(false);
const savingConfig = ref(false);
const items = ref<SyncItem[]>([]);
const config = ref<SyncConfig | null>(null);
const keyVerification = ref("");
const keyVersion = ref(1);
const configMessage = ref("");
const configError = ref("");

const activeItems = computed(() => items.value.filter((item) => !item.deletedAt));
const deletedItems = computed(() => items.value.filter((item) => Boolean(item.deletedAt)));

const stats = computed(() => ({
  total: activeItems.value.length,
  connection: activeItems.value.filter((item) => isSyncItemType(item.dataType, "connection")).length,
  workspace: activeItems.value.filter((item) => isSyncItemType(item.dataType, "workspace")).length,
  credential: activeItems.value.filter((item) => isSyncItemType(item.dataType, "credential")).length,
  appSettings: activeItems.value.filter((item) => isSyncItemType(item.dataType, "app_settings")).length,
}));

const previewItems = computed(() => activeItems.value.slice(0, 5));

function formatDate(value: string) {
  return new Date(value).toLocaleString("zh-CN");
}

async function loadData() {
  loading.value = true;
  configMessage.value = "";
  configError.value = "";

  try {
    const token = auth.token!;
    const [nextConfig, nextItems] = await Promise.all([
      api.getSyncConfig(token),
      api.listSyncItems(token),
    ]);
    config.value = nextConfig;
    items.value = nextItems;
    keyVerification.value = nextConfig?.keyVerification ?? "";
    keyVersion.value = nextConfig?.keyVersion ?? 1;
  } catch (error) {
    configError.value = error instanceof ApiError ? error.message : "读取同步数据失败";
  } finally {
    loading.value = false;
  }
}

async function saveConfig() {
  savingConfig.value = true;
  configMessage.value = "";
  configError.value = "";

  try {
    config.value = await api.putSyncConfig(auth.token!, keyVerification.value, keyVersion.value);
    configMessage.value = "同步配置已更新";
  } catch (error) {
    configError.value = error instanceof ApiError ? error.message : "保存同步配置失败";
  } finally {
    savingConfig.value = false;
  }
}

onMounted(() => {
  void loadData();
});
</script>
