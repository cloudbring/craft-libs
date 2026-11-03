//! Named sets of overrides: the app's built-in sets (read only) and the user's own.
//!
//! ```
//! use craft_keymap::{Overrides, Profiles};
//!
//! let mut classic = Overrides::new();
//! classic.insert("edit.copy", vec!["Cmd+Shift+C".into()]);
//! let builtins = [("Craft", Overrides::new()), ("Classic", classic)];
//! let mut sets = Profiles::default();
//! assert_eq!(sets.active_name(&builtins), "Craft");
//!
//! sets.duplicate("Classic", "Mine", &builtins).unwrap();
//! let text = sets.export("Mine", &builtins).unwrap();
//! assert_eq!(sets.import(&text, &builtins), Ok("Mine 2".into()));
//! assert_eq!(sets.active_overrides(&builtins), sets.get("Mine", &builtins).unwrap());
//! ```

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Value, json};

use crate::import::{ImportPlan, ImportReport};
use crate::keymap::{Keymap, Overrides};

/// Default maximum number of user sets.
pub const MAX_SETS: usize = 256;
/// Default maximum set-name length, in chars.
pub const MAX_SET_NAME: usize = 256;
/// Largest export document [`Profiles::import`] reads, in bytes.
pub const MAX_EXPORT_BYTES: usize = 4 << 20;

const FORMAT: &str = "craft-keymap";
const VERSION: u64 = 1;
/// The version stored [`Profiles`] carry; stored profiles without one read as version 1.
const STORE_VERSION: u64 = 1;
// Load notes are kept for `Profiles::problems`; a hostile file must not grow them without bound.
const MAX_NOTES: usize = 256;
const IMPORTED: &str = "Imported";

/// Limits for loading and editing profiles. Name lengths count Unicode scalar values.
/// These are caller policy, not stored settings; pass them again when reloading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileLimits {
    pub max_sets: usize,
    pub max_set_name: usize,
    pub max_notes: usize,
}

impl Default for ProfileLimits {
    fn default() -> Self {
        Self { max_sets: MAX_SETS, max_set_name: MAX_SET_NAME, max_notes: MAX_NOTES }
    }
}

impl ProfileLimits {
    /// Opt out of limits for trusted stores. Input allocation is still bounded by available memory.
    pub const fn unlimited() -> Self {
        Self { max_sets: usize::MAX, max_set_name: usize::MAX, max_notes: usize::MAX }
    }
}

/// How loading handles names longer than the configured limit. Sets are never truncated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overflow {
    #[default]
    Error,
    /// Shorten storage names, retaining the original in each set's `display_name` field.
    /// Collisions receive unique suffixes; active selection follows the storage name.
    TruncateNames,
}

/// A store could not be loaded without exceeding the caller's limits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfilesError {
    TooManySets { count: usize, max: usize },
    NameTooLong { name: String, max: usize },
    BadFile(String),
}

impl fmt::Display for ProfilesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManySets { count, max } => write!(f, "{count} shortcut sets exceed limit {max}"),
            Self::NameTooLong { name, max } => write!(f, "shortcut set name {name:?} exceeds {max} characters"),
            Self::BadFile(error) => f.write_str(error),
        }
    }
}

impl std::error::Error for ProfilesError {}

/// The app's built-in sets: name and overrides against the catalog defaults.
pub type Builtins<'a> = [(&'a str, Overrides)];

/// Why a set operation was refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum ProfileError {
    /// Empty, over the configured name limit, or with control characters.
    BadName,
    NameTaken,
    NotFound,
    /// Built-in sets cannot be changed, renamed or deleted.
    BuiltIn,
    /// The configured maximum number of user sets exists.
    TooMany,
    /// Not a JSON export document.
    BadFile,
    /// A `format` other than `craft-keymap`.
    WrongFormat,
    /// A `version` newer than this crate reads.
    NewerVersion(u64),
    /// Over [`MAX_EXPORT_BYTES`].
    TooLarge,
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ProfileError::BadName => "bad set name",
            ProfileError::NameTaken => "a set with this name exists",
            ProfileError::NotFound => "no such set",
            ProfileError::BuiltIn => "built-in sets are read only",
            ProfileError::TooMany => "too many sets",
            ProfileError::BadFile => "not a shortcut set file",
            ProfileError::WrongFormat => "not a craft-keymap file",
            ProfileError::NewerVersion(v) => return write!(f, "shortcut set file version {v} is newer than this app reads"),
            ProfileError::TooLarge => "shortcut set file too large",
        };
        f.write_str(s)
    }
}

