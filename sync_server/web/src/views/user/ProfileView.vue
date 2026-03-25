<template>
  <div class="grid gap-6 xl:grid-cols-[1fr_1fr]">
    <section class="panel rounded-[28px] p-6">
      <p class="text-xs uppercase tracking-[0.32em] text-[var(--muted)]">资料设置</p>
      <h2 class="mt-3 text-2xl font-semibold text-[var(--text)]">设置昵称</h2>
      <p class="mt-3 text-sm leading-7 text-[var(--muted)]">
        昵称默认与邮箱相同。你可以把它改成更容易识别的名称，修改后会立即同步到当前账号信息展示。
      </p>

      <form class="mt-8 space-y-4" @submit.prevent="submitNickname">
        <label class="block">
          <span class="mb-2 block text-sm font-medium text-[var(--text)]">昵称</span>
          <input
            v-model="nickname"
            type="text"
            maxlength="255"
            required
            class="w-full rounded-2xl border border-[var(--line)] bg-white/80 px-4 py-3 outline-none transition focus:border-[var(--accent)] focus:accent-ring"
            placeholder="输入昵称"
          />
        </label>

        <p v-if="nicknameMessage" class="rounded-2xl bg-emerald-50 px-4 py-3 text-sm text-emerald-700">
          {{ nicknameMessage }}
        </p>
        <p v-if="nicknameError" class="rounded-2xl bg-rose-50 px-4 py-3 text-sm text-rose-700">
          {{ nicknameError }}
        </p>

        <button class="accent-button rounded-2xl px-4 py-3 text-sm font-semibold text-white" :disabled="submittingNickname">
          {{ submittingNickname ? "保存中..." : "保存昵称" }}
        </button>
      </form>
    </section>

    <section class="panel rounded-[28px] p-6">
      <p class="text-xs uppercase tracking-[0.32em] text-[var(--muted)]">账号设置</p>
      <h2 class="mt-3 text-2xl font-semibold text-[var(--text)]">修改密码</h2>
      <p class="mt-3 text-sm leading-7 text-[var(--muted)]">
        这里修改的是账号登录密码，不会自动替换当前已保存的同步密钥内容。
      </p>

      <form class="mt-8 space-y-4" @submit.prevent="submitPassword">
        <label class="block">
          <span class="mb-2 block text-sm font-medium text-[var(--text)]">当前密码</span>
          <input
            v-model="currentPassword"
            type="password"
            minlength="8"
            required
            class="w-full rounded-2xl border border-[var(--line)] bg-white/80 px-4 py-3 outline-none transition focus:border-[var(--accent)] focus:accent-ring"
          />
        </label>

        <label class="block">
          <span class="mb-2 block text-sm font-medium text-[var(--text)]">新密码</span>
          <input
            v-model="nextPassword"
            type="password"
            minlength="8"
            required
            class="w-full rounded-2xl border border-[var(--line)] bg-white/80 px-4 py-3 outline-none transition focus:border-[var(--accent)] focus:accent-ring"
          />
        </label>

        <p v-if="passwordMessage" class="rounded-2xl bg-emerald-50 px-4 py-3 text-sm text-emerald-700">
          {{ passwordMessage }}
        </p>
        <p v-if="passwordError" class="rounded-2xl bg-rose-50 px-4 py-3 text-sm text-rose-700">
          {{ passwordError }}
        </p>

        <button class="accent-button rounded-2xl px-4 py-3 text-sm font-semibold text-white" :disabled="submittingPassword">
          {{ submittingPassword ? "提交中..." : "修改密码" }}
        </button>
      </form>
    </section>

    <section class="panel rounded-[28px] p-6 xl:col-span-2">
      <p class="text-xs uppercase tracking-[0.32em] text-[var(--muted)]">同步数据管理</p>
      <h2 class="mt-3 text-2xl font-semibold text-[var(--text)]">清空当前账号云端数据</h2>
      <p class="mt-3 text-sm leading-7 text-[var(--muted)]">
        该操作会删除当前账号的同步配置和全部同步项。适合重置同步状态，但不会删除账号本身。
      </p>

      <div class="mt-8 rounded-3xl border border-rose-200 bg-rose-50 p-5">
        <p class="text-sm font-semibold text-rose-800">危险操作</p>
        <p class="mt-2 text-sm leading-7 text-rose-700">
          删除后，其他设备将无法继续使用原有云端同步状态，需要重新写入同步配置。
        </p>
      </div>

      <p v-if="clearMessage" class="mt-5 rounded-2xl bg-emerald-50 px-4 py-3 text-sm text-emerald-700">
        {{ clearMessage }}
      </p>
      <p v-if="clearError" class="mt-5 rounded-2xl bg-rose-50 px-4 py-3 text-sm text-rose-700">
        {{ clearError }}
      </p>

      <button
        class="mt-6 rounded-2xl bg-rose-600 px-4 py-3 text-sm font-semibold text-white transition hover:bg-rose-700"
        :disabled="clearing"
        @click="clearData"
      >
        {{ clearing ? "清理中..." : "清空我的同步数据" }}
      </button>
    </section>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from "vue";
import { ApiError, api } from "@/services/api";
import { useAuthStore } from "@/stores/auth";

const auth = useAuthStore();

const nickname = ref("");
const nicknameMessage = ref("");
const nicknameError = ref("");
const submittingNickname = ref(false);

const currentPassword = ref("");
const nextPassword = ref("");
const passwordMessage = ref("");
const passwordError = ref("");
const submittingPassword = ref(false);

const clearMessage = ref("");
const clearError = ref("");
const clearing = ref(false);

watch(
  () => auth.user?.nickname,
  (nextNickname) => {
    nickname.value = nextNickname ?? "";
  },
  { immediate: true },
);

async function submitNickname() {
  nicknameMessage.value = "";
  nicknameError.value = "";
  submittingNickname.value = true;

  try {
    const nextUser = await api.updateProfile(auth.token!, nickname.value);
    auth.replaceUser(nextUser);
    nickname.value = nextUser.nickname;
    nicknameMessage.value = "昵称已更新";
  } catch (error) {
    nicknameError.value = error instanceof ApiError ? error.message : "更新昵称失败";
  } finally {
    submittingNickname.value = false;
  }
}

async function submitPassword() {
  passwordMessage.value = "";
  passwordError.value = "";
  submittingPassword.value = true;

  try {
    await api.changePassword(auth.token!, currentPassword.value, nextPassword.value);
    passwordMessage.value = "密码已更新";
    currentPassword.value = "";
    nextPassword.value = "";
  } catch (error) {
    passwordError.value = error instanceof ApiError ? error.message : "修改密码失败";
  } finally {
    submittingPassword.value = false;
  }
}

async function clearData() {
  const confirmed = window.confirm("确认要清空当前账号的同步配置和同步数据吗？");
  if (!confirmed) {
    return;
  }

  clearMessage.value = "";
  clearError.value = "";
  clearing.value = true;

  try {
    await api.clearSyncData(auth.token!);
    clearMessage.value = "云端同步数据已清空";
  } catch (error) {
    clearError.value = error instanceof ApiError ? error.message : "清空同步数据失败";
  } finally {
    clearing.value = false;
  }
}
</script>
