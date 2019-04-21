use std::{fs::{create_dir_all, File}};
use std::fmt::{Display, Error, Formatter};
use std::path::{Path, PathBuf};

use clap::{App, load_yaml};
use rusqlite::{Connection, NO_PARAMS};
use select::document::Document;
use select::predicate::{Class, Name, Predicate};

fn main() {
    let yaml = load_yaml!("../cli.yml");
    let matches = App::from_yaml(yaml).get_matches();
    let input = matches.value_of("INPUT").expect("INPUT is a required field.");
    let input_dir = Path::new(input);
    let output = matches.value_of("OUTPUT").expect("OUTPUT is a required field.");
    if has_docs(input_dir) {
        let all_docs = DocsetBuilder::new("Rocket", output, input)
            .create_skeleton()
            .create_plist()
            .touch_db()
            .get_all();
        let all_entries: Vec<Vec<Entry>> = all_docs.iter().map(|page| {
            use self::RustType::*;
            let mut entries: Vec<Entry> = Vec::new();
            vec![Struct, Enum, Macro, Typedef, Constant].iter().for_each(|kind| {
                entries.append(&mut extract_entries(page, kind))
            });
            entries
        }).collect();
        dbg!(all_entries);
    }
}

#[derive(Debug)]
struct DocsetBuilder {
    name: String,
    source: PathBuf,
    root: PathBuf,
    contents_path: PathBuf,
    documents_path: PathBuf,
}

#[derive(Debug)]
pub struct Entry {
    name: String,
    rust_type: RustType,
    path: String,
}

#[derive(Debug, Clone)]
enum RustType {
    Struct,
    Enum,
    Macro,
    Typedef,
    Constant,
}

impl Display for RustType {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        use self::RustType::*;
        match *self {
            Struct => write!(f, "structs"),
            Enum => write!(f, "enums"),
            Macro => write!(f, "macros"),
            Typedef => write!(f, "types"),
            Constant => write!(f, "constants"),
        }
    }
}

impl DocsetBuilder {
    fn new<'a>(name: &'a str, root: &'a str, src: &'a str) -> Self {
        let mut root_docset = PathBuf::from(root);
        root_docset.push(format!("{}.docset", name));
        let mut src = PathBuf::from(src);
        src.push("target/doc");
        let mut dsb = DocsetBuilder {
            name: String::from(name),
            source: src,
            root: PathBuf::from(root),
            contents_path: root_docset.clone(),
            documents_path: root_docset,
        };
        dsb.contents_path.push("Contents");
        dsb.documents_path.push("Contents/Resources/Documents/");
        dsb
    }

    fn create_skeleton(&self) -> &Self {
        let path = self.documents_path.as_path();
        create_dir_all(path).unwrap();
        &self
    }

    fn create_plist(&self) -> &Self {
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
        </plist>"#, self.name.to_lowercase(), self.name, self.name.to_lowercase());
        let info_plist_path = self.contents_path.join("info.plist");
        let mut file = File::create(info_plist_path).unwrap();
        file.write_all(info_plist.as_bytes()).unwrap();
        &self
    }

    fn touch_db(&self) -> &Self {
        File::create(self.documents_path.as_path().join("docset.dsidx").as_path()).unwrap();
        let conn = Connection::open(self.documents_path.as_path().join("docset.dsidx").as_path()).unwrap();
        setup_table(&conn);
        &self
    }

    fn get_all(&self) -> Vec<String> {
        use glob::glob;
        let mut result = Vec::new();
        let dir = self.source.as_path();
        for entry in glob(&format!("{}/*/all.html",
                                   dir.to_str().unwrap())).expect("Failed to read glob pattern") {
            match entry {
                Ok(path) => result.push(path.to_owned().to_str().unwrap().to_string()),
                Err(_) => ()
            }
        }
        result
    }
}

fn setup_table(conn: &Connection) {
    conn.execute("CREATE TABLE searchIndex(\
    id INTEGER PRIMARY KEY, name TEXT, type TEXT, path TEXT\
    );", NO_PARAMS).unwrap();
    conn.execute("CREATE UNIQUE INDEX anchor ON searchIndex (name, type, path);", NO_PARAMS).unwrap();
}

fn has_docs(path: &Path) -> bool {
    let doc_path = path.join("target/doc");
    doc_path.is_dir()
}

fn extract_entries(path: &String, rust_type: &RustType) -> Vec<Entry> {
    let document = Document::from_read(File::open(path).unwrap()).unwrap();
    document.find(Class(rust_type.to_string().as_ref()).descendant(Name("a"))).into_iter().map(|node| {
        let mut entry: Entry;
        let name: &str = node.first_child().unwrap().as_text().unwrap();
        let raw_name: Vec<_> = name.split("::").collect();
        let name = raw_name.last().unwrap().to_string();
        let link = node.attr("href").unwrap();
        Entry {
            name,
            rust_type: rust_type.clone(),
            path: link.to_string(),
        }
    }).collect()
}