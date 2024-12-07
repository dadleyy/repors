#![deny(clippy::missing_docs_in_private_items, missing_docs, dead_code, unused_mut)]

//! This library code for the `repors` crate has been extracted from the binary itself in the event
//! that it ever proves useful beyond this project.

/// This module holds types associated with our xml schema.
mod manifest;
pub use manifest::{Manifest, Source};

/// This module holds types related to our layer tree.
mod tree;

/// This module holds types associated with performing work.
mod execution;
pub use execution::WorkerPool;

#[cfg(test)]
mod tests {
  use super::{tree::Location, LayerTree};
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
