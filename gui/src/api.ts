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

export const listMonitors = () => invoke<string[]>("monitors");

export const readSettings = (index: number) => invoke<Setting[]>("status", { index });

export const changeSetting = (index: number, key: string, value: string, force: boolean) =>
  invoke<Setting>("set", { index, key, value, force });

export const autostartEnabled = () => invoke<boolean>("autostart");

export const setAutostart = (enabled: boolean) => invoke<void>("set_autostart", { enabled });
