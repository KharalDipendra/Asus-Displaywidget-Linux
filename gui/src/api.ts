import { invoke } from "@tauri-apps/api/core";

export interface Choice {
  raw: number;
  name: string;
  label: string;
}

/** A monitor setting with its current value, as sent by src-tauri/src/main.rs. */
export interface Setting {
  key: string;
  label: string;
  code: number;
  kind: "continuous" | "enum" | "toggle" | "action" | "info";
  group: string;
  choices: Choice[];
  force: boolean;
  note: string | null;
  error: string | null;
  current?: number;
  max?: number;
  text?: string;
  unavailable?: boolean;
  on?: boolean;
  selected?: string;
}

/** A setting the monitor doesn't expose, with the reason (dev build only). */
export interface Unsupported {
  key: string;
  label: string;
  code: number;
  group: string;
  reason: string;
}

export const listMonitors = () => invoke<string[]>("monitors");

export const readSettings = (index: number) => invoke<Setting[]>("status", { index });

export const listUnsupported = (index: number) => invoke<Unsupported[]>("unsupported", { index });

export const changeSetting = (index: number, key: string, value: string, force: boolean) =>
  invoke<Setting>("set", { index, key, value, force });

export const autostartEnabled = () => invoke<boolean>("autostart");

export const setAutostart = (enabled: boolean) => invoke<void>("set_autostart", { enabled });

export const findLightbars = () => invoke<string[]>("lightbars");

export const setLightbarWhite = (brightness: number, kelvin: number) =>
  invoke<void>("lightbar_set", { white: [brightness, kelvin], rgb: null });

export const setLightbarOff = () => invoke<void>("lightbar_set", { white: null, rgb: null });
