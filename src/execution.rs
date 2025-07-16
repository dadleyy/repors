use crate::{manifest, tree};
use std::io;

/// During the execution subcommand, we will send instances of this types into background workers
/// where they will perform their work.
enum Job {
  /// This is our main job type - we provide the worker with the sending half of our channel
  /// where it can tell us where the repo was cloned and where we should put it.
  Cloner {
    /// The sender of locations.
    results: std::sync::mpsc::Sender<io::Result<(std::path::PathBuf, std::path::PathBuf)>>,
    /// The layer we should clone.
    source: manifest::Source,
  },
  /// This variant is used to signal termination.
  Terminate,
}

/// Our threadpool is based on this type, which is used to communicate from the threads we spawn
/// back up into the main thread.
#[derive(Debug)]
enum WorkerEvent {
  /// We will immediately send this type when the thread starts executing.
  Online(String, std::sync::mpsc::Sender<Job>),
  /// This variant is send when a worker finishes a job.
  Idle(String),
}

/// This is the handle we will use in our pool to communicate with our spawned threads.
struct WorkerHandle {
  /// This is the pipe going into the spawned thread, where we will send it clone jobs.
  jobs: std::sync::mpsc::Sender<Job>,
  /// This is the thread's join handle so we can clean up nicely when we are done.
  handle: std::thread::JoinHandle<()>,
}

/// This is a container of threads.
pub struct WorkerPool {
  /// For every worker, will will want to keep a unique id
  workers: std::collections::HashMap<String, WorkerHandle>,
  /// This is used for synchronizing state between the workers themselves and our pool.
  events: std::sync::mpsc::Receiver<WorkerEvent>,
  /// This is the channel we will clone senders for, providing them to the jobs passed to our
  /// workers. After sending all layers, we receive on the other half, creating our tree from the
  /// items received.
  #[allow(clippy::type_complexity)]
  results: (
    std::sync::mpsc::Sender<io::Result<(std::path::PathBuf, std::path::PathBuf)>>,
    std::sync::mpsc::Receiver<io::Result<(std::path::PathBuf, std::path::PathBuf)>>,
  ),
}

impl WorkerPool {
  /// This method will attempt to spawn `amount` number of threads, registering themselves with the
  /// returned pool which can then be used to `execute` against some manifest.
  pub fn create(amount: usize, destination: std::path::PathBuf) -> io::Result<Self> {
    let mut workers = std::collections::HashMap::new();
    let (event_sender, events) = std::sync::mpsc::channel();

    std::fs::create_dir_all(&destination)?;

    let mut temp_path = std::env::temp_dir();
    temp_path.push(format!("repors-{}", uuid::Uuid::new_v4()));

    for i in 0..amount {
      let es = event_sender.clone();
      let dp = destination.clone();
      let tp = temp_path.clone();

      let handle = std::thread::spawn(move || {
        let id = uuid::Uuid::new_v4().to_string();
        let (job_sender, job_receiver) = std::sync::mpsc::channel();

        if let Err(error) = es.send(WorkerEvent::Online(id.clone(), job_sender)) {
          log::error!("unable to notify worker pool of readiness - {error:?}");
          return;
        }

        while let Ok(job) = job_receiver.recv() {
          let Job::Cloner {
            results: sender,
            source,
          } = job
          else {
            break;
          };

          log::debug!("thread[{i}] assigned to '{}'", source.origin);

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

          log::debug!("starting to clone '{}'", source.origin);
          let start = std::time::Instant::now();

          let mut builder = git2::build::RepoBuilder::new();
          let clone_result = builder.clone(&origin, &temp_dest);

          let repo = match clone_result {
            Err(error) => {
              log::warn!("failed cloning '{}' - {error:?}", source.origin);
              let wrapped_err = io::Error::new(io::ErrorKind::Other, error.to_string());

              if let Err(error) = sender.send(Err(wrapped_err)) {
                log::warn!("worker failed to notify pool of error during execution - {error:?}");
              }

              return;
            }
            Ok(repo) => repo,
          };

          let duration = std::time::Instant::now().duration_since(start).as_millis();
          log::debug!("'{}' clone complete ({duration}ms)", source.origin);

          let start = std::time::Instant::now();
          let commit = match repo.find_commit_by_prefix(&source.revision) {
            Ok(c) => c,
            Err(error) => {
              log::warn!("unable to find '{}' in '{}'", source.revision, source.origin);
              let wrapped_err = io::Error::new(io::ErrorKind::Other, error.to_string());

              if let Err(error) = sender.send(Err(wrapped_err)) {
                log::warn!("worker failed to notify pool of error during execution - {error:?}");
              }

              return;
            }
          };

          log::debug!("pointing '{}' to {commit:?}", source.origin);
          let oid = commit.as_object().id();

          if let Err(error) = repo.set_head_detached(oid) {
            let wrapped_err = io::Error::new(io::ErrorKind::Other, error.to_string());

            if let Err(error) = sender.send(Err(wrapped_err)) {
              log::warn!("worker failed to notify pool of error during execution - {error:?}");
            }

            return;
          }

          log::debug!("'{}' was updated to '{}'", source.origin, source.revision);

          if let Err(error) = repo.checkout_head(None) {
            let wrapped_err = io::Error::new(io::ErrorKind::Other, error.to_string());

            if let Err(error) = sender.send(Err(wrapped_err)) {
              log::warn!("worker failed to notify pool of error during execution - {error:?}");
            }

            return;
          }

          if let Err(error) = repo.reset(commit.as_object(), git2::ResetType::Hard, None) {
            log::warn!("'{}' failed checkout - {error:?}", source.origin);
            let wrapped_err = io::Error::new(io::ErrorKind::Other, error.to_string());
            if let Err(error) = sender.send(Err(wrapped_err)) {
              log::warn!("worker failed to notify pool of error during execution - {error:?}");
            }
            return;
          }

          let duration = std::time::Instant::now().duration_since(start).as_millis();
          log::debug!("'{}' checkout complete ({duration}ms)", source.origin);

          if let Err(error) = sender.send(Ok((source_path, temp_dest))) {
            log::error!("unable to send job execution result - {error:?}, terminating worker");
            break;
          }

          if let Err(error) = es.send(WorkerEvent::Idle(id.clone())) {
            log::error!("unable to worker availability, terminating worker ({error:?})");
            break;
          }
        }

        log::info!("worker '{id}' terminating");
      });

      let Ok(WorkerEvent::Online(id, jobs)) = events.recv() else {
        return Err(io::Error::new(io::ErrorKind::Other, ""));
      };

      log::debug!("worker '{id}' is ready for jobs");
      workers.insert(id, WorkerHandle { jobs, handle });
    }

    Ok(Self {
      workers,
      events,
      results: std::sync::mpsc::channel(),
    })
  }

