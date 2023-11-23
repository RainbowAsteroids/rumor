use std::fs;
use std::fs::File;

use std::io;
use std::io::Read;
use std::io::Seek;

use rumor::qsync::*;

#[test]
fn source_test() {
    let test_resources_path = std::env!("CARGO_MANIFEST_DIR").to_owned() + "/resources/test/";

    let mut older_file = File::open(test_resources_path.clone() + "PlayerMovement.older.cs").unwrap();
    let mut latest_file = File::open(test_resources_path + "PlayerMovement.latest.cs").unwrap();

    let file_digest = FileDigestBuilder::new()
            .build(&mut older_file).unwrap();

    let file_recipe = FileRecipe::new(
        &mut latest_file,
        &file_digest,
    ).unwrap();

    //dbg!(&file_recipe);

    let mut buffer = vec![];
    latest_file.seek(io::SeekFrom::Start(0)).unwrap();
    latest_file.read_to_end(&mut buffer).unwrap();

    let data = file_recipe.get_data(&mut older_file).collect::<io::Result<Vec<_>>>().unwrap();

    // we're ignoring the error because this is a convenience feature of this test
    // let _ = fs::write("/tmp/test-output.ron", format!("{:?}", &file_recipe));
    let _ = fs::write("/tmp/test-output.cs", &data);

    assert_eq!(buffer.len(), data.len());
    assert_eq!(buffer, data);
}

#[test]
fn tiff_test() {
    let test_resources_path = std::env!("CARGO_MANIFEST_DIR").to_owned() + "/resources/test/";

    let mut older_file = File::open(test_resources_path.clone() + "unsafe.tiff").unwrap();
    let mut latest_file = File::open(test_resources_path + "unsafe_prime.tiff").unwrap();

    let file_digest = FileDigestBuilder::new()
        .build(&mut older_file).unwrap();

    let file_recipe = FileRecipe::new(
        &mut latest_file,
        &file_digest,
    ).unwrap();

    //dbg!(&file_recipe);

    let mut buffer = vec![];
    latest_file.seek(io::SeekFrom::Start(0)).unwrap();
    latest_file.read_to_end(&mut buffer).unwrap();

    let data = file_recipe.get_data(&mut older_file).collect::<io::Result<Vec<_>>>().unwrap();

    // we're ignoring the error because this is a convenience feature of this test
    // let _ = fs::write("/tmp/test-output.ron", format!("{:?}", &file_recipe));
    let _ = fs::write("/tmp/test-output.tiff", &data);

    assert_eq!(buffer.len(), data.len());
    assert_eq!(buffer, data);
}

#[test]
fn tiff_uneven_test() {
    let test_resources_path = std::env!("CARGO_MANIFEST_DIR").to_owned() + "/resources/test/";

    let mut older_file = File::open(test_resources_path.clone() + "unsafe.tiff").unwrap();
    let mut latest_file = File::open(test_resources_path + "unsafe_prime.tiff").unwrap();

    let file_digest = FileDigestBuilder::new()
        .chunk_size(1024 + 512)
        .build(&mut older_file).unwrap();

    let file_recipe = FileRecipe::new(
        &mut latest_file,
        &file_digest,
    ).unwrap();

    //dbg!(&file_recipe);

    let mut buffer = vec![];
    latest_file.seek(io::SeekFrom::Start(0)).unwrap();
    latest_file.read_to_end(&mut buffer).unwrap();

    let data = file_recipe.get_data(&mut older_file).collect::<io::Result<Vec<_>>>().unwrap();

    // we're ignoring the error because this is a convenience feature of this test
    // let _ = fs::write("/tmp/test-output.ron", format!("{:?}", &file_recipe));
    let _ = fs::write("/tmp/test-output.tiff", &data);

    assert_eq!(buffer.len(), data.len());
    assert_eq!(buffer, data);
}
