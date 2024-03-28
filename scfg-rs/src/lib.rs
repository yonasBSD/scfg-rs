//! # scfg-rs
//! A rust library for parsing [scfg] files. Scfg is a simple line oriented
//! configuration file format. Every line may contain at most one directive per
//! line. A directive consists of a name, followed by optional parameters
//! separated by whitespace followed by an optional child block, delimited by
//! `{` and `}`. Whitespace at the beginning of lines is insignificant. Lines
//! beginning with `#` are comments and are ignored.
//!
//! [scfg]: https://git.sr.ht/~emersion/scfg
//!
//! ## Examples
//! ```
//! # use scfg::*;
//! // an scfg document
//! static SCFG_DOC: &str = r#"train "Shinkansen" {
//!     model "E5" {
//!         max-speed 320km/h
//!         weight 453.5t
//!
//!         lines-served "Tōhoku" "Hokkaido"
//!     }
//!
//!     model "E7" {
//!         max-speed 275km/h
//!         weight 540t
//!
//!         lines-served "Hokuriku" "Jōetsu"
//!     }
//! }"#;
//! let doc = SCFG_DOC.parse::<Scfg>().expect("invalid document");
//!
//! // the above document can also be created with this builder style api
//! let mut scfg = Scfg::new();
//! let train = scfg
//!     .add("train")
//!     .append_param("Shinkansen")
//!     .get_or_create_child();
//! let e5 = train.add("model").append_param("E5").get_or_create_child();
//! e5.add("max-speed").append_param("320km/h");
//! e5.add("weight").append_param("453.5t");
//! e5.add("lines-served")
//!     .append_param("Tōhoku")
//!     .append_param("Hokkaido");
//! let e7 = train.add("model").append_param("E7").get_or_create_child();
//! e7.add("max-speed").append_param("275km/h");
//! e7.add("weight").append_param("540t");
//! e7.add("lines-served")
//!     .append_param("Hokuriku")
//!     .append_param("Jōetsu");
//!
//! assert_eq!(doc, scfg);
//! ```
use std::{borrow::Borrow, hash::Hash, io, str::FromStr};

#[cfg(feature = "preserve_order")]
use indexmap::IndexMap;
#[cfg(not(feature = "preserve_order"))]
use std::collections::BTreeMap;

mod parser;

pub type ParseError = parser::Error;

/// An scfg document. Implemented as a multimap.
///
/// If the `preserve_order` feature is enabled, the directive names will be kept
/// in the order of their first appearance.  Otherwise, they will be sorted by name.
#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct Scfg {
    directives: Map<String, Vec<Directive>>,
}

#[cfg(not(feature = "preserve_order"))]
type Map<K, V> = BTreeMap<K, V>;
#[cfg(feature = "preserve_order")]
type Map<K, V> = IndexMap<K, V>;

impl Scfg {
    /// Creates a new empty document
    pub fn new() -> Self {
        Default::default()
    }

    /// Retrieves the first directive with a particular name.
    ///
    /// This will return `None` if either, the name is not found, or if the name
    /// somehow has no directives.
    pub fn get<Q>(&self, name: &Q) -> Option<&Directive>
    where
        String: Borrow<Q>,
        Q: Ord + Eq + Hash + ?Sized,
    {
        self.directives.get(name).and_then(|d| d.first())
    }

    /// Retrieves the all directives with a particular name.
    pub fn get_all<Q>(&self, name: &Q) -> Option<&[Directive]>
    where
        String: Borrow<Q>,
        Q: Ord + Eq + Hash + ?Sized,
    {
        self.directives.get(name).map(|ds| ds.as_ref())
    }

    /// Retrieves a mutable reference to all directives with a particular name.
    pub fn get_all_mut<Q>(&mut self, name: &Q) -> Option<&mut Vec<Directive>>
    where
        String: Borrow<Q>,
        Q: Ord + Eq + Hash + ?Sized,
    {
        self.directives.get_mut(name)
    }

    /// Does the document contain a directive with `name`.
    ///
    /// ```
    /// # use scfg::*;
    /// let mut scfg = Scfg::new();
    /// scfg.add("foo");
    /// assert!(scfg.contains("foo"));
    /// assert!(!scfg.contains("bar"));
    /// ```
    pub fn contains<Q>(&self, name: &Q) -> bool
    where
        String: Borrow<Q>,
        Q: Ord + Eq + Hash + ?Sized,
    {
        self.directives.contains_key(name)
    }

