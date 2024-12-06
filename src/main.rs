use std::io;

use clap::Parser;

#[derive(clap::Subcommand, Clone, Debug)]
enum Subcommand {
  Execute {
    #[clap(long, short)]
    manifest: String,
    #[clap(long, short)]
    destination: Option<String>,
    #[clap(long, short = 'x', default_value = "false")]
    overwrite: bool,
  },
}

#[derive(Parser)]
struct CommandLine {
  #[clap(subcommand)]
  subcommand: Subcommand,
}

fn main() -> io::Result<()> {
  let _ = env_logger::init();
  let cli = CommandLine::parse();

  match cli.subcommand {
    Subcommand::Execute {
      manifest,
      destination,
      overwrite,
    } => {
      log::debug!("attempting to do repo stuff against manifest '{manifest}'");
      let bytes = std::fs::read(&manifest)?;
      let mut cursor = std::io::Cursor::new(&bytes);
      let manifest = Manifest::from_reader(cursor)?;
      log::debug!("manifest loaded - {manifest:?}");
      let destination = destination
        .or(std::env::current_dir()?.to_str().map(str::to_string))
        .ok_or_else(|| {
          io::Error::new(
            io::ErrorKind::Other,
            "unable to determine a destination directory for execution",
          )
        })?;

      match (overwrite, std::fs::metadata(&destination)) {
        (_, Err(_)) => (),
        (false, Ok(_)) => {
          let message = format!("'{destination}' already exists, must provide -x to allow overwrite");
          return Err(std::io::Error::new(std::io::ErrorKind::Other, message));
        }
        (true, Ok(_)) => {
          std::fs::remove_dir_all(&destination)?;
        }
      }

      std::fs::create_dir_all(&destination)?;

      let destination_path = std::path::PathBuf::from(destination);

      for source in &manifest.sources {
        let mut source_path = destination_path.clone();
        source_path.push(&source.destination);
        log::debug!("starting to clone '{source:?}' into '{source_path:?}'");
        let mut repo = git2::Repository::init_bare(&source_path)
          .map_err(|error| io::Error::new(io::ErrorKind::Other, error.to_string()))?;
        let mut remote = repo
          .remote("origin", &source.destination)
          .map_err(|error| io::Error::new(io::ErrorKind::Other, error.to_string()))?;
        let specs = remote
          .fetch(&[&source.revision], None, None)
          .map_err(|error| io::Error::new(io::ErrorKind::Other, error.to_string()))?;

        // log::debug!("remote refspecs: {:?}", remote.fetch_refspecs().map(|list| list));
      }
    }
  }

  Ok(())
}

#[derive(Debug)]
struct Source {
  revision: String,
  origin: String,
  destination: String,
}

#[derive(Debug)]
struct Manifest {
  default_remote: Option<String>,
  remotes: std::collections::HashMap<String, String>,
  sources: Vec<Source>,
}

fn string_attr<'a, B>(boundary: &quick_xml::events::BytesStart<'a>, key: B) -> Option<String>
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
  fn from_reader<R>(reader: R) -> io::Result<Self>
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

#[cfg(test)]
mod tests {
  use super::Manifest;
  use std::io;

  const FIXTURE: &[u8] = include_bytes!("../test/fixtures/default.xml");

  #[test]
  fn it_works() {
    let mut cursor = io::Cursor::new(FIXTURE);
    let manifest = Manifest::from_reader(cursor);
    println!("{manifest:?}");
  }
}
