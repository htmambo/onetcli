<template>
  <section class="panel hover-card rounded-3xl p-5">
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
      return "status-success";
    case "danger":
      return "status-danger";
    default:
      return "status-neutral";
  }
});
</script>
