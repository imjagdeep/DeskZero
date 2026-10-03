// Tiny shared stores over Tauri events (no state library).

import { listen } from "@tauri-apps/api/event";
import { useSyncExternalStore } from "react";
import { api, errorText } from "./api";
import type { AppConfig, PlannedOp, Status } from "./types";

function createStore<T>(initial: T) {
  let value = initial;
  const subs = new Set<() => void>();
  return {
    get: () => value,
    set(next: T) {
      value = next;
      subs.forEach((s) => s());
    },
    subscribe(fn: () => void) {
      subs.add(fn);
      return () => subs.delete(fn);
    },
  };
}

const statusStore = createStore<Status | null>(null);
const configStore = createStore<AppConfig | null>(null);
const previewStore = createStore<PlannedOp[] | null>(null);
const toastStore = createStore<{ id: number; text: string; error: boolean }[]>([]);

export function useStatus() {
  return useSyncExternalStore(statusStore.subscribe, statusStore.get);
}

export function useConfig() {
  return useSyncExternalStore(configStore.subscribe, configStore.get);
}

export function useToasts() {
  return useSyncExternalStore(toastStore.subscribe, toastStore.get);
}

/** The one "Organize N files?" sheet, opened from anywhere. */
export function usePreview() {
  return useSyncExternalStore(previewStore.subscribe, previewStore.get);
}
export function openPreview(plan: PlannedOp[]) {
  previewStore.set(plan);
}
export function closePreview() {
  previewStore.set(null);
}

let toastId = 0;
export function toast(text: string, error = false) {
  const id = ++toastId;
  toastStore.set([...toastStore.get(), { id, text, error }]);
  setTimeout(() => toastStore.set(toastStore.get().filter((t) => t.id !== id)), error ? 7000 : 3500);
}

export function toastError(e: unknown) {
  toast(errorText(e), true);
}

export async function refreshConfig() {
  try {
    configStore.set(await api.getConfig());
  } catch (e) {
    toastError(e);
  }
}

export async function refreshStatus() {
  try {
    statusStore.set(await api.getStatus());
  } catch (e) {
    toastError(e);
  }
}

let started = false;
/** Load initial state and follow backend events. Call once. */
export function startStores() {
  if (started) return;
  started = true;
  void refreshConfig();
  void refreshStatus();
  void listen<Status>("status", (e) => statusStore.set(e.payload));
}
