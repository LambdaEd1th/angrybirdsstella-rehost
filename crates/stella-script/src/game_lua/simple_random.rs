//! `SimpleRandomNative` recovered from Purple's global CMWC/MSVC-LCG paths.

use std::sync::{Mutex, OnceLock};

use mlua::{Lua, MultiValue, Result as LuaResult, Table, Value};

use crate::{
    native_fcvtzu_f32, native_required_integer, native_required_number, native_required_string,
    runtime_error,
};

/// Process-global CMWC source used only by `newSeed`/`newSeedString`.
#[derive(Debug)]
pub(crate) struct NativeSeedRandom {
    words: Vec<u32>,
    carry: u32,
    index: u32,
}

impl NativeSeedRandom {
    pub(crate) fn new() -> Self {
        let mut words = vec![0; 4096];
        let mut x = 123_456_789_u32;
        let mut y = 362_436_069_u32;
        let mut z = 521_288_629_u32;
        let mut w = 88_675_123_u32;
        for word in &mut words {
            let temporary = x ^ x.wrapping_shl(11);
            x = y;
            y = z;
            z = w;
            w ^= w.wrapping_shr(19) ^ temporary ^ temporary.wrapping_shr(8);
            *word = w;
        }
        Self {
            words,
            carry: 362_436,
            index: 4095,
        }
    }

    pub(crate) fn next_word(&mut self) -> u32 {
        self.index = self.index.wrapping_add(1) & 0x0fff;
        let product =
            18_782_u64 * u64::from(self.words[self.index as usize]) + u64::from(self.carry);
        let mut carry = (product >> 32) as u32;
        let mut folded = (product as u32).wrapping_add(carry);
        if folded < carry {
            carry = carry.wrapping_add(1);
            folded = folded.wrapping_add(1);
        }
        let word = 0xffff_fffe_u32.wrapping_sub(folded);
        self.words[self.index as usize] = word;
        self.carry = carry;
        word
    }

    pub(crate) fn new_seed(&mut self) -> u32 {
        // sub_100094E8C requests [0, UINT32_MAX) as a double and FCVTZU's
        // the result. Keep the floating operation because its upper endpoint
        // intentionally turns every non-zero generated word into word - 1.
        (f64::from(self.next_word()) * (1.0 / 4_294_967_296.0) * f64::from(u32::MAX)) as u32
    }
}

static NATIVE_SEED_RANDOM: OnceLock<Mutex<NativeSeedRandom>> = OnceLock::new();

pub(crate) fn install(lua: &Lua, globals: &Table) -> LuaResult<()> {
    let simple_random = lua.create_table()?;
    simple_random.set(
        "newSeed",
        lua.create_function(|_, _: MultiValue| {
            Ok(NATIVE_SEED_RANDOM
                .get_or_init(|| Mutex::new(NativeSeedRandom::new()))
                .lock()
                .expect("native seed random lock poisoned")
                .new_seed())
        })?,
    )?;
    simple_random.set(
        "newSeedString",
        lua.create_function(|_, _: MultiValue| {
            let seed = NATIVE_SEED_RANDOM
                .get_or_init(|| Mutex::new(NativeSeedRandom::new()))
                .lock()
                .expect("native seed random lock poisoned")
                .new_seed();
            Ok(seed.to_string())
        })?,
    )?;
    simple_random.set(
        "newSeedFromNumber",
        lua.create_function(|_, args: MultiValue| {
            let seed = native_required_number(&args, 0, "newSeedFromNumber")? as f32;
            Ok(native_fcvtzu_f32(seed))
        })?,
    )?;
    simple_random.set(
        "newSeedFromString",
        lua.create_function(|_, args: MultiValue| {
            let seed = native_required_string(&args, 0, "newSeedFromString")?;
            Ok(
                seed_from_decimal_string(&seed).map_or_else(MultiValue::new, |seed| {
                    MultiValue::from_vec(vec![Value::Integer(i64::from(seed))])
                }),
            )
        })?,
    )?;
    simple_random.set(
        "random",
        lua.create_function(|_, args: MultiValue| {
            let seed = native_required_integer(&args, 0, "random")? as u32;
            let minimum = native_required_number(&args, 1, "random")? as f32;
            let maximum = native_required_number(&args, 2, "random")? as f32;
            let mut next = seed.wrapping_mul(214_013).wrapping_add(2_531_011);
            if next == u32::MAX {
                next = next.wrapping_add(1);
            }
            let minimum = native_fcvtzu_f32(minimum);
            let maximum = native_fcvtzu_f32(maximum);
            let span = 1_u32.wrapping_sub(minimum).wrapping_add(maximum);
            if span == 0 {
                return Err(runtime_error(
                    "SimpleRandomNative.random has an empty range",
                ));
            }
            let value = ((next >> 16) % span).wrapping_add(minimum);
            Ok((next, value as f32))
        })?,
    )?;
    simple_random.set(
        "seedToString",
        lua.create_function(|_, args: MultiValue| {
            let seed = native_required_integer(&args, 0, "seedToString")? as u32;
            Ok(seed.to_string())
        })?,
    )?;
    globals.set("SimpleRandomNative", simple_random)?;
    Ok(())
}

fn seed_from_decimal_string(source: &str) -> Option<u32> {
    // std::istream >> unsigned int skips leading whitespace, accepts a sign,
    // and succeeds after the first digit run without requiring EOF.
    let source = source.trim_start();
    let (negative, source) = match source.as_bytes().first().copied() {
        Some(b'-') => (true, &source[1..]),
        Some(b'+') => (false, &source[1..]),
        _ => (false, source),
    };
    let mut found_digit = false;
    let mut value = 0_u64;
    for byte in source.bytes() {
        if !byte.is_ascii_digit() {
            break;
        }
        found_digit = true;
        value = value.checked_mul(10)?.checked_add(u64::from(byte - b'0'))?;
        if value > u64::from(u32::MAX) {
            return None;
        }
    }
    found_digit.then(|| {
        let value = value as u32;
        if negative {
            0_u32.wrapping_sub(value)
        } else {
            value
        }
    })
}
