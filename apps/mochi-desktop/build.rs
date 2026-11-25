use std::env;
use std::fs;
use std::path::PathBuf;

fn push_res_header(
    out: &mut Vec<u8>,
    data_size: u32,
    type_id: u16,
    name_id: u16,
    mem_flags: u16,
    lang_id: u16,
) {
    out.extend_from_slice(&data_size.to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes()); // HeaderSize for numeric Type + Name
    out.extend_from_slice(&0xFFFFu16.to_le_bytes());
    out.extend_from_slice(&type_id.to_le_bytes());
    out.extend_from_slice(&0xFFFFu16.to_le_bytes());
    out.extend_from_slice(&name_id.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // DataVersion
    out.extend_from_slice(&mem_flags.to_le_bytes());
    out.extend_from_slice(&lang_id.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // Version
    out.extend_from_slice(&0u32.to_le_bytes()); // Characteristics
}

fn pad_to_dword(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

fn build_icon_res(ico: &[u8]) -> Vec<u8> {
    assert!(ico.len() >= 6, "invalid ICO header");
    let reserved = u16::from_le_bytes([ico[0], ico[1]]);
    let kind = u16::from_le_bytes([ico[2], ico[3]]);
    let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
    assert!(
        reserved == 0 && kind == 1 && count > 0,
        "invalid ICO format"
    );
    assert!(ico.len() >= 6 + count * 16, "truncated ICO directory");

    let mut res = Vec::with_capacity(ico.len() + 512);
    // Mandatory 32-byte empty resource header at start of .res file
    push_res_header(&mut res, 0, 0, 0, 0, 0);

    const RT_ICON: u16 = 3;
    const RT_GROUP_ICON: u16 = 14;
    const MEM_FLAGS: u16 = 0x1030; // MOVEABLE | PURE | DISCARDABLE
    const LANG_EN_US: u16 = 0x0409;

    let mut group = Vec::with_capacity(6 + count * 14);
    group.extend_from_slice(&0u16.to_le_bytes()); // idReserved
    group.extend_from_slice(&1u16.to_le_bytes()); // idType = 1 (icon)
    group.extend_from_slice(&(count as u16).to_le_bytes());

    for i in 0..count {
        let entry = &ico[6 + i * 16..6 + (i + 1) * 16];
        let size = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as usize;
        let offset = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as usize;
        assert!(offset + size <= ico.len(), "ICO image out of bounds");

        let icon_id = (i + 1) as u16;
        push_res_header(
            &mut res,
            size as u32,
            RT_ICON,
            icon_id,
            MEM_FLAGS,
            LANG_EN_US,
        );
        res.extend_from_slice(&ico[offset..offset + size]);
        pad_to_dword(&mut res);

        group.extend_from_slice(&entry[..12]);
        group.extend_from_slice(&icon_id.to_le_bytes());
    }

    push_res_header(
        &mut res,
        group.len() as u32,
        RT_GROUP_ICON,
        1,
        MEM_FLAGS,
        LANG_EN_US,
    );
    res.extend_from_slice(&group);
    pad_to_dword(&mut res);
    res
}

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let ico_path = manifest
        .join("../../crates/mochi-ui/assets/mochi.ico")
        .canonicalize()
        .expect("locate mochi.ico");
    println!("cargo:rerun-if-changed={}", ico_path.display());

    let ico = fs::read(&ico_path).expect("read mochi.ico");
    let res = build_icon_res(&ico);

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let res_path = out_dir.join("mochi_icon.res");
    fs::write(&res_path, res).expect("write mochi_icon.res");

    println!("cargo:rustc-link-arg-bin=Mochi={}", res_path.display());
}
