use std::{fmt::{Display, Error, Formatter}, fs::File};

use rusqlite::{Connection, ToSql};
use select::document::Document;
use select::predicate::{Class, Name, Predicate};
use strum_macros::EnumIter;

#[derive(Debug)]
pub struct Entry {
    name: String,
    rust_type: RustType,
    path: String,
}

impl Entry {
    pub fn insert_entry(&self, conn: &Connection) {
        let rust_type = &self.rust_type.to_string();
        let rust_type = rust_type[..rust_type.len() - 1].to_string();
        conn.execute("INSERT OR IGNORE INTO searchIndex (name, type, path) VALUES (?1, ?2, ?3)",
                     &[&self.name as &ToSql, &rust_type as &ToSql, &self.path]).unwrap();
    }
}

#[derive(Debug, Clone, EnumIter)]
pub enum RustType {
    Struct,
    Enum,
    Macro,
    Typedef,
    Constant,
    Trait,
    Function,
    Union,
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
            Trait => write!(f, "traits"),
            Function => write!(f, "functions"),
            Union => write!(f, "unions"),
        }
    }
}

pub fn extract_entries(path: &String, rust_type: &RustType) -> Vec<Entry> {
    let module = path.clone();
    let module: Vec<_> = module.split("/").collect();
    let prefix = module[module.len() - 2];
    let document = Document::from_read(File::open(path)
        .expect(&format!("Could not open {:?}", path)))
        .expect("Could not parse html from file.");
    document.find(Class(rust_type.to_string().as_ref())
        .descendant(Name("a"))).into_iter().map(|node| {
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