use bimap::BiMap;
use hashcow::CowHashMap;
use log::info;
use std::collections::HashMap;
use std::hash::Hash;

/// Cow HashMaps are used for holding mappings because the mojang mappings
/// are modified so they become owned strings but the bukkit mappings are
/// not owned
type CowMapping<'a> = CowHashMap<'a, str, str>;

/// Structure for manipulating, converting and merging
/// mapping files.
pub struct Mapper<'a> {
    /// A list of comments present in the loaded bukkit mappings.
    /// (Lines prefixed with #)
    bukkit_comments: Vec<&'a str>,

    /// The Obfuscated -> Bukkit mappings these are all string
    /// slices from the provided bukkit string. CowMapping used
    /// to generalize the type.
    obf_2_bukkit: CowMapping<'a>,

    /// The Bukkit -> Obfuscated mappings these are all string
    /// slices and aren't used in the same function as the
    /// mojang mappings so they are a normal HashMap
    bukkit_2_obf: HashMap<&'a str, &'a str>,
}

/// Represents a parsed item from a mojang mappings file
#[derive(Debug)]
enum MappedMember<'a> {
    Field {
        name: &'a str,
        obf_name: &'a str,
    },
    Method {
        name: &'a str,
        obf_name: &'a str,
        return_type: &'a str,
        args: &'a str,
    },
}

impl<'a> Mapper<'a> {
    /// Creates a new mappings generator from the provided
    /// bukkit mappings.
    pub fn new(bukkit: &'a str) -> Self {
        let mut bukkit_comments = Vec::new();
        let mut obf_2_bukkit = CowMapping::new();
        let mut bukkit_2_obf = HashMap::new();
        for line in bukkit.lines() {
            if line.starts_with('#') {
                bukkit_comments.push(line);
            } else {
                if let Some((obf_name, bukkit_name)) = Self::try_parse_bukkit(line) {
                    obf_2_bukkit.insert_borrowed(obf_name, bukkit_name);
                    bukkit_2_obf.insert(bukkit_name, obf_name);
                }
            }
        }
        Self {
            bukkit_comments,
            obf_2_bukkit,
            bukkit_2_obf,
        }
    }

    /// Attempts to parse a line from a bukkit mappings file
    /// these are simply the obfuscated name and bukkit name
    /// split by whitespace.
    fn try_parse_bukkit(line: &str) -> Option<(&str, &str)> {
        let mut parts = line.split_whitespace();
        let obf_name = parts.next()?;
        let bukkit_name = parts.next()?;
        Some((obf_name, bukkit_name))
    }

    /// Determines the mapped value to the provided value by
    /// looking in the provided map. If the value is made of
    /// of nested values seperated by $ they are mapped too.
    fn mapped_value(value: &str, map: &CowMapping) -> Option<String> {
        // Non nested values can be retrieved immediately
        if let Some(mapped) = map.get(value) {
            return Some(mapped.to_string());
        }

        // Skip values with no nesting that can't be mapped
        if !value.contains('$') {
            return None;
        }

        let mut inner = String::new();
        // The current available slice of `value`
        let mut current = value;

        loop {
            if let Some(index) = current.rfind('$') {
                inner.insert_str(0, &current[index..]);
                current = &current[..index];
                if let Some(mapped) = map.get(current) {
                    let mut out = String::with_capacity(inner.len() + mapped.len());
                    out.push_str(mapped);
                    out.push_str(&inner);

                    return Some(out);
                }
            } else {
                return None;
            }
        }
    }

    /// Translates the provided obfuscated name into the
    /// bukkit mapping value. None if unable to find one.
    fn get_bukkit_name(&self, obfuscated: &str) -> Option<String> {
        Self::mapped_value(obfuscated, &self.obf_2_bukkit)
    }

    /// Translates the provided mojang name into the
    /// bukkit mapped value for its obfuscated name.
    /// `mappings` is the Mojang to obfuscated mappings
    fn translate_name(&self, mojang: &str, mappings: &CowMapping) -> Option<String> {
        let obfuscated_name = Self::mapped_value(mojang, mappings)?;
        self.get_bukkit_name(&obfuscated_name)
    }

    /// Loads the mojang mappings into the `mojang_2_obf` map
    fn load_mojang(mojang: &str, out: &mut CowMapping) {
        for line in mojang.lines() {
            /// Line formatted like (net.minecraft.Util$5 -> ad$4:)
            if !line.ends_with(':') {
                continue;
            }

            if let Some((mojang_name, obf_name)) = Self::try_parse_class_line(line) {
                out.insert_owned(mojang_name, obf_name);
            }
        }
    }

    /// Attempts to parse a class definition line
    fn try_parse_class_line(line: &str) -> Option<(String, String)> {
        if !line.ends_with(':') {
            return None;
        }

        let mut parts = line.split(" -> ");
        let mojang_name = parts
            .next()?
            .replace('.', "/");
        let obf_name = parts.next()?;
        let obf_name = (&obf_name[..obf_name.len() - 1]).replace('.', "/");
        Some((mojang_name, obf_name))
    }