impl std::error::Error for ProfileError {}

/// Something loading dropped from stored [`Profiles`]; [`Profiles::problems`] lists them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum SetProblem {
    /// A set without a valid name: none, empty, with control characters or surrounding spaces.
    /// `name` is what was stored, cut to the configured name limit for this diagnostic only.
    BadName { name: String },
    /// A second set of this name; the first one stays.
    Duplicate { name: String },
    /// Overrides that are neither a list nor a map.
    BadOverrides { name: String },
    /// Legacy diagnostic; loading now returns [`ProfilesError::TooManySets`] instead.
    TooMany { ignored: usize },
    /// Stored by a newer craft-keymap; what this version reads is loaded.
    NewerVersion(u64),
}

/// The user's sets and which set is active. The built-in sets are passed to each call.
///
/// An active name that matches no set means the first built-in set (or no overrides when the app
/// has none).
///
/// A user set may carry a built-in's name, for instance after an update adds that built-in. Until
/// [`Profiles::reconcile`] renames it, the user set is the one that name reads and writes. Call
/// `reconcile` after loading; every method that changes the profiles calls it first.
///
/// Stored as `{ version, active, sets }`. Serde loading uses default limits and rejects overflow.
/// [`Profiles::load`] accepts explicit limits and a name-overflow policy. Malformed entries
/// remain reported by [`Profiles::problems`]; exceeding limits never drops a set.
#[derive(Clone, Debug, Default)]
pub struct Profiles {
    active: String,
    sets: BTreeMap<String, Overrides>,
    notes: Vec<SetProblem>,
    display_names: BTreeMap<String, String>,
    limits: ProfileLimits,
}

impl PartialEq for Profiles {
    fn eq(&self, other: &Profiles) -> bool {
        self.active == other.active && self.sets == other.sets && self.display_names == other.display_names
    }
}

impl Eq for Profiles {}

static NO_OVERRIDES: Overrides = Overrides::new();

/// A name as given, trimmed; `None` when empty, too long or with control characters.
pub fn set_name(name: &str) -> Option<String> {
    checked_name(name, MAX_SET_NAME)
}

fn checked_name(name: &str, max: usize) -> Option<String> {
    let n = name.trim();
    let ok = !n.is_empty() && n.chars().count() <= max && !n.chars().any(char::is_control);
    ok.then(|| n.to_string())
}

impl Profiles {
    /// Empty profiles with caller-selected limits, also used by subsequent editing operations.
    pub fn with_limits(limits: ProfileLimits) -> Self {
        Self { limits, ..Self::default() }
    }

    pub fn limits(&self) -> ProfileLimits {
        self.limits
    }

    /// A trimmed valid storage name under this instance's configured limits.
    pub fn validate_name(&self, name: &str) -> Option<String> {
        checked_name(name, self.limits.max_set_name)
    }

    /// The original name retained by `TruncateNames`, or the storage name when not truncated.
    pub fn display_name<'a>(&'a self, name: &'a str) -> &'a str {
        self.display_names.get(name).map_or(name, String::as_str)
    }

    /// Load a JSON store with explicit limits. Failure leaves the caller's existing profiles
    /// untouched. Both version-1 arrays and legacy maps of names to overrides are accepted.
    pub fn load(text: &str, limits: ProfileLimits, overflow: Overflow) -> Result<Self, ProfilesError> {
        let stored = serde_json::from_str(text).map_err(|error: serde_json::Error| ProfilesError::BadFile(error.to_string()))?;
        Self::load_stored(stored, limits, overflow)
    }

    /// The stored active name, which may match no set; [`active_name`](Self::active_name) is the
    /// set in use.
    pub fn active(&self) -> &str {
        &self.active
    }

    /// The user's sets.
    pub fn sets(&self) -> &BTreeMap<String, Overrides> {
        &self.sets
    }

    /// What loading dropped.
    pub fn problems(&self) -> &[SetProblem] {
        &self.notes
    }

    /// Built-in names first, then the user's sets.
    pub fn names(&self, builtins: &Builtins) -> Vec<String> {
        builtins.iter().map(|(n, _)| n.to_string()).chain(self.sets.keys().cloned()).collect()
    }

