<template>
  <div class="space-y-6">
    <section class="panel rounded-[28px] p-6">
      <p class="text-xs uppercase tracking-[0.32em] text-[var(--muted)]">全部同步项</p>
      <div class="mt-4 flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
        <div>
          <h2 class="text-3xl font-semibold text-[var(--text)]">当前账号的完整同步列表</h2>
          <p class="mt-2 max-w-2xl text-sm leading-7 text-[var(--muted)]">
            这里展示当前账号全部同步项，按更新时间倒序排列，并保留软删除状态提示，便于检查完整同步数据。
          </p>
        </div>
        <button
          class="rounded-2xl border border-[var(--line)] bg-white/70 px-4 py-3 text-sm font-medium text-[var(--text)] transition hover:bg-white"
          @click="loadItems"
        >
          刷新列表
        </button>
      </div>
    </section>

    <section class="panel rounded-[28px] p-6">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p class="text-xs uppercase tracking-[0.28em] text-[var(--muted)]">同步项列表</p>
          <h3 class="mt-2 text-xl font-semibold text-[var(--text)]">共 {{ items.length }} 条记录</h3>
          <p class="mt-2 text-sm leading-6 text-[var(--muted)]">
            密钥版本表示这条记录使用第几代同步密钥加密；记录版本表示这条记录内容已经更新到第几版。
          </p>
        </div>
        <RouterLink
          to="/app"
          class="rounded-2xl border border-[var(--line)] bg-white/70 px-4 py-3 text-sm font-medium text-[var(--text)] transition hover:bg-white"
        >
          返回同步概览
        </RouterLink>
      </div>

      <p v-if="errorMessage" class="mt-5 rounded-2xl bg-rose-50 px-4 py-3 text-sm text-rose-700">
        {{ errorMessage }}
      </p>
      <div v-else-if="loading" class="mt-6 text-sm text-[var(--muted)]">读取中...</div>
      <div v-else-if="items.length === 0" class="mt-6 rounded-2xl bg-white/60 p-4 text-sm text-[var(--muted)]">
        当前没有同步数据。
      </div>
      <div v-else class="mt-6 overflow-x-auto">
        <table class="min-w-full text-left text-sm">
          <thead class="text-[var(--muted)]">
            <tr class="border-b soft-line">
              <th class="px-3 py-3 font-medium">类型</th>
              <th class="px-3 py-3 font-medium">同步项 ID</th>
              <th class="px-3 py-3 font-medium">密钥版本</th>
              <th class="px-3 py-3 font-medium">记录版本</th>
              <th class="px-3 py-3 font-medium">更新时间</th>
              <th class="px-3 py-3 font-medium">状态</th>
              <th class="px-3 py-3 font-medium">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="item in items" :key="item.id" class="border-b soft-line last:border-none">
              <td class="px-3 py-4 font-medium text-[var(--text)]">{{ getSyncItemTypeLabel(item.dataType) }}</td>
              <td class="px-3 py-4">
                <p class="max-w-[28rem] break-all font-mono text-xs text-[var(--muted)]">{{ item.id }}</p>
              </td>
              <td class="px-3 py-4">{{ formatKeyVersion(item.keyVersion) }}</td>
              <td class="px-3 py-4">{{ formatRecordVersion(item.version) }}</td>
              <td class="px-3 py-4 text-[var(--muted)]">{{ formatDate(item.updatedAt) }}</td>
              <td class="px-3 py-4">
                <span
                  class="rounded-full px-3 py-1 text-xs font-medium"
                  :class="item.deletedAt ? 'bg-rose-100 text-rose-700' : 'bg-emerald-100 text-emerald-700'"
                >
                  {{ item.deletedAt ? "已软删除" : "有效" }}
                </span>
              </td>
              <td class="px-3 py-4">
                <RouterLink
                  :to="{ name: 'sync-item-detail', params: { id: item.id } }"
                  class="inline-flex rounded-xl border border-[var(--line)] bg-white px-3 py-2 text-xs font-medium text-[var(--text)] transition hover:bg-[var(--accent-soft)]"
                >
                  查看详情
                </RouterLink>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { ApiError, api } from "@/services/api";
import { useAuthStore } from "@/stores/auth";
import type { SyncItem } from "@/types/api";
import { getSyncItemTypeLabel } from "@/utils/syncItemType";
import { formatKeyVersion, formatRecordVersion } from "@/utils/syncItemVersion";

const auth = useAuthStore();

const loading = ref(false);
const items = ref<SyncItem[]>([]);
const errorMessage = ref("");

function formatDate(value: string) {
  return new Date(value).toLocaleString("zh-CN");
}

async function loadItems() {
  loading.value = true;
  errorMessage.value = "";

  try {
    items.value = await api.listSyncItems(auth.token!);
  } catch (error) {
    errorMessage.value = error instanceof ApiError ? error.message : "读取同步项列表失败";
  } finally {
    loading.value = false;
  }
}

onMounted(() => {
  void loadItems();
});
</script>
