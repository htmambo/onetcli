<template>
  <div class="space-y-6">
    <section class="panel hover-card rounded-[28px] p-6">
      <p class="text-xs uppercase tracking-[0.32em] text-[var(--muted)]">管理界面</p>
      <div class="mt-4 flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
        <div>
          <h2 class="text-3xl font-semibold text-[var(--text)]">账号与同步数据管理</h2>
          <p class="mt-2 max-w-2xl text-sm leading-7 text-[var(--muted)]">
            管理员可以查看账号列表、禁用账号、切换角色，以及清理某个账号的同步配置和同步数据。
          </p>
        </div>
        <button
          class="ghost-button rounded-2xl px-4 py-3 text-sm font-medium"
          @click="loadData"
        >
          刷新管理数据
        </button>
      </div>
    </section>

    <section class="grid gap-4 md:grid-cols-4">
      <StatCard label="账号总数" :value="overview?.users ?? 0" tone="success" tone-text="总览" />
      <StatCard label="活跃账号" :value="overview?.activeUsers ?? 0" />
      <StatCard label="同步项总数" :value="overview?.syncItems ?? 0" />
      <StatCard label="同步配置数" :value="overview?.syncConfigs ?? 0" />
    </section>

    <section class="panel hover-card rounded-[28px] p-6">
      <div class="flex items-center justify-between">
        <div>
          <p class="text-xs uppercase tracking-[0.28em] text-[var(--muted)]">账号列表</p>
          <h3 class="mt-2 text-xl font-semibold text-[var(--text)]">用户与同步状态</h3>
        </div>
        <span class="status-neutral rounded-full px-3 py-1 text-xs font-medium">
          {{ users.length }} 个账号
        </span>
      </div>

      <p v-if="errorMessage" class="feedback-danger mt-5 rounded-2xl px-4 py-3 text-sm">
        {{ errorMessage }}
      </p>

      <div class="mt-6 overflow-x-auto">
        <table class="min-w-full text-left text-sm">
          <thead class="text-[var(--muted)]">
            <tr class="border-b soft-line">
              <th class="px-3 py-3 font-medium">昵称 / 邮箱</th>
              <th class="px-3 py-3 font-medium">角色</th>
              <th class="px-3 py-3 font-medium">状态</th>
              <th class="px-3 py-3 font-medium">同步配置</th>
              <th class="px-3 py-3 font-medium">同步项</th>
              <th class="px-3 py-3 font-medium">最近登录</th>
              <th class="px-3 py-3 font-medium">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="user in users" :key="user.id" class="border-b soft-line last:border-none">
              <td class="px-3 py-4">
                <p class="font-medium text-[var(--text)]">{{ user.nickname }}</p>
                <p v-if="user.nickname !== user.email" class="mt-1 text-xs text-[var(--muted)]">{{ user.email }}</p>
              </td>
              <td class="px-3 py-4">{{ user.role }}</td>
              <td class="px-3 py-4">
                <span
                  class="rounded-full px-3 py-1 text-xs font-medium"
                  :class="user.status === 'active' ? 'status-success' : 'status-danger'"
                >
                  {{ user.status }}
                </span>
              </td>
              <td class="px-3 py-4">{{ user.hasSyncConfig ? "已配置" : "未配置" }}</td>
              <td class="px-3 py-4">{{ user.syncItemCount }}</td>
              <td class="px-3 py-4 text-[var(--muted)]">
                {{ user.lastLoginAt ? formatDate(user.lastLoginAt) : "未登录" }}
              </td>
              <td class="px-3 py-4">
                <div class="flex flex-wrap gap-2">
                  <button
                    class="inline-button rounded-xl px-3 py-2 text-xs font-medium"
                    @click="toggleStatus(user.id, user.status)"
                  >
                    {{ user.status === "active" ? "禁用" : "启用" }}
                  </button>
                  <button
                    class="inline-button rounded-xl px-3 py-2 text-xs font-medium"
                    @click="toggleRole(user.id, user.role)"
                  >
                    {{ user.role === "admin" ? "降为用户" : "升为管理员" }}
                  </button>
                  <button
                    class="danger-button rounded-xl px-3 py-2 text-xs font-medium text-white"
                    @click="purgeData(user.id, user.nickname, user.email)"
                  >
                    清空数据
                  </button>
                </div>
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
import StatCard from "@/components/StatCard.vue";
import { ApiError, api } from "@/services/api";
import { useAuthStore } from "@/stores/auth";
import type { AdminOverview, AdminUserSummary } from "@/types/api";

const auth = useAuthStore();

const overview = ref<AdminOverview | null>(null);
const users = ref<AdminUserSummary[]>([]);
const errorMessage = ref("");

function formatDate(value: string) {
  return new Date(value).toLocaleString("zh-CN");
}

async function loadData() {
  errorMessage.value = "";

  try {
    const token = auth.token!;
    const [nextOverview, nextUsers] = await Promise.all([
      api.getAdminOverview(token),
      api.listAdminUsers(token),
    ]);
    overview.value = nextOverview;
    users.value = nextUsers;
  } catch (error) {
    errorMessage.value = error instanceof ApiError ? error.message : "读取管理数据失败";
  }
}

async function toggleStatus(userId: string, current: AdminUserSummary["status"]) {
  try {
    await api.updateAdminUser(auth.token!, userId, {
      status: current === "active" ? "disabled" : "active",
    });
    await loadData();
  } catch (error) {
    errorMessage.value = error instanceof ApiError ? error.message : "更新账号状态失败";
  }
}

async function toggleRole(userId: string, current: AdminUserSummary["role"]) {
  try {
    await api.updateAdminUser(auth.token!, userId, {
      role: current === "admin" ? "user" : "admin",
    });
    await loadData();
  } catch (error) {
    errorMessage.value = error instanceof ApiError ? error.message : "更新账号角色失败";
  }
}

async function purgeData(userId: string, nickname: string, email: string) {
  const label = nickname === email ? email : `${nickname}（${email}）`;
  const confirmed = window.confirm(`确认要清空 ${label} 的同步配置和同步数据吗？`);
  if (!confirmed) {
    return;
  }

  try {
    await api.purgeAdminUserData(auth.token!, userId);
    await loadData();
  } catch (error) {
    errorMessage.value = error instanceof ApiError ? error.message : "清空用户数据失败";
  }
}

onMounted(() => {
  void loadData();
});
</script>
