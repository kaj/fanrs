extern crate xmltree;
#[macro_use]
extern crate failure;

use failure::Error;
use std::fmt;
use std::fs::File;
use xmltree::Element;

fn main() {
    for year in 1950..1959 {
        load_year(year).expect("Load data");
    }
}

fn load_year(year: isize) -> Result<(), Error> {
    let file = File::open(format!("/home/kaj/proj/fantomen/{}.data", year))?;
    let data = Element::parse(file)?;

    for i in data.children {
        if i.name == "info" {
            // ignore
        } else if i.name == "issue" {
            let nr = i
                .attributes
                .get("nr")
                .ok_or_else(|| format_err!("nr missing"))?;
            println!("Found issue {}/{}", nr, year);
            for c in i.children {
                if c.name == "omslag" {
                    let by = c.get_child("by").and_then(|e| e.text.as_ref());
                    let best = c.get_child("best").and_then(|e| {
                        e.attributes
                            .get("plac")
                            .and_then(|s| s.parse::<i16>().ok())
                    });
                    println!("  -> omslag {:?} {:?}", by, best);
                } else if c.name == "text" {
                    let title =
                        c.get_child("title").and_then(|e| e.text.as_ref());
                    let subtitle =
                        c.get_child("subtitle").and_then(|e| e.text.as_ref());
                    println!("  -> text {:?} {:?}", title, subtitle);
                } else if c.name == "serie" {
                    let title =
                        c.get_child("title").and_then(|e| e.text.as_ref());
                    let episode =
                        c.get_child("episode").and_then(|e| e.text.as_ref());
                    let part = Part::of(&c);
                    println!(
                        "  -> serie {:?} {:?} {}",
                        title,
                        episode,
                        part.map(|p| format!("({})", p))
                            .unwrap_or_else(|| "".to_string())
                    );
                } else if c.name == "skick" {
                    // ignore
                } else {
                    return Err(format_err!("Unexepcetd element: {:?}", c));
                }
            }
        }
        //println!("{:?}", c);
    }
    Ok(())
}

#[derive(Debug)]
struct Part {
    no: Option<u8>,
    name: Option<String>,
}

impl Part {
    fn of(e: &Element) -> Option<Self> {
        e.get_child("part").map(|e| Part {
            no: e.attributes.get("no").and_then(|n| n.parse::<u8>().ok()),
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
