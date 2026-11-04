//! Tab → sidecar pool: deferred starts, focused/visible forwarding, session ownership (F-OWN),
//! and request-id routing back to the originating tab (F-UI-ORIGIN).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde_json::Value;

use omp_core::{Error, Result, TabId};

/// Anything that can receive a frame. Implemented by [`crate::sidecar::Sidecar`] and by test
/// doubles, so the pool's bookkeeping is testable without spawning processes.
pub trait SidecarHandle: Send {
    fn send(&self, frame: &Value) -> Result<()>;
    fn dispose(self);
}

impl SidecarHandle for crate::sidecar::Sidecar {
    fn send(&self, frame: &Value) -> Result<()> {
        crate::sidecar::Sidecar::send(self, frame)
    }
    fn dispose(self) {
        self.terminate();
    }
}

/// A tab the window has open. `handle` is `None` until the tab is first shown (`deferStart`).
struct Slot<H> {
    handle: Option<H>,
    started: bool,
}

/// Which tabs of one window receive the full event stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewState {
    pub focused: TabId,
    /// One or two tabs; always contains `focused`.
    pub visible: Vec<TabId>,
}

impl ViewState {
    /// Reject anything but "the focused tab plus at most one split partner".
    pub fn validate(focused: TabId, visible: &[TabId]) -> Result<Self> {
        let mut seen = HashSet::new();
        let mut ordered = Vec::with_capacity(visible.len());
        for id in visible {
            if seen.insert(id.clone()) {
                ordered.push(id.clone());
            }
        }
        if ordered.is_empty() || ordered.len() > 2 {
            return Err(Error::msg("visible tabs must be one or two"));
        }
        if !ordered.contains(&focused) {
            return Err(Error::msg("focused tab must be visible"));
        }
        Ok(ViewState {
            focused,
            visible: ordered,
        })
    }
}

pub struct SidecarPool<H: SidecarHandle> {
    slots: Mutex<HashMap<TabId, Slot<H>>>,
    views: Mutex<HashMap<omp_core::WindowId, ViewState>>,
    session_owners: Mutex<HashMap<PathBuf, TabId>>,
    requests: Mutex<HashMap<String, TabId>>,
}

impl<H: SidecarHandle> Default for SidecarPool<H> {
    fn default() -> Self {
        Self::new()
    }
}

impl<H: SidecarHandle> SidecarPool<H> {
    pub fn new() -> Self {
        SidecarPool {
            slots: Mutex::new(HashMap::new()),
            views: Mutex::new(HashMap::new()),
            session_owners: Mutex::new(HashMap::new()),
            requests: Mutex::new(HashMap::new()),
        }
    }

    /// Register a tab without spawning: the process starts on first show.
    pub fn register_deferred(&self, tab: TabId) {
        self.slots.lock().insert(
            tab,
            Slot {
                handle: None,
                started: false,
            },
        );
    }

    /// Register a tab and attach an already-spawned sidecar.
    pub fn attach(&self, tab: TabId, handle: H) {
        self.slots.lock().insert(
            tab,
            Slot {
                handle: Some(handle),
                started: true,
            },
        );
    }

    /// Attach a lazily-spawned sidecar for a tab that was registered deferred.
    pub fn start_deferred(&self, tab: &TabId, handle: H) -> Result<()> {
        let mut slots = self.slots.lock();
        let slot = slots
            .get_mut(tab)
            .ok_or_else(|| Error::msg(format!("unknown tab: {tab}")))?;
        slot.handle = Some(handle);
        slot.started = true;
        Ok(())
    }

    pub fn is_started(&self, tab: &TabId) -> bool {
        self.slots
            .lock()
            .get(tab)
            .map(|s| s.started)
            .unwrap_or(false)
    }

    pub fn tab_count(&self) -> usize {
        self.slots.lock().len()
    }

    pub fn tabs(&self) -> Vec<TabId> {
        self.slots.lock().keys().cloned().collect()
    }

    /// Drop a tab, disposing its process handle. Returns whether the tab existed.
    pub fn close(&self, tab: &TabId) -> bool {
        let removed = self.slots.lock().remove(tab);
        if let Some(slot) = removed {
            if let Some(handle) = slot.handle {
                handle.dispose();
            }
            self.session_owners.lock().retain(|_, owner| owner != tab);
            self.requests.lock().retain(|_, owner| owner != tab);
            for view in self.views.lock().values_mut() {
                view.visible.retain(|id| id != tab);
            }
            return true;
        }
        false
    }