    /// Adds a new name returning the new (empty) directive.
    /// ```
    /// # use scfg::*;
    /// let mut scfg = Scfg::new();
    /// let dir = scfg.add("dir1");
    /// assert_eq!(*dir, Directive::default());
    /// ```
    ///
    /// # Note
    /// This does not validate that `name` is a legal scfg word. It is possible to create
    /// unparsable documents should `name` contain control characters or newlines.
    pub fn add(&mut self, name: impl Into<String>) -> &mut Directive {
        self.add_directive(name, Directive::default())
    }

    fn add_directive(&mut self, name: impl Into<String>, directive: Directive) -> &mut Directive {
        let entry = self.directives.entry(name.into()).or_insert_with(Vec::new);
        entry.push(directive);
        entry.last_mut().unwrap()
    }

    /// Removes all directives with the supplied name, returning them.
    pub fn remove<Q>(&mut self, name: &Q) -> Option<Vec<Directive>>
    where
        String: Borrow<Q>,
        Q: Ord + Eq + Hash + ?Sized,
    {
        self.directives.remove(name)
    }

    /// Removes all directives with the supplied name, returning them, and their
    /// key.
    pub fn remove_entry<Q>(&mut self, name: &Q) -> Option<(String, Vec<Directive>)>
    where
        String: Borrow<Q>,
        Q: Ord + Eq + Hash + ?Sized,
    {
        self.directives.remove_entry(name)
    }

    /// Writes the document to the specified writer. If efficiency is a concern,
    /// it may be best to wrap the writer in a [`BufWriter`] first. This will
    /// not write any comments that the document had if it was parsed first.
    ///
    /// [`BufWriter`]: std::io::BufWriter
    pub fn write<W>(&self, writer: &mut W) -> io::Result<()>
    where
        W: io::Write,
    {
        self.write_with_indent(0, writer)
    }

    fn write_with_indent<W>(&self, indent: usize, wtr: &mut W) -> io::Result<()>
    where
        W: io::Write,
    {
        let mut prefix = "";
        for (name, directives) in &self.directives {
            for directive in directives {
                wtr.write_all(prefix.as_ref())?;
                prefix = "";
                for _ in 0..indent {
                    write!(wtr, "\t")?;
                }
                write!(wtr, "{}", shell_words::quote(&name))?;
                for param in &directive.params {
                    write!(wtr, " {}", shell_words::quote(&param))?;
                }

                if let Some(ref child) = directive.child {
                    wtr.write_all(b" {\n")?;
                    child.write_with_indent(indent + 1, wtr)?;
                    for _ in 0..indent {
                        wtr.write_all(b"\t")?;
                    }
                    wtr.write_all(b"}")?;
                    prefix = "\n";
                }
                wtr.write_all(b"\n")?;
            }
        }

        Ok(())
    }
}

impl FromStr for Scfg {
    type Err = ParseError;
    fn from_str(src: &str) -> Result<Self, Self::Err> {
        let r = std::io::Cursor::new(src.as_bytes());
        parser::document(r)
    }
}

impl<K: Into<String>> std::iter::FromIterator<(K, Directive)> for Scfg {
    fn from_iter<T>(it: T) -> Self
    where
        T: IntoIterator<Item = (K, Directive)>,
    {
        let mut scfg = Self::default();

        for (name, directive) in it {
            let name = name.into();
            scfg.directives
                .entry(name)
                .or_insert_with(Vec::new)
                .push(directive);
        }

        scfg
    }
}

/// A single scfg directive, containing any number of parameters, and possibly
/// one child block.
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct Directive {
    params: Vec<String>,
    child: Option<Scfg>,
}

impl Directive {
    /// Creates a new empty directive.
    pub fn new() -> Self {
        Default::default()
    }

    /// Get this directive's parameters
    pub fn params(&self) -> &[String] {
        &self.params
    }

    /// Appends the supplied parameter. Returns `&mut self` to support method
    /// chaining.
    ///
    /// # Note
    /// This does not validate that `param` is a legal scfg word. It is possible to create
    /// unparsable documents should `param` contain control characters or newlines.
    pub fn append_param(&mut self, param: impl Into<String>) -> &mut Self {
        self.params.push(param.into());
        self
    }

    /// Clears all parameters from this directive.
    pub fn clear_params(&mut self) {
        self.params.clear();
    }

