use ordered_float::OrderedFloat;
use rusqlite::{Connection, OpenFlags};
use serde_derive::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

fn mk_true() -> bool {
    true
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct PathConfigIgnore {
    #[serde(default)]
    pub this: bool,

    #[serde(default)]
    pub constructors: Option<bool>,
    #[serde(default)]
    pub destructors: Option<bool>,

    #[serde(default)]
    pub functions: Vec<String>,

    #[serde(default)]
    pub line_ranges: Vec<(usize, usize)>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct PathConfig {
    #[serde(default)]
    pub ignore: PathConfigIgnore,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct ConfigIgnore {
    #[serde(default)]
    pub path_prefix: Vec<String>,

    #[serde(default = "mk_true")]
    pub constructors: bool,
    #[serde(default = "mk_true")]
    pub destructors: bool,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub db: PathBuf,
    pub p: f64,

    #[serde(default)]
    pub replace_path_prefix: Option<HashMap<String, String>>,

    #[serde(default)]
    pub ignore: ConfigIgnore,

    #[serde(default)]
    pub path: HashMap<PathBuf, PathConfig>,
}

impl Config {
    pub fn get_table_name(&self) -> Result<String, String> {
        if self.p <= 0.0 || self.p > 1.00 {
            return Err("Expected a p value in range (0,1]".to_string());
        }

        let table_name = format!(
            "optimization_result_p0_{}",
            (OrderedFloat(self.p) * OrderedFloat(10000.0)).round() as u32
        );
        Ok(table_name)
    }

    pub fn connect_to_db(&self) -> Result<Connection, Box<dyn std::error::Error>> {
        let conn = Connection::open_with_flags(&self.db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        Ok(conn)
    }

    pub fn ignore_path_prefix(&self, path: &PathBuf) -> bool {
        for k in &self.ignore.path_prefix {
            if path.starts_with(k) {
                return true;
            }
        }
        false
    }

    pub fn replace_path_prefix(&self, path: PathBuf) -> PathBuf {
        if let Some(prefixes) = &self.replace_path_prefix {
            for (k, v) in prefixes {
                if path.starts_with(k) {
                    let path_str = path.display().to_string();
                    let path = path_str.replacen(k, v, 1);
                    return PathBuf::from(path);
                }
            }
        }
        path
    }

    pub fn ignore_path(
        &self,
        path: &PathBuf,
        name: &str,
        start_line: &usize,
        end_line: &usize,
    ) -> bool {
        if let Some(path_config) = self.path.get(path) {
            if path_config.ignore.this || path_config.ignore.functions.contains(&name.to_string()) {
                return true;
            }

            for (s, e) in &path_config.ignore.line_ranges {
                if start_line >= s && end_line <= e {
                    return true;
                }
            }

            if let Some(ignore_constructors) = path_config.ignore.constructors {
                // FIXME: Somehow detect these
                if path.to_str().unwrap().starts_with("constructor") {
                    return ignore_constructors;
                }
            }

            if let Some(ignore_destructors) = path_config.ignore.destructors {
                if path.to_str().unwrap().contains("::~") {
                    return ignore_destructors;
                }
            }
        }
        // FIXME: Somehow detect these
        if path.to_str().unwrap().starts_with("constructor") {
            return self.ignore.constructors;
        }
        if path.to_str().unwrap().contains("::~") {
            return self.ignore.destructors;
        }

        self.ignore_path_prefix(path)
    }
}
