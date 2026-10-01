import { LOCALES, MESSAGES } from "./locales.js";

export { LOCALES, MESSAGES };
let current = "en", preferenceStorage, preferenceKey;
const listeners = new Set();
export const language = () => current;
export const languageIndex = () => LOCALES.findIndex(locale => locale.id === current);
export const languagePreferenceKey = basePath => `stella-rehost:language:v1:${basePath}`;

export function detectLanguage(preferences = []) {
  for (const preference of preferences) {
    if (typeof preference !== "string") continue;
    const [base, ...parts] = preference.replaceAll("_", "-").toLowerCase().split("-");
    if (base === "zh") {
      let script, region;
      for (const part of parts) {
        if (part.length === 1) break; // Extensions/private use do not select a translation.
        if (/^[a-z]{4}$/.test(part)) script = part;
        if (/^[a-z]{2}$/.test(part)) region = part;
      }
      if (script && !["hans", "hant"].includes(script)) continue;
      return script === "hant" || (!script && ["tw", "hk", "mo"].includes(region)) ? "zh-Hant" : "zh-Hans";
    }
    if (base === "pt") return "pt-BR";
    if (LOCALES.some(locale => locale.id === base)) return base;
  }
  return "en";
}

export function configureLanguage({ storage, basePath, preferences = [] }) {
  preferenceStorage = storage;
  preferenceKey = languagePreferenceKey(basePath);
  let saved;
  try { saved = storage?.getItem(preferenceKey); } catch { /* Detection works without storage. */ }
  current = LOCALES.some(locale => locale.id === saved) ? saved : detectLanguage(preferences);
  return current;
}

export function setLanguage(id) {
  if (!LOCALES.some(locale => locale.id === id)) throw new RangeError(`Unsupported language: ${id}`);
  current = id;
  try { preferenceStorage?.setItem(preferenceKey, id); } catch { /* Translation remains usable. */ }
  for (const listener of listeners) listener(id);
}
export function onLanguageChange(listener) { listeners.add(listener); return () => listeners.delete(listener); }
export function message(key, params = {}) { return { key, params }; }
export function t(key, params = {}, id = current) {
  const template = MESSAGES[id][key];
  if (template === undefined) throw new Error(`Missing translation: ${id}/${key}`);
  return template.replace(/\{(\w+)\}/g, (_, name) => {
    const value = params[name];
    if (value instanceof Error) return formatMessage(value);
    if (value instanceof Date) return new Intl.DateTimeFormat(id, name === "time"
      ? { hour: "2-digit", minute: "2-digit" }
      : { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" }).format(value);
    return String(value ?? `{${name}}`);
  });
}
export class LocalizedError extends Error {
  constructor(code, params = {}) { super(t(code, params)); this.name = "LocalizedError"; this.code = code; this.params = params; }
}
export function formatMessage(value) {
  if (value instanceof LocalizedError) return t(value.code, value.params);
  if (value?.key) {
    const params = Object.fromEntries(Object.entries(value.params).map(([key, param]) => [key, param instanceof Error ? formatMessage(param) : param]));
    return t(value.key, params);
  }
  return value instanceof Error ? value.message : String(value ?? "");
}
export function translateDocument(root = document) {
  root.documentElement.lang = current;
  for (const element of root.querySelectorAll("[data-i18n]")) element.textContent = t(element.dataset.i18n);
  for (const attribute of ["aria-label", "title", "content"]) {
    for (const element of root.querySelectorAll(`[data-i18n-${attribute}]`)) element.setAttribute(attribute, t(element.getAttribute(`data-i18n-${attribute}`)));
  }
}
