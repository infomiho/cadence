use std::sync::{OnceLock, mpsc};

use anyhow::{Context as _, Result};

type Job = Box<dyn FnOnce() + Send>;

static JOBS: OnceLock<mpsc::Sender<Job>> = OnceLock::new();

fn jobs() -> Result<&'static mpsc::Sender<Job>> {
    if let Some(jobs) = JOBS.get() {
        return Ok(jobs);
    }
    let (sender, receiver) = mpsc::channel::<Job>();
    std::thread::Builder::new()
        .name("cadence-credentials".to_owned())
        .spawn(move || {
            while let Ok(job) = receiver.recv() {
                job();
            }
        })
        .context("could not start credential worker")?;
    let _ = JOBS.set(sender);
    JOBS.get().context("could not initialize credential worker")
}

pub(crate) async fn run<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    jobs()?
        .send(Box::new(move || {
            let _ = sender.send(operation());
        }))
        .map_err(|_| anyhow::anyhow!("credential worker is unavailable"))?;
    receiver.await.context("credential worker stopped")?
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn cancelled_callers_do_not_reorder_credential_jobs() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let first_order = order.clone();
        let first = tokio::spawn(async move {
            super::run(move || {
                std::thread::sleep(std::time::Duration::from_millis(10));
                first_order.lock().unwrap().push(1);
                Ok(())
            })
            .await
        });
        tokio::task::yield_now().await;
        first.abort();

        let second_order = order.clone();
        super::run(move || {
            second_order.lock().unwrap().push(2);
            Ok(())
        })
        .await
        .unwrap();

        assert_eq!(*order.lock().unwrap(), vec![1, 2]);
    }
}
