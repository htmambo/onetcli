<template>
  <div class="min-h-screen px-4 py-6 md:px-6">
    <div class="mx-auto grid max-w-7xl gap-6 lg:grid-cols-[280px_minmax(0,1fr)]">
      <aside class="panel flex flex-col rounded-[28px] p-6">
        <div class="border-b soft-line pb-5">
          <p class="text-xs uppercase tracking-[0.32em] text-[var(--muted)]">Sync Server</p>
          <h1 class="mt-3 text-2xl font-semibold text-[var(--text)]">账号与同步中心</h1>
          <p class="mt-2 text-sm leading-6 text-[var(--muted)]">
            统一管理账户、同步密钥和云端数据。
          </p>
        </div>

        <nav class="mt-6 flex flex-col gap-2">
          <RouterLink
            v-for="item in links"
            :key="item.to"
            :to="item.to"
            class="rounded-2xl px-4 py-3 text-sm font-medium transition"
            :class="isActive(item.to) ? 'bg-[var(--accent-soft)] text-[var(--accent-deep)]' : 'text-[var(--muted)] hover:bg-white/60'"
          >
            {{ item.label }}
          </RouterLink>
        </nav>

        <div class="mt-auto pt-6">
          <RouterLink
            to="/app/profile"
            class="block rounded-2xl border px-4 py-4 transition"
            :class="isActive('/app/profile') ? 'border-[var(--accent)] bg-[var(--accent-soft)]' : 'border-[var(--line)] bg-white/70 hover:bg-white'"
          >
            <p class="text-xs uppercase tracking-[0.28em] text-[var(--muted)]">当前账号</p>
            <p class="mt-3 text-base font-semibold text-[var(--text)]">{{ displayName }}</p>
            <p v-if="showEmail" class="mt-1 text-sm text-[var(--muted)]">{{ auth.user?.email }}</p>
            <div class="mt-2 flex items-center justify-between gap-3 text-sm text-[var(--muted)]">
              <span>{{ auth.user?.role === "admin" ? "管理员" : "普通用户" }}</span>
              <span>进入账号设置</span>
            </div>
          </RouterLink>

        <button
          class="mt-4 w-full rounded-2xl border border-[var(--line)] bg-white/60 px-4 py-3 text-sm font-medium text-[var(--text)] transition hover:bg-white"
          @click="handleLogout"
        >
          退出登录
        </button>
        </div>
      </aside>

      <main class="min-w-0">
        <RouterView />
      </main>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useAuthStore } from "@/stores/auth";

const auth = useAuthStore();
const route = useRoute();
const router = useRouter();

const displayName = computed(() => auth.user?.nickname ?? auth.user?.email ?? "");
const showEmail = computed(() => Boolean(auth.user?.nickname && auth.user.nickname !== auth.user.email));

const links = computed(() => {
  const base = [
    { to: "/app", label: "同步概览" },
    { to: "/app/sync-items", label: "全部同步项" },
    { to: "/app/profile", label: "账号设置" },
  ];

  if (auth.isAdmin) {
    base.push({ to: "/app/admin", label: "管理界面" });
  }

  return base;
});

async function handleLogout() {
  await auth.logout();
  await router.push("/login");
}

function isActive(path: string) {
  if (path === "/app/sync-items") {
    return route.path === path || route.path.startsWith(`${path}/`);
  }

  return route.path === path;
}
</script>
