// Run before the stylesheet so a saved dark theme does not flash a light page.
(() => {
  const root = document.documentElement;
  const basePath = new URL(".", document.currentScript.src).pathname;
  const key = `stella-rehost:theme:v1:${basePath}`;
  const modes = new Set(["system", "light", "dark"]);
  const system = window.matchMedia("(prefers-color-scheme: dark)");
  let storage, mode = "system";
  try { storage = window.localStorage; mode = storage.getItem(key) ?? mode; } catch { /* Optional preference. */ }
  if (!modes.has(mode)) mode = "system";

  function apply() {
    const resolved = mode === "system" ? (system.matches ? "dark" : "light") : mode;
    root.dataset.theme = resolved;
    root.dataset.themeMode = mode;
    document.querySelector('meta[name="theme-color"]').content = resolved === "dark" ? "#151e1a" : "#f6f7f2";
    for (const control of document.querySelectorAll("[data-theme-control]")) control.value = mode;
  }
  apply();
  system.addEventListener("change", apply);
  document.addEventListener("DOMContentLoaded", () => {
    for (const control of document.querySelectorAll("[data-theme-control]")) {
      control.addEventListener("change", () => {
        mode = modes.has(control.value) ? control.value : "system";
        try { storage?.setItem(key, mode); } catch { /* Theme remains usable without storage. */ }
        apply();
        if (!document.getElementById("game").hidden) document.getElementById("canvas").focus({ preventScroll: true });
      });
    }
    apply();
  });
  window.addEventListener("storage", event => {
    if (event.storageArea !== storage || (event.key !== null && event.key !== key)) return;
    mode = modes.has(event.newValue) ? event.newValue : "system";
    apply();
  });
})();
