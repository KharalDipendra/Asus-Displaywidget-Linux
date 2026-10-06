<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import * as api from "./api";
import type { Setting, Unsupported } from "./api";
import SettingRow from "./SettingRow.vue";
import Icon, { type IconName } from "./Icon.vue";

const dev = import.meta.env.DEV;

const monitors = ref<string[]>([]);
const active = ref(0);
const settings = ref<Setting[]>([]);
const unsupported = ref<Unsupported[]>([]);
const autostart = ref(false);
const loading = ref(true);
const busy = ref(false);
const error = ref("");
const section = ref("");

// Icon, colour and subtitle per backend group name. Order matters: GamePlus before Gaming.
const SECTIONS: { match: RegExp; icon: IconName; tone: string; sub: string }[] = [
  { match: /^picture/i, icon: "sun", tone: "#38bdf8", sub: "Luminance and clarity" },
  { match: /colou?r/i, icon: "palette", tone: "#8b5cf6", sub: "Temperature and gain" },
  { match: /hdr/i, icon: "contrast", tone: "#f59e0b", sub: "High dynamic range" },
  { match: /gameplus/i, icon: "game", tone: "#d946ef", sub: "Crosshair, timer, counter" },
  { match: /gam/i, icon: "game", tone: "#d946ef", sub: "Response time and sync" },
  { match: /oled/i, icon: "shield", tone: "#34d399", sub: "Burn-in protection" },
  { match: /pip/i, icon: "pip", tone: "#22d3ee", sub: "Picture-in-picture" },
  { match: /osd/i, icon: "sliders", tone: "#a78bfa", sub: "On-screen keys and shortcuts" },
  { match: /system/i, icon: "plug", tone: "#fbbf24", sub: "Input, power and language" },
  { match: /info/i, icon: "info", tone: "#94a3b8", sub: "Firmware and version" },
];
const meta = (group: string) => SECTIONS.find((s) => s.match.test(group)) ?? { icon: "sliders" as IconName, tone: "#94a3b8", sub: "" };
const slug = (group: string) => "sec-" + group.toLowerCase().replace(/[^a-z0-9]+/g, "-");

// Brightness and contrast get pulled up into the live display card, and out of the Picture card.
const HERO_KEYS = ["brightness", "contrast"];
const quick = computed(() =>
  HERO_KEYS.map((key) => settings.value.find((s) => s.key === key)).filter((s): s is Setting => !!s && !s.error && !s.unavailable),
);

const groups = computed(() => {
  const inHero = new Set(quick.value.map((s) => s.key));
  const groups = new Map<string, Setting[]>();
  for (const s of settings.value) {
    if (inHero.has(s.key)) continue;
    groups.set(s.group, [...(groups.get(s.group) ?? []), s]);
  }
  return groups;
});

const status = computed(() =>
  !monitors.value.length && !loading.value ? { text: "Not detected", cls: "off" } : busy.value ? { text: "Syncing…", cls: "warn" } : { text: "Connected", cls: "" },
);

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
    unsupported.value = dev && monitors.value.length ? await api.listUnsupported(active.value) : [];
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

function pick(i: number) {
  if (i === active.value || busy.value) return;
  active.value = i;
  settings.value = [];
  load();
}

// Confirmation for settings that need force (input switch, reset).
const dialog = ref<HTMLDialogElement>();
const pending = ref<Setting>();
let resolveConfirm: (ok: boolean) => void = () => {};
function ask(setting: Setting) {
  pending.value = setting;
  dialog.value?.showModal();
  return new Promise<boolean>((r) => (resolveConfirm = r));
}
function answer(ok: boolean) {
  dialog.value?.close();
  resolveConfirm(ok);
}

