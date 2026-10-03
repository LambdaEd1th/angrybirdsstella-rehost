import { readFile } from "node:fs/promises";
import { resolve, join } from "node:path";
import { pathToFileURL } from "node:url";
import { runGamerServicesSuite } from "./gamer-services-flow.mjs";
const artifact = resolve(process.argv[2] ?? "dist/pages"), engine = join(artifact, "engine");
const { default: createStella } = await import(pathToFileURL(join(engine, "stella_web.js")));
const data = await readFile(join(engine, "stella_web.data"));
const module = await createStella({ noInitialRun: true, locateFile: name => join(engine, name), getPreloadedPackage: () => data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength), print: () => {}, printErr: text => { if (!text.includes("Missing sound file: AB_")) console.error(text); } });
console.log(JSON.stringify(runGamerServicesSuite(module), null, 2));