    /// Attempts to parse a member definition line
    fn try_parse_member_line(line: &str, methods: bool) -> Option<MappedMember> {
        let mut parts = line
            .trim_start()
            .split_whitespace();

        let ty = parts.next()?;
        let ty = if ty.contains(':') {
            let end_of_num = ty.rfind(":").unwrap_or(0);
            &ty[end_of_num + 1..]
        } else {
            ty
        };

        let name = parts.next()?;

        // Skip ->
        parts.next()?;

        let obf_name = parts.next()?;

        if name.contains('(') {
            if !methods {
                return None;
            }

            let args_start = name.find('(')?;
            let args = &name[args_start + 1..name.len() - 1];
            let name = &name[..args_start];

            if obf_name.eq(name)
                || name.contains('$')
                || obf_name.eq("<init>")
                || obf_name.eq("<clinit>")
            {
                return None;
            }

            Some(MappedMember::Method {
                return_type: ty,
                name,
                args,
                obf_name,
            })
        } else {
            if obf_name.eq(name) || name.contains('$') {
                return None;
            }
            Some(MappedMember::Field { name, obf_name })
        }
    }

    /// Converts the provided arguments and return type string into a
    /// descriptor string for the csrg output
    fn make_csrg_descriptor(
        &mut self,
        args: &str,
        return_type: &str,
        mappings: &CowMapping,
    ) -> String {
        let mut output = String::new();
        output.push('(');

        for part in args.split(',') {
            if part.is_empty() {
                continue;
            }
            let jvm_type = self.convert_type(part, mappings);
            output.push_str(&jvm_type);
        }
        output.push(')');
        let return_type = self.convert_type(return_type, mappings);
        output.push_str(&return_type);
        output
    }

    /// Attempts to convert the provided value type into a
    /// JVM type name.
    fn get_jvm_type(value: &str) -> Option<char> {
        Some(match value {
            "byte" => 'B',
            "char" => 'C',
            "double" => 'D',
            "float" => 'F',
            "int" => 'I',
            "long" => 'J',
            "short" => 'S',
            "boolean" => 'Z',
            "void" => 'V',
            _ => return None,
        })
    }

    /// Converts the provided value type to a csrg / bukkit type
    fn convert_type(&self, value: &str, mappings: &CowMapping) -> String {
        if let Some(jvm_char) = Self::get_jvm_type(value) {
            String::from(jvm_char)
        } else if value.ends_with("[]") {
            // Array types
            if value.len() <= 2 {
                String::from("[]")
            } else {
                let segment = self.convert_type(&value[..value.len() - 2], mappings);
                format!("[{segment}")
            }
        } else {
            // Class types
            let class = value.replace('.', "/");
            let bukkit_name = self
                .translate_name(&class, mappings)
                .unwrap_or(class);
            format!("L{bukkit_name};")
        }
    }

    pub fn make_csrg<'b>(&mut self, mojang: &'b str, members: bool) -> String {
        let mut mojang_mappings = CowMapping::new();
        if members {
            Self::load_mojang(mojang, &mut mojang_mappings);
        }

        let mut out = Vec::new();
        for comment in &self.bukkit_comments {
            out.push(comment.to_string());
        }

        let mut current_class = None;

        for line in mojang.lines() {
            if line.starts_with("#") {
                continue;
            }

            if line.ends_with(":") {
                current_class = None;
                if let Some((_, obf_name)) = Self::try_parse_class_line(line) {
                    if let Some(name) = self.get_bukkit_name(&obf_name) {
                        current_class = Some(name)
                    }
                }
            } else if let Some(current_class) = &current_class {
                if let Some(member) = Self::try_parse_member_line(line, members) {
                    let line = match member {
                        MappedMember::Field {
                            name,
                            obf_name: obfuscated,
                        } => {
                            if !members && (obfuscated.eq("if") || obfuscated.eq("do")) {
                                format!("{current_class} {obfuscated}_ {name}")
                            } else {
                                format!("{current_class} {obfuscated} {name}")
                            }
                        }
                        MappedMember::Method {
                            name,
                            obf_name: obfuscated,
                            args,
                            return_type,
                        } => {
                            let descriptor =
                                self.make_csrg_descriptor(args, return_type, &mojang_mappings);
                            format!("{current_class} {obfuscated} {descriptor} {name}")
                        }
                    };
                    out.push(line)
                }
            }
        }

        out.sort();
        out.join("\n")
    }
}

#[cfg(test)]
mod test {
    use crate::build_tools::mapping::Mapper;
    use std::fs::{read, write};
    use std::path::Path;

    #[test]
    fn test() {
        dotenv::dotenv().ok();
        env_logger::init();

        let bukkit_path = Path::new("test/build/bukkit-1.18-cl.csrg");
        let mojang_path = Path::new("test/build/server.txt");

        let bukkit = read(bukkit_path).unwrap();
        let bukkit = String::from_utf8_lossy(&bukkit);

        let mojang = read(mojang_path).unwrap();
        let mojang = String::from_utf8_lossy(&mojang);

        let mut mapper = Mapper::new(bukkit.as_ref());
        let out = mapper.make_csrg(mojang.as_ref(), true);
        let out_path = Path::new("test/build/output.csrg");
        write(out_path, out).unwrap();
    }
}