    pub fn is_builtin(name: &str, builtins: &Builtins) -> bool {
        builtins.iter().any(|(n, _)| *n == name)
    }

    /// The name of the set in use.
    pub fn active_name<'a>(&'a self, builtins: &'a Builtins) -> &'a str {
        if self.sets.contains_key(&self.active) || Self::is_builtin(&self.active, builtins) {
            &self.active
        } else {
            builtins.first().map_or("", |(n, _)| n)
        }
    }

    /// The overrides of the set in use, to hand to [`Keymap::set_overrides`](crate::Keymap::set_overrides).
    pub fn active_overrides<'a>(&'a self, builtins: &'a Builtins) -> &'a Overrides {
        self.get(self.active_name(builtins), builtins).unwrap_or(&NO_OVERRIDES)
    }

    pub fn get<'a>(&'a self, name: &str, builtins: &'a Builtins) -> Option<&'a Overrides> {
        self.sets.get(name).or_else(|| builtins.iter().find(|(n, _)| *n == name).map(|(_, o)| o))
    }

    /// Renames user sets whose name matches a built-in's (ignoring case) to a free name; `active`
    /// follows. Returns whether anything was renamed.
    pub fn reconcile(&mut self, builtins: &Builtins) -> bool {
        let clashing: Vec<String> = self.sets.keys().filter(|k| self.builtin_like(k, builtins)).cloned().collect();
        let mut renamed = false;
        for old in clashing {
            let Some(set) = self.sets.remove(&old) else { continue };
            // `unique` cannot fail here: a set was just removed, so there is room and the
            // candidate names outnumber the sets.
            let name = self.unique(&old, builtins).unwrap_or_else(|_| old.clone());
            if self.active == old {
                self.active = name.clone();
            }
            renamed |= name != old;
            if let Some(display_name) = self.display_names.remove(&old) {
                self.display_names.insert(name.clone(), display_name);
            }
            self.sets.insert(name, set);
        }
        renamed
    }

    pub fn select(&mut self, name: &str, builtins: &Builtins) -> Result<(), ProfileError> {
        self.reconcile(builtins);
        if self.get(name, builtins).is_none() {
            return Err(ProfileError::NotFound);
        }
        self.active = name.to_string();
        Ok(())
    }

    /// Saves edited overrides to the set in use. A built-in set is not changed: the edit goes to a
    /// new set named after `copy_name` (made unique), which becomes active and is returned. The
    /// new name is returned too when [`Profiles::reconcile`] renamed the set in use.
    pub fn store(&mut self, overrides: Overrides, builtins: &Builtins, copy_name: &str) -> Result<Option<String>, ProfileError> {
        let was = self.active_name(builtins).to_string();
        self.reconcile(builtins);
        let active = self.active_name(builtins).to_string();
        if let Some(set) = self.sets.get_mut(&active) {
            *set = overrides;
            return Ok((active != was).then_some(active));
        }
        if self.get(&active, builtins) == Some(&overrides) || (active.is_empty() && overrides.is_empty()) {
            return Ok(None);
        }
        let base = checked_name(copy_name, self.limits.max_set_name).ok_or(ProfileError::BadName)?;
        self.add(&base, overrides, builtins).map(Some)
    }

    /// Saves `overrides` as a user set (replacing one of that name) and makes it active.
    pub fn save_as(&mut self, name: &str, overrides: Overrides, builtins: &Builtins) -> Result<String, ProfileError> {
        self.reconcile(builtins);
        let name = checked_name(name, self.limits.max_set_name).ok_or(ProfileError::BadName)?;
        if self.builtin_like(&name, builtins) {
            return Err(ProfileError::BuiltIn);
        }
        if !self.sets.contains_key(&name) {
            if self.sets.keys().any(|k| k.to_lowercase() == name.to_lowercase()) {
                return Err(ProfileError::NameTaken);
            }
            if self.sets.len() >= self.limits.max_sets {
                return Err(ProfileError::TooMany);
            }
        }
        self.sets.insert(name.clone(), overrides);
        self.active = name.clone();
        Ok(name)
    }

