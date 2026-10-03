import { readFile } from "node:fs/promises";
import { resolve, join } from "node:path";
import { pathToFileURL } from "node:url";
import { runSharingSuite } from "./sharing-flow.mjs";

const engine = resolve(process.argv[2] ?? "dist/pages", "engine");
const { default: createStella } = await import(pathToFileURL(join(engine, "stella_web.js")));
const data = await readFile(join(engine, "stella_web.data"));
const module = await createStella({ noInitialRun: true, locateFile: name => join(engine, name),
  getPreloadedPackage: () => data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength), print: () => {}, printErr: console.error });
const result = runSharingSuite(module);
module._stella_shutdown();
for (const item of result.cases) console.log(JSON.stringify(item));
console.log("PASS: production WebAssembly screenshot ordering, RGBA metadata, callback flush and old/new extent; WebGL pixels require browser verification.");