    /// Drop a tab's process but keep the tab registered, so it can start again later.
    pub fn release_process(&self, tab: &TabId) -> bool {
        let handle = {
            let mut slots = self.slots.lock();
            let Some(slot) = slots.get_mut(tab) else {
                return false;
            };
            let Some(handle) = slot.handle.take() else {
                return false;
            };
            slot.started = false;
            handle
        };
        handle.dispose();
        true
    }

    /// Atomically set the focused tab and the one or two full-stream tabs for a window.
    pub fn set_view(&self, window: &omp_core::WindowId, focused: TabId, visible: &[TabId]) -> Result<()> {
        let view = ViewState::validate(focused, visible)?;
        self.views.lock().insert(window.clone(), view);
        Ok(())
    }

    /// Whether this tab receives every event, rather than only `tab:status`.
    pub fn forwards_full(&self, window: &omp_core::WindowId, tab: &TabId) -> bool {
        self.views
            .lock()
            .get(window)
            .map(|v| v.visible.contains(tab))
            .unwrap_or(false)
    }

    /// Tabs that must be started because they are now visible.
    pub fn visible_unstarted(&self, window: &omp_core::WindowId) -> Vec<TabId> {
        let visible = match self.views.lock().get(window) {
            Some(v) => v.visible.clone(),
            None => return Vec::new(),
        };
        let slots = self.slots.lock();
        visible
            .into_iter()
            .filter(|id| slots.get(id).map(|s| !s.started).unwrap_or(false))
            .collect()
    }

    /// Send one frame to one tab.
    pub fn dispatch(&self, tab: &TabId, frame: &Value) -> Result<()> {
        let slots = self.slots.lock();
        let slot = slots
            .get(tab)
            .ok_or_else(|| Error::msg(format!("unknown tab: {tab}")))?;
        match &slot.handle {
            Some(handle) => handle.send(frame),
            None => Ok(()),
        }
    }

    /// Send one frame to every started tab.
    pub fn broadcast(&self, frame: &Value) {
        let slots = self.slots.lock();
        for slot in slots.values() {
            if let Some(handle) = &slot.handle {
                let _ = handle.send(frame);
            }
        }
    }

    /// Claim a session for a tab (F-OWN). Fails when another live tab already owns it.
    pub fn claim_session(&self, tab: &TabId, session: &Path) -> Result<()> {
        let mut owners = self.session_owners.lock();
        if let Some(existing) = owners.get(session)
            && existing != tab
        {
            return Err(Error::msg(format!(
                "session already open in tab {existing}"
            )));
        }
        owners.insert(session.to_path_buf(), tab.clone());
        Ok(())
    }

    pub fn release_session(&self, session: &Path) {
        self.session_owners.lock().remove(session);
    }

    pub fn session_owner(&self, session: &Path) -> Option<TabId> {
        self.session_owners.lock().get(session).cloned()
    }

    /// Record where an outbound request came from, so its reply reaches the same tab.
    pub fn register_request(&self, request_id: &str, tab: &TabId) {
        self.requests
            .lock()
            .insert(request_id.to_owned(), tab.clone());
    }

    /// Resolve and forget the tab a reply belongs to.
    pub fn take_request(&self, request_id: &str) -> Option<TabId> {
        self.requests.lock().remove(request_id)
    }

    /// Look up the tab a request belongs to without consuming the route.
    pub fn request_owner(&self, request_id: &str) -> Option<TabId> {
        self.requests.lock().get(request_id).cloned()
    }