    pub fn rename(&mut self, from: &str, to: &str, builtins: &Builtins) -> Result<String, ProfileError> {
        self.reconcile(builtins);
        if Self::is_builtin(from, builtins) {
            return Err(ProfileError::BuiltIn);
        }
        let to = checked_name(to, self.limits.max_set_name).ok_or(ProfileError::BadName)?;
        if !self.sets.contains_key(from) {
            return Err(ProfileError::NotFound);
        }
        // A settings file can hold names that differ only in case, so a case-only rename still
        // checks the other sets.
        let lower = to.to_lowercase();
        let clash = self.sets.keys().any(|k| k != from && k.to_lowercase() == lower);
        if to != from && (clash || (self.builtin_like(&to, builtins) && lower != from.to_lowercase())) {
            return Err(ProfileError::NameTaken);
        }
        if let Some(set) = self.sets.remove(from) {
            self.sets.insert(to.clone(), set);
        }
        if let Some(display_name) = self.display_names.remove(from) {
            self.display_names.insert(to.clone(), display_name);
        }
        if self.active == from {
            self.active = to.clone();
        }
        Ok(to)
    }

    pub fn delete(&mut self, name: &str, builtins: &Builtins) -> Result<(), ProfileError> {
        self.reconcile(builtins);
        if Self::is_builtin(name, builtins) {
            return Err(ProfileError::BuiltIn);
        }
        self.sets.remove(name).ok_or(ProfileError::NotFound)?;
        self.display_names.remove(name);
        if self.active == name {
            self.active = builtins.first().map_or(String::new(), |(n, _)| n.to_string());
        }
        Ok(())
    }

    /// Copies a set (built-in or user) to a new user set named `to` (made unique), which becomes
    /// active. Returns its name.
    pub fn duplicate(&mut self, from: &str, to: &str, builtins: &Builtins) -> Result<String, ProfileError> {
        self.reconcile(builtins);
        let overrides = self.get(from, builtins).ok_or(ProfileError::NotFound)?.clone();
        let base = checked_name(to, self.limits.max_set_name).ok_or(ProfileError::BadName)?;
        let display_name = self.display_names.get(from).cloned();
        let name = self.add(&base, overrides, builtins)?;
        if let Some(display_name) = display_name {
            self.display_names.insert(name.clone(), display_name);
        }
        Ok(name)
    }

    /// A set as a JSON document: `{"format":"craft-keymap","version":1,"name":…,"overrides":…}`,
    /// overrides as [`Overrides`] serializes them.
    pub fn export(&self, name: &str, builtins: &Builtins) -> Result<String, ProfileError> {
        let overrides = self.get(name, builtins).ok_or(ProfileError::NotFound)?;
        let mut doc = json!({"format": FORMAT, "version": VERSION, "name": name, "overrides": overrides});
        if let Some(display_name) = self.display_names.get(name) {
            doc["display_name"] = json!(display_name);
        }
        serde_json::to_string_pretty(&doc).map_err(|_| ProfileError::BadFile)
    }

    /// Reads an exported set into a new user set (renamed when the name is taken) and makes it
    /// active. Returns its name.
    pub fn import(&mut self, text: &str, builtins: &Builtins) -> Result<String, ProfileError> {
        if text.len() > MAX_EXPORT_BYTES {
            return Err(ProfileError::TooLarge);
        }
        let doc: Value = serde_json::from_str(text).map_err(|_| ProfileError::BadFile)?;
        if doc.get("format").and_then(Value::as_str) != Some(FORMAT) {
            return Err(ProfileError::WrongFormat);
        }
        match doc.get("version").and_then(Value::as_u64) {
            Some(VERSION) => {}
            Some(v) if v > VERSION => return Err(ProfileError::NewerVersion(v)),
            _ => return Err(ProfileError::BadFile),
        }
        let overrides = match doc.get("overrides") {
            Some(v @ (Value::Object(_) | Value::Array(_))) => Overrides::deserialize(v).map_err(|_| ProfileError::BadFile)?,
            None => Overrides::new(),
            Some(_) => return Err(ProfileError::BadFile),
        };
        self.reconcile(builtins);
        let base = doc.get("name").and_then(Value::as_str).unwrap_or(IMPORTED);
        let base = checked_name(base, self.limits.max_set_name).ok_or(ProfileError::BadName)?;
        let name = self.add(&base, overrides, builtins)?;
        if let Some(display_name) = doc.get("display_name").and_then(Value::as_str) {
            self.display_names.insert(name.clone(), display_name.to_string());
        }
        Ok(name)
    }

