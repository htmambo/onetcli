<template>
  <div class="flex min-h-screen items-center justify-center px-4 py-10">
    <div class="grid w-full max-w-5xl gap-6 lg:grid-cols-[1.1fr_0.9fr]">
      <section class="panel hover-card rounded-[32px] p-8 lg:p-10">
        <p class="text-xs uppercase tracking-[0.32em] text-[var(--muted)]">Sync Server</p>
        <h1 class="mt-4 text-4xl font-semibold leading-tight text-[var(--text)]">
          登录后统一管理你的跨平台同步状态
        </h1>
        <p class="mt-4 max-w-xl text-sm leading-7 text-[var(--muted)]">
          一个账号、一套云端状态。登录后即可管理同步密钥、查看同步内容，并在需要时清空当前账号的数据。
        </p>
        <div class="mt-10 grid gap-4 md:grid-cols-3">
          <div class="hover-card rounded-3xl border border-[var(--line)] bg-[var(--panel-strong)] p-5">
            <p class="text-sm font-semibold text-[var(--text)]">账号登录</p>
            <p class="mt-2 text-sm text-[var(--muted)]">统一身份入口，适合多平台复用。</p>
          </div>
          <div class="hover-card rounded-3xl border border-[var(--line)] bg-[var(--panel-strong)] p-5">
            <p class="text-sm font-semibold text-[var(--text)]">同步密钥</p>
            <p class="mt-2 text-sm text-[var(--muted)]">与账号解耦，适合独立管理同步内容。</p>
          </div>
          <div class="hover-card rounded-3xl border border-[var(--line)] bg-[var(--panel-strong)] p-5">
            <p class="text-sm font-semibold text-[var(--text)]">管理界面</p>
            <p class="mt-2 text-sm text-[var(--muted)]">管理员可以直接管理账号和同步数据。</p>
          </div>
        </div>
      </section>

      <section class="panel hover-card rounded-[32px] p-8 lg:p-10">
        <p class="text-xs uppercase tracking-[0.32em] text-[var(--muted)]">账号登录</p>
        <h2 class="mt-3 text-2xl font-semibold text-[var(--text)]">进入控制台</h2>
        <form class="mt-8 space-y-5" @submit.prevent="handleSubmit">
          <label class="block">
            <span class="mb-2 block text-sm font-medium text-[var(--text)]">邮箱</span>
            <input
              v-model="email"
              type="email"
              required
              class="input-shell w-full rounded-2xl border px-4 py-3 outline-none"
            />
          </label>

          <label class="block">
            <span class="mb-2 block text-sm font-medium text-[var(--text)]">密码</span>
            <input
              v-model="password"
              type="password"
              required
              minlength="8"
              class="input-shell w-full rounded-2xl border px-4 py-3 outline-none"
            />
          </label>

          <p v-if="errorMessage" class="feedback-danger rounded-2xl px-4 py-3 text-sm">
            {{ errorMessage }}
          </p>

          <button
            type="submit"
            class="accent-button w-full rounded-2xl px-4 py-3 text-sm font-semibold text-white transition"
            :disabled="submitting"
          >
            {{ submitting ? "登录中..." : "登录" }}
          </button>
        </form>

        <p class="mt-5 text-sm text-[var(--muted)]">
          还没有账号？
          <RouterLink class="link-accent font-semibold" to="/register">立即注册</RouterLink>
        </p>
      </section>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { useRouter } from "vue-router";
import { ApiError } from "@/services/api";
import { useAuthStore } from "@/stores/auth";

const router = useRouter();
const auth = useAuthStore();

const email = ref("");
const password = ref("");
const errorMessage = ref("");
const submitting = ref(false);

async function handleSubmit() {
  submitting.value = true;
  errorMessage.value = "";

  try {
    await auth.login(email.value, password.value);
    await router.push("/app");
  } catch (error) {
    errorMessage.value = error instanceof ApiError ? error.message : "登录失败";
  } finally {
    submitting.value = false;
  }
}
</script>
