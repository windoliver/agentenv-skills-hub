use hub_search::{lexical_rank, SearchDocument};

fn doc(name: &str, description: &str) -> SearchDocument {
    SearchDocument {
        namespace: "core".to_owned(),
        name: name.to_owned(),
        version: "1.0.0".to_owned(),
        description: Some(description.to_owned()),
    }
}

#[test]
fn lexical_search_prefers_exact_and_prefix_matches() {
    let docs = vec![
        doc("code-review-helper", "Review pull requests"),
        doc("release-notes", "Generate release notes from commits"),
        doc("code-review", "Review code changes"),
    ];

    let ranked = lexical_rank("code-review", docs);

    assert_eq!(ranked[0].name, "code-review");
}

#[test]
fn lexical_search_matches_description() {
    let docs = vec![
        doc("policy-writer", "Draft repository policy"),
        doc("cargo-lints", "Analyze rust code quality"),
        doc("release-notes", "Generate changelog entries"),
    ];

    let ranked = lexical_rank("rust", docs);

    assert_eq!(ranked[0].name, "cargo-lints");
}
