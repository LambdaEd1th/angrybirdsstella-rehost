//! Measure installed AnimationWrapper updates with private synthetic timelines.
//!
//! cargo run -p stella-script --release --example animation_event_bench -- hold 64 40000
//! Scenarios: hold (unchanged nonempty state), dense (a key change per update),
//! and empty (unchanged empty state). An optional `alloc` argument observes
//! Rust System allocator traffic; use separate runs for timing comparisons.
//! This measures CPU animation updates, not rendering, GPU work, or level play.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    error::Error,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use stella_script::StellaLua;

struct ObservedAllocator;
static OBSERVE: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static REALLOCS: AtomicU64 = AtomicU64::new(0);
static DEALLOCS: AtomicU64 = AtomicU64::new(0);
static REQUESTED_BYTES: AtomicU64 = AtomicU64::new(0);

// Observation is limited to the measured update loop. Both timing binaries
// have the same disabled observation branch. Counts include mlua's requests
// through std::alloc, but exclude native requests made directly to C allocators.
unsafe impl GlobalAlloc for ObservedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarding the caller's valid Layout to System unchanged.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() && OBSERVE.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            REQUESTED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarding the caller's valid Layout to System unchanged.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() && OBSERVE.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            REQUESTED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if OBSERVE.load(Ordering::Relaxed) {
            DEALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: this allocator forwards every allocation to System.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: forwarding the caller's allocation and requested size.
        let replacement = unsafe { System.realloc(pointer, layout, size) };
        if !replacement.is_null() && OBSERVE.load(Ordering::Relaxed) {
            REALLOCS.fetch_add(1, Ordering::Relaxed);
            REQUESTED_BYTES.fetch_add(size as u64, Ordering::Relaxed);
        }
        replacement
    }
}

#[global_allocator]
static ALLOCATOR: ObservedAllocator = ObservedAllocator;

struct Fixture(PathBuf);

