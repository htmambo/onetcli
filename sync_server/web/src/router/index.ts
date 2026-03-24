import { createRouter, createWebHistory } from "vue-router";
import { useAuthStore } from "@/stores/auth";

const routes = [
  {
    path: "/",
    redirect: "/app",
  },
  {
    path: "/login",
    name: "login",
    component: () => import("@/views/auth/LoginView.vue"),
    meta: {
      guestOnly: true,
    },
  },
  {
    path: "/register",
    name: "register",
    component: () => import("@/views/auth/RegisterView.vue"),
    meta: {
      guestOnly: true,
    },
  },
  {
    path: "/app",
    component: () => import("@/layouts/AppLayout.vue"),
    meta: {
      requiresAuth: true,
    },
    children: [
      {
        path: "",
        name: "dashboard",
        component: () => import("@/views/user/DashboardView.vue"),
      },
      {
        path: "profile",
        name: "profile",
        component: () => import("@/views/user/ProfileView.vue"),
      },
      {
        path: "admin",
        name: "admin",
        component: () => import("@/views/admin/AdminOverviewView.vue"),
        meta: {
          requiresAdmin: true,
        },
      },
    ],
  },
];

export const router = createRouter({
  history: createWebHistory(),
  routes,
});

router.beforeEach(async (to) => {
  const auth = useAuthStore();
  await auth.initialize();

  if (to.meta.requiresAuth && !auth.isAuthenticated) {
    return { name: "login" };
  }

  if (to.meta.guestOnly && auth.isAuthenticated) {
    return { name: "dashboard" };
  }

  if (to.meta.requiresAdmin && !auth.isAdmin) {
    return { name: "dashboard" };
  }

  return true;
});
