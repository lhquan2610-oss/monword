use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};
use std::io::Cursor;

#[derive(Debug, Clone)]
pub enum XmlNode {
    Element(XmlElement),
    Text(String),
    CData(String),
    Comment(String),
    PI(String),
    DocType(String),
}

#[derive(Debug, Clone)]
pub struct XmlElement {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<XmlNode>,
}

#[derive(Debug, Clone)]
pub struct XmlDocument {
    pub decl: Option<(String, Option<String>, Option<String>)>,
    pub prelude: Vec<XmlNode>,
    pub root: XmlElement,
}

impl XmlElement {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attrs: Vec::new(),
            children: Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub fn with_attrs(name: impl Into<String>, attrs: Vec<(&str, &str)>) -> Self {
        Self {
            name: name.into(),
            attrs: attrs
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            children: Vec::new(),
        }
    }

    pub fn get_attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn set_attr(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let k = key.into();
        let v = value.into();
        if let Some((_, existing)) = self.attrs.iter_mut().find(|(ek, _)| ek == &k) {
            *existing = v;
        } else {
            self.attrs.push((k, v));
        }
    }

    pub fn remove_attr(&mut self, key: &str) {
        self.attrs.retain(|(k, _)| k != key);
    }

    pub fn child(&self, name: &str) -> Option<&XmlElement> {
        self.children.iter().find_map(|n| match n {
            XmlNode::Element(el) if el.name == name => Some(el),
            _ => None,
        })
    }

    pub fn child_mut(&mut self, name: &str) -> Option<&mut XmlElement> {
        self.children.iter_mut().find_map(|n| match n {
            XmlNode::Element(el) if el.name == name => Some(el),
            _ => None,
        })
    }

    pub fn children_elements(&self) -> impl Iterator<Item = &XmlElement> {
        self.children.iter().filter_map(|n| match n {
            XmlNode::Element(el) => Some(el),
            _ => None,
        })
    }

    #[allow(dead_code)]
    pub fn children_elements_mut(&mut self) -> impl Iterator<Item = &mut XmlElement> {
        self.children.iter_mut().filter_map(|n| match n {
            XmlNode::Element(el) => Some(el),
            _ => None,
        })
    }

    pub fn ensure_child_ordered(&mut self, name: &str, order: &[&str]) -> &mut XmlElement {
        let target_idx = if let Some(idx) = self
            .children
            .iter()
            .position(|n| matches!(n, XmlNode::Element(e) if e.name == name))
        {
            idx
        } else {
            let insert_idx = find_ordered_insert_index(&self.children, name, order);
            self.children
                .insert(insert_idx, XmlNode::Element(XmlElement::new(name)));
            insert_idx
        };
        match &mut self.children[target_idx] {
            XmlNode::Element(el) => el,
            _ => unreachable!(),
        }
    }

    pub fn insert_child_ordered(&mut self, child: XmlElement, order: &[&str]) {
        let idx = find_ordered_insert_index(&self.children, &child.name, order);
        self.children.insert(idx, XmlNode::Element(child));
    }

    pub fn ensure_child_first(&mut self, name: &str) -> &mut XmlElement {
        let target_idx = if let Some(idx) = self
            .children
            .iter()
            .position(|n| matches!(n, XmlNode::Element(e) if e.name == name))
        {
            idx
        } else {
            self.children.insert(0, XmlNode::Element(XmlElement::new(name)));
            0
        };
        match &mut self.children[target_idx] {
            XmlNode::Element(el) => el,
            _ => unreachable!(),
        }
    }

    pub fn remove_children_by_name(&mut self, name: &str) {
        self.children
            .retain(|n| !matches!(n, XmlNode::Element(e) if e.name == name));
    }

    pub fn extract_text(&self) -> String {
        let mut out = String::new();
        collect_element_text(self, &mut out);
        out
    }

    pub fn set_text(&mut self, value: impl Into<String>) {
        self.children.clear();
        self.children.push(XmlNode::Text(value.into()));
    }
}

fn find_ordered_insert_index(children: &[XmlNode], target_name: &str, order: &[&str]) -> usize {
    let target_rank = order
        .iter()
        .position(|&x| x == target_name)
        .unwrap_or(order.len());
    for (idx, node) in children.iter().enumerate() {
        if let XmlNode::Element(el) = node {
            let rank = order
                .iter()
                .position(|&x| x == el.name)
                .unwrap_or(order.len());
            if rank > target_rank {
                return idx;
            }
        }
    }
    children.len()
}