    pub fn pending_request_count(&self) -> usize {
        self.requests.lock().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeHandle {
        sent: Arc<Mutex<Vec<Value>>>,
        disposed: Arc<AtomicUsize>,
    }

    impl SidecarHandle for FakeHandle {
        fn send(&self, frame: &Value) -> Result<()> {
            self.sent.lock().push(frame.clone());
            Ok(())
        }
        fn dispose(self) {
            self.disposed.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn fake() -> (FakeHandle, Arc<Mutex<Vec<Value>>>, Arc<AtomicUsize>) {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let disposed = Arc::new(AtomicUsize::new(0));
        (
            FakeHandle {
                sent: sent.clone(),
                disposed: disposed.clone(),
            },
            sent,
            disposed,
        )
    }

    fn tab(name: &str) -> TabId {
        TabId::new(name)
    }

    #[test]
    fn deferred_tab_does_not_start_until_visible() {
        let pool: SidecarPool<FakeHandle> = SidecarPool::new();
        pool.register_deferred(tab("a"));
        assert!(!pool.is_started(&tab("a")));
        let window = omp_core::WindowId::new("w1");
        pool.set_view(&window, tab("a"), &[tab("a")]).unwrap();
        assert_eq!(pool.visible_unstarted(&window), vec![tab("a")]);
        let (handle, _, _) = fake();
        pool.start_deferred(&tab("a"), handle).unwrap();
        assert!(pool.is_started(&tab("a")));
        assert!(pool.visible_unstarted(&window).is_empty());
    }

    #[test]
    fn view_rejects_more_than_two_or_missing_focus() {
        let pool: SidecarPool<FakeHandle> = SidecarPool::new();
        let window = omp_core::WindowId::new("w1");
        assert!(pool
            .set_view(&window, tab("a"), &[tab("a"), tab("b"), tab("c")])
            .is_err());
        assert!(pool.set_view(&window, tab("a"), &[tab("b")]).is_err());
        assert!(pool.set_view(&window, tab("a"), &[]).is_err());
        assert!(pool.set_view(&window, tab("a"), &[tab("a"), tab("b")]).is_ok());
    }

    #[test]
    fn only_visible_tabs_forward_full() {
        let pool: SidecarPool<FakeHandle> = SidecarPool::new();
        for name in ["a", "b", "c"] {
            let (handle, _, _) = fake();
            pool.attach(tab(name), handle);
        }
        let window = omp_core::WindowId::new("w1");
        pool.set_view(&window, tab("a"), &[tab("a"), tab("b")]).unwrap();
        assert!(pool.forwards_full(&window, &tab("a")));
        assert!(pool.forwards_full(&window, &tab("b")));
        assert!(!pool.forwards_full(&window, &tab("c")));
    }

    #[test]
    fn duplicate_visible_ids_collapse() {
        let view = ViewState::validate(tab("a"), &[tab("a"), tab("a")]).unwrap();
        assert_eq!(view.visible, vec![tab("a")]);
    }

    #[test]
    fn dispatch_targets_one_tab_only() {
        let pool: SidecarPool<FakeHandle> = SidecarPool::new();
        let (a, a_sent, _) = fake();
        let (b, b_sent, _) = fake();
        pool.attach(tab("a"), a);
        pool.attach(tab("b"), b);
        pool.dispatch(&tab("a"), &serde_json::json!({ "type": "x" })).unwrap();
        assert_eq!(a_sent.lock().len(), 1);
        assert!(b_sent.lock().is_empty());
    }

    #[test]
    fn close_disposes_and_releases_ownership() {
        let pool: SidecarPool<FakeHandle> = SidecarPool::new();
        let (handle, _, disposed) = fake();
        pool.attach(tab("a"), handle);
        pool.claim_session(&tab("a"), Path::new("C:/s/x.jsonl")).unwrap();
        assert!(pool.close(&tab("a")));
        assert_eq!(disposed.load(Ordering::Relaxed), 1);
        assert_eq!(pool.tab_count(), 0);
        assert_eq!(pool.session_owner(Path::new("C:/s/x.jsonl")), None);
    }

    #[test]
    fn session_cannot_be_claimed_by_two_tabs() {
        let pool: SidecarPool<FakeHandle> = SidecarPool::new();
        let path = Path::new("C:/s/x.jsonl");
        pool.claim_session(&tab("a"), path).unwrap();
        let err = pool.claim_session(&tab("b"), path).unwrap_err();
        assert!(err.to_string().contains("already open in tab a"));
        assert_eq!(pool.session_owner(path), Some(tab("a")));
        // Re-claiming from the same tab is a no-op, and releasing frees it.
        pool.claim_session(&tab("a"), path).unwrap();
        pool.release_session(path);
        pool.claim_session(&tab("b"), path).unwrap();
    }

    #[test]
    fn requests_route_back_to_origin_tab_once() {
        let pool: SidecarPool<FakeHandle> = SidecarPool::new();
        pool.register_request("req-1", &tab("a"));
        assert_eq!(pool.take_request("req-1"), Some(tab("a")));
        assert_eq!(pool.take_request("req-1"), None);
        assert_eq!(pool.pending_request_count(), 0);
    }

    #[test]
    fn closing_a_tab_drops_its_pending_requests() {
        let pool: SidecarPool<FakeHandle> = SidecarPool::new();
        let (handle, _, _) = fake();
        pool.attach(tab("a"), handle);
        pool.register_request("req-1", &tab("a"));
        pool.close(&tab("a"));
        assert_eq!(pool.take_request("req-1"), None);
    }
}
