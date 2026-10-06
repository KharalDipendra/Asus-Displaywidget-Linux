<script setup lang="ts">
const paths = {
  monitor: "M3 4h18a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1z M8 21h8 M12 17v4",
  "monitor-off": "M3 4h18a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1z M8 21h8 M12 17v4 M9 8l6 6 M15 8l-6 6",
  sun: "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z M12 2v2 M12 20v2 M4.93 4.93l1.41 1.41 M17.66 17.66l1.41 1.41 M2 12h2 M20 12h2 M6.34 17.66l-1.41 1.41 M19.07 4.93l-1.41 1.41",
  contrast: "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z M12 2v20 M12 7h5 M12 12h6 M12 17h5",
  palette: "M12 22a10 10 0 1 1 10-10c0 2.8-2.2 4-4 4h-1.8a2 2 0 0 0-1.5 3.3A1.6 1.6 0 0 1 12 22z M7.5 10.5h.01 M10.5 7h.01 M15.5 8h.01",
  game: "M6 12h4 M8 10v4 M15 13h.01 M18 11h.01 M17.32 5H6.68a4 4 0 0 0-3.98 3.59l-.7 6.06A3 3 0 0 0 5 18c1 0 1.5-.5 2-1l1.41-1.41A2 2 0 0 1 9.83 15h4.34a2 2 0 0 1 1.42.59L17 17c.5.5 1 1 2 1a3 3 0 0 0 3-3.35l-.7-6.06A4 4 0 0 0 17.32 5z",
  eye: "M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z",
  plug: "M12 22v-5 M9 8V2 M15 8V2 M18 8v5a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4V8z",
  sliders: "M4 21v-7 M4 10V3 M12 21v-9 M12 8V3 M20 21v-5 M20 12V3 M1 14h6 M9 8h6 M17 16h6",
  volume: "M11 5L6 9H2v6h4l5 4V5z M15.54 8.46a5 5 0 0 1 0 7.07 M19.07 4.93a10 10 0 0 1 0 14.14",
  refresh: "M21 12a9 9 0 1 1-2.64-6.36L21 8 M21 3v5h-5",
  reset: "M3 12a9 9 0 1 0 2.64-6.36L3 8 M3 3v5h5",
  alert: "M12 9v4 M12 17h.01 M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z",
  "alert-circle": "M12 8v4 M12 16h.01 M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z",
  ban: "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z M4.93 4.93l14.14 14.14",
  chevron: "M6 9l6 6 6-6",
  x: "M18 6L6 18 M6 6l12 12",
  pip: "M3 4h18a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1z M13 11h7v6h-7z",
  info: "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z M12 11v5 M12 7.5h.01",
  shield: "M12 2l8 3v6c0 5-3.5 8.5-8 11-4.5-2.5-8-6-8-11V5z M9 12l2 2 4-4",
  bug: "M9 7h6a3 3 0 0 1 3 3v3a6 6 0 0 1-12 0v-3a3 3 0 0 1 3-3z M8 2l1.5 2.5 M16 2l-1.5 2.5 M3 9h3 M18 9h3 M3 14h3 M18 14h3 M12 7v12",
} as const;

export type IconName = keyof typeof paths;
defineProps<{ name: IconName }>();
</script>

<template>
  <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <path :d="paths[name]" />
  </svg>
</template>
