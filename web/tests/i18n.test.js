import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { LOCALES, MESSAGES, configureLanguage, detectLanguage, language, languagePreferenceKey, setLanguage, t, formatMessage, LocalizedError } from "../i18n.js";

test("every shipped language covers every message and preserves interpolation fields", () => {
  assert.equal(LOCALES.length, 11);
  const keys = Object.keys(MESSAGES.en).sort();
  const fields = value => [...value.matchAll(/\{(\w+)\}/g)].map(match => match[1]).sort();
  for (const locale of LOCALES) {
    assert.deepEqual(Object.keys(MESSAGES[locale.id]).sort(), keys, locale.id);
    for (const key of keys) {
      assert.ok(MESSAGES[locale.id][key].trim(), `${locale.id}/${key}`);
      assert.deepEqual(fields(MESSAGES[locale.id][key]), fields(MESSAGES.en[key]), `${locale.id}/${key}`);
    }
  }
});

test("page labels, attributes and dynamic messages all have translations", async () => {
  const html = await readFile(new URL("../index.html", import.meta.url), "utf8");
  const keys = [...html.matchAll(/data-i18n(?:-[a-z-]+)?="([^"]+)"/g)].map(match => match[1]);
  for (const file of ["launcher.js", "storage.js", "backup.js", "renderer.js"]) {
    const source = await readFile(new URL(`../${file}`, import.meta.url), "utf8");
    keys.push(...[...source.matchAll(/\b(?:message|t|LocalizedError)\("([^"]+)"/g)].map(match => match[1]));
  }
  for (const locale of LOCALES) for (const key of keys) assert.ok(MESSAGES[locale.id][key], `${locale.id}/${key}`);
});

test("browser language matching respects preference order, script and regional variants", () => {
  for (const [input, expected] of [
    [["xx", "fr-CA", "en-US"], "fr"], [["de-AT"], "de"], [["it-CH"], "it"], [["es-MX"], "es"],
    [["pt-PT"], "pt-BR"], [["pt_BR"], "pt-BR"], [["ja-JP"], "ja"], [["ko-KR"], "ko"], [["ru-RU"], "ru"],
    [["zh-CN"], "zh-Hans"], [["zh-SG"], "zh-Hans"], [["zh-HK"], "zh-Hant"], [["zh_MO"], "zh-Hant"],
    [["zh-Hant-CN"], "zh-Hant"], [["zh-Hans-TW"], "zh-Hans"], [["zh-Latn-CN", "ja"], "ja"],
    [["zh-x-Hant"], "zh-Hans"], [["ja", "zh-TW"], "ja"], [["ar", "xx"], "en"], [[], "en"],
  ]) assert.equal(detectLanguage(input), expected, JSON.stringify(input));
});

test("saved language takes priority, stays isolated by site path and works when storage is denied", () => {
  const values = new Map();
  const storage = { getItem: key => values.get(key) ?? null, setItem: (key, value) => values.set(key, value) };
  configureLanguage({ storage, basePath: "/first/", preferences: ["ja"] }); setLanguage("fr");
  assert.equal(values.get(languagePreferenceKey("/first/")), "fr");
  assert.equal(configureLanguage({ storage, basePath: "/first/", preferences: ["de"] }), "fr");
  assert.equal(configureLanguage({ storage, basePath: "/second/", preferences: ["ko"] }), "ko");
  values.set(languagePreferenceKey("/second/"), "invalid");
  assert.equal(configureLanguage({ storage, basePath: "/second/", preferences: ["zh-TW"] }), "zh-Hant");
  const denied = { getItem() { throw Error("SecurityError"); }, setItem() { throw Error("SecurityError"); } };
  configureLanguage({ storage: denied, basePath: "/denied/", preferences: ["ja"] }); setLanguage("ru");
  assert.equal(language(), "ru"); assert.throws(() => setLanguage("xx"), RangeError);
});

test("existing error messages and dates follow a later language change", () => {
  setLanguage("en"); const error = new LocalizedError("readError", { slot: 2 });
  setLanguage("zh-Hant"); assert.equal(formatMessage(error), "無法讀取存檔 2，請匯入備份或清空此存檔槽。");
  const date = new Date("2026-10-01T06:00:00Z");
  setLanguage("ja"); assert.ok(t("savedDate", { date }).includes("10"));
  assert.ok(!t("savedStatus", { slot: 1, time: date }).includes("{time}"));
  setLanguage("en");
});

test("web language list matches the original game table when runtime data is available", async context => {
  let data;
  try { data = await readFile(new URL("../../runtime/data/localization/TEXTS_BASIC.dat", import.meta.url)); }
  catch (error) { if (error.code === "ENOENT") { context.skip("Original runtime data not present"); return; } throw error; }
  const chunk = data.indexOf("LDAT"); assert.ok(chunk >= 0);
  const count = data.readUInt16BE(chunk + 8), names = []; let offset = chunk + 10;
  for (let index = 0; index < count; index++) {
    const size = data.readUInt16BE(offset); offset += 2;
    names.push(data.subarray(offset, offset + size).toString("utf8")); offset += size;
  }
  assert.deepEqual(LOCALES.map(locale => locale.game), names);
});
