use std::{fs::{create_dir_all, File}};
use std::path::Path;

use clap::{App, load_yaml};
use rusqlite::{Connection, NO_PARAMS, Result};
use rusqlite::types::ToSql;

fn main() {
    let yaml = load_yaml!("../cli.yml");
    let matches = App::from_yaml(yaml).get_matches();
    let input = matches.value_of("INPUT").expect("INPUT is a required field.");
    let input_dir = Path::new(input);
    let output_dir = matches.value_of("OUTPUT").expect("OUTPUT is a required field.");
    let output_dir = Path::new(output_dir);
    if has_docs(input_dir) {
        create_skeleton("rocket", output_dir);
        create_plist("rocket", output_dir.join("rocket.docset").as_path());
        let db_src = touch_db(output_dir.join(
            "rocket.docset/Contents/Resources/Documents/").as_path());
        let db_path = output_dir.join(
            "rocket.docset/Contents/Resources/Documents/docset.dsidx").as_path().to_owned();
        let conn = Connection::open(db_path).unwrap();
        setup_table(&conn);
    }
}

fn has_docs(path: &Path) -> bool {
    let doc_path = path.join("target/doc");
    doc_path.is_dir()
}

fn create_skeleton(name: &'static str, path: &Path) {
    let path = path.join(&format!("{}.docset/Contents/Resources/Documents/", name));
    create_dir_all(path).unwrap()
}

fn create_plist(name: &'static str, path: &Path) {
    use std::io::Write;
    let info_plist = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
        <plist version="1.0">
        <dict>
        <key>CFBundleIdentifier</key>
        <string>{}</string>
        <key>CFBundleName</key>
        <string>{}</string>
        <key>DocSetPlatformFamily</key>
        <string>{}</string>
        <key>isDashDocset</key>
        <true/>
        </dict>
        </plist>"#, name.to_lowercase(), name, name.to_lowercase());
    let info_plist_path = path.join("Contents/info.plist");
    let mut file = File::create(info_plist_path).unwrap();
    file.write_all(info_plist.as_bytes()).unwrap();
}

fn touch_db(path: &Path) {
    dbg!(&path);
    File::create(path.join("docset.dsidx")).unwrap();
}

fn setup_table(conn: &Connection) {
    conn.execute("CREATE TABLE searchIndex(\
    id INTEGER PRIMARY KEY, name TEXT, type TEXT, path TEXT\
    );", NO_PARAMS).unwrap();
    conn.execute("CREATE UNIQUE INDEX anchor ON searchIndex (name, type, path);", NO_PARAMS).unwrap();
}