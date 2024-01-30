use super::Attributes;
use crate::builder::{CrateName, Edition};

// FIXME: check that we successfully detect the name even if leading inner attrs
// have the form `#![path::to::attr = GARBAGE_TT]` (overapprox, should be expr),
// `#![path::to::attr ( GARBAGE_TT )]`

fn parse(source: &str) -> Attributes<'_> {
    Attributes::parse(source, &[], Edition::Edition2015)
}

#[test]
fn crate_name() {
    assert_eq!(
        parse(r#"#![crate_name = "name"]"#),
        Attributes {
            crate_name: Some(CrateName::new("name")),
            crate_type: None
        }
    );
}

#[test]
fn crate_name_spaced() {
    assert_eq!(
        parse(r#" # ! [ crate_name = "name" ] "#),
        Attributes {
            crate_name: Some(CrateName::new("name")),
            crate_type: None,
        }
    );
}

#[test]
fn crate_name_leading_inner_attributes() {
    assert_eq!(
        parse(
            r#"
//! Module-level documentation.
#![feature(rustc_attrs)]
#![cfg_attr(not(FALSE), doc = "\n")]
#![crate_name = "name"]
"#,
        ),
        Attributes {
            crate_name: Some(CrateName::new("name")),
            crate_type: None
        }
    );
}

#[test]
fn crate_name_not_at_beginning_leading_item() {
    assert_eq!(
        parse(
            r#"
fn main() {}
#![crate_name = "name"]
    "#
        ),
        Attributes::default()
    );
}

#[test]
fn crate_name_not_at_beginning_leading_outer_attribute() {
    // FIXME
    assert_eq!(
        parse(
            r#"
#[allow(unused)] // <-- notice the lack of `!` here
#![crate_type = "name"]
"#
        ),
        Attributes::default()
    );
}