fn collect_element_text(el: &XmlElement, out: &mut String) {
    if matches!(
        el.name.as_str(),
        "w:drawing"
            | "w:pict"
            | "mc:AlternateContent"
            | "w:instrText"
            | "w:delText"
            | "w:footnoteReference"
            | "w:endnoteReference"
    ) {
        return;
    }
    if el.name == "w:tab" {
        out.push('\t');
        return;
    }
    if el.name == "w:br" || el.name == "w:cr" {
        out.push(' ');
        return;
    }
    for child in &el.children {
        match child {
            XmlNode::Text(t) => out.push_str(t),
            XmlNode::Element(sub) => collect_element_text(sub, out),
            _ => {}
        }
    }
}

impl XmlDocument {
    pub fn parse(xml: &str) -> Result<Self, String> {
        let mut reader = Reader::from_str(xml);
        let mut decl: Option<(String, Option<String>, Option<String>)> = None;
        let mut prelude: Vec<XmlNode> = Vec::new();
        let mut stack: Vec<XmlElement> = Vec::new();
        let mut root: Option<XmlElement> = None;

        loop {
            match reader.read_event() {
                Ok(Event::Decl(d)) => {
                    let version = d
                        .version()
                        .ok()
                        .and_then(|v| String::from_utf8(v.into_owned()).ok())
                        .unwrap_or_else(|| "1.0".to_string());
                    let encoding = d
                        .encoding()
                        .and_then(|r| r.ok())
                        .and_then(|v| String::from_utf8(v.into_owned()).ok());
                    let standalone = d
                        .standalone()
                        .and_then(|r| r.ok())
                        .and_then(|v| String::from_utf8(v.into_owned()).ok());
                    decl = Some((version, encoding, standalone));
                }
                Ok(Event::Start(e)) => {
                    let el = parse_start_element(&reader, &e)?;
                    stack.push(el);
                }
                Ok(Event::Empty(e)) => {
                    let el = parse_start_element(&reader, &e)?;
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(XmlNode::Element(el));
                    } else if root.is_none() {
                        root = Some(el);
                    }
                }
                Ok(Event::End(_)) => {
                    if let Some(done) = stack.pop() {
                        if let Some(parent) = stack.last_mut() {
                            parent.children.push(XmlNode::Element(done));
                        } else if root.is_none() {
                            root = Some(done);
                        }
                    }
                }
                Ok(Event::Text(t)) => {
                    let decoded = t
                        .unescape()
                        .map_err(|e| format!("Lỗi giải mã XML text: {}", e))?
                        .into_owned();
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(XmlNode::Text(decoded));
                    } else if !decoded.trim().is_empty() {
                        prelude.push(XmlNode::Text(decoded));
                    }
                }
                Ok(Event::CData(c)) => {
                    let text = reader
                        .decoder()
                        .decode(&c)
                        .map_err(|e| format!("Lỗi giải mã CDATA: {}", e))?
                        .into_owned();
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(XmlNode::CData(text));
                    }
                }
                Ok(Event::Comment(c)) => {
                    let text = reader
                        .decoder()
                        .decode(&c)
                        .map_err(|e| format!("Lỗi giải mã Comment: {}", e))?
                        .into_owned();
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(XmlNode::Comment(text));
                    } else {
                        prelude.push(XmlNode::Comment(text));
                    }
                }
                Ok(Event::PI(pi)) => {
                    let text = reader
                        .decoder()
                        .decode(&pi)
                        .map_err(|e| format!("Lỗi giải mã PI: {}", e))?
                        .into_owned();
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(XmlNode::PI(text));
                    } else {
                        prelude.push(XmlNode::PI(text));
                    }
                }
                Ok(Event::DocType(dt)) => {
                    let text = reader
                        .decoder()
                        .decode(&dt)
                        .map_err(|e| format!("Lỗi giải mã DocType: {}", e))?
                        .into_owned();
                    prelude.push(XmlNode::DocType(text));
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(format!("Lỗi phân tích cú pháp XML: {}", e)),
            }
        }

        let root = root.ok_or_else(|| "Tài liệu XML không có phần tử gốc".to_string())?;
        Ok(Self {
            decl,
            prelude,
            root,
        })
    }

    pub fn to_xml_string(&self) -> Result<String, String> {
        let mut writer = Writer::new(Cursor::new(Vec::<u8>::new()));
        if let Some((ver, enc, standalone)) = &self.decl {
            let decl = BytesDecl::new(
                ver,
                enc.as_deref(),
                standalone.as_deref(),
            );
            writer
                .write_event(Event::Decl(decl))
                .map_err(|e| format!("Lỗi ghi XML Decl: {}", e))?;
        } else {
            let decl = BytesDecl::new("1.0", Some("UTF-8"), Some("yes"));
            writer
                .write_event(Event::Decl(decl))
                .map_err(|e| format!("Lỗi ghi XML Decl: {}", e))?;
        }

        for node in &self.prelude {
            write_node(&mut writer, node)?;
        }
        write_element(&mut writer, &self.root)?;

        let bytes = writer.into_inner().into_inner();
        String::from_utf8(bytes).map_err(|e| format!("Lỗi mã hóa UTF-8 XML: {}", e))
    }
}

