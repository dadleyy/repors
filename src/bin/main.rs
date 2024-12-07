#![deny(clippy::missing_docs_in_private_items, missing_docs)]

//! This command line tool is pure-rust implementation of the `repo` tool that is used in the
//! openembedded community for layer management during the production of linux images.

use std::io;

use clap::Parser;

/// We want our  command line interface is split across subcommands so we can add more
/// functionality in the future.
#[derive(clap::Subcommand, Clone, Debug)]
enum Subcommand {
  /// This command will actually perform the git cloning of all the repositories listed in a
  /// manifest xml file.
  Execute {
    /// The number of threads to spawn for handling the cloning process.
    #[clap(long, default_value = "3")]
    threads: usize,
    /// The location (filesystem path) of our xml manifest file.
    #[clap(long, short)]
    manifest: String,
    /// The filesystem location we will consider as the root of our operation, where the `path`
    /// values from the manifest will be relative to.
    #[clap(long, short)]
    destination: Option<String>,
    /// When true, if `destination` exists, we will delete it.
    #[clap(long, short = 'x', default_value = "false")]
    overwrite: bool,
  },
}

/// The `repors` command line tool is meant to be a replacement of the `repo` command line tool
/// used by google. This tool has less "bells and whistles" and is not intended to be used to
/// manage some monorepo type project, but purely as a means to build openembedded projects.
#[derive(Parser)]
#[clap(version = option_env!("REPORS_VERSION").unwrap_or("dev"), verbatim_doc_comment, author)]
struct CommandLine {
  /// The subcommand.
  #[clap(subcommand)]
  subcommand: Subcommand,
}

fn main() -> io::Result<()> {
  let _ = env_logger::try_init();
  let cli = CommandLine::parse();

  match cli.subcommand {
    Subcommand::Execute {
      threads,
      manifest,
      destination,
      overwrite,
    } => {
      log::debug!("attempting to do repo stuff against manifest '{manifest}'");
      let bytes = std::fs::read(&manifest)?;
      let cursor = std::io::Cursor::new(&bytes);
      let manifest = repors::Manifest::from_reader(cursor)?;
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

      let pool = repors::WorkerPool::create(threads, destination_path.clone())?;
      pool.execute(manifest)?;
    }
  }

  Ok(())
}
