//! Tests golden (§9) : l'invariant round-trip parse→serialize == identité
//! doit tenir sur chaque fixture, octet pour octet.

use std::fs;
use std::path::PathBuf;

use ubercron_lib::backend::cron::parser::Crontab;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/crontabs")
}

#[test]
fn round_trip_identite_sur_chaque_fixture() {
    let dir = fixtures_dir();
    let mut checked = 0;
    for entry in fs::read_dir(&dir).expect("dossier fixtures introuvable") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        let serialized = Crontab::parse(&text).serialize();
        assert_eq!(
            serialized,
            text,
            "round-trip cassé sur {}",
            path.file_name().unwrap().to_string_lossy()
        );
        checked += 1;
    }
    assert!(checked >= 6, "seulement {checked} fixtures vérifiées");
}
