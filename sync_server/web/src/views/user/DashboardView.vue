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

    <section class="grid gap-4 md:grid-cols-4">
      <StatCard label="同步项总数" :value="stats.total" tone="success" tone-text="已建立" />
      <StatCard label="连接项" :value="stats.connection" />
      <StatCard label="工作区" :value="stats.workspace" />
      <StatCard label="应用设置" :value="stats.appSettings" />
    </section>

    <section class="grid gap-6 xl:grid-cols-[1fr_1.1fr]">
      <div class="panel rounded-[28px] p-6">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-xs uppercase tracking-[0.28em] text-[var(--muted)]">同步密钥配置</p>
            <h3 class="mt-2 text-xl font-semibold text-[var(--text)]">更新 key_verification</h3>
          </div>
          <span
            class="rounded-full px-3 py-1 text-xs font-medium"
            :class="config ? 'bg-emerald-100 text-emerald-700' : 'bg-amber-100 text-amber-700'"
          >
            {{ config ? "已配置" : "未配置" }}
          </span>
        </div>

        <form class="mt-6 space-y-4" @submit.prevent="saveConfig">
          <label class="block">
            <span class="mb-2 block text-sm font-medium text-[var(--text)]">key_verification</span>
            <textarea
              v-model="keyVerification"
              rows="5"
              class="w-full rounded-2xl border border-[var(--line)] bg-white/80 px-4 py-3 outline-none transition focus:border-[var(--accent)] focus:accent-ring"
              placeholder="输入或粘贴同步密钥校验串"
            />
          </label>
          <label class="block">
            <span class="mb-2 block text-sm font-medium text-[var(--text)]">key_version</span>
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
          <span class="rounded-full bg-white/70 px-3 py-1 text-xs font-medium text-[var(--muted)]">
            {{ items.length }} 条
          </span>
        </div>

        <div v-if="loading" class="mt-6 text-sm text-[var(--muted)]">读取中...</div>
        <div v-else-if="items.length === 0" class="mt-6 rounded-2xl bg-white/60 p-4 text-sm text-[var(--muted)]">
          当前没有同步数据。
        </div>
        <div v-else class="mt-6 space-y-3">
          <article
            v-for="item in items.slice(0, 8)"
            :key="item.id"
            class="rounded-2xl border border-[var(--line)] bg-white/60 p-4"
          >
            <div class="flex items-start justify-between gap-4">
              <div class="min-w-0">
                <p class="text-sm font-semibold text-[var(--text)]">{{ item.dataType }}</p>
                <p class="mt-1 truncate text-xs text-[var(--muted)]">{{ item.id }}</p>
              </div>
              <span class="rounded-full bg-white px-3 py-1 text-xs font-medium text-[var(--muted)]">
                v{{ item.version }}
              </span>
            </div>
            <div class="mt-3 flex flex-wrap gap-2 text-xs text-[var(--muted)]">
              <span>key_version: {{ item.keyVersion }}</span>
              <span>updated_at: {{ formatDate(item.updatedAt) }}</span>
              <span v-if="item.deletedAt" class="text-rose-700">已软删除</span>
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

const auth = useAuthStore();

const loading = ref(false);
const savingConfig = ref(false);
const items = ref<SyncItem[]>([]);
const config = ref<SyncConfig | null>(null);
const keyVerification = ref("");
const keyVersion = ref(1);
const configMessage = ref("");
const configError = ref("");

const stats = computed(() => ({
  total: items.value.length,
  connection: items.value.filter((item) => item.dataType === "connection").length,
  workspace: items.value.filter((item) => item.dataType === "workspace").length,
  appSettings: items.value.filter((item) => item.dataType === "app_settings").length,
}));

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
