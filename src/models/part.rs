use std::fmt;
use xmltree::Element;

#[derive(Debug, Queryable)]
pub struct Part {
    pub no: Option<i16>,
    pub name: Option<String>,
}

impl Part {
    pub fn of(elem: &Element) -> Option<Self> {
        elem.get_child("part").map(|e| Part {
            no: e.attributes.get("no").and_then(|n| n.parse().ok()),
            name: e.text.clone(),
        })
    }
}

impl fmt::Display for Part {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        if let Some(no) = self.no {
            write!(out, "del {}", no)?;
            if self.name.is_some() {
                write!(out, ":")?;
            }
        }
        if let Some(ref name) = self.name {
            write!(out, "{}", name)?;
        }
        Ok(())
    }
}
