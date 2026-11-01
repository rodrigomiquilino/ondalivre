
use std::path::PathBuf;

// Access via running from package - need public API through binary crate.
// For now we duplicate minimal parse expectations using process + files.

#[test]
fn sample_lists_exist() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/music");
    assert!(root.join("demo.lst").exists());
    assert!(root.join("idents.rot").exists());
    assert!(root.join("ads.seq").exists());
    assert!(root.join("demo.m3u").exists());
}
