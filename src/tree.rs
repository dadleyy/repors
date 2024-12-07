/// When we are cloning our repositories, we will clone them outside of where they will ultimately
/// live to avoid any internal repo nuances for `git2`. This type is used to make sure we can order
/// the "placement operations" in a way that makes sense and is simple.
#[derive(Debug, PartialEq, Clone)]
pub(crate) struct Location {
  /// The path that this location represents.
  pub(crate) root: std::path::PathBuf,
  /// The path where our content is actually stored.
  pub(crate) temp: std::path::PathBuf,
  /// A list of childrent that should be placed _after we are_.
  #[allow(clippy::vec_box)]
  pub(crate) children: Vec<Box<Location>>,
}

impl Location {
  /// Creates an empty node.
  pub(crate) fn root(root: std::path::PathBuf, temp: std::path::PathBuf) -> Self {
    Self {
      root,
      temp,
      children: Default::default(),
    }
  }

  /// Will attempt to update this node if the provided path is a child of us, or one of our
  /// children has some sort of relationship to that path.
  pub(crate) fn recognize(
    &mut self,
    other: std::path::PathBuf,
    temp: std::path::PathBuf,
  ) -> Option<std::path::PathBuf> {
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
  pub(crate) locations: Vec<Location>,
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
