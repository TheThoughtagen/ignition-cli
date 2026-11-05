//! Production-scale gate for deploy transport changes (quick 261104-s3n).
//!
//! `tests/common/prod_shape.rs` generates the shape of a production pair's
//! core tag folders (14,119 + 156 files, 7,500 + 85 folders, depth 14, fanout
//! 89, 7 paths of 260 characters or more). That fixture and the tests on it
//! are the gate for any deploy transport change:
//!
//! - the offline connection-count tests in `src/deploy/transport/sftp.rs`
//!   (`windows_prod_shape_*`): a Windows read of this tree opens a fixed
//!   number of ssh connections, the same as for a tiny tree;
//! - this file: the generator holds the recorded numbers, and a full read of
//!   the materialized tree finishes inside a measured wall-clock budget;
//! - the live `wflwvm01_prod_shape_*` gate in `tests/live_transport_wflwvm01.rs`.

mod common;

use common::prod_shape::{self, Bytes, Entry, PROVIDER};
use ignition_core::deploy::transport::remote::safe_remote_path;
use ignition_core::deploy::transport::winps::windows_component_problem;

fn files(entries: &[Entry]) -> usize {
    entries.iter().filter(|e| !e.dir).count()
}

fn dirs(entries: &[Entry]) -> usize {
    entries.iter().filter(|e| e.dir).count()
}

fn bytes(entries: &[Entry]) -> u64 {
    entries.iter().map(|e| e.size).sum()
}

fn within_2_percent(got: u64, want: u64) -> bool {
    got.abs_diff(want) * 50 <= want
}

/// Every component passes both name gates, the first is the provider, and
/// no two siblings are equal ignoring case.
fn assert_names(entries: &[Entry]) {
    let mut folded = std::collections::BTreeSet::new();
    for entry in entries {
        assert_eq!(entry.rel.split('/').next(), Some(PROVIDER), "{}", entry.rel);
        for component in entry.rel.split('/') {
            assert!(component.len() <= 255, "{component}");
            assert_eq!(windows_component_problem(component), None, "{}", entry.rel);
            safe_remote_path(component).expect(&entry.rel);
        }
        assert!(
            folded.insert(entry.rel.to_lowercase()),
            "{} is listed twice ignoring case",
            entry.rel
        );
    }
    // sorted by path bytes, so a parent comes before its children
    assert!(
        entries
            .windows(2)
            .all(|w| w[0].rel.as_bytes() < w[1].rel.as_bytes())
    );
}

#[test]
fn prod_shape_matches_the_recorded_production_numbers() {
    let td = prod_shape::tag_definition(Bytes::Small);
    assert_eq!(files(&td), prod_shape::TAG_DEFINITION_FILES);
    assert_eq!(dirs(&td), prod_shape::TAG_DEFINITION_DIRS);
    assert_eq!(prod_shape::depth(&td), prod_shape::TAG_DEFINITION_DEPTH);
    let fan = prod_shape::fanouts(&td);
    let widest = fan.values().copied().max().unwrap_or(0);
    assert_eq!(widest, prod_shape::TAG_DEFINITION_FANOUT);
    assert_eq!(
        fan.values().filter(|n| **n == widest).count(),
        1,
        "one folder with the widest fanout"
    );
    let lengths: Vec<usize> = td
        .iter()
        .map(|e| prod_shape::windows_full_len("core", "tag-definition", &e.rel))
        .collect();
    let long = lengths.iter().filter(|n| **n >= 260).count();
    assert!(long >= prod_shape::TAG_DEFINITION_LONG, "{long}");
    assert_eq!(
        lengths.iter().copied().max(),
        Some(prod_shape::TAG_DEFINITION_LONGEST)
    );
    assert_names(&td);

    let tt = prod_shape::tag_type_definition(Bytes::Small);
    assert_eq!(files(&tt), prod_shape::TAG_TYPE_FILES);
    assert_eq!(dirs(&tt), prod_shape::TAG_TYPE_DIRS);
    assert_eq!(prod_shape::depth(&tt), prod_shape::TAG_TYPE_DEPTH);
    assert_eq!(
        prod_shape::fanouts(&tt).values().copied().max(),
        Some(prod_shape::TAG_TYPE_FANOUT)
    );
    assert!(
        tt.iter()
            .all(|e| { prod_shape::windows_full_len("core", "tag-type-definition", &e.rel) < 260 })
    );
    assert_names(&tt);

    // production bytes land within 2% of the recorded totals
    let td_prod = prod_shape::tag_definition(Bytes::Prod);
    let tt_prod = prod_shape::tag_type_definition(Bytes::Prod);
    assert!(
        within_2_percent(bytes(&td_prod), prod_shape::TAG_DEFINITION_BYTES),
        "{}",
        bytes(&td_prod)
    );
    assert!(
        within_2_percent(bytes(&tt_prod), prod_shape::TAG_TYPE_BYTES),
        "{}",
        bytes(&tt_prod)
    );
    // the same paths whatever the byte profile
    let paths = |e: &[Entry]| e.iter().map(|e| (e.rel.clone(), e.dir)).collect::<Vec<_>>();
    assert_eq!(paths(&td), paths(&td_prod));
    assert!(td.iter().all(|e| e.dir || (16..=64).contains(&e.size)));

    // deterministic
    assert_eq!(td, prod_shape::tag_definition(Bytes::Small));
    assert_eq!(tt_prod, prod_shape::tag_type_definition(Bytes::Prod));

    // content is exactly the size, valid JSON, and a function of the path
    for entry in td_prod.iter().chain(&td).filter(|e| !e.dir).step_by(97) {
        let body = prod_shape::content(&entry.rel, entry.size);
        assert_eq!(body.len() as u64, entry.size, "{}", entry.rel);
        serde_json::from_slice::<serde_json::Value>(&body).expect(&entry.rel);
        assert_eq!(body, prod_shape::content(&entry.rel, entry.size));
    }

    let tiny = prod_shape::tiny_tag_definition();
    assert!(tiny.len() <= 20 && prod_shape::depth(&tiny) <= 3);
    assert_names(&tiny);
}
