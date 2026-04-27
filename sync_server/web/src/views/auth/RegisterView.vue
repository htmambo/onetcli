<template>
  <div class="flex min-h-screen items-center justify-center px-4 py-10">
    <section class="panel hover-card w-full max-w-2xl rounded-[32px] p-8 lg:p-10">
      <p class="text-xs uppercase tracking-[0.32em] text-[var(--muted)]">创建账号</p>
      <h1 class="mt-3 text-3xl font-semibold text-[var(--text)]">注册新的同步账号</h1>
      <p class="mt-3 text-sm leading-7 text-[var(--muted)]">
        注册后即可配置同步密钥，并通过同一账号在不同平台之间保持内容一致。
      </p>

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

        <label class="block">
          <span class="mb-2 block text-sm font-medium text-[var(--text)]">确认密码</span>
          <input
            v-model="confirmPassword"
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
          {{ submitting ? "注册中..." : "注册并进入" }}
        </button>
      </form>

      <p class="mt-5 text-sm text-[var(--muted)]">
        已有账号？
        <RouterLink class="link-accent font-semibold" to="/login">返回登录</RouterLink>
      </p>
    </section>
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
const confirmPassword = ref("");
const errorMessage = ref("");
const submitting = ref(false);

async function handleSubmit() {
  errorMessage.value = "";

  if (password.value !== confirmPassword.value) {
    errorMessage.value = "两次输入的密码不一致";
    return;
  }

  submitting.value = true;

  try {
    await auth.register(email.value, password.value);
    await router.push("/app");
  } catch (error) {
    errorMessage.value = error instanceof ApiError ? error.message : "注册失败";
  } finally {
    submitting.value = false;
  }
}
</script>
