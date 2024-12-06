#![deny(clippy::missing_docs_in_private_items, missing_docs)]

//! This library code for the `repors` crate has been extracted from the binary itself in the event
//! that it ever proves useful beyond this project.

use std::io;

/// This type represents a listing the manifest xml file.
#[derive(Debug)]
pub struct Source {
  /// The version of the layer we should use.
  #[allow(dead_code)]
  pub revision: String,
  /// The url/remote information about this layer.
  pub origin: String,
  /// Where, relative to our destination we should store the layer once cloned.
  pub destination: String,
}

/// This type represents what we will deserialize _from_ the manifest xml file.
#[derive(Debug)]
pub struct Manifest {
  #[allow(dead_code, clippy::missing_docs_in_private_items)]
  default_remote: Option<String>,
  #[allow(dead_code, clippy::missing_docs_in_private_items)]
  remotes: std::collections::HashMap<String, String>,
  /// The parsed list of layers.
  pub sources: Vec<Source>,
}

/// This method is used to handle grabbing a string from an element in our `quick_xml` parsing.
fn string_attr<B>(boundary: &quick_xml::events::BytesStart<'_>, key: B) -> Option<String>
where
  B: AsRef<[u8]>,
{
  boundary
    .attributes()
    .flatten()
    .find(|att| att.key.as_ref() == key.as_ref())
    .and_then(|origin_att| {
      let parsed_str = std::str::from_utf8(origin_att.value.as_ref());
      parsed_str.map(|inner| inner.to_string()).ok()
    })
}

impl Manifest {
  /// This method will attempt to create a `Manifest` from some type that implements `io::Read`.
  pub fn from_reader<R>(reader: R) -> io::Result<Self>
  where
    R: io::Read + io::BufRead,
  {
    let mut xml_reader = quick_xml::Reader::from_reader(reader);
    let mut buffer = Vec::default();

    let mut remotes = std::collections::HashMap::default();
    let mut sources = Vec::default();
    let mut default_remote = None;

    loop {
      let event = xml_reader
        .read_event_into(&mut buffer)
        .map_err(|error| io::Error::new(io::ErrorKind::Other, format!("xml parsing error: {error:?}")))?;

      match event {
        quick_xml::events::Event::Eof => break,
        quick_xml::events::Event::Empty(boundary) => {
          let name = boundary.name();
          match name.as_ref() {
            b"project" => {
              let name = string_attr(&boundary, "name");
              let path = string_attr(&boundary, "path");
              let rev = string_attr(&boundary, "revision");
              let remote = string_attr(&boundary, "remote");
              let fully_qualified_remote = remote
                .as_ref()
                .or(default_remote.as_ref())
                .and_then(|value| remotes.get(value))
                .zip(name)
                .map(|(origin, name)| format!("{origin}/{name}"))
                .ok_or_else(|| {
                  let error_message = format!("unable to find actual remote for '{boundary:?}'");
                  io::Error::new(io::ErrorKind::Other, error_message)
                })?;

              if let Some((revision, destination)) = rev.zip(path) {
                sources.push(Source {
                  revision,
                  destination,
                  origin: fully_qualified_remote,
                });
              }
            }
            b"default" => {
              default_remote = string_attr(&boundary, "remote");
            }
            b"remote" => {
              let name = string_attr(&boundary, "name");
              let origin = string_attr(&boundary, "fetch");
              if let Some((name, origin)) = name.zip(origin) {
                remotes.insert(name, origin);
              }
            }
            _ => (),
          }
        }
        _ => (),
      }
    }

    Ok(Self {
      remotes,
      sources,
      default_remote,
    })
  }
}

/// When we are cloning our repositories, we will clone them outside of where they will ultimately
/// live to avoid any internal repo nuances for `git2`. This type is used to make sure we can order
/// the "placement operations" in a way that makes sense and is simple.
#[derive(Debug, PartialEq, Clone)]
struct Location {
  /// The path that this location represents.
  root: std::path::PathBuf,
  /// The path where our content is actually stored.
  temp: std::path::PathBuf,
  /// A list of childrent that should be placed _after we are_.
  #[allow(clippy::vec_box)]
  children: Vec<Box<Location>>,
}

impl Location {
  /// Creates an empty node.
  fn root(root: std::path::PathBuf, temp: std::path::PathBuf) -> Self {
    Self {
      root,
      temp,
      children: Default::default(),
    }
  }

  /// Will attempt to update this node if the provided path is a child of us, or one of our
  /// children has some sort of relationship to that path.
  fn recognize(&mut self, other: std::path::PathBuf, temp: std::path::PathBuf) -> Option<std::path::PathBuf> {
    for child in self.children.iter_mut() {
      child.recognize(other.clone(), temp.clone())?;
    }

    for ancestor in other.ancestors() {
      if ancestor == self.root {
        let new_child = Self::root(other, temp);
        self.children.push(Box::new(new_child));
        return None;
      }
    }

    Some(other)
  }
}

/// During the "placement phase" of the `repors` cloning process, we use this type to order the
/// operations such that we do not have any conflicts with attempting to rename a directory to one
/// that was already created for a child.
#[derive(Default)]
pub struct LayerTree {
  /// The list of locations, which is effectively a graph-like representation of filesystem paths.
  locations: Vec<Location>,
}

impl LayerTree {
  /// Attempts to place the path into our tree.
  pub fn add(&mut self, path: std::path::PathBuf, temp: std::path::PathBuf) {
    if self.locations.is_empty() {
      self.locations.push(Location::root(path, temp));
      return;
    }

    let mut remainder = None;
    for loc in self.locations.iter_mut() {
      let rem = loc.recognize(path.clone(), temp.clone());
      if rem.is_none() {
        return;
      }
      remainder = rem;
    }

    if let Some(rem) = remainder {
      self.locations.push(Location::root(rem, temp));
    }
  }

  /// Returns the correctly-ordered list of filesystem path pairs that can be iterated over for
  /// placement.
  pub fn consume(self) -> Vec<(std::path::PathBuf, std::path::PathBuf)> {
    let mut out = Vec::default();
    let mut queue = self.locations;

    while !queue.is_empty() {
      let Some(mut current) = queue.pop() else {
        break;
      };

      if current.children.is_empty() {
        out.push((current.root, current.temp));
        continue;
      }

      for child in current.children.drain(0..) {
        queue.push(*child);
      }

      queue.push(current);
    }

    out
  }
}

#[cfg(test)]
mod tests {
  use super::{LayerTree, Location};
  use std::io;

  const FIXTURE: &[u8] = include_bytes!("../test/fixtures/default.xml");

  #[test]
  fn location_recognize_child() {
    let temp = std::path::PathBuf::from("");
    let mut loc = Location {
      root: std::path::PathBuf::from("/test/deep/nest"),
      temp: temp.clone(),
      children: vec![],
    };
    let other = std::path::PathBuf::from("/test/deep/nest/foo");

    let remainder = loc.recognize(other.clone(), temp.clone());

    assert_eq!(
      loc,
      Location {
        root: loc.root.clone(),
        temp: temp.clone(),
        children: vec![Box::new(Location::root(other.clone(), temp.clone()))]
      }
    );

    assert_eq!(remainder, None)
  }

  #[test]
  fn location_recognize_grandchild() {
    let temp = std::path::PathBuf::from("");
    let mut loc = Location {
      root: std::path::PathBuf::from("/test/deep/nest"),
      temp: temp.clone(),
      children: vec![],
    };
    let first = std::path::PathBuf::from("/test/deep/nest/foo");
    let remainder = loc.recognize(first.clone(), temp.clone());

    assert_eq!(
      loc,
      Location {
        root: loc.root.clone(),
        temp: temp.clone(),
        children: vec![Box::new(Location::root(first.clone(), temp.clone()))]
      }
    );
    assert_eq!(remainder, None);

    let second = std::path::PathBuf::from("/test/deep/nest/foo/bar");
    let remainder = loc.recognize(second.clone(), temp.clone());

    assert_eq!(
      loc,
      Location {
        root: loc.root.clone(),
        temp: temp.clone(),
        children: vec![Box::new(Location {
          root: first.clone(),
          temp: temp.clone(),
          children: vec![Box::new(Location {
            root: second.clone(),
            temp: temp.clone(),
            children: vec![],
          })],
        })]
      }
    );
    assert_eq!(remainder, None);
  }

  #[test]
  fn location_recognize_great_grandchild_sibling() {
    let temp = std::path::PathBuf::from("");
    let mut loc = Location {
      root: std::path::PathBuf::from("/test/deep/nest"),
      temp: temp.clone(),
      children: vec![],
    };
    let first = std::path::PathBuf::from("/test/deep/nest/foo");
    let remainder = loc.recognize(first.clone(), temp.clone());

    assert_eq!(
      loc,
      Location {
        temp: temp.clone(),
        root: loc.root.clone(),
        children: vec![Box::new(Location::root(first.clone(), temp.clone()))]
      }
    );
    assert_eq!(remainder, None);

    let second = std::path::PathBuf::from("/test/deep/nest/foo/bar");
    let remainder = loc.recognize(second.clone(), temp.clone());

    assert_eq!(
      loc,
      Location {
        temp: temp.clone(),
        root: loc.root.clone(),
        children: vec![Box::new(Location {
          root: first.clone(),
          temp: temp.clone(),
          children: vec![Box::new(Location {
            root: second.clone(),
            temp: temp.clone(),
            children: vec![],
          })],
        })]
      }
    );
    assert_eq!(remainder, None);

    let third = std::path::PathBuf::from("/test/deep/nest/foo/bar/baz");
    let remainder = loc.recognize(third.clone(), temp.clone());

    assert_eq!(
      loc,
      Location {
        root: loc.root.clone(),
        temp: temp.clone(),
        children: vec![Box::new(Location {
          root: first.clone(),
          temp: temp.clone(),
          children: vec![Box::new(Location {
            root: second.clone(),
            temp: temp.clone(),
            children: vec![Box::new(Location {
              root: third.clone(),
              temp: temp.clone(),
              children: vec![],
            })],
          })],
        })]
      }
    );
    assert_eq!(remainder, None);

    let fourth = std::path::PathBuf::from("/test/deep/nest/foo/bar/bud");
    let remainder = loc.recognize(fourth.clone(), temp.clone());

    assert_eq!(
      loc,
      Location {
        root: loc.root.clone(),
        temp: temp.clone(),
        children: vec![Box::new(Location {
          root: first.clone(),
          temp: temp.clone(),
          children: vec![Box::new(Location {
            root: second.clone(),
            temp: temp.clone(),
            children: vec![
              Box::new(Location {
                root: third.clone(),
                temp: temp.clone(),
                children: vec![],
              }),
              Box::new(Location {
                root: fourth.clone(),
                temp: temp.clone(),
                children: vec![],
              })
            ],
          })],
        })]
      }
    );
    assert_eq!(remainder, None);
  }

  #[test]
  fn location_recognize_great_grandchild() {
    let temp = std::path::PathBuf::from("");
    let mut loc = Location {
      root: std::path::PathBuf::from("/test/deep/nest"),
      temp: temp.clone(),
      children: vec![],
    };
    let first = std::path::PathBuf::from("/test/deep/nest/foo");
    let remainder = loc.recognize(first.clone(), temp.clone());

    assert_eq!(
      loc,
      Location {
        root: loc.root.clone(),
        temp: temp.clone(),
        children: vec![Box::new(Location::root(first.clone(), temp.clone()))]
      }
    );
    assert_eq!(remainder, None);

    let second = std::path::PathBuf::from("/test/deep/nest/foo/bar");
    let remainder = loc.recognize(second.clone(), temp.clone());

    assert_eq!(
      loc,
      Location {
        root: loc.root.clone(),
        temp: temp.clone(),
        children: vec![Box::new(Location {
          root: first.clone(),
          temp: temp.clone(),
          children: vec![Box::new(Location {
            root: second.clone(),
            temp: temp.clone(),
            children: vec![],
          })],
        })]
      }
    );
    assert_eq!(remainder, None);

    let third = std::path::PathBuf::from("/test/deep/nest/foo/bar/baz");
    let remainder = loc.recognize(third.clone(), temp.clone());

    assert_eq!(
      loc,
      Location {
        root: loc.root.clone(),
        temp: temp.clone(),
        children: vec![Box::new(Location {
          root: first.clone(),
          temp: temp.clone(),
          children: vec![Box::new(Location {
            root: second.clone(),
            temp: temp.clone(),
            children: vec![Box::new(Location {
              root: third.clone(),
              temp: temp.clone(),
              children: vec![],
            })],
          })],
        })]
      }
    );
    assert_eq!(remainder, None);
  }

  #[test]
  #[ignore]
  fn location_recognize() {
    let temp = std::path::PathBuf::from("");
    let mut loc = Location {
      root: std::path::PathBuf::from("/test/deep/nest"),
      temp: temp.clone(),
      children: vec![],
    };
    let other = std::path::PathBuf::from("/test/deep/other");

    let original = loc.clone();
    let remainder = loc.recognize(other.clone(), temp.clone());

    assert_eq!(original, loc);
    assert_eq!(remainder, Some(other.clone()));
  }

  #[test]
  fn location_tree() {
    let temp = std::path::PathBuf::from("");
    let mut tree = LayerTree::default();
    tree.add(std::path::PathBuf::from("/test/deep/nest"), temp.clone());
    tree.add(std::path::PathBuf::from("/test/deep/other"), temp.clone());
    assert_eq!(
      tree.locations,
      vec![
        Location::root(std::path::PathBuf::from("/test/deep/nest"), temp.clone()),
        Location::root(std::path::PathBuf::from("/test/deep/other"), temp.clone()),
      ]
    );
  }

  #[test]
  fn location_tree_children() {
    let temp = std::path::PathBuf::from("");
    let mut tree = LayerTree::default();
    tree.add(std::path::PathBuf::from("/test/deep/nest"), temp.clone());
    tree.add(std::path::PathBuf::from("/test/deep/nest/foo"), temp.clone());
    let mut expected = Location::root(std::path::PathBuf::from("/test/deep/nest"), temp.clone());
    assert!(expected
      .recognize(std::path::PathBuf::from("/test/deep/nest/foo"), temp.clone())
      .is_none());
    assert_eq!(tree.locations, vec![expected]);
  }

  #[test]
  fn location_tree_children_ordering() {
    let temp = std::path::PathBuf::from("");
    let mut tree = LayerTree::default();
    tree.add(std::path::PathBuf::from("/test/deep/nest"), temp.clone());
    tree.add(std::path::PathBuf::from("/test/deep/nest/foo"), temp.clone());
    let mut expected = Location::root(std::path::PathBuf::from("/test/deep/nest"), temp.clone());
    assert!(expected
      .recognize(std::path::PathBuf::from("/test/deep/nest/foo"), temp.clone())
      .is_none());

    assert_eq!(
      tree.consume(),
      vec![
        (std::path::PathBuf::from("/test/deep/nest"), temp.clone()),
        (std::path::PathBuf::from("/test/deep/nest/foo"), temp.clone()),
      ]
    )
  }

  #[test]
  fn location_tree_children_ordering_sibling() {
    let temp = std::path::PathBuf::from("");
    let mut tree = LayerTree::default();
    tree.add(std::path::PathBuf::from("/test/deep/nest"), temp.clone());
    tree.add(std::path::PathBuf::from("/test/deep/nest/foo"), temp.clone());
    tree.add(std::path::PathBuf::from("/test/deep/nest/other"), temp.clone());

    assert_eq!(
      tree.consume(),
      vec![
        (std::path::PathBuf::from("/test/deep/nest"), temp.clone()),
        (std::path::PathBuf::from("/test/deep/nest/other"), temp.clone()),
        (std::path::PathBuf::from("/test/deep/nest/foo"), temp.clone()),
      ]
    )
  }

  #[test]
  fn location_tree_children_ordering_grandchild_sibling() {
    let mut tree = LayerTree::default();
    tree.add(
      std::path::PathBuf::from("/test/deep/nest"),
      std::path::PathBuf::from(""),
    );
    tree.add(
      std::path::PathBuf::from("/test/deep/nest/foo"),
      std::path::PathBuf::from(""),
    );
    tree.add(
      std::path::PathBuf::from("/test/deep/nest/foo/bar"),
      std::path::PathBuf::from(""),
    );
    tree.add(
      std::path::PathBuf::from("/test/deep/nest/foo/baz"),
      std::path::PathBuf::from(""),
    );

    assert_eq!(
      tree.consume(),
      vec![
        (
          std::path::PathBuf::from("/test/deep/nest"),
          std::path::PathBuf::from("")
        ),
        (
          std::path::PathBuf::from("/test/deep/nest/foo"),
          std::path::PathBuf::from("")
        ),
        (
          std::path::PathBuf::from("/test/deep/nest/foo/baz"),
          std::path::PathBuf::from("")
        ),
        (
          std::path::PathBuf::from("/test/deep/nest/foo/bar"),
          std::path::PathBuf::from("")
        ),
      ]
    )
  }

  #[test]
  #[ignore]
  fn location_ordering_roots() {
    let temp = std::path::PathBuf::from("");
    let mut tree = LayerTree::default();
    tree.add(std::path::PathBuf::from("/test/deep/nest"), temp.clone());
    tree.add(std::path::PathBuf::from("/test/deep/other"), temp.clone());
    assert_eq!(
      tree.consume(),
      vec![
        (std::path::PathBuf::from("/test/deep/other"), temp.clone()),
        (std::path::PathBuf::from("/test/deep/nest"), temp.clone()),
      ]
    );
  }

  use super::Manifest;

  #[test]
  fn it_works() {
    let mut cursor = io::Cursor::new(FIXTURE);
    let manifest = Manifest::from_reader(cursor);
    println!("{manifest:?}");
  }
}
