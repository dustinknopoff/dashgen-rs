use std::{fs::{create_dir_all, File}, path::{Path, PathBuf}, process};

use fs_extra::{copy_items, dir};
use rayon::prelude::*;
use rusqlite::{Connection, NO_PARAMS};

use crate::entry::{Entry, extract_entries};

#[derive(Debug)]
pub struct DocsetBuilder {
    name: String,
    source: PathBuf,
    root: PathBuf,
    contents_path: PathBuf,
    documents_path: PathBuf,
}

impl DocsetBuilder {
    pub fn new(root: Option<String>, src: Option<String>, name: Option<String>) -> Self {
        let out = root
            .clone()
            .map_or_else(|| Self::determine_dir(true),
                         |dir| PathBuf::from(&format!("{}_dash", dir)));
        let src = src
            .clone()
            .map_or_else(|| Self::determine_dir(false),
                         |dir| PathBuf::from(dir));
        let out = Self::clean_canonical(out);
        let name = if name.is_none() {
            match Self::get_name() {
                Ok(ref package_name) if !package_name.is_empty() => {
                    package_name.replace("-", "_")
                }
                Ok(_) => {
                    println!("Package name could not be found. Perhaps you are using a \
                virtual workspace. See README for failure explanation.");
                    println!("You can add the --name argument to avoid this error.");
                    process::exit(1);
                }
                Err(_) => {
                    panic!("Could not find a Cargo.toml.");
                }
            }
        } else {
            name.unwrap()
        };
        let mut root_docset = PathBuf::from(src);
        root_docset.push(format!("{}.docset", name));
        let src = PathBuf::from(out);
        let mut dsb = DocsetBuilder {
            name,
            source: src,
            root: root_docset.clone(),
            contents_path: root_docset.clone(),
            documents_path: root_docset,
        };
        dsb.contents_path.push("Contents");
        dsb.documents_path.push("Contents/Resources/Documents/");
        dsb
    }

    pub fn build(root: Option<String>, src: Option<String>, name: Option<String>) {
        let builder = DocsetBuilder::new(root, src, name);
        let all_docs = builder
            .create_skeleton()
            .create_plist()
            .copy_all()
            .get_all();
        let conn = builder.touch_db();
        let all_entries: Vec<Entry> = all_docs.par_iter().flat_map(|page| {
            use crate::entry::RustType;
            use strum::IntoEnumIterator;
            let mut entries: Vec<Entry> = Vec::new();
            RustType::iter().for_each(|kind| {
                let mut extracted_entry = extract_entries(page, &kind);
                entries.append(&mut extracted_entry);
            });
            entries
        }).collect();
        all_entries.iter().for_each(|entry| {
            entry.insert_entry(&conn);
        });
    }

    fn create_skeleton(&self) -> &Self {
        let path = self.documents_path.as_path();
        match create_dir_all(path) {
            Ok(_) => &self,
            Err(_) => panic!("Could not create skeleton of docset.")
        }
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
        <key>isJavaScriptEnabled</key><true/>
        </dict>
        </plist>"#, self.name.to_lowercase(), self.name, self.name.to_lowercase(), self.name, self.name.to_lowercase());
        let info_plist_path = self.contents_path.join("info.plist");
        let mut file = File::create(info_plist_path).expect("Could create info.plist file");
        file.write_all(info_plist.as_bytes()).expect("Could not write to info.plist");
        &self
    }

    fn touch_db(&self) -> Connection {
        File::create(self.contents_path.as_path().join("Resources/docset.dsidx").as_path()).expect("Could not create path for docset index.");
        let conn = Connection::open(self.contents_path.as_path().join("Resources/docset.dsidx").as_path()).expect("Could not connect to docset index.");
        Self::setup_table(&conn);
        conn
    }

    fn get_all(&self) -> Vec<String> {
        use glob::glob;
        let mut result = Vec::new();
        let dir = self.source.as_path().parent().expect("Could not extract documentation.");
        for entry in glob(&format!("{}/*/all.html",
                                   dir.to_str().expect("Could not parse documentation path."))).expect("Failed to read glob pattern") {
            match entry {
                Ok(path) => result.push(path.to_owned().to_str().expect("Could not convert path to string.").to_string()),
                Err(_) => ()
            }
        }
        result
    }

    fn copy_all(&self) -> &Self {
        use walkdir::WalkDir;
        let mut options = dir::CopyOptions::new();
        options.skip_exist = true;
        let src = self.source.clone();
        let src = if src.ends_with("doc") {
            src.as_path()
        } else {
            src.parent().expect("Could not extract parent from path.")
        };
        let files: Vec<_> = WalkDir::new(src)
            .into_iter()
            .map(|file| file.expect("Could not extract file from docs directory.").into_path()).collect();
        match copy_items(&files, self.documents_path.clone(), &options) {
            Ok(_) => (),
            Err(_) => {
                eprintln!("documentation could not be copied to {:?}.", self.documents_path);
                eprintln!("You will need to copy them manually for a valid docset.");
            }
        }
        self
    }

    fn determine_dir(is_input: bool) -> PathBuf {
        match Self::get_name() {
            Ok(package_name) => {
                let package_name = package_name.replace("-", "_");
                if is_input {
                    Path::new("target").join("doc").join(package_name)
                } else {
                    Path::new(&package_name).to_owned()
                }
            }
            Err(_) => {
                panic!("Could not find a Cargo.toml.");
            }
        }
    }

    fn get_name() -> Result<String, ()> {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| String::from("cargo"));
        let mut cmd = std::process::Command::new(cargo);
        cmd.arg("pkgid");

        let output = cmd.output().unwrap();
        let stdout = std::str::from_utf8(&output.stdout).unwrap();
        let split_forward_slash: Vec<_> = stdout.split("/").collect();
        let split_hash: Vec<_> = split_forward_slash.last().unwrap().split("#").collect();
        let name: Vec<_> = split_hash.last().unwrap().split(":").collect();
        let name: &str = name.iter().next().unwrap();
        Ok(name.to_string())
    }

    fn setup_table(conn: &Connection) {
        conn.execute("CREATE TABLE searchIndex(\
    id INTEGER PRIMARY KEY, name TEXT, type TEXT, path TEXT\
    );", NO_PARAMS).unwrap();
        conn.execute("CREATE UNIQUE INDEX anchor ON searchIndex (name, type, path);", NO_PARAMS).unwrap();
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
}