impl Fixture {
    fn new(scenario: &str, targets: u32) -> Result<Self, Box<dyn Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root =
            std::env::temp_dir().join(format!("stella-event-bench-{}-{nonce}", std::process::id()));
        fs::create_dir(&root)?;
        let fixture = Self(root);
        fs::create_dir_all(fixture.data().join("animations"))?;
        fs::create_dir(fixture.0.join("appdata"))?;
        let mut entities = serde_json::Map::new();
        let mut children = Vec::new();
        for target in 0..targets {
            let name = format!("TARGET_{target:04}");
            let payload = if scenario == "empty" {
                String::new()
            } else {
                format!("pulse{target:04}:7:1.25:{}", "payload-".repeat(16))
            };
            let keys = if scenario == "dense" {
                // Equal payloads at different indices must still invoke the
                // callback. This also checks the native last-key selection.
                (0..=64)
                    .map(|key| serde_json::json!([key, payload]))
                    .collect::<Vec<_>>()
            } else {
                vec![
                    serde_json::json!([0, payload]),
                    serde_json::json!([100_000_000, payload]),
                ]
            };
            entities.insert(
                name.clone(),
                serde_json::json!({"spineEvent":{"type":"DiscreteString","keyframes":keys}}),
            );
            children.push(serde_json::json!({"name":name}));
        }
        let document = serde_json::json!({
            "children": children,
            "comps": [{"type":"game::Animation","data":{"actions":{
                "idle":{"clips":{"":{"targets":entities}}}
            }}}]
        });
        fs::write(
            fixture.data().join("animations/bench.anim.json"),
            serde_json::to_vec(&document)?,
        )?;
        fs::write(fixture.data().join("animations/bench.skins.json"), b"{}")?;
        Ok(fixture)
    }

    fn data(&self) -> PathBuf {
        self.0.join("data")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn process_cpu_seconds() -> Result<Option<f64>, Box<dyn Error>> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let mut time = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: time points to writable timespec storage; the clock is
        // supported on these host targets and a failure is reported.
        if unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut time) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Some(time.tv_sec as f64 + time.tv_nsec as f64 * 1e-9))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Ok(None)
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let scenario = args.next().unwrap_or_else(|| "hold".to_owned());
    let targets = args.next().map_or(Ok(64_u32), |value| value.parse())?;
    let iterations = args.next().map_or(Ok(40_000_u32), |value| value.parse())?;
    let mode = args.next().unwrap_or_else(|| "time".to_owned());
    if !matches!(scenario.as_str(), "hold" | "dense" | "empty")
        || !matches!(mode.as_str(), "time" | "alloc")
        || targets == 0
        || targets > 4096
        || iterations == 0
        || iterations > 10_000_000
        || args.next().is_some()
    {
        return Err("use: animation_event_bench [hold|dense|empty] [1..4096 targets] [1..10000000 updates] [time|alloc]".into());
    }
    let fixture = Fixture::new(&scenario, targets)?;
    let runtime = StellaLua::new_with_resolution(fixture.data(), 16, 16)?;
    let step = if scenario == "dense" { 1.0 } else { 1.0 / 60.0 };
    runtime.execute_source(&format!(
        "AnimationWrapperNative.loadFromBundle('scene','animations/bench.anim.json')
         callback_count=0; repeat_count=0; payload_errors=0
         AnimationWrapperNative.setPlaybackEvent('scene',function(tag,action,name,i,n,text)
             if name=='PLAYBACK_REPEAT' then
                 repeat_count=repeat_count+1
                 if tag~='scene' or action~='idle' or i~=0 or n~=0 or text~=''
                 then payload_errors=payload_errors+1 end
                 return
             end
             callback_count=callback_count+1
             if tag~='scene' or action~='idle' or string.sub(name,1,5)~='pulse'
                 or i~=7 or n~=1.25 or text~=string.rep('payload-',16)
             then payload_errors=payload_errors+1 end
         end)
         AnimationWrapperNative.start('scene','idle','repeat')
         AnimationWrapperNative.update(0)
         if callback_count~={} or payload_errors~=0 then error('initial event mismatch') end
         for _=1,100 do AnimationWrapperNative.update({step}) end
         callback_count=0; repeat_count=0
         run_updates=function() for _=1,{iterations} do AnimationWrapperNative.update({step}) end end",
        if scenario == "empty" { 0 } else { targets * 2 }
    ))?;
    let started_cpu = process_cpu_seconds()?;
    let started = Instant::now();
    OBSERVE.store(mode == "alloc", Ordering::Relaxed);
    let result = runtime.execute_source("run_updates()");
    OBSERVE.store(false, Ordering::Relaxed);
    let seconds = started.elapsed().as_secs_f64();
    let cpu_seconds = process_cpu_seconds()?
        .zip(started_cpu)
        .map(|(end, start)| end - start);
    result?;
    let expected = if scenario == "dense" {
        u64::from(targets) * u64::from(iterations)
    } else {
        0
    };
    let expected_repeats = if scenario == "dense" {
        (iterations + 100) / 64 - 100 / 64
    } else {
        0
    };
    runtime.execute_source(&format!(
        "if callback_count~={expected} or repeat_count~={expected_repeats} or payload_errors~=0 then error('measured event mismatch') end
         AnimationWrapperNative.seek('scene',0)
         AnimationWrapperNative.update(0)
         if callback_count~={} or payload_errors~=0 then error('forced event mismatch') end",
        expected + if scenario == "empty" { 0 } else { u64::from(targets) }
    ))?;
    println!(
        "{}",
        serde_json::json!({
            "scenario":scenario, "targets":targets, "updates":iterations, "warmup_updates":100,
            "mode":mode, "update_seconds":seconds, "process_cpu_seconds":cpu_seconds,
            "microseconds_per_update":seconds*1e6/f64::from(iterations),
            "rust_allocations": ALLOCS.load(Ordering::Relaxed),
            "rust_reallocations": REALLOCS.load(Ordering::Relaxed),
            "rust_deallocations": DEALLOCS.load(Ordering::Relaxed),
            "rust_requested_bytes": REQUESTED_BYTES.load(Ordering::Relaxed),
            "allocator_observation_enabled":mode=="alloc",
            "allocator_scope":"Rust global allocator, including mlua requests routed through it; excludes direct C allocators",
            "expected_measured_callbacks":expected, "correctness_checks_passed":true,
            "expected_measured_repeat_callbacks":expected_repeats,
            "isolated_data_directory":fixture.0,
        })
    );
    Ok(())
}
