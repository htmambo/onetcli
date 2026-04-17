<template>
  <div class="space-y-6">
    <section class="panel hover-card rounded-[28px] p-6">
      <p class="text-xs uppercase tracking-[0.32em] text-[var(--muted)]">全部同步项</p>
      <div class="mt-4 flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
        <div>
          <h2 class="text-3xl font-semibold text-[var(--text)]">当前账号的完整同步列表</h2>
          <p class="mt-2 max-w-2xl text-sm leading-7 text-[var(--muted)]">
            这里展示当前账号全部同步项，按更新时间倒序排列，并保留软删除状态提示，便于检查完整同步数据。
          </p>
        </div>
        <button
          class="ghost-button rounded-2xl px-4 py-3 text-sm font-medium"
          @click="loadItems"
        >
          刷新列表
        </button>
      </div>
    </section>

    <section class="panel hover-card rounded-[28px] p-6">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p class="text-xs uppercase tracking-[0.28em] text-[var(--muted)]">同步项列表</p>
          <h3 class="mt-2 text-xl font-semibold text-[var(--text)]">
            共 {{ filteredItems.length }} 条记录
          </h3>
          <p class="mt-2 text-sm leading-6 text-[var(--muted)]">
            密钥版本表示这条记录使用第几代同步密钥加密；记录版本表示这条记录内容已经更新到第几版。
          </p>
        </div>
        <RouterLink
          to="/app"
          class="ghost-button rounded-2xl px-4 py-3 text-sm font-medium"
        >
          返回同步概览
        </RouterLink>
      </div>

      <p v-if="errorMessage" class="feedback-danger mt-5 rounded-2xl px-4 py-3 text-sm">
        {{ errorMessage }}
      </p>
      <div v-else-if="loading" class="mt-6 text-sm text-[var(--muted)]">读取中...</div>
      <div v-else-if="items.length === 0" class="mt-6 rounded-2xl border border-[var(--line)] bg-[var(--panel-strong)] p-4 text-sm text-[var(--muted)]">
        当前没有同步数据。
      </div>
      <div v-else class="mt-6 space-y-6">
        <p v-if="actionMessage" class="feedback-success rounded-2xl px-4 py-3 text-sm">
          {{ actionMessage }}
        </p>
        <p v-if="actionError" class="feedback-danger rounded-2xl px-4 py-3 text-sm">
          {{ actionError }}
        </p>

        <div class="hover-card rounded-3xl border border-[var(--line)] bg-[var(--panel-strong)] p-5">
          <div class="space-y-4">
            <div class="grid gap-4 md:grid-cols-3">
              <label class="block">
                <span class="mb-2 block text-sm font-medium text-[var(--text)]">类型筛选</span>
                <select
                  v-model="selectedType"
                  class="input-shell w-full rounded-2xl border px-4 py-3 text-sm outline-none"
                >
                  <option :value="ALL_TYPES">全部类型</option>
                  <option
                    v-for="option in itemTypeOptions"
                    :key="option.value"
                    :value="option.value"
                  >
                    {{ option.label }}
                  </option>
                </select>
              </label>

              <label class="block">
                <span class="mb-2 block text-sm font-medium text-[var(--text)]">状态筛选</span>
                <select
                  v-model="selectedStatus"
                  class="input-shell w-full rounded-2xl border px-4 py-3 text-sm outline-none"
                >
                  <option :value="ACTIVE_STATUS">仅有效</option>
                  <option :value="ALL_STATUS">全部状态</option>
                  <option :value="DELETED_STATUS">仅已软删除</option>
                </select>
              </label>

              <label class="block">
                <span class="mb-2 block text-sm font-medium text-[var(--text)]">每页条数</span>
                <select
                  v-model.number="pageSize"
                  class="input-shell w-full rounded-2xl border px-4 py-3 text-sm outline-none"
                >
                  <option v-for="option in pageSizeOptions" :key="option" :value="option">
                    {{ option }} 条/页
                  </option>
                </select>
              </label>
            </div>

            <div class="flex flex-wrap gap-3 text-sm text-[var(--muted)]">
              <span class="status-neutral rounded-full px-3 py-2">共 {{ items.length }} 条</span>
              <span class="status-neutral rounded-full px-3 py-2">有效 {{ activeItemCount }} 条</span>
              <span class="status-neutral rounded-full px-3 py-2">已软删除 {{ deletedItemCount }} 条</span>
              <span class="status-neutral rounded-full px-3 py-2">筛选后 {{ filteredItems.length }} 条</span>
              <span class="status-neutral rounded-full px-3 py-2">第 {{ currentPage }} / {{ totalPages }} 页</span>
            </div>
          </div>
        </div>

        <p class="feedback-warning rounded-2xl px-4 py-3 text-sm leading-6">
          远端删除采用软删除保留审计痕迹。删除工作区时只会解除子级关联，不会删除子项；恢复工作区时只会恢复工作区记录本身，不会自动重新关联此前解绑的子连接；如果删除与更新并发，以更新为准。默认仅显示有效项；如果需要核对删除记录，可切换到“仅已软删除”或“全部状态”。
        </p>

        <div
          v-if="filteredItems.length === 0"
          class="rounded-2xl border border-[var(--line)] bg-[var(--panel-strong)] p-4 text-sm text-[var(--muted)]"
        >
          当前筛选条件下没有同步数据。
        </div>

        <template v-else>
          <div class="overflow-x-auto">
            <table class="min-w-full text-left text-sm">
              <thead class="text-[var(--muted)]">
                <tr class="border-b soft-line">
                  <th class="px-3 py-3 font-medium">类型</th>
                  <th class="px-3 py-3 font-medium">名称</th>
                  <th class="px-3 py-3 font-medium">密钥版本</th>
                  <th class="px-3 py-3 font-medium">记录版本</th>
                  <th class="px-3 py-3 font-medium">更新时间</th>
                  <!-- <th class="px-3 py-3 font-medium">状态</th> -->
                  <th class="px-3 py-3 font-medium">操作</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="item in paginatedItems" :key="item.id" class="border-b soft-line last:border-none">
                  <td class="px-3 py-4 font-medium text-[var(--text)]">{{ getSyncItemTypeLabel(item.dataType) }}</td>
                  <td class="px-3 py-4">
                    <p class="max-w-[16rem] truncate font-medium text-[var(--text)]">{{ item.name || item.id }}</p>
                  </td>
                  <td class="px-3 py-4">{{ formatKeyVersion(item.keyVersion) }}</td>
                  <td class="px-3 py-4">{{ formatRecordVersion(item.version) }}</td>
                  <td class="px-3 py-4 text-[var(--muted)]">{{ formatDate(item.updatedAt) }}</td>
                  <!-- <td class="px-3 py-4">
                    <span
                      class="rounded-full px-3 py-1 text-xs font-medium"
                      :class="item.deletedAt ? 'status-danger' : 'status-success'"
                    >
                      {{ item.deletedAt ? "已软删除" : "有效" }}
                    </span>
                  </td> -->
                  <td class="px-3 py-4">
                    <div class="flex flex-wrap gap-2">
                      <RouterLink
                        :to="{ name: 'sync-item-detail', params: { id: item.id } }"
                        class="inline-button inline-flex rounded-xl px-3 py-2 text-xs font-medium"
                      >
                        查看详情
                      </RouterLink>
                      <button
                        v-if="!item.deletedAt"
                        class="danger-button rounded-xl px-3 py-2 text-xs font-medium text-white disabled:cursor-not-allowed disabled:opacity-50"
                        :disabled="deletingItemId === item.id"
                        @click="deleteItem(item)"
                      >
                        {{ deletingItemId === item.id ? "删除中..." : "删除" }}
                      </button>
                      <button
                        v-else
                        class="inline-button rounded-xl px-3 py-2 text-xs font-medium disabled:cursor-not-allowed disabled:opacity-50"
                        :disabled="restoringItemId === item.id"
                        @click="restoreItem(item)"
                      >
                        {{ restoringItemId === item.id ? "恢复中..." : "恢复" }}
                      </button>
                    </div>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>

          <div class="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
            <p class="text-sm text-[var(--muted)]">
              当前显示第 {{ pageRange.start }} - {{ pageRange.end }} 条，共 {{ filteredItems.length }} 条筛选结果。
            </p>

            <div class="flex flex-wrap items-center gap-2">
              <button
                class="inline-button rounded-xl px-3 py-2 text-sm font-medium disabled:cursor-not-allowed disabled:opacity-50"
                :disabled="currentPage === 1"
                @click="goToPreviousPage"
              >
                上一页
              </button>

              <button
                v-for="page in visiblePages"
                :key="page"
                class="rounded-xl border px-3 py-2 text-sm font-medium transition"
                :class="page === currentPage ? 'border-[color:rgba(0,217,163,0.22)] bg-[var(--accent-soft)] text-[var(--accent)]' : 'inline-button'"
                @click="goToPage(page)"
              >
                {{ page }}
              </button>

              <button
                class="inline-button rounded-xl px-3 py-2 text-sm font-medium disabled:cursor-not-allowed disabled:opacity-50"
                :disabled="currentPage === totalPages"
                @click="goToNextPage"
              >
                下一页
              </button>
            </div>
          </div>
        </template>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { ApiError, api } from "@/services/api";
