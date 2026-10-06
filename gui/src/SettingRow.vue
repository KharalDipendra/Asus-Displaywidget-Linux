<script setup lang="ts">
import { computed, ref, watchEffect } from "vue";
import type { Setting } from "./api";
import Icon, { type IconName } from "./Icon.vue";

const props = defineProps<{ setting: Setting; disabled: boolean; icon?: IconName }>();
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

const pct = computed(() => `${props.setting.max ? (level.value / props.setting.max) * 100 : 0}%`);
// Short option lists read better as a segmented control; long ones fall back to a dropdown.
const segmented = computed(() => {
  const s = props.setting;
  return s.kind === "enum" && s.choices.length <= 4 && s.choices.every((c) => c.label.length <= 12);
});
const stacked = computed(() => !props.setting.error && !props.setting.unavailable && (props.setting.kind === "continuous" || segmented.value));
</script>

<template>
  <div class="row" :class="{ stacked, disabled: setting.error || setting.unavailable }" :title="setting.note ?? undefined">
    <template v-if="setting.error">
      <label>{{ setting.label }}</label>
      <span class="muted err" :title="setting.error"><Icon name="alert-circle" />Couldn't read</span>
    </template>

    <template v-else-if="setting.unavailable">
      <label>{{ setting.label }}</label>
      <span class="muted"><Icon name="ban" />Not available in this mode</span>
    </template>

    <template v-else-if="setting.kind === 'continuous'">
      <div class="row-head">
        <label :for="setting.key"><Icon v-if="icon" :name="icon" />{{ setting.label }}</label>
        <output :for="setting.key">{{ level }}</output>
      </div>
      <input
        :id="setting.key"
        v-model.number="level"
        type="range"
        min="0"
        :max="setting.max"
        :style="{ '--pct': pct }"
        :disabled="disabled"
        @change="emit('change', String(level))"
      />
    </template>

    <template v-else-if="segmented">
      <div class="row-head">
        <label>{{ setting.label }}</label>
        <Icon v-if="setting.force" name="alert" class="warn-icon" />
      </div>
      <div class="segmented" role="radiogroup" :aria-label="setting.label">
        <button
          v-for="c in setting.choices"
          :key="c.name"
          role="radio"
          :aria-checked="c.name === setting.selected"
          :class="{ on: c.name === setting.selected }"
          :disabled="disabled"
          @click="c.name !== setting.selected && emit('change', c.name)"
        >
          {{ c.label }}
        </button>
      </div>
    </template>

    <template v-else-if="setting.kind === 'enum'">
      <label :for="setting.key">{{ setting.label }}</label>
      <div class="select">
        <select :id="setting.key" v-model="choice" :disabled="disabled" @change="emit('change', choice)">
          <option v-if="!setting.selected" value="" disabled>{{ setting.text }}</option>
          <option v-for="c in setting.choices" :key="c.name" :value="c.name">{{ c.label }}</option>
        </select>
        <Icon name="chevron" />
      </div>
    </template>

    <template v-else-if="setting.kind === 'toggle'">
      <label :for="setting.key">{{ setting.label }}</label>
      <input
        :id="setting.key"
        v-model="on"
        type="checkbox"
        role="switch"
        :disabled="disabled"
        @change="emit('change', on ? 'on' : 'off')"
      />
    </template>

    <template v-else-if="setting.kind === 'action'">
      <label>{{ setting.label }}</label>
      <div class="actions">
        <button v-for="c in setting.choices" :key="c.name" :disabled="disabled" @click="emit('change', c.name)">
          <Icon name="reset" />{{ c.label }}
        </button>
      </div>
    </template>

    <template v-else>
      <label>{{ setting.label }}</label>
      <span :id="setting.key" class="info">{{ setting.text }}</span>
    </template>
  </div>
</template>
