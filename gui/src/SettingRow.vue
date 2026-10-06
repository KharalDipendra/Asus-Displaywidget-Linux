<script setup lang="ts">
import { ref, watchEffect } from "vue";
import type { Setting } from "./api";

const props = defineProps<{ setting: Setting; disabled: boolean }>();
const emit = defineEmits<{ change: [value: string] }>();

// Controls move freely and snap back to the monitor's value whenever a new reading arrives.
const level = ref(0);
const on = ref(false);
const choice = ref("");
watchEffect(() => {
  const s = props.setting;
  level.value = s.current ?? 0;
  on.value = s.on ?? false;
  choice.value = s.selected ?? "";
});
</script>

<template>
  <div class="row" :title="setting.note ?? undefined">
    <label :for="setting.key">{{ setting.label }}</label>

    <span v-if="setting.error" class="muted" :title="setting.error">Couldn't read</span>
    <span v-else-if="setting.unavailable" class="muted">Not available in this mode</span>

    <div v-else-if="setting.kind === 'continuous'" class="slider">
      <input
        :id="setting.key"
        v-model.number="level"
        type="range"
        min="0"
        :max="setting.max"
        :disabled="disabled"
        @change="emit('change', String(level))"
      />
      <output :for="setting.key">{{ level }}</output>
    </div>

    <select v-else-if="setting.kind === 'enum'" :id="setting.key" v-model="choice" :disabled="disabled" @change="emit('change', choice)">
      <option v-if="!setting.selected" value="" disabled>{{ setting.text }}</option>
      <option v-for="c in setting.choices" :key="c.name" :value="c.name">{{ c.label }}</option>
    </select>

    <input
      v-else-if="setting.kind === 'toggle'"
      :id="setting.key"
      v-model="on"
      type="checkbox"
      role="switch"
      :disabled="disabled"
      @change="emit('change', on ? 'on' : 'off')"
    />

    <div v-else-if="setting.kind === 'action'" class="actions">
      <button v-for="c in setting.choices" :key="c.name" :disabled="disabled" @click="emit('change', c.name)">
        {{ c.label }}
      </button>
    </div>

    <span v-else :id="setting.key">{{ setting.text }}</span>
  </div>
</template>
