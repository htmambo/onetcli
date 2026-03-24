<template>
  <section class="panel rounded-3xl p-5">
    <p class="text-xs uppercase tracking-[0.28em] text-[var(--muted)]">{{ label }}</p>
    <div class="mt-4 flex items-end justify-between gap-4">
      <strong class="text-3xl font-semibold text-[var(--text)]">{{ value }}</strong>
      <span
        class="rounded-full px-3 py-1 text-xs font-medium"
        :class="toneClass"
      >
        {{ toneText }}
      </span>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed } from "vue";

const props = defineProps<{
  label: string;
  value: string | number;
  tone?: "neutral" | "success" | "danger";
  toneText?: string;
}>();

const toneClass = computed(() => {
  switch (props.tone) {
    case "success":
      return "bg-emerald-100 text-emerald-700";
    case "danger":
      return "bg-rose-100 text-rose-700";
    default:
      return "bg-white/70 text-[var(--muted)]";
  }
});
</script>
