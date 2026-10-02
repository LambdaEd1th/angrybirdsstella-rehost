import { readFile } from "node:fs/promises";
import { resolve, join } from "node:path";
import { pathToFileURL } from "node:url";
import { runAccountSuite } from "./account-flow.mjs";
const engine = resolve(process.argv[2] ?? "dist/pages", "engine");
const { default: createStella } = await import(pathToFileURL(join(engine, "stella_web.js")));
const data = await readFile(join(engine, "stella_web.data"));
const module = await createStella({ noInitialRun: true, locateFile: name => join(engine, name), getPreloadedPackage: () => data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength), print: () => {}, printErr: console.error });
console.log(JSON.stringify(runAccountSuite(module), null, 2));
