use crate::templates::ToHtml;
use std::io::{self, Write};
use xmltree::Element;

#[derive(Debug, Queryable)]
pub struct Part {
    pub id: i32,
    pub no: Option<i16>,
    pub name: Option<String>,
}

impl Part {
    pub fn of(elem: &Element) -> Option<Self> {
        elem.get_child("part").map(|e| Part {
            id: 0, // unknown  TODO:  Should id be Option<i32> ?
            no: e.attributes.get("no").and_then(|n| n.parse().ok()),
            name: e.text.clone(),
        })
    }
}

impl ToHtml for Part {
    fn to_html(&self, out: &mut Write) -> io::Result<()> {
        if !(self.no.is_some() || self.name.is_some()) {
            return Ok(());
        }
        write!(out, "<span class='part'>")?;
        if let Some(no) = self.no {
            write!(out, "del {}", no)?;
            if self.name.is_some() {
                write!(out, ":")?;
            }
        }
        if let Some(ref name) = self.name {
            write!(out, "{}", name)?;
        }
        write!(out, "</span>")
    }
}
