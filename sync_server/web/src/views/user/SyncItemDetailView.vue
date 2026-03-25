<template>
  <div class="space-y-6">
    <section class="panel rounded-[28px] p-6">
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
            class="rounded-2xl border border-[var(--line)] bg-white/70 px-4 py-3 text-sm font-medium text-[var(--text)] transition hover:bg-white"
          >
            返回列表
          </RouterLink>
          <button
            class="rounded-2xl border border-[var(--line)] bg-white/70 px-4 py-3 text-sm font-medium text-[var(--text)] transition hover:bg-white"
            @click="loadItem"
          >
            刷新详情
          </button>
        </div>
      </div>
    </section>

    <p v-if="errorMessage" class="rounded-2xl bg-rose-50 px-4 py-3 text-sm text-rose-700">
      {{ errorMessage }}
    </p>
    <div v-else-if="loading" class="text-sm text-[var(--muted)]">读取中...</div>
    <template v-else-if="item">
      <section class="grid gap-6 xl:grid-cols-[1.1fr_0.9fr]">
        <div class="panel rounded-[28px] p-6">
          <p class="text-xs uppercase tracking-[0.28em] text-[var(--muted)]">基础信息</p>
          <div class="mt-6 grid gap-4 md:grid-cols-2">
            <article class="rounded-2xl border border-[var(--line)] bg-white/60 p-4">
              <p class="text-xs uppercase tracking-[0.24em] text-[var(--muted)]">数据类型</p>
              <p class="mt-2 text-base font-semibold text-[var(--text)]">{{ getSyncItemTypeLabel(item.dataType) }}</p>
            </article>
            <article class="rounded-2xl border border-[var(--line)] bg-white/60 p-4">
              <p class="text-xs uppercase tracking-[0.24em] text-[var(--muted)]">当前状态</p>
              <p class="mt-2 text-base font-semibold" :class="item.deletedAt ? 'text-rose-700' : 'text-emerald-700'">
                {{ item.deletedAt ? "已软删除" : "有效" }}
              </p>
            </article>
            <article class="rounded-2xl border border-[var(--line)] bg-white/60 p-4 md:col-span-2">
              <p class="text-xs uppercase tracking-[0.24em] text-[var(--muted)]">同步项 ID</p>
              <p class="mt-2 break-all font-mono text-xs text-[var(--text)]">{{ item.id }}</p>
            </article>
            <article class="rounded-2xl border border-[var(--line)] bg-white/60 p-4 md:col-span-2">
              <p class="text-xs uppercase tracking-[0.24em] text-[var(--muted)]">owner_id</p>
              <p class="mt-2 break-all font-mono text-xs text-[var(--text)]">{{ item.ownerId }}</p>
            </article>
            <article class="rounded-2xl border border-[var(--line)] bg-white/60 p-4">
              <p class="text-xs uppercase tracking-[0.24em] text-[var(--muted)]">密钥版本</p>
              <p class="mt-2 text-base font-semibold text-[var(--text)]">{{ formatKeyVersion(item.keyVersion) }}</p>
              <p class="mt-2 text-xs leading-6 text-[var(--muted)]">
                表示这条记录是用第几代同步密钥加密的。
              </p>
            </article>
            <article class="rounded-2xl border border-[var(--line)] bg-white/60 p-4">
              <p class="text-xs uppercase tracking-[0.24em] text-[var(--muted)]">记录版本</p>
              <p class="mt-2 text-base font-semibold text-[var(--text)]">{{ formatRecordVersion(item.version) }}</p>
              <p class="mt-2 text-xs leading-6 text-[var(--muted)]">
                表示这条记录内容已经更新到第几版，每次修改都会递增。
              </p>
            </article>
            <article class="rounded-2xl border border-[var(--line)] bg-white/60 p-4">
              <p class="text-xs uppercase tracking-[0.24em] text-[var(--muted)]">创建时间</p>
              <p class="mt-2 text-sm text-[var(--text)]">{{ formatDate(item.createdAt) }}</p>
            </article>
            <article class="rounded-2xl border border-[var(--line)] bg-white/60 p-4">
              <p class="text-xs uppercase tracking-[0.24em] text-[var(--muted)]">更新时间</p>
              <p class="mt-2 text-sm text-[var(--text)]">{{ formatDate(item.updatedAt) }}</p>
            </article>
            <article class="rounded-2xl border border-[var(--line)] bg-white/60 p-4 md:col-span-2">
              <p class="text-xs uppercase tracking-[0.24em] text-[var(--muted)]">删除时间</p>
              <p class="mt-2 text-sm text-[var(--text)]">
                {{ item.deletedAt ? formatDate(item.deletedAt) : "未删除" }}
              </p>
            </article>
          </div>
        </div>

        <div class="space-y-6">
          <section class="panel rounded-[28px] p-6">
            <p class="text-xs uppercase tracking-[0.28em] text-[var(--muted)]">校验值</p>
            <p class="mt-4 break-all font-mono text-xs leading-6 text-[var(--text)]">{{ item.checksum || "空" }}</p>
          </section>

          <section class="panel rounded-[28px] p-6">
            <p class="text-xs uppercase tracking-[0.28em] text-[var(--muted)]">加密数据</p>
            <pre class="mt-4 max-h-[30rem] overflow-auto rounded-2xl border border-[var(--line)] bg-white/60 p-4 text-xs leading-6 text-[var(--text)] whitespace-pre-wrap break-all">{{ item.encryptedData }}</pre>
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
import type { SyncItem } from "@/types/api";
import { getSyncItemTypeLabel } from "@/utils/syncItemType";
import { formatKeyVersion, formatRecordVersion } from "@/utils/syncItemVersion";

const auth = useAuthStore();
const route = useRoute();

const loading = ref(false);
const item = ref<SyncItem | null>(null);
const errorMessage = ref("");

const syncItemId = computed(() => String(route.params.id ?? ""));

function formatDate(value: string) {
  return new Date(value).toLocaleString("zh-CN");
}

async function loadItem() {
  if (!syncItemId.value) {
    item.value = null;
    errorMessage.value = "同步项 ID 无效";
    return;
  }

  loading.value = true;
  errorMessage.value = "";

  try {
    item.value = await api.getSyncItem(auth.token!, syncItemId.value);
  } catch (error) {
    item.value = null;
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
