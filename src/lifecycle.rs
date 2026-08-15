use std::{
    fs,
    io::{self, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::{Context as _, Result};
use directories::ProjectDirs;

const ACTIVATE_MESSAGE: &[u8] = b"activate\n";

pub enum Instance {
    Primary(Arc<InstanceLifecycle>),
    Secondary,
}

pub struct InstanceLifecycle {
    socket_path: PathBuf,
    activations: async_channel::Receiver<()>,
    shutdown: Arc<AtomicBool>,
    listener_thread: Option<JoinHandle<()>>,
}

impl InstanceLifecycle {
    pub fn acquire() -> Result<Instance> {
        let project_dirs = ProjectDirs::from("com", "Cadence", "Cadence")
            .context("could not determine the Cadence cache directory")?;
        Self::acquire_at(project_dirs.cache_dir().join("activation.sock"))
    }

    fn acquire_at(socket_path: PathBuf) -> Result<Instance> {
        if let Some(parent) = socket_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("could not create lifecycle directory {}", parent.display())
            })?;
        }

        match UnixListener::bind(&socket_path) {
            Ok(listener) => Self::start_primary(socket_path, listener),
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
                if Self::request_activation(&socket_path).is_ok() {
                    return Ok(Instance::Secondary);
                }

                // A crashed process can leave its socket path behind.
                let _ = fs::remove_file(&socket_path);
                let listener = UnixListener::bind(&socket_path).with_context(|| {
                    format!(
                        "could not recover lifecycle socket {}",
                        socket_path.display()
                    )
                })?;
                Self::start_primary(socket_path, listener)
            }
            Err(error) => Err(error).with_context(|| {
                format!("could not bind lifecycle socket {}", socket_path.display())
            }),
        }
    }

    fn start_primary(socket_path: PathBuf, listener: UnixListener) -> Result<Instance> {
        listener
            .set_nonblocking(true)
            .context("could not configure lifecycle socket")?;
        let (activation_tx, activation_rx) = async_channel::bounded(1);
        let shutdown = Arc::new(AtomicBool::new(false));
        let listener_shutdown = shutdown.clone();
        let listener_thread = thread::Builder::new()
            .name("cadence-activation".into())
            .spawn(move || {
                while !listener_shutdown.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((_, _)) => {
                            if listener_shutdown.load(Ordering::Relaxed) {
                                break;
                            }
                            let _ = activation_tx.try_send(());
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(50));
                        }
                        Err(_) => break,
                    }
                }
            })
            .context("could not start lifecycle listener")?;

        Ok(Instance::Primary(Arc::new(Self {
            socket_path,
            activations: activation_rx,
            shutdown,
            listener_thread: Some(listener_thread),
        })))
    }

    fn request_activation(socket_path: &Path) -> io::Result<()> {
        let mut stream = UnixStream::connect(socket_path)?;
        stream.write_all(ACTIVATE_MESSAGE)
    }

    pub fn take_activation(&self) -> bool {
        let mut activated = false;
        while self.activations.try_recv().is_ok() {
            activated = true;
        }
        activated
    }

    pub fn activation_receiver(&self) -> async_channel::Receiver<()> {
        self.activations.clone()
    }
}

impl Drop for InstanceLifecycle {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = UnixStream::connect(&self.socket_path);
        if let Some(listener_thread) = self.listener_thread.take() {
            let _ = listener_thread.join();
        }
        let _ = fs::remove_file(&self.socket_path);
    }
}

#[cfg(test)]
mod tests {
    use super::{Instance, InstanceLifecycle};
    use std::{fs, time::Duration};

    fn socket_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "cadence-lifecycle-{}-{name}.sock",
            std::process::id()
        ))
    }

    #[test]
    fn second_instance_activates_primary() {
        let path = socket_path("activate");
        let _ = fs::remove_file(&path);
        let Instance::Primary(primary) =
            InstanceLifecycle::acquire_at(path.clone()).expect("primary should start")
        else {
            panic!("first instance was not primary");
        };

        assert!(matches!(
            InstanceLifecycle::acquire_at(path),
            Ok(Instance::Secondary)
        ));
        for _ in 0..20 {
            if primary.take_activation() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("primary did not receive activation");
    }

    #[test]
    fn stale_socket_is_recovered() {
        let path = socket_path("stale");
        let _ = fs::remove_file(&path);
        std::os::unix::net::UnixListener::bind(&path).expect("stale socket should bind");

        assert!(matches!(
            InstanceLifecycle::acquire_at(path),
            Ok(Instance::Primary(_))
        ));
    }

    #[test]
    fn primary_stops_if_its_socket_path_was_removed() {
        let path = socket_path("removed");
        let _ = fs::remove_file(&path);
        let Instance::Primary(primary) =
            InstanceLifecycle::acquire_at(path.clone()).expect("primary should start")
        else {
            panic!("first instance was not primary");
        };

        fs::remove_file(path).expect("socket path should be removable");
        drop(primary);
    }
}
