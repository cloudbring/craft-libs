//! Generic OPC packaging. Provenance: deckcraft/crates/pptx/src/opc.rs,
//! designcraft/crates/pptx/src/package.rs, wordcraft/crates/docx/src/package.rs,
//! gridcraft/crates/xlsx/src/package.rs. Slide, document, and sheet models remain in apps.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read, Write};
use std::sync::Arc;

use craft_archive::{ZipError, ZipGuard, ZipLimits};

use crate::dom::{self, Limits, XmlError};
use crate::write::XmlWriter;

const CT_NS: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
const REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const REL_TYPE: &str = "application/vnd.openxmlformats-package.relationships+xml";

/// Ordering of package parts after the content-types entry.
#[derive(Default)]
pub enum PartOrder {
    Insertion,
    #[default]
    Sorted,
    /// Patterns are applied in order; unmatched parts retain insertion order.
    Explicit(Vec<String>),
    Ranked(Box<dyn Fn(&str) -> u32>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Deflate(Option<i64>),
    Store,
}

/// Compression selection for each ZIP entry (including metadata entries).
pub enum MethodRule {
    AllDeflate(Option<i64>),
    StoreMedia,
    Custom(Box<dyn Fn(&str) -> Method>),
}

impl MethodRule {
    fn for_name(&self, name: &str) -> Method {
        match self {
            Self::AllDeflate(level) => Method::Deflate(*level),
            Self::StoreMedia if is_media(name) => Method::Store,
            Self::StoreMedia => Method::Deflate(Some(6)),
            Self::Custom(rule) => rule(name),
        }
    }
}

fn is_media(name: &str) -> bool {
    [".png", ".jpg", ".jpeg", ".gif", ".webp"].iter().any(|ext| name.to_ascii_lowercase().ends_with(ext))
}

/// How to produce `[Content_Types].xml`.
pub enum ContentTypesPolicy {
    Emit,
    EmitWithDefaults(Vec<(String, String)>),
    Preserve(Vec<u8>),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RelsPlacement {
    BeforeParts,
    AfterParts,
    #[default]
    Sorted,
}

/// ZIP DOS timestamp (defaults to the ZIP crate's deterministic 1980-01-01).
pub type DosTime = zip::DateTime;

/// Output choices independent of the OPC part/relationship model.
pub struct WritePolicy {
    pub part_order: PartOrder,
    pub method: MethodRule,
    pub content_types: ContentTypesPolicy,
    pub rels_placement: RelsPlacement,
    pub timestamps: DosTime,
}

impl Default for WritePolicy {
    fn default() -> Self {
        Self {
            part_order: PartOrder::Sorted,
            method: MethodRule::AllDeflate(None),
            content_types: ContentTypesPolicy::Emit,
            rels_placement: RelsPlacement::Sorted,
            timestamps: DosTime::default(),
        }
    }
}

impl WritePolicy {
    /// Provenance: deckcraft/crates/pptx/src/opc.rs `PackageOut::finish` and `binary`.
    pub fn deckcraft() -> Self {
        Self {
            part_order: PartOrder::Insertion,
            method: MethodRule::Custom(Box::new(|name| {
                if name.ends_with(".xml") || name.ends_with(".rels") { Method::Deflate(None) } else { Method::Store }
            })),
            content_types: ContentTypesPolicy::EmitWithDefaults(vec![("xml".into(), "application/xml".into())]),
            rels_placement: RelsPlacement::AfterParts,
            ..Self::default()
        }
    }

    /// Provenance: wordcraft/crates/docx/src/package.rs `zip_entries`.
    pub fn wordcraft() -> Self {
        Self { part_order: PartOrder::Insertion, method: MethodRule::StoreMedia, rels_placement: RelsPlacement::AfterParts, ..Self::default() }
    }

    /// Provenance: gridcraft/crates/xlsx/src/package.rs `ZipOut::add` and xlsx/src/write.rs `rank`.
    pub fn gridcraft() -> Self {
        Self {
            part_order: PartOrder::Ranked(Box::new(|name| match name {
                "_rels/.rels" => 0,
                "xl/_rels/workbook.xml.rels" => 1,
                "xl/workbook.xml" => 2,
                name if name.starts_with("docprops/") => 4,
                _ => 3,
            })),
            method: MethodRule::AllDeflate(Some(6)),
            rels_placement: RelsPlacement::AfterParts,
            ..Self::default()
        }
    }

