use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;

fn decode_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp + 1..];
        if let Some(semi) = rest.find(';') {
            let ent = &rest[..semi];
            match ent {
                "amp" => out.push('&'),
                "lt" => out.push('<'),
                "gt" => out.push('>'),
                "quot" => out.push('"'),
                "apos" => out.push('\''),
                _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                    if let Ok(code) = u32::from_str_radix(&ent[2..], 16)
                        && let Some(c) = char::from_u32(code)
                    {
                        out.push(c);
                    }
                }
                _ if ent.starts_with('#') => {
                    if let Ok(code) = ent[1..].parse::<u32>()
                        && let Some(c) = char::from_u32(code)
                    {
                        out.push(c);
                    }
                }
                _ => {
                    out.push('&');
                    out.push_str(ent);
                    out.push(';');
                }
            }
            rest = &rest[semi + 1..];
        } else {
            out.push('&');
        }
    }
    out.push_str(rest);
    out
}

fn parse_resx(path: &PathBuf) -> BTreeMap<String, String> {
    let xml = fs::read_to_string(path).expect("read resx");
    let mut map = BTreeMap::new();
    let mut pos = 0usize;
    while let Some(start) = xml[pos..].find("<data ") {
        let tag_start = pos + start;
        let Some(end_rel) = xml[tag_start..].find("</data>") else {
            break;
        };
        let block = &xml[tag_start..tag_start + end_rel];
        pos = tag_start + end_rel + 7;

        let Some(name_attr) = block.find("name=\"") else {
            continue;
        };
        let name_rest = &block[name_attr + 6..];
        let Some(name_end) = name_rest.find('"') else {
            continue;
        };
        let key = &name_rest[..name_end];

        let Some(val_start) = block.find("<value>") else {
            continue;
        };
        let val_rest = &block[val_start + 7..];
        let Some(val_end) = val_rest.find("</value>") else {
            continue;
        };
        let val = decode_xml(&val_rest[..val_end]);
        map.insert(key.to_string(), val);
    }
    map
}

fn to_snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if c == '_' {
            out.push('_');
        } else if c.is_uppercase() {
            if i > 0 && !out.ends_with('_') {
                out.push('_');
            }
            for lc in c.to_lowercase() {
                out.push(lc);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let en_path = manifest.join("resources/Strings.resx");
    let id_path = manifest.join("resources/Strings.id.resx");
    println!("cargo:rerun-if-changed={}", en_path.display());
    println!("cargo:rerun-if-changed={}", id_path.display());
    println!("cargo:rerun-if-env-changed=MOCHI_GOOGLE_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=MOCHI_GOOGLE_CLIENT_SECRET");

    let en = parse_resx(&en_path);
    let id = parse_resx(&id_path);
    for key in en.keys() {
        assert!(
            id.contains_key(key),
            "Strings.id.resx is missing key: {key}"
        );
    }
    for key in id.keys() {
        assert!(
            en.contains_key(key),
            "Strings.id.resx has extra key not in Strings.resx: {key}"
        );
    }

    let mut code = String::new();
    code.push_str("#[derive(Debug, Clone, Copy)]\npub struct Strings {\n");
    for key in en.keys() {
        code.push_str(&format!("    pub {}: &'static str,\n", to_snake(key)));
    }
    code.push_str("}\n\npub static EN: Strings = Strings {\n");
    for (key, val) in &en {
        code.push_str(&format!("    {}: {:?},\n", to_snake(key), val));
    }
    code.push_str("};\n\npub static ID: Strings = Strings {\n");
    for (key, en_val) in &en {
        let val = id.get(key).unwrap_or(en_val);
        code.push_str(&format!("    {}: {:?},\n", to_snake(key), val));
    }
    code.push_str("};\n");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("strings_gen.rs"), code).expect("write strings_gen.rs");
}
