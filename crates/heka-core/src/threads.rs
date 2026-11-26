use std::io;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};

pub type Task = Box<dyn FnOnce() + Send>;

pub trait Threads: Send + Sync {
    fn spawn(&self, name: &str, task: Task) -> io::Result<JoinHandle<()>>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemThreads;

impl Threads for SystemThreads {
    fn spawn(&self, name: &str, task: Task) -> io::Result<JoinHandle<()>> {
        thread::Builder::new().name(name.into()).spawn(task)
    }
}

pub(crate) struct Worker<R> {
    thread: JoinHandle<()>,
    result: Arc<Mutex<Option<R>>>,
}

impl<R: Send + 'static> Worker<R> {
    pub(crate) fn spawn<T: Send + 'static>(
        threads: &dyn Threads,
        name: &str,
        parts: T,
        run: impl FnOnce(T) -> R + Send + 'static,
    ) -> Result<Self, (io::Error, T)> {
        let parts = Arc::new(Mutex::new(Some(parts)));
        let result = Arc::new(Mutex::new(None));
        let task: Task = Box::new({
            let parts = Arc::clone(&parts);
            let result = Arc::clone(&result);
            move || {
                if let Some(parts) = take(&parts) {
                    let ended = run(parts);
                    *result.lock().unwrap_or_else(PoisonError::into_inner) = Some(ended);
                }
            }
        });
        match threads.spawn(name, task) {
            Ok(thread) => Ok(Self { thread, result }),
            Err(error) => {
                let parts = take(&parts).expect("a thread that never started took nothing");
                Err((error, parts))
            }
        }
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.thread.is_finished()
    }

    pub(crate) fn join(self) -> R {
        if let Err(panic) = self.thread.join() {
            std::panic::resume_unwind(panic);
        }
        take(&self.result).expect("a thread that did not panic left its result")
    }
}

fn take<T>(slot: &Mutex<Option<T>>) -> Option<T> {
    slot.lock().unwrap_or_else(PoisonError::into_inner).take()
}
