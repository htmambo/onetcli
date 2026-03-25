import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./app/App.vue";
import { router } from "./router";
import "./style.css";

const app = createApp(App);
app.use(createPinia());
app.use(router);
app.mount("#app");

// 为 hover 卡片同步鼠标位置，驱动聚光灯效果。
window.addEventListener("pointermove", (event) => {
  const target = event.target;
  if (!(target instanceof Element)) {
    return;
  }

  const card = target.closest(".hover-card") as HTMLElement | null;
  if (!card) {
    return;
  }

  const rect = card.getBoundingClientRect();
  card.style.setProperty("--spotlight-x", `${event.clientX - rect.left}px`);
  card.style.setProperty("--spotlight-y", `${event.clientY - rect.top}px`);
});
