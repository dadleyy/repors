use std::io;

/// This type represents a listing the manifest xml file.
#[derive(Debug)]
pub struct Source {
  /// The version of the layer we should use.
  #[allow(dead_code)]
  pub revision: String,
  /// The url/remote information about this layer.
  pub origin: String,
  #[allow(dead_code, missing_docs)]
  pub origin_ref: std::sync::Arc<OriginReference>,
  /// Where, relative to our destination we should store the layer once cloned.
  pub destination: String,
}

#[derive(Debug)]
pub struct OriginReference {
  pub(crate) origin: String,
  pub(crate) ssh: bool,
}

impl OriginReference {
  fn resolve_repo(&self, name: &str) -> io::Result<String> {
    if self.ssh {
      let original = url::Url::parse(&self.origin)
        .map_err(|error| io::Error::other(format!("unable to parse origin for ssh - {error}")))?;

      let host_name = original
        .host_str()
        .ok_or_else(|| io::Error::other(format!("Missing host from {:?}", self.origin)))?;

      let remote_path = original.path().trim_start_matches('/');

      let mapped = format!("git@{host_name}:{remote_path}/{name}");

      return Ok(mapped);
    }

    Ok(format!("{}/{name}", self.origin))
  }
}

/// This type represents what we will deserialize _from_ the manifest xml file.
#[derive(Debug)]
pub struct Manifest {
  #[allow(dead_code, clippy::missing_docs_in_private_items)]
  default_remote: Option<String>,
  #[allow(dead_code, clippy::missing_docs_in_private_items)]
  remotes: std::collections::HashMap<String, std::sync::Arc<OriginReference>>,
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

    let mut remotes = std::collections::HashMap::<String, std::sync::Arc<OriginReference>>::new();
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

              let (origin_ref, name) = remote
                .as_ref()
                .or(default_remote.as_ref())
                .and_then(|value| remotes.get(value))
                .zip(name)
                .ok_or_else(|| {
                  let error_message = format!("unable to find actual remote for '{boundary:?}'");
                  io::Error::new(io::ErrorKind::Other, error_message)
                })?;

              let fully_qualified_remote = origin_ref.resolve_repo(&name)?;

              if let Some((revision, destination)) = rev.zip(path) {
                sources.push(Source {
                  revision,
                  destination,
                  origin: fully_qualified_remote,
                  origin_ref: origin_ref.clone(),
                });
              }
            }
            b"default" => {
              default_remote = string_attr(&boundary, "remote");
            }
            b"remote" => {
              let name = string_attr(&boundary, "name");
              let origin = string_attr(&boundary, "fetch");

              let ssh = string_attr(&boundary, "ssh");

              log::debug!("Remote '{origin:?}' ssh - {ssh:?}");

              if let Some((name, origin)) = name.zip(origin) {
                remotes.insert(
                  name,
                  std::sync::Arc::new(OriginReference {
                    origin,
                    ssh: ssh.map(|v| v == "true").unwrap_or(false),
                  }),
                );
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