import { useAuthStore } from "@/stores/auth";
import type { SyncItem } from "@/types/api";
import { getSyncItemTypeLabel, isSyncItemType, normalizeSyncItemType } from "@/utils/syncItemType";
import { formatKeyVersion, formatRecordVersion } from "@/utils/syncItemVersion";

const ALL_TYPES = "__all_types__";
const ALL_STATUS = "__all_status__";
const ACTIVE_STATUS = "active";
const DELETED_STATUS = "deleted";
const pageSizeOptions = [20, 50, 100];

const auth = useAuthStore();

const loading = ref(false);
const items = ref<SyncItem[]>([]);
const errorMessage = ref("");
const actionMessage = ref("");
const actionError = ref("");
const selectedType = ref(ALL_TYPES);
const selectedStatus = ref(ACTIVE_STATUS);
const pageSize = ref(20);
const currentPage = ref(1);
const deletingItemId = ref("");
const restoringItemId = ref("");

const itemTypeOptions = computed(() => {
  const uniqueTypes = new Map<string, string>();
  for (const item of items.value) {
    const normalizedType = normalizeSyncItemType(item.dataType);
    if (!normalizedType || uniqueTypes.has(normalizedType)) {
      continue;
    }

    uniqueTypes.set(normalizedType, getSyncItemTypeLabel(item.dataType));
  }

  return [...uniqueTypes.entries()]
    .sort((left, right) => {
      return left[1].localeCompare(right[1], "zh-CN");
    })
    .map(([value, label]) => ({
      value,
      label,
    }));
});

