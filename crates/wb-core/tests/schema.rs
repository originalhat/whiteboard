use std::fs;

const PUBLISHED: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schema/wb-0.1.schema.json"
);

#[test]
fn the_published_schema_matches_the_types() {
    let generated = serde_json::to_string_pretty(&wb_core::schema()).unwrap() + "\n";
    if std::env::var_os("UPDATE_SCHEMA").is_some() {
        fs::create_dir_all(std::path::Path::new(PUBLISHED).parent().unwrap()).unwrap();
        fs::write(PUBLISHED, &generated).unwrap();
    }

    let published = fs::read_to_string(PUBLISHED).unwrap_or_default();

    assert!(
        published == generated,
        "schema/wb-0.1.schema.json is stale; rerun with UPDATE_SCHEMA=1 and review the diff"
    );
}