    /// Applies an import plan to a copy of `keymap` (the keymap the plan was made from) and keeps
    /// the resulting overrides as a new user set, which becomes active; `keymap` is not changed.
    /// The set is named `name`, else the foreign set's name, else `Imported`, made unique.
    /// Returns its name and the report of [`ImportPlan::apply`].
    pub fn import_plan(
        &mut self,
        plan: &ImportPlan,
        keymap: &Keymap,
        name: Option<&str>,
        builtins: &Builtins,
    ) -> Result<(String, ImportReport), ProfileError> {
        self.reconcile(builtins);
        if self.sets.len() >= self.limits.max_sets {
            return Err(ProfileError::TooMany);
        }
        let mut copy = keymap.clone();
        let report = plan.apply(&mut copy);
        let base = checked_name(name.or(plan.report.set.as_deref()).unwrap_or(IMPORTED), self.limits.max_set_name).ok_or(ProfileError::BadName)?;
        let name = self.add(&base, copy.overrides().clone(), builtins)?;
        Ok((name, report))
    }

    /// Adds a user set under a unique name from `base` and makes it active.
    fn add(&mut self, base: &str, overrides: Overrides, builtins: &Builtins) -> Result<String, ProfileError> {
        let name = self.unique(base, builtins)?;
        self.sets.insert(name.clone(), overrides);
        self.active = name.clone();
        Ok(name)
    }

    fn builtin_like(&self, name: &str, builtins: &Builtins) -> bool {
        let lower = name.to_lowercase();
        builtins.iter().any(|(n, _)| n.to_lowercase() == lower)
    }

    /// Whether `name` matches a built-in or user set, ignoring case.
    pub(crate) fn taken(&self, name: &str, builtins: &Builtins) -> bool {
        let lower = name.to_lowercase();
        self.builtin_like(name, builtins) || self.sets.keys().any(|k| k.to_lowercase() == lower)
    }

    /// `base`, or `base 2`, `base 3`… cut to fit the configured name limit.
    fn unique(&self, base: &str, builtins: &Builtins) -> Result<String, ProfileError> {
        if self.sets.len() >= self.limits.max_sets {
            return Err(ProfileError::TooMany);
        }
        if !self.taken(base, builtins) {
            return Ok(base.to_string());
        }
        // `Custom 2` taken: try `Custom 3`, not `Custom 2 2`.
        let base = match base.rsplit_once(' ') {
            Some((stem, n)) if !stem.is_empty() && (1..=3).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_digit()) => stem,
            _ => base,
        };
        for n in 2..self.sets.len().saturating_add(builtins.len()).saturating_add(3) {
            let suffix = format!(" {n}");
            if suffix.chars().count() > self.limits.max_set_name {
                continue;
            }
            let keep = self.limits.max_set_name.saturating_sub(suffix.chars().count());
            let name: String = base.chars().take(keep).collect::<String>().trim_end().to_string() + &suffix;
            if !self.taken(&name, builtins) {
                return Ok(name);
            }
        }
        Err(ProfileError::NameTaken)
    }
}

impl Serialize for Profiles {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("Profiles", 3)?;
        s.serialize_field("version", &STORE_VERSION)?;
        s.serialize_field("active", &self.active)?;
        s.serialize_field("sets", &SetList(self))?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for Profiles {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Profiles, D::Error> {
        Self::load_stored(Stored::deserialize(deserializer)?, ProfileLimits::default(), Overflow::Error).map_err(de::Error::custom)
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Stored {
    version: Value,
    active: Value,
    #[serde(deserialize_with = "read_sets")]
    sets: Vec<StoredSet>,
}

struct StoredSet {
    name: String,
    overrides: Option<Value>,
    display_name: Option<String>,
}

fn read_sets<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<StoredSet>, D::Error> {
    deserializer.deserialize_any(SetsVisitor)
}

struct SetsVisitor;

macro_rules! no_sets {
    ($($method:ident: $kind:ty),*) => {
        $(fn $method<E: de::Error>(self, _: $kind) -> Result<Self::Value, E> {
            Ok(Vec::new())
        })*
    };
}

impl<'de> Visitor<'de> for SetsVisitor {
    type Value = Vec<StoredSet>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a list of shortcut sets or a map of names to overrides")
    }

    no_sets!(visit_bool: bool, visit_i64: i64, visit_u64: u64, visit_i128: i128, visit_u128: u128, visit_f64: f64, visit_str: &str, visit_bytes: &[u8], visit_char: char);

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(Vec::new())
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(Vec::new())
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        read_sets(deserializer)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
        let mut sets = Vec::new();
        while let Some(mut value) = access.next_element::<Value>()? {
            sets.push(StoredSet {
                name: value.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
                display_name: value.get("display_name").and_then(Value::as_str).map(str::to_string),
                overrides: value.get_mut("overrides").map(Value::take),
            });
        }
        Ok(sets)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
        let mut sets = Vec::new();
        while let Some((name, overrides)) = access.next_entry::<String, Value>()? {
            sets.push(StoredSet { name, overrides: Some(overrides), display_name: None });
        }
        Ok(sets)
    }
}

struct SetList<'a>(&'a Profiles);

