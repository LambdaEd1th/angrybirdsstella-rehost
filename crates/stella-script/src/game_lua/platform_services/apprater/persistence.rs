use super::*;
use serde_json::Value as Json;

pub(super) struct RatingStore(RegistryNamespace);

impl RatingStore {
    pub(super) fn open(path: PathBuf) -> Result<Self, StoreError> {
        RegistryNamespace::open(path, &["fusion", "Apprater"]).map(Self)
    }

    fn integer(&self, key: &str) -> Result<i32, StoreError> {
        // Native JSON Number retains both a double and a signed integer
        // projection. getStoredInt uses its low 32 bits, not string parsing.
        Ok(self
            .0
            .get(key)?
            .as_ref()
            .filter(|v| v.is_number())
            .map_or(0, |v| {
                v.as_i64()
                    .unwrap_or_else(|| v.as_f64().unwrap_or(0.0) as i64) as i32
            }))
    }

    fn double(&self, key: &str, default: f64) -> Result<f64, StoreError> {
        Ok(self.0.get(key)?.and_then(|v| v.as_f64()).unwrap_or(default))
    }

    fn boolean(&self, key: &str) -> Result<bool, StoreError> {
        Ok(self.0.get(key)?.and_then(|v| v.as_bool()).unwrap_or(false))
    }

    pub(super) fn add_try(&self, now: &dyn Fn() -> i64) -> Result<(), StoreError> {
        let tries = self.integer("tryCount")?;
        let stored = self.double("storedTime", 0.0)?;
        if stored == 0.0 || stored > now() as f64 {
            self.0.set("storedTime", Json::from(now() as f64))?;
        }
        self.0.set("tryCount", Json::from(tries.wrapping_add(1)))
    }

    pub(super) fn need_to_prompt(
        &self,
        visible: bool,
        version: &str,
        now: &dyn Fn() -> i64,
    ) -> Result<bool, StoreError> {
        if visible {
            return Ok(false);
        }
        let mut declined = self.boolean("userHasDeclined")?;
        let mut rated = self.boolean("userHasRated")?;
        let mut later = self.boolean("userPromptedLater")?;
        let mut tries = self.integer("tryCount")?;
        let time = now() as f64;
        let stored = self.double("storedTime", time)?;
        let mut days = (time - stored) * f64::from_bits(0x3ee8_45c8_a0ce_5129);
        let previous = self
            .0
            .get("versionString")?
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        if previous != version {
            self.0.set("versionString", Json::from(version))?;
            if !version_prefix(&previous).is_empty()
                && version_prefix(&previous) != version_prefix(version)
            {
                for key in ["userHasDeclined", "userHasRated", "userPromptedLater"] {
                    self.0.set(key, Json::Bool(false))?;
                }
                self.0.set("tryCount", Json::from(0))?;
                self.0.set("storedTime", Json::from(time))?;
                declined = false;
                rated = false;
                later = false;
                tries = 0;
                days = 0.0;
            }
        }
        Ok(!declined
            && !rated
            && tries >= if later { 0 } else { 6 }
            && days >= if later { 2.0 } else { 0.0 })
    }

    pub(super) fn begin_answer(&self, now: &dyn Fn() -> i64) -> Result<i32, StoreError> {
        self.0.set("storedTime", Json::from(now() as f64))?;
        let count = self.integer("promptCount")?.wrapping_add(1);
        self.0.set("promptCount", Json::from(count))?;
        Ok(count)
    }

    pub(super) fn answer(&self, choice: AppRatingChoice) -> Result<(), StoreError> {
        let key = match choice {
            AppRatingChoice::Later => "userPromptedLater",
            AppRatingChoice::Decline => "userHasDeclined",
            AppRatingChoice::Rate => "userHasRated",
        };
        self.0.set(key, Json::Bool(true))?;
        if choice == AppRatingChoice::Later {
            self.0.set("tryCount", Json::from(0))?;
        }
        Ok(())
    }
}

fn version_prefix(version: &str) -> &str {
    version
        .match_indices('.')
        .nth(1)
        .map_or(version, |(index, _)| &version[..index])
}
