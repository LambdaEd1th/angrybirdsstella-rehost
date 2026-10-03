// Consume each native external action once, in call order. OS launch results
// are advisory: choosing Rate never claims that a review was submitted.
export function dispatchPlatformActions(game, { window = globalThis.window } = {}) {
  const { module } = game, pointer = module._stella_platform_packet();
  if (!pointer) throw new Error(module.UTF8ToString(module._stella_error()));
  const actions = JSON.parse(module.UTF8ToString(pointer));
  for (const action of actions) {
    try {
      if (action.kind === "openUrl") window.open(action.url, "_blank", "noopener,noreferrer");
      else if (action.kind === "appStoreProduct") window.open(`https://apps.apple.com/app/id${action.productId}`, "_blank", "noopener,noreferrer");
      else {
        // Media/Game Center presentation still needs a browser provider. Keep
        // their complete payload available to an embedding host at the boundary.
        window.dispatchEvent(new window.CustomEvent("stella-platform-action", { detail: action }));
      }
    } catch (error) { console.warn("Platform action failed", error); }
  }
  return actions;
}