impl Serialize for SetList<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Set<'a> {
            name: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            display_name: Option<&'a str>,
            #[serde(serialize_with = "crate::keymap::overrides_list::serialize")]
            overrides: &'a Overrides,
        }
        serializer.collect_seq(self.0.sets.iter().map(|(name, overrides)| Set {
            name,
            display_name: self.0.display_names.get(name).map(String::as_str),
            overrides,
        }))
    }
}

impl Profiles {
    fn note(&mut self, problem: SetProblem) {
        if self.notes.len() < self.limits.max_notes {
            self.notes.push(problem);
        }
    }

    fn load_stored(stored: Stored, limits: ProfileLimits, overflow: Overflow) -> Result<Self, ProfilesError> {
        let active = stored.active.as_str().unwrap_or_default();
        let mut out = Self { active: active.to_string(), ..Self::with_limits(limits) };
        if let Some(version) = stored.version.as_u64().filter(|version| *version > STORE_VERSION) {
            out.note(SetProblem::NewerVersion(version));
        }
        let entries: Vec<(&str, Option<&Value>, Option<&str>)> =
            stored.sets.iter().map(|set| (set.name.as_str(), set.overrides.as_ref(), set.display_name.as_deref())).collect();
        if entries.len() > limits.max_sets {
            return Err(ProfilesError::TooManySets { count: entries.len(), max: limits.max_sets });
        }
        let reserved: std::collections::BTreeSet<&str> = entries.iter().map(|(name, _, _)| *name).collect();
        let mut seen = std::collections::BTreeSet::new();
        for (original, overrides, display_name) in &entries {
            let too_long = original.chars().count() > limits.max_set_name;
            if too_long && overflow == Overflow::Error {
                return Err(ProfilesError::NameTooLong { name: (*original).to_string(), max: limits.max_set_name });
            }
            let shown = || original.chars().take(limits.max_set_name).collect::<String>();
            if original.is_empty() || original.trim() != *original || original.chars().any(char::is_control) {
                out.note(SetProblem::BadName { name: shown() });
                continue;
            }
            if !seen.insert(*original) {
                out.note(SetProblem::Duplicate { name: shown() });
                continue;
            }
            let Some(overrides) = overrides.and_then(overrides_of) else {
                out.note(SetProblem::BadOverrides { name: shown() });
                continue;
            };
            let name = if too_long { out.short_name(original, &reserved)? } else { (*original).to_string() };
            if active == *original {
                out.active = name.clone();
            }
            if let Some(display_name) = display_name.or(too_long.then_some(*original)) {
                out.display_names.insert(name.clone(), display_name.to_string());
            }
            out.sets.insert(name, overrides);
        }
        Ok(out)
    }

    fn short_name(&self, original: &str, reserved: &std::collections::BTreeSet<&str>) -> Result<String, ProfilesError> {
        let max = self.limits.max_set_name;
        let prefix: String = original.chars().take(max).collect::<String>().trim_end().to_string();
        if !prefix.is_empty() && !reserved.contains(prefix.as_str()) && !self.sets.contains_key(&prefix) {
            return Ok(prefix);
        }
        for number in 1..=reserved.len().saturating_add(self.sets.len()).saturating_add(1) {
            let suffix = number.to_string();
            if suffix.len() > max {
                break;
            }
            let name = original.chars().take(max - suffix.len()).collect::<String>() + &suffix;
            if !reserved.contains(name.as_str()) && !self.sets.contains_key(&name) {
                return Ok(name);
            }
        }
        Err(ProfilesError::NameTooLong { name: original.to_string(), max })
    }
}

/// A set's overrides: a list or a map, anything else is no set.
fn overrides_of(v: &Value) -> Option<Overrides> {
    match v {
        Value::Object(_) | Value::Array(_) => Overrides::deserialize(v).ok(),
        _ => None,
    }
}
