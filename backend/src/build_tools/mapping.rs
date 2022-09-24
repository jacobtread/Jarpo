use crate::define_from_value;
use bimap::{BiHashMap, BiMap};
use lazy_static::lazy_static;
use regex::{Match, Regex};
use std::path::Path;
use tokio::io;

pub struct Mapper<'a, 'b> {
    comments: Vec<&'a str>,
    mojang: &'b str,
    obf_2_bukkit: BiMap<&'a str, &'a str>,
    mojang_2_obf: BiMap<&'b str, &'b str>,
}

#[derive(Debug)]
enum MapperError {
    RegexFailure,
}
define_from_value! {
    MapperError {
        IO = io::Error,
    }
}

impl<'a, 'b> Mapper<'a, 'b> {
    fn from_buk(value: &'a str, mojang: &'b str) -> Result<Self, MapperError> {
        let (comments, obf_2_bukkit) = Self::load_bukkit(value)?;
        let mut mojang_2_obf = Self::load_mojang(mojang)?;
        Ok(Self {
            comments,
            mojang,
            obf_2_bukkit,
            mojang_2_obf,
        })
    }

    /// Loads the bukkit Obfuscated -> Bukkit mappings usually in a file ending with
    /// ".csrg"
    fn load_bukkit(value: &str) -> Result<(Vec<&str>, BiMap<&str, &str>), MapperError> {
        let mut comments = Vec::new();
        let mut mapping = BiMap::new();
        let lines = value.lines();
        for line in lines {
            if line.starts_with('#') {
                comments.push(line);
            } else {
                let parts: Vec<&str> = line
                    .split_whitespace()
                    .collect();
                if parts.len() == 2 {
                    mapping.insert(parts[0], parts[1]);
                }
            }
        }
        Ok((comments, mapping))
    }

    fn load_mojang(value: &str) -> Result<BiMap<&str, &str>, MapperError> {
        let mut mapping = BiMap::new();
        let type_regex = Regex::new(r"(?<original>.*)\s->\s(?<mapped>.*):")
            .map_err(|_| MapperError::RegexFailure)?;
        let captures = type_regex.captures_iter(value);
        for capture in captures {
            let original = match capture.name("original") {
                Some(value) => value.as_str(),
                None => continue,
            };
            let obfuscated = match capture.name("obfuscated") {
                Some(value) => value.as_str(),
                None => continue,
            };

            mapping.insert(original, obfuscated)
        }
        Ok(mapping)
    }

    fn get_name_from(value: &str, map: &BiMap<&str, &str>) -> Option<String> {
        let mut out: Option<String> = None;
        if let Some(mapped) = map.get_by_left(value) {
            out = Some(mapped.to_string())
        } else {
            let mut inner = String::new();
            let mut curr = value;
            while out.is_none() {
                if let Some(index) = curr.rfind('$') {
                    inner.insert_str(0, &curr[index..]);
                    curr = &curr[..index];
                    if let Some(mapped) = map.get_by_left(curr) {
                        out = Some(mapped.to_string())
                    }
                } else {
                    return None;
                }
            }
        }
        return out;
    }

    fn make_csrg(&mut self, methods: bool) -> Result<String, MapperError> {
        let mut out = String::new();
        for comment in self.comments {
            out.push_str(comment);
            out.push('\n');
        }

        let type_regex = Regex::new(r"(?<original>.*)\s->\s(?<mapped>.*):")
            .map_err(|_| MapperError::RegexFailure)?;
        let member_regex = Regex::new(r"(?:\d+:\d+:)?(.*?) (.*?) -> (.*)")
            .map_err(|_| MapperError::RegexFailure)?;

        let lines = self.mojang.lines();
        let mut current_class: Option<String> = None;
        for line in lines {
            if line.starts_with("#") {
                continue;
            }

            if let Some(capture) = type_regex.captures(line) {
                current_class = None;
                let obfuscated = match capture.name("obfuscated") {
                    Some(value) => value
                        .as_str()
                        .replace(".", "/"),
                    None => continue,
                };
                let bukkit = Self::get_name_from(&obfuscated, &self.obf_2_bukkit);
                if let Some(bukkit) = bukkit {
                    current_class = Some(bukkit)
                }
            } else if let Some(current_class) = &current_class {
                if let Some(capture) = member_regex.captures(line) {
                    let original = match capture.get(2) {
                        Some(value) => value
                            .as_str()
                            .replace(".", "/"),
                        None => continue,
                    };

                    let mut obfuscated = match capture.get(2) {
                        Some(value) => value
                            .as_str()
                            .replace(".", "/"),
                        None => continue,
                    };

                    if !original.contains('"') {
                        if original.eq(&obfuscated) || original.contains('$') {
                            continue;
                        }
                        if !methods && (obfuscated.eq("if") || obfuscated.eq("do")) {
                            obfuscated.push('_');
                        }
                        out.push_str(current_class);
                        out.push(' ');
                        out.push_str(&obfuscated);
                        out.push(' ');
                        out.push_str(&original)
                    } else if methods {
                    }
                }
            }
        }

        Ok(out)
    }
}
