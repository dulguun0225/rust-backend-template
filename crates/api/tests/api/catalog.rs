//! The error catalog: one snapshot of every wire code, its status, where it is declared and, for a field code,
//! the params it declares; one wire code maps to one status and one param list across every catalog; every
//! param name is one lower-case word, since it is the wire name; and the catalogs listed here are every catalog
//! the source declares, so a new `wire_errors!` or `field_codes!` enum fails this test until it is listed. A
//! field code raised without its declared params does not compile: each is a field of its variant.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use api::greeting::{GreetingErrorCode, GreetingFieldCode};
use platform::catalog::{Catalog, CatalogRow};
use web::codes::{ApiErrorCode, ApiFieldCode};

const SNAPSHOT: &str = "snapshots/error-catalog.txt";

fn listed() -> Vec<(&'static str, Vec<CatalogRow>)> {
    vec![
        (ApiErrorCode::NAME, ApiErrorCode::rows()),
        (ApiFieldCode::NAME, ApiFieldCode::rows()),
        (GreetingErrorCode::NAME, GreetingErrorCode::rows()),
        (GreetingFieldCode::NAME, GreetingFieldCode::rows()),
    ]
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn snapshot_text() -> String {
    let mut rows: Vec<CatalogRow> = listed().into_iter().flat_map(|(_, rows)| rows).collect();
    rows.sort();
    let mut text: String = rows.iter().map(|r| format!("{}\n", r.line())).collect();
    text.insert_str(
        0,
        "# wire code -> status (declaring enum); a field code: `-` for the status, then {param: JSON type, ...}. Written by crates/api/tests/api/catalog.rs.\n",
    );
    text
}

#[tokio::test]
async fn the_catalog_matches_its_committed_snapshot() {
    let expected = snapshot_text();
    let committed = tokio::fs::read_to_string(root().join(SNAPSHOT)).await.unwrap_or_default();
    if committed != expected {
        let actual = root().join("target").join(SNAPSHOT);
        tokio::fs::create_dir_all(actual.parent().unwrap()).await.unwrap();
        tokio::fs::write(&actual, &expected).await.unwrap();
        panic!("the error catalog changed: review {} and copy it to {SNAPSHOT}", actual.display());
    }
}

#[test]
fn one_wire_code_maps_to_one_status() {
    let mut statuses: BTreeMap<&str, BTreeSet<u16>> = BTreeMap::new();
    for (_, rows) in listed() {
        for row in rows {
            if let Some(status) = row.status {
                statuses.entry(row.wire).or_default().insert(status);
            }
        }
    }
    let conflicting: Vec<_> = statuses.iter().filter(|(_, s)| s.len() > 1).collect();
    assert!(conflicting.is_empty(), "{conflicting:?}");
}

#[test]
fn one_wire_code_declares_one_param_list() {
    let mut lists: BTreeMap<&str, BTreeSet<Vec<(&str, &str)>>> = BTreeMap::new();
    for (_, rows) in listed() {
        for row in rows {
            if let Some(params) = row.params {
                lists.entry(row.wire).or_default().insert(params.to_vec());
            }
        }
    }
    let conflicting: Vec<_> = lists.iter().filter(|(_, l)| l.len() > 1).collect();
    assert!(conflicting.is_empty(), "{conflicting:?}");
}

#[test]
fn every_param_name_is_one_lower_case_word() {
    let names: Vec<&str> = listed()
        .into_iter()
        .flat_map(|(_, rows)| rows)
        .flat_map(|row| row.params.unwrap_or_default().iter().map(|(name, _)| *name))
        .collect();
    assert!(!names.is_empty(), "no code declares a param: this check would pass vacuously");
    let bad: Vec<&str> = names.into_iter().filter(|n| !is_one_lower_case_word(n)).collect();
    assert!(bad.is_empty(), "a param name is its wire name, so it is one lower-case word: {bad:?}");
}

fn is_one_lower_case_word(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

#[test]
fn the_param_name_check_refuses_a_word_break() {
    assert!(is_one_lower_case_word("max"));
    assert!(is_one_lower_case_word("max2"));
    for bad in ["max_chars", "maxChars", "2max", ""] {
        assert!(!is_one_lower_case_word(bad), "{bad}");
    }
}

#[tokio::test]
async fn every_catalog_the_source_declares_is_listed() {
    let mut declared = BTreeSet::new();
    let mut pending = vec![root().join("crates")];
    while let Some(dir) = pending.pop() {
        let mut entries = tokio::fs::read_dir(&dir).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type().await.unwrap().is_dir() {
                // platform declares the macros and only test catalogs; tests declare none that ship.
                if name != "target" && name != "tests" && !path.ends_with("crates/platform") {
                    pending.push(path);
                }
            } else if name.ends_with(".rs") {
                declared.extend(catalog_names(&tokio::fs::read_to_string(&path).await.unwrap()));
            }
        }
    }
    let listed: BTreeSet<String> = listed().into_iter().map(|(name, _)| name.to_owned()).collect();
    assert_eq!(declared, listed, "a catalog enum is declared but not listed here, or listed but gone");
}

/// The enum names declared by `wire_errors!` and `field_codes!` invocations in one file.
fn catalog_names(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    for marker in ["wire_errors!", "field_codes!"] {
        for (at, _) in source.match_indices(marker) {
            if let Some((_, after)) = source[at..].split_once("enum ") {
                names.push(after.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect());
            }
        }
    }
    names
}

#[test]
fn the_declaration_scan_finds_a_catalog() {
    let fixture = "platform::wire_errors! {\n    /// doc\n    pub enum FixtureErrorCode {\n        Gone = (\"gone\", 410),\n    }\n}\n";
    assert_eq!(catalog_names(fixture), ["FixtureErrorCode"]);
}
