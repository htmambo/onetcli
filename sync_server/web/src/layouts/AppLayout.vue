<template>
  <div class="min-h-screen px-4 py-6 md:px-6">
    <div class="mx-auto grid max-w-7xl gap-6 lg:grid-cols-[280px_minmax(0,1fr)]">
      <aside class="panel rounded-[28px] p-6">
        <div class="border-b soft-line pb-5">
          <p class="text-xs uppercase tracking-[0.32em] text-[var(--muted)]">Sync Server</p>
          <h1 class="mt-3 text-2xl font-semibold text-[var(--text)]">账号与同步中心</h1>
          <p class="mt-2 text-sm leading-6 text-[var(--muted)]">
            统一管理账户、同步密钥和云端数据。
          </p>
        </div>

        <div class="mt-6 rounded-2xl bg-white/70 p-4">
          <p class="text-xs uppercase tracking-[0.28em] text-[var(--muted)]">当前账号</p>
          <p class="mt-3 text-base font-semibold text-[var(--text)]">{{ auth.user?.email }}</p>
          <p class="mt-1 text-sm text-[var(--muted)]">
            {{ auth.user?.role === "admin" ? "管理员" : "普通用户" }}
          </p>
        </div>

        <nav class="mt-6 flex flex-col gap-2">
          <RouterLink
            v-for="item in links"
            :key="item.to"
            :to="item.to"
            class="rounded-2xl px-4 py-3 text-sm font-medium transition"
            :class="route.path === item.to ? 'bg-[var(--accent-soft)] text-[var(--accent-deep)]' : 'text-[var(--muted)] hover:bg-white/60'"
          >
            {{ item.label }}
          </RouterLink>
        </nav>

        <button
          class="mt-8 w-full rounded-2xl border border-[var(--line)] bg-white/60 px-4 py-3 text-sm font-medium text-[var(--text)] transition hover:bg-white"
          @click="handleLogout"
        >
          退出登录
        </button>
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

const links = computed(() => {
  const base = [
    { to: "/app", label: "同步概览" },
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
</script>
