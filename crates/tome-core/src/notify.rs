//! Push notifications from the server: locks taken and released, branches created, deleted and
//! pushed.
//!
//! Lore's subscribe call sends Complete as soon as the subscription is set up, but not End: the
//! call stays open and the notifications keep coming to the same callback until unsubscribe. So
//! the callback context here is not tied to End like in [`crate::call`]; [`Subscription`] owns
//! it, unsubscribes, waits for the subscribe call's End and only then frees it.

use std::sync::{Mutex, mpsc};

use lore::interface::{LoreEvent, LoreEventCallbackConfig};
use serde::Serialize;
use serde_json::Value;

use crate::{Repository, interface};

/// One push notification: `kind` is the event without its `notification` prefix
/// (`resourceLocked`, `resourceUnlocked`, `branchPushed`, `branchCreated`, `branchDeleted`).
#[derive(Debug, Clone, Serialize)]
pub struct Notification {
    pub kind: String,
    pub data: Value,
}

impl Notification {
    /// Paths of a lock notification.
    pub fn paths(&self) -> Vec<String> {
        self.data["paths"].as_array().map(|items| items.iter().filter_map(|p| p.as_str().map(str::to_string)).collect()).unwrap_or_default()
    }
}

type Handler = Box<dyn Fn(Notification) + Send + Sync>;

/// Lives from subscribe until after unsubscribe.
struct Context {
    handler: Handler,
    /// While the subscribe call runs: where its Complete status and End go.
    call: Mutex<Option<mpsc::Sender<Option<(i32, String)>>>>,
}

unsafe extern "C" fn on_notification(event: &LoreEvent, user_context: u64) {
    // SAFETY: the context is freed only by Subscription::drop after unsubscribe has returned,
    // or by subscribe after its failed call has ended; no event arrives after either.
    let context = unsafe { &*(user_context as *const Context) };
    match event {
        LoreEvent::Complete(data) => {
            if let Some(sender) = context.call.lock().unwrap().as_ref() {
                let _ = sender.send(Some((data.status, data.error.message.as_str().to_string())));
            }
        }
        LoreEvent::End(_) => {
            if let Some(sender) = context.call.lock().unwrap().take() {
                let _ = sender.send(None);
            }
        }
        LoreEvent::Log(_) => {}
        other => {
            let value = serde_json::to_value(other).unwrap_or(Value::Null);
            let tag = value["tagName"].as_str().unwrap_or("");
            if let Some(kind) = tag.strip_prefix("notification")
                && kind != "Subscribed" && kind != "Unsubscribed" {
                    let mut kind = kind.to_string();
                    kind[..1].make_ascii_lowercase();
                    (context.handler)(Notification { kind, data: value["data"].clone() });
                }
        }
    }
}

/// An active subscription; dropping it unsubscribes.
pub struct Subscription {
    repository: Repository,
    context: usize,
    /// Receives the subscribe call's End (None) once the subscription is over.
    ended: Mutex<mpsc::Receiver<Option<(i32, String)>>>,
}

impl Repository {
    /// Subscribes to the server's notifications for this repository; `handler` runs on a Lore
    /// thread for each one. Needs the server (not offline).
    pub fn subscribe(&self, handler: impl Fn(Notification) + Send + Sync + 'static) -> Result<Subscription, String> {
        let (sender, receiver) = mpsc::channel();
        let context = Box::into_raw(Box::new(Context { handler: Box::new(handler), call: Mutex::new(Some(sender)) }));
        let online = Repository { path: self.path.clone(), offline: false, identity: self.identity.clone() };
        let globals = online.globals();
        let args = lore::notification::LoreNotificationSubscribeArgs {};
        interface::lore_notification_subscribe_async(&globals, &args, LoreEventCallbackConfig { user_context: context as u64, func: Some(on_notification) });
        // Complete says whether it worked; a failed call also ends (End = None) right away.
        let outcome = match receiver.recv_timeout(std::time::Duration::from_secs(30)) {
            Ok(Some(complete)) => complete,
            Ok(None) => (-1, "subscribe ended without a result".to_string()),
            // No answer: the context may still be used, so it is left allocated.
            Err(_) => return Err("notification subscribe did not answer".into()),
        };
        if outcome.0 != 0 {
            if matches!(receiver.recv_timeout(std::time::Duration::from_secs(5)), Ok(None)) {
                // SAFETY: the failed call has ended (End seen); no event uses the context.
                drop(unsafe { Box::from_raw(context) });
            }
            return Err(outcome.1);
        }
        Ok(Subscription { repository: online, context: context as usize, ended: Mutex::new(receiver) })
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let args = lore::notification::LoreNotificationUnsubscribeArgs {};
        crate::call(interface::lore_notification_unsubscribe_async, &self.repository.globals(), &args);
        // The subscribe call ends after the unsubscribe. Only once its End has been seen is it
        // certain that no event will use the context; without it the context is left allocated
        // (a few bytes) rather than risk a use after free.
        let ended = self.ended.lock().unwrap();
        loop {
            match ended.recv_timeout(std::time::Duration::from_secs(5)) {
                Ok(None) => {
                    // SAFETY: End was the subscribe call's last event.
                    drop(unsafe { Box::from_raw(self.context as *mut Context) });
                    return;
                }
                Ok(Some(_)) => continue,
                Err(_) => return,
            }
        }
    }
}