    /// Provenance: designcraft/crates/pptx/src/package.rs `write` (media except SVG is stored).
    pub fn designcraft() -> Self {
        Self {
            part_order: PartOrder::Insertion,
            method: MethodRule::Custom(Box::new(|name| {
                if name.starts_with("ppt/media/") && !name.ends_with(".svg") { Method::Store } else { Method::Deflate(None) }
            })),
            rels_placement: RelsPlacement::AfterParts,
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OpcLimits {
    pub zip: ZipLimits,
    pub xml: Limits,
    pub max_archive_bytes: usize,
}

impl Default for OpcLimits {
    fn default() -> Self {
        Self {
            zip: ZipLimits { max_entries: 20_000, max_entry_bytes: 256 * 1024 * 1024, max_total_bytes: 512 * 1024 * 1024 },
            xml: Limits::default(),
            max_archive_bytes: 512 * 1024 * 1024,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OpcError {
    #[error(transparent)]
    Zip(#[from] ZipError),
    #[error("ZIP error: {0}")]
    ZipFormat(#[from] zip::result::ZipError),
    #[error("ZIP I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Xml(#[from] XmlError),
    #[error("invalid OPC: {0}")]
    Invalid(String),
    #[error("OPC limit exceeded: {0}")]
    Limit(&'static str),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TargetMode {
    #[default]
    Internal,
    External,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relationship {
    pub id: String,
    pub kind: String,
    /// Resolved part name (internal) or untouched URI (external).
    pub target: String,
    pub mode: TargetMode,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Relationships(pub Vec<Relationship>);

impl Relationships {
    pub fn by_id(&self, id: &str) -> Option<&Relationship> {
        self.0.iter().find(|rel| rel.id == id)
    }
    pub fn by_type(&self, kind: &str) -> Option<&Relationship> {
        self.0.iter().find(|rel| rel.kind == kind)
    }
    pub fn iter(&self) -> impl Iterator<Item = &Relationship> {
        self.0.iter()
    }
}

#[derive(Clone, Debug, Default)]
pub struct ContentTypes {
    pub defaults: BTreeMap<String, String>,
    pub overrides: BTreeMap<String, String>,
}

impl ContentTypes {
    pub fn of(&self, name: &str) -> Option<&str> {
        let name = name.trim_start_matches('/').to_ascii_lowercase();
        self.overrides.get(&name).or_else(|| name.rsplit_once('.').and_then(|(_, ext)| self.defaults.get(ext))).map(String::as_str)
    }
}

/// Opt-in tolerant-read switches for [`Package::open_with`]. The default is fully strict.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tolerance {
    /// Case-duplicate entries merge into one part, preferring the spelling a relationship names.
    pub merge_case_duplicates: bool,
    /// A missing `[Content_Types].xml` yields empty content types plus a warning.
    pub missing_content_types: bool,
    /// Malformed `.rels` (root, missing attribute, duplicate id) degrade to a partial list.
    pub lenient_rels: bool,
    /// A corrupt or unreadable ZIP entry is skipped; its bytes still count against the budget.
    pub skip_corrupt_entries: bool,
}

impl Tolerance {
    pub fn lenient() -> Self {
        Self { merge_case_duplicates: true, missing_content_types: true, lenient_rels: true, skip_corrupt_entries: true }
    }
}

struct Part {
    name: String,
    data: Arc<[u8]>,
}

type Groups = BTreeMap<String, Vec<(String, Arc<[u8]>)>>;

pub struct Package {
    parts: BTreeMap<String, Part>,
    pub content_types: ContentTypes,
    relationships: BTreeMap<String, Relationships>,
    warnings: Vec<String>,
}

impl Package {
    pub fn open(bytes: &[u8], limits: &OpcLimits) -> Result<Self, OpcError> {
        Self::open_with(bytes, limits, &Tolerance::default())
    }

    /// Like [`open`](Self::open) with the given tolerances; problems are reported in `warnings()`.
    pub fn open_with(bytes: &[u8], limits: &OpcLimits, tolerance: &Tolerance) -> Result<Self, OpcError> {
        if bytes.len() > limits.max_archive_bytes {
            return Err(OpcError::Limit("archive bytes"));
        }
        let archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        let names: Vec<_> = (0..archive.len())
            .map(|index| {
                archive
                    .name_for_index(index)
                    .ok_or_else(|| OpcError::Invalid("missing entry name".into()))?
                    .map(|name| name.into_owned())
                    .map_err(OpcError::from)
            })
            .collect::<Result<_, _>>()?;
        let mut guard = ZipGuard::new(archive, limits.zip)?;
        let mut groups = Groups::new();
        let mut warnings = Vec::new();
        // Declared bytes of skipped entries that the guard did not see decoded.
        let mut unread = 0u64;
        for (index, name) in names.iter().enumerate() {
            let before = guard.total_read();
            let entry = match guard.read_entry(index) {
                Ok(entry) => entry,
                Err(error @ (ZipError::Archive(_) | ZipError::Io(_))) if tolerance.skip_corrupt_entries => {
                    let declared = guard.entry(index).map_or(0, |entry| entry.size);
                    unread = unread.saturating_add(declared.saturating_sub(guard.total_read() - before));
                    if guard.total_read().saturating_add(unread) > limits.zip.max_total_bytes {
                        return Err(ZipError::TotalLimit.into());
                    }
                    warnings.push(format!("{name}: unreadable entry skipped: {error}"));
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            if name.ends_with('/') {
                continue;
            }
            let original = checked_part(name)?;
            let group = groups.entry(original.to_ascii_lowercase()).or_default();
            if !group.is_empty() && !tolerance.merge_case_duplicates {
                return Err(OpcError::Invalid("case-duplicate part".into()));
            }
            group.push((original.to_string(), Arc::from(entry)));
        }
        Self::assemble(groups, limits, tolerance, warnings)
    }

    /// Build a package from in-memory parts keyed by part name. `Lenient` also enables every
    /// [`Tolerance`] switch (there are no ZIP entries to skip, but names may collide by case).
    pub fn from_parts(parts: BTreeMap<String, Vec<u8>>, limits: &OpcLimits, mode: dom::ReadMode) -> Result<Self, OpcError> {
        let mut limits = *limits;
        limits.xml.read_mode = mode;
        let tolerance = if mode == dom::ReadMode::Lenient { Tolerance::lenient() } else { Tolerance::default() };
        let mut total = 0u64;
        let mut groups = Groups::new();
        for (name, data) in parts {
            if data.len() as u64 > limits.zip.max_entry_bytes {
                return Err(ZipError::EntryLimit.into());
            }
            total = total.saturating_add(data.len() as u64);
            if total > limits.zip.max_total_bytes {
                return Err(ZipError::TotalLimit.into());
            }
            let original = checked_part(&name)?.to_string();
            let group = groups.entry(original.to_ascii_lowercase()).or_default();
            if !group.is_empty() && !tolerance.merge_case_duplicates {
                return Err(OpcError::Invalid("case-duplicate part".into()));
            }
            group.push((original, Arc::from(data)));
        }
        Self::assemble(groups, &limits, &tolerance, Vec::new())
    }

    fn assemble(groups: Groups, limits: &OpcLimits, tolerance: &Tolerance, mut warnings: Vec<String>) -> Result<Self, OpcError> {
        let mut chosen: BTreeMap<String, (String, Arc<[u8]>)> = BTreeMap::new();
        let mut pending = Vec::new();
        for (key, mut group) in groups {
            if group.len() == 1 {
                chosen.insert(key, group.remove(0));
            } else if key == "[content_types].xml" || source_of_rels(&key).is_some() {
                warnings.push(format!("case-duplicate {key}: kept {}", group[0].0));
                chosen.insert(key, group.remove(0));
            } else {
                pending.push((key, group));
            }
        }
        if !pending.is_empty() {
            let mut targets = BTreeSet::new();
            for (name, (_, data)) in &chosen {
                if let Some(source) = source_of_rels(name)
                    && let Ok(rels) = parse_rels(name, &source, data, limits, &Tolerance::lenient(), &mut Vec::new())
                {
                    targets.extend(rels.0.into_iter().filter(|rel| rel.mode == TargetMode::Internal).map(|rel| rel.target));
                }
            }
            for (key, group) in pending {
                let position = group.iter().position(|(name, _)| targets.contains(name)).unwrap_or(0);
                let spellings: Vec<_> = group.iter().map(|(name, _)| name.as_str()).collect();
                warnings.push(format!("case-duplicate parts {} merged into {}", spellings.join(", "), spellings[position]));
                if let Some(entry) = group.into_iter().nth(position) {
                    chosen.insert(key, entry);
                }
            }
        }
        let mut content_types = ContentTypes::default();
        match chosen.get("[content_types].xml") {
            Some((_, ct_bytes)) => content_types = parse_content_types(ct_bytes, &limits.xml)?,
            None if tolerance.missing_content_types => warnings.push("missing [Content_Types].xml: using empty content types".into()),
            None => return Err(OpcError::Invalid("missing content types".into())),
        }
        let mut relationships = BTreeMap::new();
        for (name, (_, data)) in &chosen {
            if let Some(source) = source_of_rels(name) {
                let rels = parse_rels(name, &source, data, limits, tolerance, &mut warnings)?;
                relationships.insert(source, rels);
            }
        }
        let parts = chosen.into_iter().map(|(key, (name, data))| (key, Part { name, data })).collect();
        Ok(Self { parts, content_types, relationships, warnings })
    }

    /// Notes about skipped or repaired input: relationships skipped under
    /// [`dom::ReadMode::Lenient`] and anything tolerated via [`Tolerance`]. Empty when strict.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub fn part(&self, name: &str) -> Option<&[u8]> {
        self.parts.get(&name.trim_start_matches('/').to_ascii_lowercase()).map(|part| &part.data[..])
    }

    /// Shared handle to a part's bytes; cheap to clone and keep after the package is dropped.
    pub fn part_shared(&self, name: &str) -> Option<Arc<[u8]>> {
        self.parts.get(&name.trim_start_matches('/').to_ascii_lowercase()).map(|part| Arc::clone(&part.data))
    }

    /// Stored (original-case) part names, in canonical (lowercase) order.
    pub fn part_names(&self) -> impl Iterator<Item = &str> {
        self.parts.values().map(|part| part.name.as_str())
    }

    /// Map a name in any case to the spelling stored in the package.
    pub fn canonical(&self, name: &str) -> Option<&str> {
        self.parts.get(&name.trim_start_matches('/').to_ascii_lowercase()).map(|part| part.name.as_str())
    }

    /// Empty string denotes the package root.
    pub fn rels_of(&self, source: &str) -> Option<&Relationships> {
        self.relationships.get(&source.trim_start_matches('/').to_ascii_lowercase())
    }
}

fn parse_rels(
    name: &str,
    source: &str,
    data: &[u8],
    limits: &OpcLimits,
    tolerance: &Tolerance,
    warnings: &mut Vec<String>,
) -> Result<Relationships, OpcError> {
    let tolerant = tolerance.lenient_rels;
    let root = match dom::parse(data, &limits.xml) {
        Ok(root) => root,
        Err(error) if tolerant => {
            warnings.push(format!("{name}: relationships ignored: {error}"));
            return Ok(Relationships::default());
        }
        Err(error) => return Err(error.into()),
    };
    if root.name != "Relationships" {
        if tolerant {
            warnings.push(format!("{name}: relationships ignored: wrong root element"));
            return Ok(Relationships::default());
        }
        return Err(OpcError::Invalid("relationships root".into()));
    }
    let mut rels = Relationships::default();
    for node in root.children("Relationship") {
        let (Some(id), Some(kind), Some(raw)) = (node.attr("Id"), node.attr("Type"), node.attr("Target")) else {
            if tolerant {
                warnings.push(format!("{name}: incomplete relationship skipped"));
                continue;
            }
            return Err(OpcError::Invalid("incomplete relationship".into()));
        };
        if rels.by_id(id).is_some() {
            if tolerant {
                warnings.push(format!("{name}: duplicate relationship id {id} skipped"));
                continue;
            }
            return Err(OpcError::Invalid("duplicate relationship id".into()));
        }
        let mode = if node.attr("TargetMode").is_some_and(|value| value.eq_ignore_ascii_case("external")) {
            TargetMode::External
        } else {
            TargetMode::Internal
        };
        let target = if mode == TargetMode::External {
            raw.to_string()
        } else {
            match resolve_target(source, raw) {
                Ok(target) => target,
                // Lenient reads skip a bad relationship instead of failing the whole
                // package; the target is never followed in either mode.
                Err(error) if limits.xml.read_mode == dom::ReadMode::Lenient => {
                    warnings.push(format!("{name}: relationship {id} skipped: {error}"));
                    continue;
                }
                Err(error) => return Err(error),
            }
        };
        rels.0.push(Relationship { id: id.into(), kind: kind.into(), target, mode });
    }
    Ok(rels)
}

/// Index-only reader for large archives (spreadsheets, ODS). Parts are decompressed one at a time
/// on request; a damaged part fails only its own read. Not OPC-specific: `[Content_Types].xml`
/// is parsed if present and otherwise ignored.
pub struct LazyPackage {
    archive: zip::ZipArchive<Cursor<Vec<u8>>>,
    index: BTreeMap<String, LazyEntry>,
    limits: OpcLimits,
    total_read: u64,
    pub content_types: ContentTypes,
    warnings: Vec<String>,
}

#[derive(Clone, Copy)]
struct LazyEntry {
    index: usize,
    size: u64,
    compressed: u64,
}

/// Expansion ratio above which a part larger than 1 MiB is treated as a zip bomb.
const LAZY_MAX_RATIO: u64 = 500;
const LAZY_RATIO_FLOOR: u64 = 1024 * 1024;

impl LazyPackage {
    pub fn open(bytes: Vec<u8>, limits: &OpcLimits) -> Result<Self, OpcError> {
        if bytes.len() > limits.max_archive_bytes {
            return Err(OpcError::Limit("archive bytes"));
        }
        let archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        if archive.len() > limits.zip.max_entries {
            return Err(OpcError::Limit("entries"));
        }
        let mut index = BTreeMap::new();
        let mut warnings = Vec::new();
        for position in 0..archive.len() {
            let name = match archive.name_for_index(position) {
                Some(Ok(name)) => name.into_owned(),
                _ => {
                    warnings.push(format!("entry {position}: unreadable name skipped"));
                    continue;
                }
            };
            if name.ends_with('/') {
                continue;
            }
            match archive.by_index_data(position) {
                Ok(entry) => {
                    index.insert(name, LazyEntry { index: position, size: entry.size(), compressed: entry.compressed_size() });
                }
                Err(error) => warnings.push(format!("{name}: unreadable entry skipped: {error}")),
            }
        }
        let mut package = Self { archive, index, limits: *limits, total_read: 0, content_types: ContentTypes::default(), warnings };
        match package.read_optional("[Content_Types].xml", package.limits.xml.max_total_bytes as u64) {
            Ok(Some(bytes)) => match parse_content_types(&bytes, &package.limits.xml) {
                Ok(content_types) => package.content_types = content_types,
                Err(error) => package.warnings.push(format!("[Content_Types].xml ignored: {error}")),
            },
            Ok(None) => {}
            Err(error) => package.warnings.push(format!("[Content_Types].xml ignored: {error}")),
        }
        Ok(package)
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Stored entry names (directories excluded), in sorted order.
    pub fn part_names(&self) -> impl Iterator<Item = &str> {
        self.index.keys().map(String::as_str)
    }

    /// Uncompressed size claimed by the ZIP index (untrusted; reads are still bounded).
    pub fn declared_size(&self, name: &str) -> Option<u64> {
        self.index.get(name).map(|entry| entry.size)
    }

    /// Bytes decoded so far across all reads, including partial reads that failed.
    pub fn total_read(&self) -> u64 {
        self.total_read
    }

    pub fn read(&mut self, name: &str, cap: u64) -> Result<Vec<u8>, OpcError> {
        self.read_optional(name, cap)?.ok_or_else(|| OpcError::Invalid(format!("missing part: {name}")))
    }

    pub fn read_optional(&mut self, name: &str, cap: u64) -> Result<Option<Vec<u8>>, OpcError> {
        let Some(entry) = self.index.get(name).copied() else {
            return Ok(None);
        };
        let cap = cap.min(self.limits.zip.max_entry_bytes);
        if entry.size > cap {
            return Err(OpcError::Limit("part bytes"));
        }
        let mut file = self.archive.by_index(entry.index)?;
        let mut output = Vec::new();
        let mut buffer = [0; 8192];
        loop {
            let remaining_entry = cap - output.len() as u64;
            let remaining_total = self.limits.zip.max_total_bytes - self.total_read;
            let chunk = buffer.len().min(usize::try_from(remaining_entry.min(remaining_total).saturating_add(1)).unwrap_or(usize::MAX));
            let count = file.read(&mut buffer[..chunk])?;
            if count == 0 {
                return Ok(Some(output));
            }
            if count as u64 > remaining_entry {
                return Err(OpcError::Limit("part bytes"));
            }
            if count as u64 > remaining_total {
                return Err(OpcError::Limit("package bytes"));
            }
            self.total_read += count as u64;
            output.try_reserve_exact(count).map_err(|_| OpcError::Limit("part bytes"))?;
            output.extend_from_slice(&buffer[..count]);
            let produced = output.len() as u64;
            if produced > LAZY_RATIO_FLOOR && produced > entry.compressed.max(1).saturating_mul(LAZY_MAX_RATIO) {
                return Err(OpcError::Limit("expansion ratio"));
            }
        }
    }
}

fn parse_content_types(bytes: &[u8], xml: &Limits) -> Result<ContentTypes, OpcError> {
    let ct = dom::parse(bytes, xml)?;
    if ct.name != "Types" {
        return Err(OpcError::Invalid("content types root".into()));
    }
    let mut content_types = ContentTypes::default();
    for node in ct.children("Default") {
        if let (Some(ext), Some(kind)) = (node.attr("Extension"), node.attr("ContentType")) {
            content_types.defaults.insert(ext.to_ascii_lowercase(), kind.to_string());
        }
    }
    for node in ct.children("Override") {
        if let (Some(name), Some(kind)) = (node.attr("PartName"), node.attr("ContentType")) {
            content_types.overrides.insert(checked_part(name)?.to_ascii_lowercase(), kind.to_string());
        }
    }
    Ok(content_types)
}

fn checked_part(name: &str) -> Result<&str, OpcError> {
    let name = name.strip_prefix('/').unwrap_or(name);
    if name.is_empty()
        || name.contains(['\\', '?', ':'])
        || name.chars().any(char::is_control)
        || name.split('/').any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(OpcError::Invalid(format!("unsafe part name: {name}")));
    }
    Ok(name)
}

fn rels_path(source: &str) -> String {
    if source.is_empty() {
        return "_rels/.rels".to_string();
    }
    match source.rsplit_once('/') {
        Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
        None => format!("_rels/{source}.rels"),
    }
}

fn source_of_rels(path: &str) -> Option<String> {
    if path == "_rels/.rels" {
        return Some(String::new());
    }
    let (dir, file) = path.rsplit_once("/_rels/").or_else(|| path.strip_prefix("_rels/").map(|file| ("", file)))?;
    let file = file.strip_suffix(".rels")?;
    Some(if dir.is_empty() { file.into() } else { format!("{dir}/{file}") })
}

fn percent_decode(value: &str) -> Result<String, OpcError> {
    let mut output = Vec::with_capacity(value.len());
    let mut bytes = value.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let high = bytes.next().and_then(|ch| (ch as char).to_digit(16));
            let low = bytes.next().and_then(|ch| (ch as char).to_digit(16));
            let (Some(high), Some(low)) = (high, low) else {
                return Err(OpcError::Invalid("invalid target escape".into()));
            };
            output.push(((high << 4) | low) as u8);
        } else {
            output.push(byte);
        }
    }
    String::from_utf8(output).map_err(|e| OpcError::Invalid(e.to_string()))
}

/// Resolve an internal relationship URI against the source part (not the `.rels` sidecar).
pub fn resolve_target(source: &str, target: &str) -> Result<String, OpcError> {
    let reference = target.split('#').next().unwrap_or("");
    if reference.is_empty() && target.starts_with('#') && !source.is_empty() {
        return Ok(source.trim_start_matches('/').to_string());
    }
    if reference.starts_with("//") {
        return Err(OpcError::Invalid("network-path target".into()));
    }
    let target = percent_decode(reference)?;
    if target.contains(['\\', '?']) {
        return Err(OpcError::Invalid("invalid internal target".into()));
    }
    let base = source.rsplit_once('/').map_or("", |(dir, _)| dir);
    let mut segments: Vec<&str> = if target.starts_with('/') || base.is_empty() { Vec::new() } else { base.split('/').collect() };
    for segment in target.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    return Err(OpcError::Invalid("target escapes package".into()));
                }
            }
            other => segments.push(other),
        }
    }
    let result = segments.join("/");
    checked_part(&result)?;
    Ok(result)
}

#[derive(Default)]
pub struct PackageWriter {
    parts: BTreeMap<String, (String, String, Vec<u8>)>,
    insertion: Vec<String>,
    rels: BTreeMap<String, Relationships>,
    /// Relationship sidecars supplied verbatim (kept parts round-trip byte-for-byte).
    raw_rels: BTreeMap<String, Vec<u8>>,
    policy: WritePolicy,
}

impl PackageWriter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_policy(policy: WritePolicy) -> Self {
        Self { policy, ..Self::default() }
    }

    pub fn add_part(&mut self, name: &str, content_type: &str, bytes: impl Into<Vec<u8>>) -> Result<(), OpcError> {
        let name = checked_part(name)?;
        let key = name.to_ascii_lowercase();
        if key == "[content_types].xml" || source_of_rels(&key).is_some() || self.parts.contains_key(&key) {
            return Err(OpcError::Invalid("reserved or duplicate part".into()));
        }
        if content_type.is_empty() {
            return Err(OpcError::Invalid("missing content type".into()));
        }
        self.insertion.push(key.clone());
        self.parts.insert(key, (name.into(), content_type.into(), bytes.into()));
        Ok(())
    }

    /// Source is a part path, or `""` for package-level relationships. Targets are URI references.
    pub fn add_relationship(&mut self, source: &str, id: &str, kind: &str, target: &str, mode: TargetMode) -> Result<(), OpcError> {
        let source = if source.is_empty() { "" } else { checked_part(source)? };
        if id.is_empty() || kind.is_empty() || target.is_empty() {
            return Err(OpcError::Invalid("incomplete relationship".into()));
        }
        if mode == TargetMode::Internal {
            resolve_target(source, target)?;
        }
        let key = source.to_ascii_lowercase();
        if self.raw_rels.contains_key(&key) {
            return Err(OpcError::Invalid("relationships for this source were supplied verbatim".into()));
        }
        let rels = self.rels.entry(key).or_default();
        if rels.by_id(id).is_some() {
            return Err(OpcError::Invalid("duplicate relationship id".into()));
        }
        rels.0.push(Relationship { id: id.into(), kind: kind.into(), target: target.into(), mode });
        Ok(())
    }

    /// Emit a part's `.rels` sidecar exactly as given (for parts carried over from an input
    /// package). The bytes must parse as a `Relationships` document whose internal targets stay
    /// inside the package; they are not reformatted. Mutually exclusive with
    /// [`add_relationship`](Self::add_relationship) for the same source.
    pub fn add_relationships_bytes(&mut self, source: &str, bytes: impl Into<Vec<u8>>) -> Result<(), OpcError> {
        let source = if source.is_empty() { "" } else { checked_part(source)? };
        let bytes = bytes.into();
        let root = dom::parse(&bytes, &Limits::default())?;
        if root.name != "Relationships" {
            return Err(OpcError::Invalid("relationships root".into()));
        }
        for node in root.children("Relationship") {
            let (Some(_), Some(_), Some(raw)) = (node.attr("Id"), node.attr("Type"), node.attr("Target")) else {
                return Err(OpcError::Invalid("incomplete relationship".into()));
            };
            if !node.attr("TargetMode").is_some_and(|value| value.eq_ignore_ascii_case("external")) {
                resolve_target(source, raw)?;
            }
        }
        let key = source.to_ascii_lowercase();
        if self.rels.contains_key(&key) || self.raw_rels.contains_key(&key) {
            return Err(OpcError::Invalid("relationships for this source already added".into()));
        }
        self.raw_rels.insert(key, bytes);
        Ok(())
    }

    fn content_types_bytes(&self) -> Vec<u8> {
        let mut defaults = BTreeMap::from([("rels".to_string(), REL_TYPE.to_string())]);
        let mut overrides = BTreeMap::new();
        if let ContentTypesPolicy::EmitWithDefaults(extra) = &self.policy.content_types {
            for (ext, kind) in extra {
                defaults.insert(ext.to_ascii_lowercase(), kind.clone());
            }
        }
        for (name, (_, kind, _)) in &self.parts {
            let ext = name.rsplit_once('.').map(|(_, ext)| ext);
            if let Some(ext) = ext {
                if let Some(prior) = defaults.get(ext) {
                    if prior != kind || (ext == "xml" && matches!(self.policy.content_types, ContentTypesPolicy::EmitWithDefaults(_))) {
                        overrides.insert(name.clone(), kind.clone());
                    }
                } else {
                    defaults.insert(ext.into(), kind.clone());
                }
            } else {
                overrides.insert(name.clone(), kind.clone());
            }
        }
        if let ContentTypesPolicy::Preserve(bytes) = &self.policy.content_types {
            bytes.clone()
        } else {
            let mut ct = XmlWriter::new();
            ct.declaration().start("Types").attr("xmlns", CT_NS);
            let mut ordered_defaults = Vec::new();
            let mut ordered_overrides = Vec::new();
            if let ContentTypesPolicy::EmitWithDefaults(extra) = &self.policy.content_types {
                ordered_defaults.push("rels".to_string());
                ordered_defaults.extend(extra.iter().map(|(ext, _)| ext.to_ascii_lowercase()));
                for name in &self.insertion {
                    if let Some(ext) = name.rsplit_once('.').map(|(_, ext)| ext)
                        && !ordered_defaults.iter().any(|prior| prior == ext)
                    {
                        ordered_defaults.push(ext.to_string());
                    }
                }
                ordered_overrides.extend(self.insertion.iter().filter(|name| overrides.contains_key(*name)).cloned());
            } else {
                ordered_defaults.extend(defaults.keys().cloned());
                ordered_overrides.extend(overrides.keys().cloned());
            }
            for ext in ordered_defaults {
                if let Some(kind) = defaults.get(&ext) {
                    ct.start("Default").attr("Extension", &ext).attr("ContentType", kind).empty();
                }
            }
            for name in ordered_overrides {
                if let Some(kind) = overrides.get(&name) {
                    let part_name = if matches!(self.policy.part_order, PartOrder::Sorted) {
                        name.as_str()
                    } else {
                        self.parts.get(&name).map_or(name.as_str(), |(original, _, _)| original.as_str())
                    };
                    ct.start("Override").attr("PartName", &format!("/{part_name}")).attr("ContentType", kind).empty();
                }
            }
            ct.end();
            ct.finish()
        }
    }

    fn relationship_files(&self) -> Result<Vec<(String, Vec<u8>)>, OpcError> {
        let mut files = Vec::new();
        for (source, bytes) in &self.raw_rels {
            if !source.is_empty() && !self.parts.contains_key(source) {
                return Err(OpcError::Invalid(format!("missing relationship source: {source}")));
            }
            let original = if source.is_empty() { "" } else { self.parts[source].0.as_str() };
            files.push((rels_path(original), bytes.clone()));
        }
        for (source, rels) in &self.rels {
            if !source.is_empty() && !self.parts.contains_key(source) {
                return Err(OpcError::Invalid(format!("missing relationship source: {source}")));
            }
            let mut rels: Vec<_> = rels.iter().collect();
            rels.sort_by(|left, right| left.id.cmp(&right.id));
            let mut xml = XmlWriter::new();
            xml.declaration().start("Relationships").attr("xmlns", REL_NS);
            for rel in rels {
                xml.start("Relationship").attr("Id", &rel.id).attr("Type", &rel.kind).attr("Target", &rel.target);
                if rel.mode == TargetMode::External {
                    xml.attr("TargetMode", "External");
                }
                xml.empty();
            }
            xml.end();
            // `self.rels` is keyed case-insensitively; the sidecar is named after the part as
            // it was added, so `chartEx1.xml` gets `_rels/chartEx1.xml.rels`.
            let original = if source.is_empty() { "" } else { self.parts[source].0.as_str() };
            let name = rels_path(original);
            files.push((name, xml.finish()));
        }
        Ok(files)
    }

    fn entry_names(&self, relation_files: &[(String, Vec<u8>)]) -> Vec<String> {
        let mut names = vec!["[Content_Types].xml".to_string()];
        let mut parts = self.insertion.clone();
        match &self.policy.part_order {
            PartOrder::Sorted => parts.sort(),
            PartOrder::Explicit(patterns) => {
                parts.sort_by_key(|name| patterns.iter().position(|pattern| glob_matches(pattern, name)).unwrap_or(patterns.len()))
            }
            PartOrder::Ranked(rank) => parts.sort_by_key(|name| rank(name)),
            PartOrder::Insertion => {}
        }
        let relation_names = relation_files.iter().map(|(name, _)| name.clone());
        if self.policy.rels_placement == RelsPlacement::BeforeParts {
            names.extend(relation_names);
            names.extend(parts);
        } else {
            names.extend(parts);
            names.extend(relation_names);
        }
        if let PartOrder::Ranked(rank) = &self.policy.part_order {
            names[1..].sort_by_key(|name| rank(name));
        } else if matches!(self.policy.part_order, PartOrder::Sorted) && self.policy.rels_placement == RelsPlacement::Sorted {
            names.sort();
        }
        names
    }

    pub fn finish(self) -> Result<Vec<u8>, OpcError> {
        let content_types = self.content_types_bytes();
        let relation_files = self.relationship_files()?;
        let names = self.entry_names(&relation_files);
        let original_names: BTreeMap<_, _> = self.parts.iter().map(|(name, (original, _, _))| (name.clone(), original.clone())).collect();
        let mut files = BTreeMap::from([("[Content_Types].xml".to_string(), content_types)]);
        for (name, (_, _, data)) in self.parts {
            files.insert(name, data);
        }
        files.extend(relation_files);
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for name in names {
            let data = files.remove(&name).ok_or_else(|| OpcError::Invalid(format!("missing part: {name}")))?;
            // Ordering uses the canonical (lowercase) key; the entry is always written under the
            // name the part was added with (`chartEx1.xml`, `customXml/…`).
            let output_name = original_names.get(&name).map_or(name.as_str(), String::as_str);
            let method = self.policy.method.for_name(output_name);
            let (compression, level) = match method {
                Method::Store => (zip::CompressionMethod::Stored, None),
                Method::Deflate(level) => (zip::CompressionMethod::Deflated, level),
            };
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(compression)
                .compression_level(level)
                .last_modified_time(self.policy.timestamps)
                .large_file(data.len() as u64 >= u32::MAX as u64);
            writer.start_file(output_name, options)?;
            writer.write_all(&data)?;
        }
        Ok(writer.finish()?.into_inner())
    }
}

fn glob_matches(pattern: &str, name: &str) -> bool {
    let (pattern, name) = (pattern.as_bytes(), name.as_bytes());
    let (mut pattern_at, mut name_at, mut star, mut retry_at) = (0, 0, None, 0);
    while name_at < name.len() {
        if pattern.get(pattern_at).is_some_and(|ch| *ch == b'?' || *ch == name[name_at]) {
            pattern_at += 1;
            name_at += 1;
        } else if pattern.get(pattern_at) == Some(&b'*') {
            star = Some(pattern_at);
            pattern_at += 1;
            retry_at = name_at;
        } else if let Some(position) = star {
            retry_at += 1;
            name_at = retry_at;
            pattern_at = position + 1;
        } else {
            return false;
        }
    }
    pattern[pattern_at..].iter().all(|ch| *ch == b'*')
}
