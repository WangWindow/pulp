//! UI-backed password requests for archive operations.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pulp::{
    ArchiveError, ArchiveResult, CancellationToken, Password, PasswordProvider, PasswordRequest,
};

/// A password request waiting for a response from the UI.
pub struct PasswordPrompt {
    /// The reason the archive provider needs a password.
    pub request: PasswordRequest,
    /// Sends the password back to the worker, or `None` to decline.
    pub response: Sender<Option<Password>>,
}

/// Owns the response side of one password dialog.
///
/// GPUI may close a dialog after an accept callback has already run. Keeping
/// the once guard with the channel makes accept, cancel, and operation
/// cancellation safe to race without sending two answers to the worker.
#[derive(Clone)]
pub struct PasswordResponder {
    sender: Sender<Option<Password>>,
    sent: Arc<AtomicBool>,
}

impl PasswordResponder {
    fn new(sender: Sender<Option<Password>>) -> Self {
        Self {
            sender,
            sent: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Sends the first response and ignores later lifecycle callbacks.
    pub fn send(&self, password: Option<Password>) -> bool {
        if self.sent.swap(true, Ordering::AcqRel) {
            return false;
        }
        self.sender.send(password).is_ok()
    }
}

impl PasswordPrompt {
    /// Creates a response handle without consuming the prompt.
    pub fn responder(&self) -> PasswordResponder {
        PasswordResponder::new(self.response.clone())
    }

    /// Converts this prompt into the response handle used by the dialog.
    pub fn into_responder(self) -> PasswordResponder {
        PasswordResponder::new(self.response)
    }
}

/// Bridges synchronous native password callbacks to the asynchronous UI.
pub struct PasswordBroker {
    prompts: Sender<PasswordPrompt>,
    cancellation: CancellationToken,
    cached: Mutex<Option<Password>>,
}

impl PasswordBroker {
    /// Creates a provider and the receiver consumed by the workspace.
    pub fn channel(cancellation: CancellationToken) -> (Arc<Self>, Receiver<PasswordPrompt>) {
        let (prompts, receiver) = mpsc::channel();
        (
            Arc::new(Self {
                prompts,
                cancellation,
                cached: Mutex::new(None),
            }),
            receiver,
        )
    }
}

impl PasswordProvider for PasswordBroker {
    fn request(&self, request: PasswordRequest) -> ArchiveResult<Option<Password>> {
        if self.cancellation.is_cancelled() {
            return Err(ArchiveError::Cancelled);
        }
        let mut cached = self
            .cached
            .lock()
            .map_err(|_| ArchiveError::backend("password prompt", "password cache is poisoned"))?;
        if request.reason == pulp::PasswordReason::Retry {
            *cached = None;
        } else if let Some(password) = cached.clone() {
            return Ok(Some(password));
        }
        drop(cached);
        let (response, receiver) = mpsc::channel();
        self.prompts
            .send(PasswordPrompt { request, response })
            .map_err(|_| {
                ArchiveError::backend("password prompt", "the UI is no longer available")
            })?;

        loop {
            if self.cancellation.is_cancelled() {
                return Err(ArchiveError::Cancelled);
            }
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(password) => {
                    if let Some(value) = password.clone() {
                        let mut cached = self.cached.lock().map_err(|_| {
                            ArchiveError::backend("password prompt", "password cache is poisoned")
                        })?;
                        *cached = Some(value);
                    }
                    return Ok(password);
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(ArchiveError::backend(
                        "password prompt",
                        "the password dialog was closed",
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;
    use pulp::PasswordReason;

    #[test]
    fn password_responder_answers_a_request_only_once() {
        let (sender, receiver) = mpsc::channel();
        let responder = PasswordResponder::new(sender);

        assert!(responder.send(Some(Password::new("secret"))));
        assert!(!responder.send(None));
        assert_eq!(
            receiver
                .recv()
                .expect("the first response should arrive")
                .expect("the first response should contain a password")
                .as_str(),
            Ok("secret")
        );
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn reuses_a_password_until_the_provider_requests_retry() {
        let cancellation = CancellationToken::new();
        let (broker, prompts) = PasswordBroker::channel(cancellation);
        let worker = Arc::clone(&broker);
        let first = thread::spawn(move || {
            worker
                .request(PasswordRequest {
                    reason: PasswordReason::Header,
                    attempt: 1,
                })
                .expect("first password request should succeed")
        });
        let prompt = prompts.recv().expect("the first prompt should arrive");
        prompt
            .response
            .send(Some(Password::new("secret")))
            .expect("the worker should still be waiting");
        assert_eq!(
            first
                .join()
                .expect("worker should finish")
                .unwrap()
                .as_str(),
            Ok("secret")
        );

        assert_eq!(
            broker
                .request(PasswordRequest {
                    reason: PasswordReason::Data,
                    attempt: 1,
                })
                .expect("cached password should be returned")
                .expect("cached password should be present")
                .as_str(),
            Ok("secret")
        );

        let worker = Arc::clone(&broker);
        let retry = thread::spawn(move || {
            worker
                .request(PasswordRequest {
                    reason: PasswordReason::Retry,
                    attempt: 2,
                })
                .expect("retry request should accept cancellation")
        });
        let prompt = prompts.recv().expect("retry should request a new prompt");
        prompt
            .response
            .send(None)
            .expect("the retry worker should still be waiting");
        assert!(retry.join().expect("retry worker should finish").is_none());
    }

    #[test]
    fn cancellation_releases_a_waiting_password_request() {
        let cancellation = CancellationToken::new();
        let (broker, prompts) = PasswordBroker::channel(cancellation.clone());
        let worker = thread::spawn(move || {
            broker.request(PasswordRequest {
                reason: PasswordReason::Data,
                attempt: 1,
            })
        });

        let _prompt = prompts.recv().expect("the password prompt should arrive");
        cancellation.cancel();

        assert!(matches!(
            worker.join().expect("worker should finish"),
            Err(ArchiveError::Cancelled)
        ));
    }
}
