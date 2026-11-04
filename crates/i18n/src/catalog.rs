//! Reading one catalog. The rules are craft-i18n's `docs/catalog-format.md`; craft-i18n's CI
//! applies them strictly, and here a row that breaks one is skipped and reported, so an app built
//! against a newer catalog still runs, with that string in English.

use std::collections::HashSet;

/// How a catalog file's rows are laid out and checked.
///
/// Most apps use three-column `context\tsource\ttranslation` TSVs ([`CatalogDialect::Standard`],
/// the default of [`crate::I18n::new`]), which accepts craft-i18n's `one|other` `@plural` keys
/// alongside single-form `one` `@plural` keys (CADCraft), `@msgplural` rows (VectorCraft),
/// `1`- or `2`-form plural translations alongside the locale's full CLDR form count, and literal
/// `\x` pass-through for non-`\n`/`\t`/`\\` sequences.
///
/// Two-column `source\ttranslation` catalogs without a `context` column (Craft Anything) are
/// ambiguous with a three-column row missing a tab, so they select [`CatalogDialect::TwoColumn`]
/// via [`crate::I18n::with_dialect`]. [`CatalogDialect::Strict`] enforces craft-i18n's repository
/// checker rules without the app-catalog extensions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CatalogDialect {
    /// Three-column `context\tsource\ttranslation` TSVs, accepting both craft-i18n's format and
    /// the three-column app variants (`one` plural keys, `@msgplural`, `1`/`2`/CLDR plural form
    /// counts, and literal unknown backslash sequences).
    #[default]
    Standard,
    /// Two-column `source\ttranslation` TSVs without a `context` column (also accepts three-column
    /// rows when present).
    TwoColumn,
    /// Strict craft-i18n repository rules: three columns, `one|other` `@plural` source, exact CLDR
    /// plural form count, only `@plural`/`@id`/`@msg` reserved contexts, and only `\n`/`\t`/`\\`
    /// escapes.
    Strict,
}

/// A row that passed, with its columns unescaped.
pub(crate) struct Entry {
    pub context: String,
    pub source: String,
    pub translation: String,
}

/// The contexts with a meaning here. Any other context starting with `@` is skipped.
const RESERVED: &[&str] = &["@plural", "@id", "@msg"];

/// The rows of a catalog whose locale has `forms` plural forms, and one problem (`line N: …`) per
/// row skipped. Of a row defined twice, the first is kept, as in craft-i18n's checker.
pub(crate) fn parse(text: &str, forms: usize, dialect: CatalogDialect) -> (Vec<Entry>, Vec<String>) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows = Vec::new();
    let mut problems = Vec::new();
    let mut seen = HashSet::new();
    for (i, line) in text.split('\n').enumerate() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match parse_row(line, forms, dialect) {
            Ok(row) if !seen.insert((row.context.clone(), row.source.clone())) => {
                problems.push(format!("line {}: defined twice (same context and source text); the first is kept", i + 1));
            }
            Ok(row) => rows.push(row),
            Err(e) => problems.push(format!("line {}: {e}", i + 1)),
        }
    }
    (rows, problems)
}

pub(crate) fn parse_row(text: &str, forms: usize, dialect: CatalogDialect) -> Result<Entry, String> {
    let columns: Vec<&str> = text.split('\t').collect();
    let (context, source, translation) = match (dialect, columns.as_slice()) {
        (_, [context, source, translation]) => (*context, *source, *translation),
        (CatalogDialect::TwoColumn, [source, translation]) => ("", *source, *translation),
        (CatalogDialect::TwoColumn, _) => {
            return Err(format!("{} columns; expected 2 or 3", columns.len()));
        }
        (_, _) => {
            return Err(format!("{} columns; expected 3", columns.len()));
        }
    };
    if text.chars().any(|c| c != '\t' && (c.is_control() || c == '\u{feff}')) {
        return Err("invisible character in the row".into());
    }
    let row = Entry { context: unescape(context, dialect)?, source: unescape(source, dialect)?, translation: unescape(translation, dialect)? };
    if row.source.is_empty() || row.translation.is_empty() {
        return Err("empty source text or translation".into());
    }
    match row.context.as_str() {
        "@id" => {}
        "@plural" => check_plural(&row.source, &row.translation, forms, dialect)?,
        "@msgplural" if dialect != CatalogDialect::Strict => {
            check_plural(&row.source, &row.translation, forms, dialect)?;
        }
        c if c.starts_with('@') && !RESERVED.contains(&c) => {
            return Err(format!("unknown reserved context {c:?}"));
        }
        _ => {
            if placeholders(&row.source) != placeholders(&row.translation) {
                return Err("placeholders differ from the source text".into());
            }
            if row.source.ends_with('…') != row.translation.ends_with('…') {
                return Err("trailing … differs from the source text".into());
            }
        }
    }
    Ok(row)
}

fn check_plural(source: &str, translation: &str, forms: usize, dialect: CatalogDialect) -> Result<(), String> {
    let english: Vec<&str> = source.split('|').collect();
    let other = match (dialect, english.as_slice()) {
        (_, [one, other]) => {
            if one.is_empty() || other.is_empty() || placeholders(one) != placeholders(other) {
                return Err("plural source forms must be nonempty with the same placeholders".into());
            }
            *other
        }
        (CatalogDialect::Standard | CatalogDialect::TwoColumn, [one]) => {
            if one.is_empty() {
                return Err("plural source forms must be nonempty with the same placeholders".into());
            }
            *one
        }
        _ => return Err("plural source text must be one|other".into()),
    };
    let translated: Vec<&str> = translation.split('|').collect();
    let count_ok = match dialect {
        CatalogDialect::Strict => translated.len() == forms,
        CatalogDialect::Standard | CatalogDialect::TwoColumn => translated.len() == forms || translated.len() == 1 || translated.len() == 2,
    };
    if !count_ok || translated.iter().any(|f| f.is_empty()) {
        return Err(format!("expected {forms} nonempty plural forms"));
    }
    if translated.iter().any(|f| placeholders(f) != placeholders(other)) {
        return Err("plural form placeholders differ from the source text".into());
    }
    Ok(())
}

fn unescape(text: &str, dialect: CatalogDialect) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some(other) if dialect != CatalogDialect::Strict => {
                out.push('\\');
                out.push(other);
            }
            Some(other) => return Err(format!("unknown escape \\{other}")),
            None => return Err("trailing backslash".into()),
        }
    }
    Ok(out)
}

/// `{name}` placeholders, sorted: translations may reorder them.
fn placeholders(text: &str) -> Vec<&str> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some((_, after)) = rest.split_once('{') {
        let Some((name, after)) = after.split_once('}') else {
            break;
        };
        names.push(name);
        rest = after;
    }
    names.sort_unstable();
    names
}
