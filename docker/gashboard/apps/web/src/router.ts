import { createRouter, createWebHistory, type RouteRecordRaw } from "vue-router";
import { useAuthStore } from "./stores/auth";

const routes: RouteRecordRaw[] = [
  {
    path: "/access",
    name: "access",
    component: () => import("./views/AccessView.vue"),
    meta: { requiresAuth: true },
  },
  {
    path: "/",
    name: "dashboard",
    component: () => import("./views/DashboardView.vue"),
    meta: { requiresAuth: true },
  },
  {
    path: "/graphs",
    name: "graphs",
    component: () => import("./views/GraphsView.vue"),
    meta: { requiresAuth: true },
  },
  {
    path: "/login",
    name: "login",
    component: () => import("./views/LoginView.vue"),
  },
  {
    path: "/auth/nostr-callback",
    name: "nostr-callback",
    component: () => import("./views/NostrCallbackView.vue"),
  },
];

export const router = createRouter({
  history: createWebHistory(),
  routes,
});

router.beforeEach((to) => {
  const auth = useAuthStore();
  if (to.meta.requiresAuth && !auth.isLoggedIn) return { name: "login" };
  if (to.name === "login" && auth.isLoggedIn) return { name: "dashboard" };
  return true;
});