    /// Get this directive's child, if there is one.
    pub fn child(&self) -> Option<&Scfg> {
        self.child.as_ref()
    }

    /// Takes this directive's child, leaving it with `None`.
    pub fn take_child(&mut self) -> Option<Scfg> {
        self.child.take()
    }

    /// Returns the child, optionally creating it if it does not exist.
    ///
    /// ```
    /// # use scfg::*;
    /// let mut directive = Directive::new();
    /// assert!(directive.child().is_none());
    /// directive.get_or_create_child();
    /// assert!(directive.child().is_some());
    /// ```
    pub fn get_or_create_child(&mut self) -> &mut Scfg {
        self.child.get_or_insert_with(Scfg::new)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    type Result = std::result::Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn flat() -> Result {
        let src = r#"dir1 param1 param2 param3
dir2
dir3 param1

# comment
dir4 "param 1" 'param 2'
"#;
        let cfg = Scfg::from_str(src)?;
        // this tests the fromiter impl
        // builder type api is generally a little cleaner
        let exp = vec![
            (
                "dir1",
                Directive {
                    params: vec!["param1".into(), "param2".into(), "param3".into()],
                    child: None,
                },
            ),
            (
                "dir2",
                Directive {
                    params: vec![],
                    child: None,
                },
            ),
            (
                "dir3",
                Directive {
                    params: vec!["param1".into()],
                    child: None,
                },
            ),
            (
                "dir4",
                Directive {
                    params: vec!["param 1".into(), "param 2".into()],
                    child: None,
                },
            ),
        ]
        .into_iter()
        .collect::<Scfg>();
        assert_eq!(cfg, exp);

        Ok(())
    }

    #[test]
    fn simple_blocks() -> Result {
        let src = r#"block1 {
    dir1 param1 param2
    dir2 param1
}

block2 {
}

block3 {
    # comment
}

block4 param1 "param2" {
    dir1
}"#;
        let cfg = Scfg::from_str(src)?;
        let mut exp = Scfg::new();
        let block1 = exp.add("block1");
        let block = block1.get_or_create_child();
        block
            .add("dir1")
            .append_param("param1")
            .append_param("param2");
        block.add("dir2").append_param("param1");
        exp.add("block2").get_or_create_child();
        exp.add("block3").get_or_create_child();
        exp.add("block4")
            .append_param("param1")
            .append_param("param2")
            .get_or_create_child()
            .add("dir1");

        assert_eq!(cfg, exp);
        Ok(())
    }

    #[test]
    fn nested() -> Result {
        let src = r#"block1 {
    block2 {
        dir1 param1
    }

    block3 {
    }
}

block4 {
    block5 {
        block6 param1 {
            dir1
        }
    }

    dir1
}"#;
        let cfg = Scfg::from_str(src)?;
        let mut exp = Scfg::new();
        let block1 = exp.add("block1").get_or_create_child();
        block1
            .add("block2")
            .get_or_create_child()
            .add("dir1")
            .append_param("param1");
        block1.add("block3").get_or_create_child();
        let block4 = exp.add("block4").get_or_create_child();
        block4
            .add("block5")
            .get_or_create_child()
            .add("block6")
            .append_param("param1")
            .get_or_create_child()
            .add("dir1");
        block4.add("dir1");

        assert_eq!(cfg, exp);

        Ok(())
    }

    #[test]
    fn write() -> Result {
        let src = r#"dir1 param1 param2 param3
dir2
dir3 param1

# comment
dir4 "param 1" 'param 2'
"#;
        let doc = Scfg::from_str(src)?;
        let mut out = Vec::new();
        doc.write(&mut out)?;
        let exp = r#"dir1 param1 param2 param3
dir2
dir3 param1
dir4 'param 1' 'param 2'
"#;
        assert_eq!(std::str::from_utf8(&out)?, exp);
        Ok(())
    }

    #[test]
    fn write_block() -> Result {
        let src = r#"block1 {
	dir1 param1 param2
	dir2 param1
}

block2 {
}

block3 {
	# comment
}

block4 param1 "param2" {
	dir1
}"#;
        let doc = Scfg::from_str(src)?;
        let mut out = Vec::new();
        doc.write(&mut out)?;
        let exp = r#"block1 {
	dir1 param1 param2
	dir2 param1
}

block2 {
}

block3 {
}

block4 param1 param2 {
	dir1
}
"#;
        assert_eq!(std::str::from_utf8(&out)?, exp);
        Ok(())
    }
}
