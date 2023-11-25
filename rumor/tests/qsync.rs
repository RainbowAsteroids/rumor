use std::fs;
use std::fs::File;

use std::io;
use std::io::Read;
use std::io::Seek;

use rumor::qsync::*;

fn test(older_file_name: &str, latest_file_name: &str, file_digest_builder: FileDigestBuilder) {
    let test_resources_path = std::env!("CARGO_MANIFEST_DIR").to_owned() + "/resources/test/";

    let mut older_file = File::open(test_resources_path.clone() + older_file_name).unwrap();
    let mut latest_file = File::open(test_resources_path + latest_file_name).unwrap();

    let file_digest = file_digest_builder.build(&mut older_file).unwrap();

    let file_recipe = FileRecipe::new(
        &mut latest_file,
        &file_digest,
    ).unwrap();

    let mut buffer = vec![];
    latest_file.seek(io::SeekFrom::Start(0)).unwrap();
    latest_file.read_to_end(&mut buffer).unwrap();

    let data = file_recipe.get_data(&mut older_file).unwrap().collect::<Vec<_>>();

    // we're ignoring the error because this is a convenience feature of this test
    // let _ = fs::write("/tmp/test-output.ron", format!("{:?}", &file_recipe));
    // let _ = fs::write("/tmp/test-output.cs", &data);

    assert_eq!(buffer.len(), data.len());
    assert_eq!(buffer, data);
}

#[test]
fn source_test() {
    test(
        "PlayerMovement.older.cs",
        "PlayerMovement.latest.cs",
        FileDigestBuilder::new()
    );
}

#[test]
fn tiff_test() {
    test(
        "unsafe.tiff",
        "unsafe_prime.tiff",
        FileDigestBuilder::new()
    );
}

#[test]
fn tiff_uneven_test() {
    test(
        "unsafe.tiff",
        "unsafe_prime.tiff",
        FileDigestBuilder::new().chunk_size(1024 + 512)
    );
}

#[test]
fn shallow_packages_test() {
    test(
        "packages.older.json",
        "packages.latest.json",
        FileDigestBuilder::new().chunk_size(16)
    );
}
