use crate::define_from_value;
use bimap::{BiHashMap, BiMap};
use lazy_static::lazy_static;
use log::info;
use regex::{Match, Regex};
use serde::de::Unexpected::Str;
use std::env::args;
use std::io::Split;
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

impl<'a, 'b> Mapper<'a, 'b> {
    fn from_buk(value: &'a str, mojang: &'b str) -> Self {
        let (comments, obf_2_bukkit) = Self::load_bukkit(value);
        let mut mojang_2_obf = Self::load_mojang(mojang);
        Self {
            comments,
            mojang,
            obf_2_bukkit,
            mojang_2_obf,
        }
    }

    /// Loads the bukkit Obfuscated -> Bukkit mappings usually in a file ending with
    /// ".csrg"
    fn load_bukkit(value: &str) -> (Vec<&str>, BiMap<&str, &str>) {
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
        (comments, mapping)
    }

    fn load_mojang(value: &str) -> BiMap<&str, &str> {
        let mut mapping = BiMap::new();
        let type_regex = Regex::new(r"(.*)\s->\s(.*):").expect("Regex failed");
        let captures = type_regex.captures_iter(value);
        for capture in captures {
            let original = match capture.get(1) {
                Some(value) => value.as_str(),
                None => continue,
            };
            let obfuscated = match capture.get(2) {
                Some(value) => value.as_str(),
                None => continue,
            };

            mapping.insert(original, obfuscated);
        }
        println!("{:#?}", mapping);
        mapping
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

    fn to_obf(&self, desc: &str) -> String {
        let mut desc = &desc[1..];
        let mut out = String::new();
        let first_char = desc.chars().next();
        if let Some(char) = first_char {
            if char == ')' {
                desc = &desc[1..];
            }
        }
        while desc.len() > 0 {
            desc = self.obf_type(desc, &mut out);
            if let Some(char) = first_char {
                if char == ')' {
                    desc = &desc[1..];
                    out.push(char)
                }
            }
        }
        out
    }

    fn obf_type<'c>(&self, value: &'c str, out: &mut String) -> &'c str {
        let mut size = 1;
        let first_char = value.chars().next();
        if let Some(char) = first_char {
            match char {
                'B' | 'C' | 'D' | 'F' | 'I' | 'J' | 'S' | 'Z' | 'V' => out.push(char),
                '[' => {
                    out.push(char);
                    return self.obf_type(&value[1..], out);
                }
                'L' => {
                    if let Some(end) = value.find(';') {
                        let ty = &value[1..end];
                        size += ty.len() + 1;
                        out.push(char);
                        let v = self
                            .obf_2_bukkit
                            .get_by_left(ty)
                            .unwrap_or(&ty);
                        out.push_str(v);
                        out.push(';');
                    }
                }
                _ => {}
            }
        }
        return &value[size..];
    }

    fn csrg_desc(&self, args: &str, ret: &str) -> String {
        let parts = &args[1..args.len() - 1];
        let mut desc = String::new();
        desc.push('(');
        for part in parts.split(",") {
            if part.is_empty() {
                continue;
            }
            let ty = self.to_jvm_type(part);
            desc.push_str(&ty)
        }
        desc.push_str(")");
        let ret_ty = self.to_jvm_type(ret);
        desc.push_str(&ret_ty);
        desc
    }

    fn to_jvm_type(&self, value: &str) -> String {
        match value {
            "byte" => String::from('B'),
            "char" => String::from('C'),
            "double" => String::from('D'),
            "float" => String::from('F'),
            "int" => String::from('I'),
            "long" => String::from('J'),
            "short" => String::from('S'),
            "boolean" => String::from('Z'),
            "void" => String::from('V'),
            value => {
                if value.ends_with("[]") {
                    if value.len() > 2 {
                        let seg = self.to_jvm_type(&value[..value.len() - 2]);
                        format!("[{}]", seg)
                    } else {
                        String::from("[]")
                    }
                } else {
                    let class = value.replace(".", "/");
                    let obf = Self::get_name_from(&class, &self.mojang_2_obf)
                        .unwrap_or_else(|| class.clone());
                    let mapped_type =
                        Self::get_name_from(&obf, &self.obf_2_bukkit).unwrap_or_else(|| class);

                    return format!("L{};", mapped_type);
                }
            }
        }
    }

    pub fn make_csrg(&self, methods: bool) -> String {
        let mut out = String::new();
        for comment in &self.comments {
            out.push_str(comment);
            out.push('\n');
        }

        let type_regex = Regex::new(r"(.*?)\s->\s(.*):").expect("Regex Failure");
        let member_regex = Regex::new(r"(?:\d+:\d+:)?(.*) (.*) -> (.*)").expect("Regex Failure");

        let lines = self.mojang.lines();
        let mut current_class: Option<String> = None;
        for line in lines {
            if line.starts_with("#") {
                continue;
            }

            if let Some(capture) = type_regex.captures(line) {
                current_class = None;
                let obfuscated = match capture.get(2) {
                    Some(value) => value
                        .as_str()
                        .replace(".", "/"),
                    None => continue,
                };
                let bukkit = Self::get_name_from(&obfuscated, &self.obf_2_bukkit);
                if let Some(bukkit) = bukkit {
                    current_class = Some(bukkit);
                }
            } else if let Some(current_class) = &current_class {
                if let Some(capture) = member_regex.captures(line) {
                    let original = match capture.get(2) {
                        Some(value) => value
                            .as_str()
                            .replace(".", "/"),
                        None => continue,
                    };

                    let mut obfuscated = match capture.get(3) {
                        Some(value) => value
                            .as_str()
                            .replace(".", "/"),
                        None => continue,
                    };

                    if !original.contains('(') {
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
                        out.push_str(&original);
                        out.push('\n');
                    } else if methods {
                        let mut ret = match capture.get(1) {
                            Some(value) => value,
                            None => continue,
                        };

                        let args_start = original
                            .find('(')
                            .unwrap_or(1);
                        let args = &original[args_start..];
                        let sig = self.csrg_desc(args, ret.as_str());
                        let moj_name = &original[0..args_start];
                        if obfuscated.eq(moj_name)
                            || moj_name.contains('$')
                            || obfuscated.eq("<init>")
                            || obfuscated.eq("<clinit>")
                        {
                            continue;
                        }
                        out.push_str(current_class);
                        out.push(' ');
                        out.push_str(&obfuscated);
                        out.push(' ');
                        out.push_str(&sig);
                        out.push(' ');
                        out.push_str(moj_name);
                        out.push('\n');
                    }
                }
            }
        }

        out
    }
}

#[cfg(test)]
mod test {
    use crate::build_tools::mapping::Mapper;
    use std::fs::{read, write};
    use std::path::Path;

    #[test]
    fn make_csrg() {
        let bukkit_path = Path::new("test/build/bukkit-1.14-cl.csrg");
        let mojang_path = Path::new("test/build/server.txt");

        let bukkit = read(bukkit_path).unwrap();
        let bukkit = String::from_utf8_lossy(&bukkit);

        let mojang = read(mojang_path).unwrap();
        let mojang = String::from_utf8_lossy(&mojang);

        let mapper = Mapper::from_buk(bukkit.as_ref(), mojang.as_ref());

        let out = mapper.make_csrg(true);

        let out_path = Path::new("test/build/output.csrg");
        write(out_path, out).unwrap();
    }
}
