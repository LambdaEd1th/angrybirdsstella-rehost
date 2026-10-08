# Discrete animation event allocation — 2026-10-08

The full goal is currently PAUSED and incomplete. The initial continuation
reported ACTIVE; the later application read confirmed PAUSED with the same
811-character objective. New goal work stopped; already-started validation and
local result preservation were closed out. This stage removes unused deep
copies while preserving the host's existing key selection, callback ordering,
forced applications, error propagation and owned deferred-event queues. It
does not establish complete native Timeline sampling parity. In particular,
the newly captured exact first-key boundary below remains a required follow-up.
Changes are committed to local Git after validation; no push or publication.

## Native basis and implementation

Only Official IDA MCP was used. The GUI lease `96bd247275aa` was released;
the database was not patched, renamed or saved. Original Purple SHA-256:
`ba45c91db09807fefa8207df0925b84250933c1c036c4846a8a71c0978749bbb`.
The ARM64 slice starts at file offset 13041664. Sixteen new instruction ranges
and six captured data ranges match the original file exactly. Captures and the
range-by-range verification are in
`target/audits/native-animation-event-sampling-20261008/`.

| Official IDA capture | Established behavior |
| --- | --- |
| `100421838` | Discrete string Timeline owns keyframe storage and before/after clamp handlers. |
| `100422050` | String Timeline computes a discrete key index using float key times; it does not return a newly allocated event payload. |
| `100422180`, `1004222C0` | State retains its Timeline and string representations; destructor releases the Timeline and atomically decrements string representation counts. This supports the reference-counted representation interpretation, not a measurement of native allocator traffic. |
| `100421E0C`, `100421E1C` | Key count and float time reads use the retained 16-byte keyframe records. |
| `100421E90` | Forced state application updates time, index and both string state values. |
| `100421F1C` | Ordinary state update compares the retained key index, writes the new time/index, assigns both strings and returns whether the index changed. String assignments still occur even when it returns false. |
| `1004225C8`, `1004225E4`, captured vtable slots | State forwards forced/ordinary sampling to the corresponding Timeline functions. |
| Reused `10041E41C` | EntityTarget mode 3 invokes its apply handler only when State update reports a change; other apply modes invoke the handler regardless. |
| Reused `1000121F4` | The event handler ignores empty strings and parses the emitted name/numeric/text fields. |

Reused capture files, their exact hashes and the prior native byte verification
are recorded in `evidence-inputs.json`. Native C++ allocation counts have not
been measured. The optimization follows the verified index gate while using
Rust's immutable parsed event storage; it does not reproduce a C++ allocator.

Previously `animation_event_state` cloned the selected event even for the
discarded start sample. The ordinary path also cloned the end sample before
checking whether its index changed. Two nonempty string fields meant four
allocations per target/update even when no callback could be emitted.
`animation_event_state_index` now returns only the index. Ordinary sampling
clones the final event only if the indices differ; forced sampling still clones
the selected event once. `pending_events`, dispatch snapshots and their owned
payloads are unchanged, so seek/close/nested callbacks cannot invalidate a
queued event. Equal payloads at different indices remain distinct triggers;
empty reset keys still suppress only their own selected event.

## Host measurements

`examples/animation_event_bench.rs` runs the installed Lua wrapper against
private, deterministic JSON. It never loads a level or uses normal player
data, services, a GPU or a solver. Scenarios hold a nonempty event, change to
an equal payload at every key, or hold an empty event. The fixture checks
initial/forced callbacks, every measured callback payload and count, and all
repeat notifications. There are 100 warmup updates. Before/after copies use
the same example, inputs, release options and compiler; saved code and binary
hashes accompany the raw measurements. Both profile runs also retain the same
fixture bytes, SHA-256
`41afcd8a9df6c14b24859c662a53d8a646bbcf814a551e0d7802cf401a4d6266`.

Host: Apple M2 Max, 12 CPUs, 64 GiB, macOS 27.2 ARM64. Timing builds use
Rust 1.98.0, thin LTO, one codegen unit, debug level 1 and retained symbols.
Production regression/strict checks use the configured Rust 1.98.1.
Measurements ran before concurrent builds/tests. Eight alternating pairs per
case provide 80 timing samples. The allocation observer is disabled in those
samples; 18 separate enabled-observer samples measure allocation requests.
The disabled allocator observation branch exists identically in both binaries.
Five-second `sample` profiles find the old cloning helper and malloc beneath
the actual wrapper update; the corresponding copies disappear after the fix.
Other wrapper/Lua allocations remain.

| Workload | Updates/run | Before µs/update, median | After µs/update, median | Median paired CPU change |
| --- | ---: | ---: | ---: | ---: |
| Hold, 16 targets | 80,000 | 2.602 | 1.080 | -58.48% |
| Hold, 64 targets | 40,000 | 9.596 | 3.780 | -60.69% |
| Hold, 256 targets | 10,000 | 42.966 | 16.990 | -60.40% |
| Dense, 64 targets | 10,000 | 52.577 | 48.165 | -8.25% |
| Empty, 64 targets | 40,000 | 3.978 | 3.781 | -5.19% |

