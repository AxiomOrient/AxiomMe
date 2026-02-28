use super::*;
use crate::tier_documents::abstract_path;

#[test]
fn bootstrap_initializes_filesystem_without_runtime_index() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app new");

    app.bootstrap().expect("bootstrap");
    assert!(temp.path().join("resources").exists());
    assert!(temp.path().join("queue").exists());
    assert!(temp.path().join(".axiomme_state.sqlite3").exists());
    assert!(
        temp.path().join("agent/ontology/schema.v1.json").exists(),
        "bootstrap should seed default ontology schema"
    );

    let resources_root = AxiomUri::root(Scope::Resources);
    let docs_uri = resources_root.join("docs").expect("join docs");
    app.fs
        .create_dir_all(&docs_uri, true)
        .expect("create docs directory");
    assert!(
        !abstract_path(&app.fs, &docs_uri).exists(),
        "runtime tier files should not be synthesized during bootstrap"
    );

    app.prepare_runtime().expect("prepare runtime");
    assert!(
        abstract_path(&app.fs, &docs_uri).exists(),
        "runtime prepare should synthesize tier files"
    );
}

#[test]
fn deleting_root_and_reinitializing_recreates_runtime_state() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("workspace_root");

    let app = AxiomMe::new(&root).expect("app new");
    app.initialize().expect("init failed");
    drop(app);

    fs::remove_dir_all(&root).expect("remove root");
    assert!(!root.exists());

    let app2 = AxiomMe::new(&root).expect("app2 new");
    app2.initialize().expect("app2 init failed");

    assert!(root.join("resources").exists());
    assert!(root.join("queue").exists());
    assert!(root.join(".axiomme_state.sqlite3").exists());
}