  /// This method consumes the manifest, sending each layer as a job into our worker pool for it to
  /// execute. Once the git operations have been completed, will will "place" the layers into their
  /// final location.
  pub fn execute(mut self, mut manifest: manifest::Manifest) -> io::Result<()> {
    let layer_count = manifest.sources.len();
    let worker_count = self.workers.len();
    let mut jobs = manifest.sources.drain(0..);
    let mut finished = Vec::default();
    let (result_sender, result_receiver) = self.results;

    for (id, handle) in &self.workers {
      let Some(job) = jobs.next() else {
        log::debug!("not enough jobs for {worker_count} workers");
        finished.push(id.clone());
        continue;
      };

      log::debug!("sending clone job to worker '{id}'");
      let results = result_sender.clone();
      let _ = handle.jobs.send(Job::Cloner { results, source: job });
    }

    loop {
      if finished.len() == worker_count {
        log::info!("all workers appear idle, exiting processing loop");
        break;
      }

      match self.events.recv() {
        Ok(WorkerEvent::Idle(id)) => {
          log::info!("worker '{id}' appears idle, checking for jobs");

          let Some(worker) = self.workers.get(&id) else {
            continue;
          };

          let Some(next) = jobs.next() else {
            log::info!("no jobs left for '{id}'");
            finished.push(id);
            continue;
          };

          log::info!("sending job to '{id}'");
          let results = result_sender.clone();
          let _ = worker.jobs.send(Job::Cloner {
            results,
            source: next,
          });
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

    drop(result_sender);
    drop(self.events);

    let mut failed = false;
    let mut layer_tree = tree::LayerTree::default();
    while let Ok(result) = result_receiver.recv() {
      match result {
        Ok((src, temp)) => {
          log::debug!("registering '{src:?}' (currently at '{temp:?}'");
          layer_tree.add(src, temp)
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

    for (id, worker) in &self.workers {
      if let Err(error) = worker.jobs.send(Job::Terminate) {
        log::warn!("unable to terminate '{id}': {error:?}");
      }
    }

    let order = layer_tree.consume();

    if layer_count != order.len() {
      log::warn!("we did not clone as many sources as there were in the manifest");
    }

    log::debug!("received all results, attempting to place into final destinations");
    for (destination, temp) in order {
      log::trace!("moving '{temp:?}' to '{destination:?}'");
      std::fs::create_dir_all(&destination)?;
      std::fs::rename(&temp, &destination)?;
    }

    for (id, handle) in self.workers.drain() {
      if let Err(error) = handle.handle.join() {
        log::error!("worker handle '{id}' did not close successfully: {error:?}");
      }
    }

    Ok(())
  }
}