fn parse_start_element(reader: &Reader<&[u8]>, e: &BytesStart<'_>) -> Result<XmlElement, String> {
    let name = reader
        .decoder()
        .decode(e.name().as_ref())
        .map_err(|err| format!("Lỗi đọc tên thẻ XML: {}", err))?
        .into_owned();

    let mut attrs = Vec::new();
    for attr_res in e.attributes() {
        let attr = attr_res.map_err(|err| format!("Lỗi đọc thuộc tính XML: {}", err))?;
        let key = reader
            .decoder()
            .decode(attr.key.as_ref())
            .map_err(|err| format!("Lỗi giải mã khóa thuộc tính: {}", err))?
            .into_owned();
        let val = attr
            .decode_and_unescape_value(reader.decoder())
            .map_err(|err| format!("Lỗi giải mã giá trị thuộc tính: {}", err))?
            .into_owned();
        attrs.push((key, val));
    }

    Ok(XmlElement {
        name,
        attrs,
        children: Vec::new(),
    })
}

fn write_node(writer: &mut Writer<Cursor<Vec<u8>>>, node: &XmlNode) -> Result<(), String> {
    match node {
        XmlNode::Element(el) => write_element(writer, el),
        XmlNode::Text(t) => writer
            .write_event(Event::Text(BytesText::new(t)))
            .map_err(|e| format!("Lỗi ghi XML Text: {}", e)),
        XmlNode::CData(c) => writer
            .write_event(Event::CData(quick_xml::events::BytesCData::new(c)))
            .map_err(|e| format!("Lỗi ghi CDATA: {}", e)),
        XmlNode::Comment(c) => writer
            .write_event(Event::Comment(BytesText::from_escaped(c.as_str())))
            .map_err(|e| format!("Lỗi ghi Comment: {}", e)),
        XmlNode::PI(p) => writer
            .write_event(Event::PI(quick_xml::events::BytesPI::new(p)))
            .map_err(|e| format!("Lỗi ghi PI: {}", e)),
        XmlNode::DocType(d) => writer
            .write_event(Event::DocType(BytesText::from_escaped(d.as_str())))
            .map_err(|e| format!("Lỗi ghi DocType: {}", e)),
    }
}

fn write_element(writer: &mut Writer<Cursor<Vec<u8>>>, el: &XmlElement) -> Result<(), String> {
    let mut start = BytesStart::new(el.name.as_str());
    for (k, v) in &el.attrs {
        start.push_attribute((k.as_str(), v.as_str()));
    }

    if el.children.is_empty() {
        writer
            .write_event(Event::Empty(start))
            .map_err(|e| format!("Lỗi ghi Empty XML '{}': {}", el.name, e))?;
    } else {
        writer
            .write_event(Event::Start(start))
            .map_err(|e| format!("Lỗi ghi Start XML '{}': {}", el.name, e))?;
        for child in &el.children {
            write_node(writer, child)?;
        }
        writer
            .write_event(Event::End(BytesEnd::new(el.name.as_str())))
            .map_err(|e| format!("Lỗi ghi End XML '{}': {}", el.name, e))?;
    }
    Ok(())
}

// OOXML Element Ordering Constants
pub const P_PR_ORDER: &[&str] = &[
    "w:pStyle",
    "w:keepNext",
    "w:keepLines",
    "w:pageBreakBefore",
    "w:framePr",
    "w:widowControl",
    "w:numPr",
    "w:suppressLineNumbers",
    "w:pBdr",
    "w:shd",
    "w:tabs",
    "w:suppressAutoHyphens",
    "w:kinsoku",
    "w:wordWrap",
    "w:overflowPunct",
    "w:topLinePunct",
    "w:autoSpaceDE",
    "w:autoSpaceDN",
    "w:bidi",
    "w:adjustRightInd",
    "w:snapToGrid",
    "w:spacing",
    "w:ind",
    "w:contextualSpacing",
    "w:mirrorIndents",
    "w:suppressOverlap",
    "w:jc",
    "w:textDirection",
    "w:textAlignment",
    "w:textboxTightWrap",
    "w:outlineLvl",
    "w:divId",
    "w:cnfStyle",
    "w:rPr",
    "w:sectPr",
    "w:pPrChange",
];

