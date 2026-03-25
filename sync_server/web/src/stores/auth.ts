import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { api, ApiError } from "@/services/api";
import type { PublicUser } from "@/types/api";

const TOKEN_KEY = "sync_server_token";

export const useAuthStore = defineStore("auth", () => {
  const token = ref<string | null>(localStorage.getItem(TOKEN_KEY));
  const user = ref<PublicUser | null>(null);
  const ready = ref(false);

  const isAuthenticated = computed(() => Boolean(token.value && user.value));
  const isAdmin = computed(() => user.value?.role === "admin");

  function setSession(nextToken: string, nextUser: PublicUser) {
    token.value = nextToken;
    user.value = nextUser;
    localStorage.setItem(TOKEN_KEY, nextToken);
  }

  function replaceUser(nextUser: PublicUser) {
    user.value = nextUser;
  }

  function clearSession() {
    token.value = null;
    user.value = null;
    localStorage.removeItem(TOKEN_KEY);
  }

  async function initialize() {
    if (ready.value) {
      return;
    }

    if (!token.value) {
      ready.value = true;
      return;
    }

    try {
      user.value = await api.me(token.value);
    } catch {
      clearSession();
    } finally {
      ready.value = true;
    }
  }

  async function register(email: string, password: string) {
    const payload = await api.register(email, password);
    setSession(payload.token, payload.user);
  }

  async function login(email: string, password: string) {
    const payload = await api.login(email, password);
    setSession(payload.token, payload.user);
  }

  async function refreshMe() {
    if (!token.value) {
      return;
    }
    user.value = await api.me(token.value);
  }

  async function logout() {
    if (token.value) {
      try {
        await api.logout(token.value);
      } catch {
        // 忽略登出失败，仍清空本地状态
      }
    }

    clearSession();
  }

  return {
    token,
    user,
    ready,
    isAuthenticated,
    isAdmin,
    initialize,
    register,
    login,
    refreshMe,
    replaceUser,
    logout,
    clearSession,
  };
});