For 2,000 observed updates of 64 held targets, global allocator allocation
requests fall from 524,010 to 12,010. Requested bytes fall from 35,562,589 to
490,589, removing exactly 512,000 requests and 35,072,000 requested bytes of
unused event copies. These are cumulative request counts/bytes, not resident
memory. Dense updates retain all 128,000 event callbacks and 31 repeat
notifications; allocation requests fall from 918,205 to 666,173. The empty
control has unchanged 12,010 allocation requests and 490,589 requested bytes.

The observation scope is Rust's global allocator, including Lua requests
that mlua routes through `std::alloc`; it excludes direct native C allocators.
The initial timing binaries incorrectly printed `observes_lua_allocator:false`
and an initial summary repeated that annotation. Inspection of the actual
mlua 0.12.2 `memory.rs`/`state/raw.rs` disproved it. Those original outputs are
preserved; their counts remain global-allocator observations. The benchmark's
post-measurement annotation is corrected separately and its measured loop is
unchanged. The correction and its verification are recorded in the audit.

Whole-process peak RSS medians are slightly higher in hold/empty cases and
lower in the dense case; there is no consistent resident-memory reduction.
These synthetic CPU-update results do not establish normal-level frame time,
FPS, GPU improvements, physical-platform performance or native allocator parity.

## Correctness and remaining work

The initial compiled input is `implementation-source`, 1,057 files,
SHA-256 `da32efb068fa16de085d5ca32c606831f9ede8661824ee1265bc2e0547c3b8d1`.
Final code input `final-code` has 1,058 files, SHA-256
`a5b6ce3acb75f45a8fbfbfed372d807be14545238f0434646c18f6abd5b31b04`.
Its only code difference is the benchmark output annotation and explanatory
comment. All code before that output macro is identical after removing
comments; all shipped production and preexisting test inputs are byte-identical.
The corrected example is rebuilt and all three allocation/payload probes match
the original optimized counts and requested bytes. The full workspace and all
six strict checks were nevertheless rerun on final-code and pass. Current
twelve saved test programs/counts are in `workspace-artifacts-v2.json`.
WebAssembly, browser, desktop, Linux focused cases and long-audit evidence reuse
the same shipped code/test inputs, verified by `annotation-correction.json`.
Final documentation updates are frozen separately; no code changed afterward.
Full-workspace tests pass 1,968 cases, zero fail and two original long audits
remain default-ignored. Twelve saved programs list 1,970 cases. Existing tests
cover event index changes, last-state emission, zero delta, forced seeks,
empty states, errors and callback-generated seek/close/nested dispatch. The
new benchmark asserts installed behavior and owned payloads rather than merely
duplicating the sampling implementation. No test assertion or ignore rule was
weakened. Six configured strict Clippy scopes pass with `-D warnings`:
macOS ARM64, Emscripten Web, Linux GNU ARM64/x86_64 and Windows MSVC ARM64/x86_64.
The Windows/Linux cross checks are not physical device execution.

Current file-image checks include 82 actual Metal cases and one CPU reference.
Linux ARM64 passes 40 focused CPU cases: five Timeline, 17 installed animation,
12 update/callback and six Poppy fixed-input cases. Linux uses actual Helvetica
bytes in the owned private container; it does not alter host fonts or relax
font assertions. Fresh formal WebAssembly passes engine/frame/storage/language,
file input, skin, matrix and five Timeline probes; a fresh private Chromium
WebGL2/ANGLE Metal session passes 22 pixel cases. Actual negative controls are
retained. Bounded self-regression checks do not prove original visual parity.
Both original long audits passed explicitly: six BirdRun idle trials and all
131 chapter entries with 180 idle frames each and both chapter restarts. No
solving was implemented. A current private desktop release rendered 1,000
frames of the fixed Chapter01 L18/Pink Shades Poppy aiming scenario. All 2,364
private resource copies still match the original inputs; 84 missing-global
queries are retained, with zero invoked fallbacks or compatibility bindings.
The image is a self-regression only: a large star-shaped element is visible
left of the sling, and this observation does not verify Poppy artwork or
original visual parity. A lexical predicate negative control fails as expected.

New native boundary finding: at `100422130`/`100422134`, `FCMP S8,S0` and
`CSEL W20,W21,WZR,LE` select `count-1` when the sampled float is at or before
the first key. The following loop then skips when already at that last index.
This raw string-Timeline behavior differs from the host's current first-key
selection at exact equality. The before/after optimization deliberately keeps
the same host selection and does not close this discrepancy. Complete callers,
clamp/force/loop semantics and the Sprite/int/curve variants need their own
Official IDA evidence, implementation and endpoint regressions. The five
existing Timeline fixtures must not be treated as proof of native endpoint
selection. This required follow-up takes priority over further animation
optimizations when the goal resumes.

All full-goal requirements remain open where not individually proven: complete
native functionality, visuals, interaction, strict resource/error/lifetime
semantics, Poppy's original reported symptom, physical platform execution,
services, frame/GPU/resident-memory performance and justified structural work.
Autoplay remains archived and excluded. Owned test data is isolated; no normal
saves, real credentials, purchases, GitHub uploads or publication are used.

Initial profiler fixture-readiness failure, failed read-only container mount,
premature desktop-verifier invocation and incorrect allocation-scope annotation
are retained in the audit. Their repairs do not suppress product errors. The
final profiler targets, strict/test/build commands, both long audits, browser
and desktop checks reached recorded terminal results. Owned IDA leases and
both cross containers are closed; unrelated containers and normal data remain
untouched. No complete-goal claim is made.