pub const R_PR_ORDER: &[&str] = &[
    "w:rStyle",
    "w:rFonts",
    "w:b",
    "w:bCs",
    "w:i",
    "w:iCs",
    "w:caps",
    "w:smallCaps",
    "w:strike",
    "w:dstrike",
    "w:outline",
    "w:shadow",
    "w:emboss",
    "w:imprint",
    "w:noProof",
    "w:snapToGrid",
    "w:vanish",
    "w:webHidden",
    "w:color",
    "w:spacing",
    "w:w",
    "w:kern",
    "w:position",
    "w:sz",
    "w:szCs",
    "w:highlight",
    "w:u",
    "w:effect",
    "w:bdr",
    "w:shd",
    "w:fitText",
    "w:vertAlign",
    "w:rtl",
    "w:cs",
    "w:em",
    "w:lang",
    "w:eastAsianLayout",
    "w:specVanish",
    "w:oMath",
];

pub const SECT_PR_ORDER: &[&str] = &[
    "w:headerReference",
    "w:footerReference",
    "w:footnotePr",
    "w:endnotePr",
    "w:type",
    "w:pgSz",
    "w:pgMar",
    "w:paperSrc",
    "w:pgBorders",
    "w:lnNumType",
    "w:pgNumType",
    "w:cols",
    "w:formProt",
    "w:vAlign",
    "w:noEndnote",
    "w:titlePg",
    "w:textDirection",
    "w:bidi",
    "w:rtlGutter",
    "w:docGrid",
    "w:printerSettings",
    "w:sectPrChange",
];

pub const TBL_PR_ORDER: &[&str] = &[
    "w:tblStyle",
    "w:tblpPr",
    "w:tblOverlap",
    "w:bidiVisual",
    "w:tblStyleRowBandSize",
    "w:tblStyleColBandSize",
    "w:tblW",
    "w:jc",
    "w:tblCellSpacing",
    "w:tblInd",
    "w:tblBorders",
    "w:shd",
    "w:tblLayout",
    "w:tblCellMar",
    "w:tblLook",
    "w:tblCaption",
    "w:tblDescription",
];

pub const TR_PR_ORDER: &[&str] = &[
    "w:cnfStyle",
    "w:divId",
    "w:gridBefore",
    "w:gridAfter",
    "w:wBefore",
    "w:wAfter",
    "w:cantSplit",
    "w:trHeight",
    "w:tblHeader",
    "w:tblCellSpacing",
    "w:jc",
    "w:hidden",
];

pub const TC_PR_ORDER: &[&str] = &[
    "w:cnfStyle",
    "w:tcW",
    "w:gridSpan",
    "w:hMerge",
    "w:vMerge",
    "w:tcBorders",
    "w:shd",
    "w:noWrap",
    "w:tcMar",
    "w:textDirection",
    "w:tcFitText",
    "w:vAlign",
    "w:hideMark",
    "w:headers",
    "w:cellIns",
    "w:cellDel",
    "w:cellMerge",
    "w:tcPrChange",
];

pub const TBL_CELL_MAR_ORDER: &[&str] = &[
    "w:top",
    "w:start",
    "w:left",
    "w:bottom",
    "w:end",
    "w:right",
];

// OOXML unit conversions
pub fn cm_to_twips(cm: f64) -> i32 {
    (cm * 566.929133858).round() as i32
}

pub fn pt_to_half_pts(pt: f64) -> u32 {
    (pt * 2.0).round() as u32
}

pub fn pt_to_twips(pt: f64) -> u32 {
    (pt * 20.0).round() as u32
}

pub fn line_to_twips(mult: f64) -> u32 {
    (mult * 240.0).round() as u32
}

pub fn set_run_font_and_size(r_pr: &mut XmlElement, font_name: &str, size_pt: Option<f64>) {
    let r_fonts = r_pr.ensure_child_ordered("w:rFonts", R_PR_ORDER);
    r_fonts.set_attr("w:ascii", font_name);
    r_fonts.set_attr("w:hAnsi", font_name);
    r_fonts.set_attr("w:cs", font_name);
    r_fonts.set_attr("w:eastAsia", font_name);
    r_fonts.remove_attr("w:asciiTheme");
    r_fonts.remove_attr("w:hAnsiTheme");
    r_fonts.remove_attr("w:cstheme");
    r_fonts.remove_attr("w:eastAsiaTheme");

    if let Some(pt) = size_pt {
        let half = pt_to_half_pts(pt).to_string();
        let sz = r_pr.ensure_child_ordered("w:sz", R_PR_ORDER);
        sz.set_attr("w:val", &half);
        let sz_cs = r_pr.ensure_child_ordered("w:szCs", R_PR_ORDER);
        sz_cs.set_attr("w:val", &half);
    }
}

pub fn ensure_wt_space_preserve(t_el: &mut XmlElement, text: &str) {
    if text.starts_with(' ') || text.ends_with(' ') || text.contains("  ") {
        t_el.set_attr("xml:space", "preserve");
    } else {
        t_el.remove_attr("xml:space");
    }
    t_el.set_text(text);
}
