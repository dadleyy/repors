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

/// During the execution subcommand, we will send instances of this types into background workers
/// where they will perform their work.
type Job = (
  std::sync::mpsc::Sender<io::Result<(std::path::PathBuf, std::path::PathBuf)>>,
  repors::Source,
);

/// Our threadpool is based on this type, which is used to communicate from the threads we spawn
/// back up into the main thread.
#[derive(Debug)]
enum WorkerEvent {
  /// We will immediately send this type when the thread starts executing.
  Online(String, std::sync::mpsc::Sender<Job>),
  /// This variant is send when a worker finishes a job.
  Idle(String),
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
      let mut manifest = repors::Manifest::from_reader(cursor)?;
      log::debug!("manifest loaded - {manifest:?}");
      let count = manifest.sources.len();

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
      let mut temp_path = std::env::temp_dir();
      temp_path.push(format!("repors-{}", uuid::Uuid::new_v4()));
      let mut tree = repors::LayerTree::default();

      let (loc_sender, loc_receiver) = std::sync::mpsc::channel();

      std::thread::scope(|scope| {
        let (job_result_sender, job_result_receiver) = std::sync::mpsc::channel();
        let worker_count = threads;

        for i in 0..worker_count {
          let events = job_result_sender.clone();
          let dp = destination_path.clone();
          let tp = temp_path.clone();

          scope.spawn(move || {
            let id = uuid::Uuid::new_v4().to_string();
            let (job_sender, job_receiver) = std::sync::mpsc::channel();

            if let Err(error) = events.send(WorkerEvent::Online(id.clone(), job_sender)) {
              log::error!("unable to notify worker pool of readiness - {error:?}");
              return;
            }

            while let Ok((sender, source)) = job_receiver.recv() {
              log::debug!("thread[{i}] doing job");

              let mut source_path = dp.clone();
              source_path.push(&source.destination);

              let mut temp_dest = tp.clone();
              temp_dest.push(uuid::Uuid::new_v4().to_string());

              let origin = source.origin.clone();

              if let Err(error) = std::fs::create_dir_all(&temp_dest) {
                log::warn!("failed preparing temp dir - {error:?}");

                if let Err(error) = sender.send(Err(error)) {
                  log::warn!("worker failed to notify pool of error during execution - {error:?}");
                }

                return;
              }

              log::debug!("starting to clone '{source:?}' into '{temp_dest:?}'");

              let mut builder = git2::build::RepoBuilder::new();

              let clone_result = builder.clone(&origin, &temp_dest).map_err(|error| {
                io::Error::new(
                  io::ErrorKind::Other,
                  format!("failed cloning '{source:?}': {error:?}"),
                )
              });

              if let Err(error) = clone_result {
                log::warn!("failed cloning - {error:?}");

                if let Err(error) = sender.send(Err(error)) {
                  log::warn!("worker failed to notify pool of error during execution - {error:?}");
                }
                return;
              }

              log::debug!("clone complete in '{temp_dest:?}'");
              if let Err(error) = sender.send(Ok((source_path, temp_dest))) {
                log::error!("unable to send job execution result - {error:?}, terminating worker");
                break;
              }

              if let Err(error) = events.send(WorkerEvent::Idle(id.clone())) {
                log::error!("unable to worker availability, terminating worker ({error:?})");
                break;
              }
            }
          });
        }

        let mut workers = std::collections::HashMap::new();
        while workers.len() < worker_count {
          match job_result_receiver.recv() {
            Ok(WorkerEvent::Online(id, sender)) => {
              log::debug!("worker '{id}' is now online");
              workers.insert(id, sender);
            }
            Ok(_) => return Err(io::Error::new(io::ErrorKind::Other, "")),
            Err(error) => {
              return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("worker registration failed - {error:?}"),
              ))
            }
          }
        }

        let mut jobs = manifest.sources.drain(0..);
        let mut finished = Vec::default();

        for (id, sender) in &workers {
          let Some(job) = jobs.next() else {
            log::debug!("not enough jobs for {worker_count} workers");
            finished.push(id.clone());
            continue;
          };

          log::debug!("sending clone job to worker '{id}'");
          let results = loc_sender.clone();
          let _ = sender.send((results, job));
        }

        loop {
          if finished.len() == worker_count {
            log::info!("all workers appear idle, exiting processing loop");
            break;
          }

          match job_result_receiver.recv() {
            Ok(WorkerEvent::Idle(id)) => {
              log::info!("worker '{id}' appears idle, checking for jobs");
              let Some(sender) = workers.get(&id) else {
                continue;
              };

              let Some(next) = jobs.next() else {
                log::info!("no jobs left for '{id}'");
                finished.push(id);
                continue;
              };

              log::info!("sending job to '{id}'");
              let results = loc_sender.clone();
              let _ = sender.send((results, next));
            }
            Ok(other) => {
              log::warn!("strange message received on result receiver - {other:?}");
            }
            Err(error) => {
              return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("failed receiving from worker threads - {error:?}"),
              ));
            }
          }
        }

        drop(job_result_receiver);
        drop(loc_sender);

        let mut failed = false;

        while let Ok(result) = loc_receiver.recv() {
          match result {
            Ok((src, temp)) => {
              log::debug!("registering '{src:?}' (currently at '{temp:?}'");
              tree.add(src, temp)
            }
            Err(error) => {
              log::warn!("error while cloning - {error:?}");
              failed = true;
            }
          }
        }

        if failed {
          return Err(io::Error::new(
            io::ErrorKind::Other,
            "not all cloned completed successfully. check logs",
          ));
        }

        let order = tree.consume();

        if count != order.len() {
          log::warn!("we did not clone as many sources as there were in the manifest");
        }

        log::debug!("received all results, attempting to place into final destinations");
        for (destination, temp) in order {
          log::debug!("moving '{temp:?}' to '{destination:?}'");
          std::fs::create_dir_all(&destination)?;
          std::fs::rename(&temp, &destination)?;
        }

        io::Result::Ok(())
      })?;
    }
  }

  Ok(())
}
