import { readFile } from "node:fs/promises";
import { resolve, join } from "node:path";
import { pathToFileURL } from "node:url";
import { FRAME_CASES, runFrameCase, runResizeCase } from "./frame-capture.mjs";

const engine = resolve(process.argv[2] ?? "dist/pages", "engine");
const { default: createStella } = await import(pathToFileURL(join(engine, "stella_web.js")));
const data = await readFile(join(engine, "stella_web.data"));
const module = await createStella({ noInitialRun: true, locateFile: name => join(engine, name),
  getPreloadedPackage: () => data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength),
  print: () => {}, printErr: console.error });
const failures = [];
for (const fixture of FRAME_CASES) {
  try { console.log(JSON.stringify(runFrameCase(module, null, fixture))); }
  catch (error) { failures.push({ name: fixture.name, error: error.message }); }
}
try { console.log(JSON.stringify(runResizeCase(module, null))); }
catch (error) { failures.push({ name: "resize callback case", error: error.message }); }
module._stella_shutdown();
if (failures.length) throw new Error(JSON.stringify(failures));
console.log(`PASS: ${FRAME_CASES.length + 1} production WebAssembly capture/clear/lifetime cases; WebGL pixels require the browser probe.`);