const activeItemCount = computed(() => items.value.filter((item) => !item.deletedAt).length);

const deletedItemCount = computed(() => items.value.filter((item) => Boolean(item.deletedAt)).length);

const filteredItems = computed(() => {
  const typeFilteredItems =
    selectedType.value === ALL_TYPES
      ? items.value
      : items.value.filter((item) => isSyncItemType(item.dataType, selectedType.value));

  if (selectedStatus.value === ACTIVE_STATUS) {
    return typeFilteredItems.filter((item) => !item.deletedAt);
  }

  if (selectedStatus.value === DELETED_STATUS) {
    return typeFilteredItems.filter((item) => Boolean(item.deletedAt));
  }

  return typeFilteredItems;
});

const totalPages = computed(() => {
  return Math.max(1, Math.ceil(filteredItems.value.length / pageSize.value));
});

const paginatedItems = computed(() => {
  const start = (currentPage.value - 1) * pageSize.value;
  const end = start + pageSize.value;
  return filteredItems.value.slice(start, end);
});

const pageRange = computed(() => {
  if (filteredItems.value.length === 0) {
    return {
      start: 0,
      end: 0,
    };
  }

  const start = (currentPage.value - 1) * pageSize.value + 1;
  const end = Math.min(filteredItems.value.length, currentPage.value * pageSize.value);

  return { start, end };
});

const visiblePages = computed(() => {
  const maxVisiblePages = 5;
  let start = Math.max(1, currentPage.value - 2);
  let end = Math.min(totalPages.value, start + maxVisiblePages - 1);
  start = Math.max(1, end - maxVisiblePages + 1);

  return Array.from({ length: end - start + 1 }, (_, index) => start + index);
});

function formatDate(value: string) {
  return new Date(value).toLocaleString("zh-CN");
}

function getItemDisplayName(item: SyncItem) {
  return item.name.trim() || item.id;
}

function buildDeletePrompt(item: SyncItem) {
  const lines = [
    `确认要删除${getSyncItemTypeLabel(item.dataType)}「${getItemDisplayName(item)}」吗？`,
    "这会把当前远端记录标记为已软删除。",
  ];

  if (isSyncItemType(item.dataType, "workspace")) {
    lines.push("删除工作区时只会解除子级连接关联，不会删除子连接本身。");
  }

  lines.push("如果这条记录已被其他设备更新，则更新优先，本次删除会被拒绝。");

  return lines.join("\n");
}

