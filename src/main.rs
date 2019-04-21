use std::{fs::{create_dir_all, File}, process};
use std::fmt::{Display, Error, Formatter};
use std::fs::read;
use std::path::{Path, PathBuf};

use cargo_metadata::Metadata;
use cargo_toml::Manifest;
use clap::{App, load_yaml};
use rayon::prelude::*;
use rusqlite::{Connection, NO_PARAMS, ToSql};
use select::document::Document;
use select::predicate::{Class, Name, Predicate};

fn main() {
    let yaml = load_yaml!("../cli.yml");
    let matches = App::from_yaml(yaml).get_matches();
    let builder = DocsetBuilder::new(matches.value_of("dir")
                                         .map(|s| s.to_string()),
                                     matches.value_of("out")
                                         .map(|s| s.to_string()));
//    let all_docs = builder
//        .create_skeleton()
//        .create_plist()
//        .get_all();
//    let conn = builder.touch_db();
//    let all_entries: Vec<Entry> = all_docs.par_iter().flat_map(|page| {
//        use self::RustType::*;
//        let mut entries: Vec<Entry> = Vec::new();
//        vec![Struct, Enum, Macro, Typedef, Constant].iter().for_each(|kind| {
//            let mut extracted_entry = extract_entries(page, kind);
//            entries.append(&mut extracted_entry);
//        });
//        entries
//    }).collect();
//    all_entries.iter().for_each(|entry| {
//        insert_entry(entry, &conn);
//    });
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
    fn new(root: Option<String>, src: Option<String>) -> Self {
        let out = root
            .clone()
            .map_or_else(|| determine_dir(true), |dir| PathBuf::from(dir));
        let src = src
            .clone()
            .map_or_else(|| determine_dir(false), |dir| PathBuf::from(dir));
        let out = clean_canonical(out);
        let src = clean_canonical(src);
        let name = match metadata_run(None) {
            Ok(manifest) => {
                let package_name = &manifest.packages[0].name;
                package_name.replace("-", "_")
            }
            Err(_) => {
                panic!("Could not find a Cargo.toml.");
                ::std::process::exit(1);
            }
        };
        let mut root_docset = PathBuf::from(out);
        root_docset.push(format!("{}.docset", name));
        let mut src = PathBuf::from(src);
        let mut dsb = DocsetBuilder {
            name,
            source: src,
            root: root_docset.clone(),
            contents_path: root_docset.clone(),
            documents_path: root_docset,
        };
        dsb.contents_path.push("Contents");
        dsb.documents_path.push("Contents/Resources/Documents/");
        dbg!(&dsb);
        dsb
    }

    fn create_skeleton(&self) -> &Self {
        let path = self.documents_path.as_path();
        create_dir_all(path).unwrap();
        &self
    }

    fn create_plist<'a>(&self) -> &Self {
        use std::io::Write;
        let mut index_path = self.source.clone();
        index_path.push(&format!("{}/index.html", self.name.to_lowercase()));
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
        <key>DashDocSetFallbackURL</key>
        <string>https://docs.rs/{}</string>
        <key>dashIndexFilePath</key>
        <string>{}/index.html</string>
        </dict>
        </plist>"#, self.name.to_lowercase(), self.name, self.name.to_lowercase(), self.name, self.name.to_lowercase());
        let info_plist_path = self.contents_path.join("info.plist");
        let mut file = File::create(info_plist_path).unwrap();
        file.write_all(info_plist.as_bytes()).unwrap();
        &self
    }

    fn touch_db(&self) -> Connection {
        File::create(self.contents_path.as_path().join("Resources/docset.dsidx").as_path()).unwrap();
        let conn = Connection::open(self.contents_path.as_path().join("Resources/docset.dsidx").as_path()).unwrap();
        setup_table(&conn);
        conn
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
    let module = path.clone();
    let module: Vec<_> = module.split("/").collect();
    let prefix = module[module.len() - 2];
    let document = Document::from_read(File::open(path).unwrap()).unwrap();
    document.find(Class(rust_type.to_string().as_ref()).descendant(Name("a"))).into_iter().map(|node| {
        let name: &str = node.first_child().unwrap().as_text().unwrap();
        let raw_name: Vec<_> = name.split("::").collect();
        let name = raw_name.last().unwrap().to_string();
        let link = format!("{}/{}", prefix, node.attr("href").unwrap());
        Entry {
            name,
            rust_type: rust_type.clone(),
            path: link.to_string(),
        }
    }).collect()
}

fn insert_entry(entry: &Entry, conn: &Connection) {
    let rust_type = &entry.rust_type.to_string();
    let rust_type = rust_type[..rust_type.len() - 1].to_string();
    conn.execute("INSERT OR IGNORE INTO searchIndex (name, type, path) VALUES (?1, ?2, ?3)",
                 &[&entry.name as &ToSql, &rust_type as &ToSql, &entry.path]).unwrap();
}

pub fn metadata_run(additional_args: Option<String>) -> Result<Metadata, ()> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| String::from("cargo"));
    let mut cmd = std::process::Command::new(cargo);
    cmd.arg("metadata");
    cmd.args(&["--format-version", "1"]);
    if let Some(additional_args) = additional_args {
        cmd.arg(&additional_args);
    }

    let output = cmd.output().unwrap();
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    let meta = serde_json::from_str(stdout).unwrap();
    Ok(meta)
}

fn determine_dir(is_input: bool) -> PathBuf {
    match metadata_run(None) {
        Ok(manifest) => {
            let mut root = &manifest.workspace_root;
            root.push("Cargo.toml");
            let manifest = Manifest::from_slice(&read(root).unwrap()).unwrap();
            let package = manifest.package.as_ref().unwrap();
            let package_name = package.name;
            let package_name = package_name.replace("-", "_");
            if is_input {
                Path::new("target").join("doc").join(package_name)
            } else {
                Path::new(&package_name).to_owned()
            }
        }
        Err(_) => {
            panic!("Could not find a Cargo.toml.");
            ::std::process::exit(1);
        }
    }
}

fn clean_canonical(path: PathBuf) -> PathBuf {
    match path.canonicalize() {
        Ok(dir) => dir,
        Err(_) => {
            println!("Could not find directory {:?}.", path);
            println!();
            println!("Please run `cargo doc` before running `cargo dashgen`.");
            process::exit(1);
        }
    }
}