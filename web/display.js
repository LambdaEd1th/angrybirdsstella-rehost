export const ORIGINAL_SIZE = { width: 1024, height: 768 };

export function displayDimensions(mode, availableWidth, availableHeight, custom = ORIGINAL_SIZE) {
  if (mode === "auto") return { width: Math.max(1, Math.floor(availableWidth)), height: Math.max(1, Math.floor(availableHeight)) };
  if (mode === "custom") return { ...custom };
  const scale = Number(mode) / 100;
  return { width: Math.round(ORIGINAL_SIZE.width * scale), height: Math.round(ORIGINAL_SIZE.height * scale) };
}

export function drawableDimensions(width, height, pixelRatio, limits, devicePixels) {
  const ratio = Number.isFinite(pixelRatio) && pixelRatio > 0 ? pixelRatio : 1;
  // Some browser emulation backends report an unscaled device-pixel box.
  // Accept exact pixel snapping only when it agrees with the current density.
  const measured = devicePixels && Math.abs(devicePixels.width - width * ratio) <= 1 && Math.abs(devicePixels.height - height * ratio) <= 1;
  const pixelWidth = Math.max(1, Math.round(measured ? devicePixels.width : width * ratio));
  const pixelHeight = Math.max(1, Math.round(measured ? devicePixels.height : height * ratio));
  // Captures use full-size textures, so the viewport and texture limits both
  // bound the drawable. Keep its aspect when a device cannot fit the request.
  const scale = Math.min(1, limits.width / pixelWidth, limits.height / pixelHeight);
  return { width: Math.max(1, Math.floor(pixelWidth * scale)), height: Math.max(1, Math.floor(pixelHeight * scale)) };
}

export function canvasPoint(canvas, event) {
  const rect = canvas.getBoundingClientRect();
  return [(event.clientX - rect.left) * canvas.width / rect.width, (event.clientY - rect.top) * canvas.height / rect.height];
}
