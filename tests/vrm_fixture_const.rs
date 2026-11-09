//! The exported fixture (feature `fixtures`) is exactly what `scripts/vrm_fixture.py` writes now
//! (run here, not read from the committed file), and it reads as a VRM.
#![cfg(feature = "fixtures")]

use alice_game_engine::vrm::fixture::TINY_VRM;
use std::process::Command;

#[test]
fn the_const_is_the_scripts_output() {
    let root = env!("CARGO_MANIFEST_DIR");
    let out = Command::new("python3")
        .arg(format!("{root}/scripts/vrm_fixture.py"))
        .arg("--stdout")
        .output()
        .expect("python3 runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!out.stdout.is_empty(), "the script wrote bytes");
    assert_eq!(out.stdout.as_slice(), TINY_VRM);
}

#[test]
fn the_const_reads_as_a_vrm() {
    let model = alice_game_engine::vrm::read_vrm(TINY_VRM).expect("reads");
    // the script lays out one mesh and three nodes
    assert_eq!(model.meshes.len(), 1);
    assert_eq!(model.nodes.len(), 3);
}
