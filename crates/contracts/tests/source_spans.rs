//! Source map and span invariant tests.

use std::sync::Arc;

use rulery_contracts::{
    LoadedSourceBundle, SourceBundle, SourceDocument, SourceFile, SourceId, SourceIntegrity,
    SourceKey, SourceMap, SourcePath,
};

#[test]
fn spans_are_half_open_utf8_intervals() {
    let key = SourceKey::new(7);
    let file = SourceFile::new(
        SourceId::new("rules").expect("source ID"),
        SourcePath::new("rules/checkout.yaml").expect("source path"),
        Arc::<str>::from("a\u{e9}z"),
    );
    let mut sources = SourceMap::new();
    sources.insert(key, file).expect("insert source");

    let span = sources.span(key, 1, 3).expect("UTF-8 span");
    assert_eq!(span.start(), 1);
    assert_eq!(span.end(), 3);
    assert_eq!(sources.line_column(key, 3).expect("position"), (1, 3));

    assert!(sources.span(key, 2, 3).is_err());
    assert!(sources.span(key, 3, 1).is_err());
    assert!(sources.span(key, 0, 5).is_err());
    assert!(
        sources
            .insert(
                key,
                SourceFile::new(
                    SourceId::new("duplicate").expect("source ID"),
                    SourcePath::new("rules/duplicate.yaml").expect("source path"),
                    Arc::<str>::from("duplicate"),
                )
            )
            .is_err()
    );

    let bundle = SourceBundle::new(vec![
        SourceDocument::new(
            SourcePath::new("rules/z.yaml").expect("source path"),
            Arc::<str>::from("z"),
        ),
        SourceDocument::new(
            SourcePath::new("rules/a.yaml").expect("source path"),
            Arc::<str>::from("a"),
        ),
    ])
    .expect("source bundle");
    assert_eq!(bundle.documents()[0].path().as_str(), "rules/a.yaml");
    let loaded = LoadedSourceBundle::new(bundle, SourceIntegrity::new([7; 32]));
    assert_eq!(loaded.integrity().as_bytes(), &[7; 32]);
}
