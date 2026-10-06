<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import * as api from "./api";
import type { Setting } from "./api";
import SettingRow from "./SettingRow.vue";

const monitors = ref<string[]>([]);
const active = ref(0);
const settings = ref<Setting[]>([]);
const autostart = ref(false);
const loading = ref(true);
const busy = ref(false);
const error = ref("");

const groups = computed(() => {
  const groups = new Map<string, Setting[]>();
  for (const s of settings.value) groups.set(s.group, [...(groups.get(s.group) ?? []), s]);
  return groups;
});

async function request(task: () => Promise<unknown>) {
  busy.value = true;
  try {
    await task();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function load() {
  loading.value = true;
  await request(async () => {
    settings.value = monitors.value.length ? await api.readSettings(active.value) : [];
  });
  loading.value = false;
}

async function refresh() {
  error.value = "";
  loading.value = true;
  await request(async () => {
    monitors.value = await api.listMonitors();
    active.value = 0;
  });
  await load();
}

async function change(setting: Setting, value: string) {
  error.value = "";
  // A fresh copy resets the control if nothing changes.
  let next: Setting = { ...setting };
  const confirmed = !setting.force || confirm(`${setting.label}: ${setting.note}\n\nContinue?`);
  if (confirmed) {
    await request(async () => {
      next = await api.changeSetting(active.value, setting.key, value, setting.force);
    });
  }
  settings.value = settings.value.map((s) => (s.key === next.key ? next : s));

  // A new mode or preset changes other settings too.
  if (confirmed && !error.value && (setting.kind === "enum" || setting.key === "reset")) await load();
}

async function toggleAutostart(enabled: boolean) {
  error.value = "";
  await request(() => api.setAutostart(enabled));
  autostart.value = await api.autostartEnabled();
}

onMounted(async () => {
  autostart.value = await api.autostartEnabled();
  await refresh();
});
</script>

<template>
  <header>
    <img src="/logo.svg" alt="" />
    <h1>AsusDisplay</h1>
    <select v-model.number="active" aria-label="Monitor" :disabled="busy || monitors.length < 2" @change="settings = []; load()">
      <option v-for="(name, i) in monitors" :key="i" :value="i">{{ name }}</option>
    </select>
    <button :disabled="busy" @click="refresh">Refresh</button>
  </header>
  <div class="progress" :class="{ active: busy }"></div>

  <p v-if="error" class="error" role="alert">{{ error }}</p>

  <main>
    <p v-if="loading && !settings.length" class="hint">Reading monitor settings…</p>
    <p v-else-if="!monitors.length" class="hint">
      No monitors found. Check that the i2c-dev module is loaded and the udev rule is installed (see the README).
    </p>
    <div v-else class="grid">
      <section v-for="[group, items] in groups" :key="group" class="card">
        <h2>{{ group }}</h2>
        <SettingRow v-for="s in items" :key="s.key" :setting="s" :disabled="busy" @change="change(s, $event)" />
        <div v-if="group === 'System'" class="row">
          <label for="autostart">Launch at login</label>
          <input
            id="autostart"
            type="checkbox"
            role="switch"
            :checked="autostart"
            :disabled="busy"
            @change="toggleAutostart(($event.target as HTMLInputElement).checked)"
          />
        </div>
      </section>
    </div>
  </main>
</template>
