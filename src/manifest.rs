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