function buildRestorePrompt(item: SyncItem) {
  const lines = [
    `确认要恢复${getSyncItemTypeLabel(item.dataType)}「${getItemDisplayName(item)}」吗？`,
    "这会清除当前远端记录的软删除标记。",
  ];

  if (isSyncItemType(item.dataType, "workspace")) {
    lines.push("恢复工作区时只会恢复工作区记录本身，不会自动重新关联此前解绑的子连接。");
  }

  lines.push("如果这条记录已被其他设备更新，则会按最新版本校验恢复请求。");

  return lines.join("\n");
}

function applyDeletedItem(itemId: string, version: number, updatedAt: string, deletedAt: string | null) {
  items.value = items.value.map((item) => {
    if (item.id !== itemId) {
      return item;
    }

    return {
      ...item,
      version,
      updatedAt,
      deletedAt,
    };
  });
}

function applyRestoredItem(restoredItem: SyncItem) {
  items.value = items.value.map((item) => {
    if (item.id !== restoredItem.id) {
      return item;
    }

    return restoredItem;
  });
}

function goToPage(page: number) {
  currentPage.value = page;
}

function goToPreviousPage() {
  if (currentPage.value > 1) {
    currentPage.value -= 1;
  }
}

function goToNextPage() {
  if (currentPage.value < totalPages.value) {
    currentPage.value += 1;
  }
}

async function loadItems() {
  loading.value = true;
  errorMessage.value = "";

  try {
    items.value = await api.listSyncItems(auth.token!);
    const hasSelectedType = items.value.some((item) => isSyncItemType(item.dataType, selectedType.value));
    if (selectedType.value !== ALL_TYPES && !hasSelectedType) {
      selectedType.value = ALL_TYPES;
    }
  } catch (error) {
    errorMessage.value = error instanceof ApiError ? error.message : "读取同步项列表失败";
  } finally {
    loading.value = false;
  }
}

async function deleteItem(item: SyncItem) {
  if (deletingItemId.value || restoringItemId.value || item.deletedAt) {
    return;
  }

  const confirmed = window.confirm(buildDeletePrompt(item));
  if (!confirmed) {
    return;
  }

  actionMessage.value = "";
  actionError.value = "";
  deletingItemId.value = item.id;

  try {
    const result = await api.deleteSyncItem(auth.token!, item.id, item.version);
    applyDeletedItem(
      result.item.id,
      result.item.version,
      result.item.updatedAt,
      result.item.deletedAt,
    );
    actionMessage.value = isSyncItemType(item.dataType, "workspace")
      ? "工作区同步项已软删除，子连接会在后续同步中解除关联。"
      : "同步项已软删除。";
  } catch (error) {
    if (error instanceof ApiError && error.status === 409) {
      actionError.value = "删除未生效：该记录已被更新或状态已变化，已刷新为最新数据。";
      await loadItems();
    } else {
      actionError.value = error instanceof ApiError ? error.message : "删除同步项失败";
    }
  } finally {
    deletingItemId.value = "";
  }
}

async function restoreItem(item: SyncItem) {
  if (restoringItemId.value || deletingItemId.value || !item.deletedAt) {
    return;
  }

  const confirmed = window.confirm(buildRestorePrompt(item));
  if (!confirmed) {
    return;
  }

  actionMessage.value = "";
  actionError.value = "";
  restoringItemId.value = item.id;

  try {
    const restored = await api.restoreSyncItem(auth.token!, item);
    applyRestoredItem(restored);
    actionMessage.value = isSyncItemType(item.dataType, "workspace")
      ? "工作区同步项已恢复。注意：此前解绑的子连接不会自动重新关联。"
      : "同步项已恢复。";
  } catch (error) {
    if (error instanceof ApiError && error.status === 409) {
      actionError.value = "恢复未生效：该记录已被更新或状态已变化，已刷新为最新数据。";
      await loadItems();
    } else {
      actionError.value = error instanceof ApiError ? error.message : "恢复同步项失败";
    }
  } finally {
    restoringItemId.value = "";
  }
}

watch(selectedType, () => {
  currentPage.value = 1;
});

watch(selectedStatus, () => {
  currentPage.value = 1;
});

watch(pageSize, () => {
  currentPage.value = 1;
});

watch(
  () => filteredItems.value.length,
  () => {
    if (currentPage.value > totalPages.value) {
      currentPage.value = totalPages.value;
    }
  },
  { immediate: true },
);

onMounted(() => {
  void loadItems();
});
</script>
