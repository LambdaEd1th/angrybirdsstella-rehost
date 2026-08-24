//! Lua 5.1 `math.random`/`math.randomseed` from Purple's libc bridge.

use std::sync::{Arc, Mutex};

use mlua::{Lua, MultiValue, Result as LuaResult, Table, Value};

use crate::runtime_error;

use super::{native_fcvtzs_f32, native_lua51_number};

const RAND_MAX: u32 = 0x7fff_ffff;
const ZERO_SEED_FALLBACK: u32 = 123_459_876;
const MINSTD_MULTIPLIER: u64 = 16_807;
const RAND_TO_UNIT_F32: f32 = f32::from_bits(0x3000_0000);

/// Darwin/iOS libc's process-global `rand` state.
///
/// Purple's other native callers share this stream with Lua. The Rust host
/// owns one instance per game runtime, which is equivalent for the original
/// single-VM process while keeping independent test/runtime instances isolated.
#[derive(Debug, Clone)]
pub(crate) struct NativeLibcRandom {
    state: u32,
}

impl Default for NativeLibcRandom {
    fn default() -> Self {
        // Darwin libc behaves as though `srand(1)` had run before the first
        // call. `math.randomseed(os.time())` normally replaces this during
        // shipped gamelogic startup.
        Self { state: 1 }
    }
}

impl NativeLibcRandom {
    pub(crate) fn seed(&mut self, seed: u32) {
        self.state = seed;
    }

    pub(crate) fn next_word(&mut self) -> u32 {
        // Darwin's do_rand special-cases a literal zero state, then advances
        // the Park-Miller MINSTD recurrence. Keeping the pre-modulo state is
        // observable: srand(RAND_MAX) returns zero once, whereas srand(0)
        // immediately substitutes ZERO_SEED_FALLBACK.
        if self.state == 0 {
            self.state = ZERO_SEED_FALLBACK;
        }
        self.state = ((u64::from(self.state) * MINSTD_MULTIPLIER) % u64::from(RAND_MAX)) as u32;
        self.state
    }

    fn next_unit_f32(&mut self) -> f32 {
        // sub_1005181B4..0x1005181E0 converts through S registers and uses
        // the exact 0x30000000 constant (2^-31), not a double division by
        // RAND_MAX.
        (self.next_word() as f32) * RAND_TO_UNIT_F32
    }
}

pub(crate) fn install(
    lua: &Lua,
    globals: &Table,
    random: Arc<Mutex<NativeLibcRandom>>,
) -> LuaResult<()> {
    let math: Table = globals.get("math")?;

    let random_source = Arc::clone(&random);
    math.set(
        "random",
        lua.create_function(move |_, args: MultiValue| {
            // Purple calls rand() before inspecting the Lua argument count or
            // validating an interval. Failed calls therefore advance state.
            let unit = random_source
                .lock()
                .expect("native libc random lock poisoned")
                .next_unit_f32();
            let result = match args.len() {
                0 => unit,
                1 => {
                    let upper = check_integer(&args, 0)?;
                    if upper <= 0 {
                        return Err(runtime_error("interval is empty"));
                    }
                    ((unit * upper as f32).floor() + 1.0_f32) as f32
                }
                2 => {
                    let lower = check_integer(&args, 0)?;
                    let upper = check_integer(&args, 1)?;
                    if upper < lower {
                        return Err(runtime_error("interval is empty"));
                    }
                    let span = 1_i32.wrapping_sub(lower).wrapping_add(upper);
                    let offset = (unit * span as f32).floor();
                    // 0x100518298..0x1005182A4 widens the floored offset and
                    // lower bound to double for the add, then narrows once.
                    (f64::from(lower) + f64::from(offset)) as f32
                }
                _ => return Err(runtime_error("wrong number of arguments")),
            };
            Ok(f64::from(result))
        })?,
    )?;

    math.set(
        "randomseed",
        lua.create_function(move |_, args: MultiValue| {
            let seed = check_integer(&args, 0)?;
            random
                .lock()
                .expect("native libc random lock poisoned")
                .seed(seed as u32);
            Ok(())
        })?,
    )?;
    Ok(())
}

fn check_integer(args: &MultiValue, index: usize) -> LuaResult<i32> {
    let value = args.iter().nth(index).unwrap_or(&Value::Nil);
    native_lua51_number(value)
        .map(|number| native_fcvtzs_f32(number as f32))
        .ok_or_else(|| runtime_error(format!("bad argument #{} (number expected)", index + 1)))
}