async function change(setting: Setting, value: string) {
  error.value = "";
  // A fresh copy resets the control if nothing changes.
  let next: Setting = { ...setting };
  const confirmed = !setting.force || (await ask(setting));
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

const mainEl = ref<HTMLElement>();
function go(group: string) {
  section.value = group;
  document.getElementById(slug(group))?.scrollIntoView({ block: "start" });
}
function onScroll() {
  const top = mainEl.value?.getBoundingClientRect().top ?? 0;
  for (const g of groups.value.keys()) {
    const el = document.getElementById(slug(g));
    if (el && el.getBoundingClientRect().top - top < 140) section.value = g;
  }
}

onMounted(async () => {
  autostart.value = await api.autostartEnabled();
  await refresh();
});
</script>

<template>
  <div class="shell">
    <aside class="sidebar">
      <div class="brand">
        <img src="/icon.png" alt="" />
        <div>
          <h1>AsusDisplay</h1>
          <p>DDC/CI monitor control</p>
        </div>
      </div>

      <div v-if="monitors.length" class="side-group">
        <div class="side-label">Monitors</div>
        <button
          v-for="(name, i) in monitors"
          :key="i"
          class="monitor-btn"
          :class="{ active: i === active }"
          :disabled="busy && i !== active"
          @click="pick(i)"
        >
          <span class="tile"><Icon name="monitor" /></span>
          <span>{{ name }}</span>
        </button>
      </div>

      <nav v-if="settings.length" class="side-group">
        <div class="side-label">Sections</div>
        <button v-for="[group] in groups" :key="group" class="nav-btn" :class="{ active: section === group }" @click="go(group)">
          <Icon :name="meta(group).icon" />{{ group }}
        </button>
      </nav>

      <div class="side-foot">
        <span class="dot" :class="status.cls"></span>{{ monitors.length ? `${monitors.length} monitor${monitors.length === 1 ? "" : "s"} detected` : "No monitor detected" }}
      </div>
    </aside>

    <div class="content">
      <header class="topbar">
        <div class="topbar-title">
          <small v-if="monitors.length">Monitor {{ active + 1 }} of {{ monitors.length }}</small>
          <h2>{{ monitors[active] ?? "AsusDisplay" }}</h2>
        </div>
        <span class="status"><span class="dot" :class="status.cls"></span>{{ status.text }}</span>
        <button class="btn" :disabled="busy" @click="refresh"><Icon name="refresh" />Refresh</button>
        <div class="progress" :class="{ active: busy }"></div>
      </header>

      <main ref="mainEl" @scroll.passive="onScroll">
        <div class="stack">
          <p v-if="error" class="error" role="alert">
            <Icon name="alert" /><span>{{ error }}</span>
            <button aria-label="Dismiss" @click="error = ''"><Icon name="x" /></button>
          </p>

          <p v-if="loading && !settings.length" class="hint">Reading monitor settings…</p>

          <div v-else-if="!monitors.length" class="empty">
            <span class="tile"><Icon name="monitor-off" /></span>
            <h3>No monitors found</h3>
            <p>Check that the i2c-dev module is loaded and the udev rule is installed. See the README for setup.</p>
            <code>sudo modprobe i2c-dev</code>
          </div>

          <template v-else>
            <section class="hero">
              <div class="hero-id">
                <img src="/icon.png" alt="" />
                <div>
                  <div class="eyebrow">Live display</div>
                  <h3>{{ monitors[active] }}</h3>
                </div>
              </div>
              <div v-if="quick.length" class="hero-controls">
                <SettingRow
                  v-for="(s, i) in quick"
                  :key="s.key"
                  :setting="s"
                  :icon="i === 0 ? 'sun' : 'contrast'"
                  :disabled="busy"
                  @change="change(s, $event)"
                />
              </div>
            </section>

            <div class="grid">
              <section v-for="[group, items] in groups" :id="slug(group)" :key="group" class="card" :style="{ '--tone': meta(group).tone }">
                <div class="card-head">
                  <span class="badge"><Icon :name="meta(group).icon" /></span>
                  <div>
                    <h2>{{ group }}</h2>
                    <p v-if="meta(group).sub">{{ meta(group).sub }}</p>
                  </div>
                </div>
                <div class="card-body">
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
                </div>
              </section>
            </div>

            <section v-if="dev && unsupported.length" class="card unsupported" :style="{ '--tone': '#64748b' }">
              <div class="card-head">
                <span class="badge"><Icon name="bug" /></span>
                <div>
                  <h2>Unsupported on this monitor</h2>
                  <p>Dev build only · {{ unsupported.length }} setting{{ unsupported.length === 1 ? "" : "s" }} hidden</p>
                </div>
              </div>
              <div class="card-body">
                <div v-for="u in unsupported" :key="u.key + u.code" class="row disabled">
                  <label>{{ u.label }} <code>0x{{ u.code.toString(16).toUpperCase().padStart(2, "0") }}</code></label>
                  <span class="muted">{{ u.reason }}</span>
                </div>
              </div>
            </section>
          </template>
        </div>
      </main>
    </div>
  </div>

  <dialog ref="dialog" @cancel.prevent="answer(false)">
    <span class="tile"><Icon name="alert" /></span>
    <h3>{{ pending?.label }}?</h3>
    <p>{{ pending?.note }}</p>
    <footer>
      <button class="btn ghost" @click="answer(false)">Cancel</button>
      <button class="btn primary" @click="answer(true)">Continue</button>
    </footer>
  </dialog>
</template>
