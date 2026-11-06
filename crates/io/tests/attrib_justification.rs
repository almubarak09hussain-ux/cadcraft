//! An attribute's vertical justification is DXF group 74; group 73 on an ATTRIB / ATTDEF is
//! the field length, which AutoCAD and DWG converters write as 0. Reading 73 as the vertical
//! justification drew Middle Center attributes half a text height too high.

use std::sync::Arc;

use cadcraft_doc::*;
use cadcraft_geom::Vec3;
use cadcraft_io::{read_dxf, write_dxf};

fn middle_text(value: &str, at: Vec3) -> Text {
    Text {
        insert: Vec3::new(at.x, at.y - 0.5, 0.0),
        align_pt: Some(at),
        height: 1.0,
        value: value.into(),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: HAlign::Center,
        valign: VAlign::Middle,
    }
}

fn attrib(value: &str) -> Attrib {
    Attrib {
        tag: "NAME".into(),
        text: middle_text(value, Vec3::new(5.0, 1.0, 0.0)),
        invisible: false,
        constant: false,
        prompt: String::new(),
        props: Default::default(),
        multiline: None,
        modes: Default::default(),
    }
}

/// Block TAG with a Middle Center definition, and a reference with its Middle Center value.
fn drawing() -> Drawing {
    let mut d = Drawing::new_metric();
    let mut b = Block::new("TAG");
    let def = Attrib { text: middle_text("X", Vec3::new(5.0, 1.0, 0.0)), ..attrib("WORKSHOP") };
    b.entities.push(Entity::new(Handle(0x9000), EntityKind::AttDef(def)));
    d.blocks.insert("TAG".into(), Arc::new(b));
    let attrib = attrib("WORKSHOP");
    let ins = Insert {
        block: "TAG".into(),
        insert: Vec3::ZERO,
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: 0.0,
        attribs: vec![attrib],
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    };
    d.add(&Space::Model, Common::default(), EntityKind::Insert(ins)).unwrap();
    d
}

/// `text` with a field length group (73 = 0) before every vertical justification (74), as
/// AutoCAD writes attributes.
fn with_field_length(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<&str> = Vec::with_capacity(lines.len() + 64);
    let mut i = 0;
    while i < lines.len() {
        if i % 2 == 0 && lines[i].trim() == "74" {
            out.push("73");
            out.push("0");
        }
        out.push(lines[i]);
        i += 1;
    }
    out.join("\r\n")
}

#[test]
fn middle_center_attributes_keep_their_justification_beside_a_field_length() {
    let text = write_dxf(&drawing());
    let patched = with_field_length(&text);
    assert!(patched.len() > text.len(), "the test file has 74 groups to patch");
    let back = read_dxf(patched.as_bytes()).unwrap();
    let Some(EntityKind::Insert(ins)) = back.model.iter().map(|e| e.kind.clone()).next() else { panic!("no insert") };
    let t = &ins.attribs[0].text;
    assert_eq!((t.halign, t.valign), (HAlign::Center, VAlign::Middle));
    assert_eq!(t.align_pt.map(|p| p.y), Some(1.0));
    let def =
        back.block("TAG").and_then(|b| b.entities.iter().find_map(|e| if let EntityKind::AttDef(a) = &e.kind { Some(a.text.valign) } else { None }));
    assert_eq!(def, Some(VAlign::Middle));
}

#[test]
fn text_still_reads_its_vertical_justification_from_group_73() {
    let mut d = Drawing::new_metric();
    d.add(&Space::Model, Common::default(), EntityKind::Text(middle_text("T", Vec3::new(0.0, 2.0, 0.0)))).unwrap();
    let back = read_dxf(write_dxf(&d).as_bytes()).unwrap();
    let Some(EntityKind::Text(t)) = back.model.iter().map(|e| e.kind.clone()).next() else { panic!("no text") };
    assert_eq!(t.valign, VAlign::Middle);
}
