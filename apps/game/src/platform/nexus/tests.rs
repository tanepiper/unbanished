#[test]
fn download_file_names() {
    assert_eq!(
        super::file_name("https://cf-files.nexusmods.com/cdn/617/19/More%20Terrain%20v6-19-6.zip?md5=x&expires=1"),
        "More Terrain v6-19-6.zip"
    );
}
