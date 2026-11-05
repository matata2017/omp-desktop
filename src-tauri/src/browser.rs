//! The Browser panel's embedded pages.
//!
//! Blueprint: `src/main/browser-host.ts`. Each page is a real child webview layered over the
//! panel's viewport, owned per tab. The agent reaches these pages through the sidecar's
//! `browser_host_*` relay, which lives in the agent process; this module only needs to keep the
//! pages, their geometry and the navigation commands in sync with what the renderer asks for.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;
use serde_json::{Value, json};
use tauri::{LogicalPosition, LogicalSize, WebviewBuilder, WebviewUrl, Window};

type Wry = tauri::Wry;

/// A page as the renderer describes it (`IpcBrowserPage`).
struct Page {
    id: u64,
    url: String,
    title: String,
    loading: bool,
    can_go_back: bool,
    can_go_forward: bool,
    opened_by: &'static str,
    agent_driving: bool,
    error: Option<String>,
    webview: tauri::Webview<Wry>,
}

impl Page {
    fn to_value(&self) -> Value {
        let mut value = json!({
            "pageId": self.id,
            "url": self.url,
            "title": self.title,
            "loading": self.loading,
            "canGoBack": self.can_go_back,
            "canGoForward": self.can_go_forward,
            "openedBy": self.opened_by,
            "agentDriving": self.agent_driving,
        });
        if let Some(error) = &self.error {
            value["error"] = json!(error);
        }
        value
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The renderer sends the address-bar text verbatim, so a bare host must still parse: anything
/// without a scheme is treated as `https://`.
pub fn normalize_url(input: &str) -> String {
    let text = input.trim();
    if text.is_empty() {
        return String::new();
    }
    let has_scheme = text
        .split_once("://")
        .is_some_and(|(scheme, _)| !scheme.is_empty() && scheme.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.'));
    if has_scheme || text.starts_with("about:") || text.starts_with("data:") || text.starts_with("file:") {
        text.to_owned()
    } else {
        format!("https://{text}")
    }
}

impl Bounds {
    /// Parse `{x,y,width,height}`; a missing or non-positive box means "hidden".
    pub fn from_value(value: Option<&Value>) -> Option<Self> {
        let value = value?.as_object()?;
        let number = |key: &str| value.get(key).and_then(Value::as_f64).unwrap_or(0.0);
        let bounds = Bounds {
            x: number("x"),
            y: number("y"),
            width: number("width"),
            height: number("height"),
        };
        (bounds.width > 0.0 && bounds.height > 0.0).then_some(bounds)
    }
}

#[derive(Default)]
pub struct BrowserHost {
    pages: std::sync::Arc<Mutex<HashMap<String, Vec<Page>>>>,
    /// The same records as `pages`, cloned into the `on_page_load` callback (which cannot borrow
    /// `self`). One map, two handles, so the callback's updates are visible to `state()`.
    loads: std::sync::Arc<Mutex<HashMap<String, Vec<Page>>>>,
    active: Mutex<HashMap<String, u64>>,
    viewport: Mutex<HashMap<String, Bounds>>,
    next_id: AtomicU64,
}

fn label_for(tab: &str, page_id: u64) -> String {
    // Webview labels must be unique per window; the tab id is already unique.
    format!("browser-{tab}-{page_id}")
}

impl BrowserHost {
    pub fn new() -> Self {
        let loads = std::sync::Arc::new(Mutex::new(HashMap::new()));
        BrowserHost {
            pages: loads.clone(),
            loads,
            active: Mutex::new(HashMap::new()),
            viewport: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(0),
        }
    }

    fn ensure_page<'a>(pages: &'a mut HashMap<String, Vec<Page>>, tab: &str) -> &'a mut Vec<Page> {
        pages.entry(tab.to_owned()).or_default()
    }

    pub fn state(&self, tab: &str, agent_linked: bool) -> Value {
        let pages = self.pages.lock();
        let entries: Vec<Value> = pages
            .get(tab)
            .map(|list| list.iter().map(Page::to_value).collect())
            .unwrap_or_default();
        let active = self.active.lock().get(tab).copied();
        json!({
            "tabId": tab,
            "pages": entries,
            "activePageId": active,
            "agentLinked": agent_linked,
        })
    }

    /// Open a URL in a new page for `tab`, over the tab's current viewport (hidden when none).
    ///
    /// The page is a child webview layered over the app's own; Tauri's unstable `add_child` builds
    /// it as a separate child HWND, which is why a second WebView2 controller works here.
    pub fn open(
        &self,
        app: &tauri::AppHandle,
        window: &Window,
        tab: &str,
        url: &str,
        opened_by: &'static str,
    ) -> Result<Value, String> {
        let target = normalize_url(url);
        let parsed = tauri::Url::parse(&target).map_err(|e| format!("invalid url: {e}"))?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let bounds = self.viewport.lock().get(tab).copied().unwrap_or(Bounds {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        });
        let loads = self.loads.clone();
        let tab_for_load = tab.to_owned();
        // Start blank and navigate after the record exists: a load that finishes inside `add_child`
        // would look up a page the map does not have yet and be dropped.
        let webview = window
            .add_child(
                WebviewBuilder::new(label_for(tab, id), WebviewUrl::App("about:blank".into()))
                    .on_page_load({
                        let loads = loads.clone();
                        let tab_for_load = tab_for_load.clone();
                        move |_webview, payload| {
                            let url = payload.url().to_string();
                            let loading = matches!(payload.event(), tauri::webview::PageLoadEvent::Started);
                            if let Some(list) = loads.lock().get_mut(&tab_for_load)
                                && let Some(page) = list.iter_mut().find(|page| page.id == id)
                            {
                                page.url = url;
                                page.loading = loading;
                            }
                            // No `show()` here: the panel owns visibility through set_viewport, and
                            // re-showing on a load would put the page back over a dialog.
                        }
                    })
                    .on_navigation({
                        let loads = loads.clone();
                        let tab_for_nav = tab_for_load.clone();
                        move |url| {
                            if let Some(list) = loads.lock().get_mut(&tab_for_nav)
                                && let Some(page) = list.iter_mut().find(|page| page.id == id)
                            {
                                page.url = url.to_string();
                            }
                            true
                        }
                    })
                    .on_document_title_changed({
                        let loads = loads.clone();
                        let tab_for_title = tab_for_load.clone();
                        move |_webview, title| {
                            if let Some(list) = loads.lock().get_mut(&tab_for_title)
                                && let Some(page) = list.iter_mut().find(|page| page.id == id)
                            {
                                page.title = title;
                            }
                        }
                    }),
                LogicalPosition::new(bounds.x, bounds.y),
                LogicalSize::new(bounds.width, bounds.height),
            )
            .map_err(|e| format!("failed to create page: {e}"))?;
        if self.viewport.lock().get(tab).is_none() {
            let _ = webview.hide();
        } else {
            let _ = webview.show();
        }

        let page = Page {
            id,
            url: parsed.to_string(),
            title: String::new(),
            loading: true,
            can_go_back: false,
            can_go_forward: false,
            opened_by,
            agent_driving: opened_by == "agent",
            error: None,
            webview,
        };
        Self::ensure_page(&mut self.pages.lock(), tab).push(page);
        self.active.lock().insert(tab.to_owned(), id);
        self.navigate_page(tab, id, target)?;
        self.show_only(tab, id);
        self.emit(app, tab);
        // The renderer applies this result as a whole `IpcBrowserState`, not a page.
        Ok(self.state(tab, false))
    }

    /// Navigate an existing page by id.
    fn navigate_page(&self, tab: &str, page_id: u64, url: String) -> Result<(), String> {
        let parsed = tauri::Url::parse(&url).map_err(|e| format!("invalid url: {e}"))?;
        let mut pages = self.pages.lock();
        let page = pages
            .get_mut(tab)
            .and_then(|list| list.iter_mut().find(|page| page.id == page_id))
            .ok_or_else(|| format!("unknown page: {page_id}"))?;
        page.webview.navigate(parsed).map_err(|e| e.to_string())?;
        page.url = url;
        page.loading = true;
        page.error = None;
        Ok(())
    }

    /// Run one `IpcBrowserAction` against a page. Returns the tab's new state.
    pub fn action(
        &self,
        app: Option<&tauri::AppHandle>,
        tab: &str,
        page_id: u64,
        action: &Value,
        agent_linked: bool,
    ) -> Result<Value, String> {
        let kind = action.get("action").and_then(Value::as_str).unwrap_or_default();
        let mut pages = self.pages.lock();

        if kind == "select" {
            let known = pages
                .get(tab)
                .is_some_and(|list| list.iter().any(|page| page.id == page_id));
            drop(pages);
            if known {
                self.active.lock().insert(tab.to_owned(), page_id);
                self.show_only(tab, page_id);
            }
            return Ok(self.state(tab, agent_linked));
        }

        let Some(page) = pages
            .get_mut(tab)
            .and_then(|list| list.iter_mut().find(|page| page.id == page_id))
        else {
            return Err(format!("unknown page: {page_id}"));
        };

        match kind {
            "navigate" => {
                let url = normalize_url(action.get("url").and_then(Value::as_str).unwrap_or_default());
                let parsed = tauri::Url::parse(&url).map_err(|e| format!("invalid url: {e}"))?;
                page.webview
                    .navigate(parsed.clone())
                    .map_err(|e| e.to_string())?;
                page.url = parsed.to_string();
                page.loading = true;
                page.error = None;
            }
            "back" => {
                page.webview.eval("history.back()").map_err(|e| e.to_string())?;
            }
            "forward" => {
                page.webview.eval("history.forward()").map_err(|e| e.to_string())?;
            }
            "reload" => {
                page.webview.eval("location.reload()").map_err(|e| e.to_string())?;
                page.loading = true;
            }
            "stop" => {
                page.webview.eval("window.stop()").map_err(|e| e.to_string())?;
                page.loading = false;
            }
            "devtools" => page.webview.open_devtools(),
            "external" => {
                let _ = tauri_plugin_opener::open_url(page.url.clone(), None::<&str>);
            }
            "close" => {
                let _ = page.webview.close();
                let id = page.id;
                if let Some(list) = pages.get_mut(tab) {
                    list.retain(|page| page.id != id);
                }
                let mut active = self.active.lock();
                if active.get(tab).copied() == Some(id) {
                    let next = pages.get(tab).and_then(|list| list.last()).map(|page| page.id);
                    match next {
                        Some(next) => {
                            active.insert(tab.to_owned(), next);
                        }
                        None => {
                            active.remove(tab);
                        }
                    }
                }
                drop(active);
                drop(pages);
                if let Some(app) = app {
                    self.emit(app, tab);
                }
                return Ok(self.state(tab, agent_linked));
            }
            // Find-in-page runs inside the page; the harness hooks the browser's own find bar.
            "find" | "stopFind" => {}
            other => return Err(format!("unknown browser action: {other}")),
        }

        let snapshot = page.to_value();
        drop(pages);
        if let Some(app) = app {
            self.emit(app, tab);
        }
        let mut state = self.state(tab, agent_linked);
        state["pages"] = json!(
            state["pages"]
                .as_array()
                .map(|list| list
                    .iter()
                    .map(|page| if page["pageId"] == snapshot["pageId"] {
                        snapshot.clone()
                    } else {
                        page.clone()
                    })
                    .collect::<Vec<_>>())
                .unwrap_or_default()
        );
        Ok(state)
    }

    /// Raise one page and hide its siblings, so multi-page chips never stack webviews.
    fn show_only(&self, tab: &str, page_id: u64) {
        let pages = self.pages.lock();
        if let Some(list) = pages.get(tab) {
            for page in list {
                if page.id == page_id {
                    let _ = page.webview.show();
                } else {
                    let _ = page.webview.hide();
                }
            }
        }
    }

    /// Push the tab's browser state to the renderer; every mutation goes through here so the
    /// address bar, chips and nav buttons always see the current page.
    fn emit(&self, app: &tauri::AppHandle, tab: &str) {
        use tauri::Emitter;
        let _ = app.emit(
            "browser:state",
            self.state(tab, false),
        );
    }

    /// Place (or hide, with `None`) the tab's active page over the panel viewport.
    pub fn set_viewport(&self, tab: &str, bounds: Option<Bounds>) -> Result<(), String> {
        let mut viewport = self.viewport.lock();
        match bounds {
            Some(bounds) => {
                viewport.insert(tab.to_owned(), bounds);
                drop(viewport);
                let active = self.active.lock().get(tab).copied();
                let pages = self.pages.lock();
                if let Some(page) = pages
                    .get(tab)
                    .and_then(|list| list.iter().find(|page| Some(page.id) == active))
                {
                    page.webview
                        .set_position(LogicalPosition::new(bounds.x, bounds.y))
                        .map_err(|e| e.to_string())?;
                    page.webview
                        .set_size(LogicalSize::new(bounds.width, bounds.height))
                        .map_err(|e| e.to_string())?;
                    page.webview.show().map_err(|e| e.to_string())?;
                }
                Ok(())
            }
            None => {
                viewport.remove(tab);
                let pages = self.pages.lock();
                if let Some(list) = pages.get(tab) {
                    for page in list {
                        let _ = page.webview.hide();
                    }
                }
                Ok(())
            }
        }
    }

    /// Close every page a tab owned; called when the tab closes or the app quits.
    pub fn close_tab(&self, tab: &str) {
        let removed = self.pages.lock().remove(tab);
        self.active.lock().remove(tab);
        self.viewport.lock().remove(tab);
        if let Some(list) = removed {
            for page in list {
                let _ = page.webview.close();
            }
        }
    }

    pub fn close_all(&self) {
        let tabs: Vec<String> = self.pages.lock().keys().cloned().collect();
        for tab in tabs {
            self.close_tab(&tab);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_hosts_get_https() {
        assert_eq!(normalize_url("example.com"), "https://example.com");
        assert_eq!(normalize_url(" example.com/path "), "https://example.com/path");
        assert_eq!(normalize_url("localhost:3000"), "https://localhost:3000");
        assert_eq!(normalize_url("http://a.test"), "http://a.test");
        assert_eq!(normalize_url("about:blank"), "about:blank");
        assert_eq!(normalize_url(""), "");
    }

    #[test]
    fn bounds_need_a_positive_box() {
        assert!(Bounds::from_value(None).is_none());
        assert!(Bounds::from_value(Some(&json!({ "x": 1, "y": 2 }))).is_none());
        let bounds = Bounds::from_value(Some(&json!({ "x": 10, "y": 20, "width": 300, "height": 400 })))
            .expect("positive box");
        assert_eq!(bounds.x, 10.0);
        assert_eq!(bounds.height, 400.0);
    }

    #[test]
    fn state_is_empty_before_any_page() {
        let host = BrowserHost::new();
        let state = host.state("tab-1", false);
        assert_eq!(state["tabId"], "tab-1");
        assert_eq!(state["pages"].as_array().unwrap().len(), 0);
        assert_eq!(state["activePageId"], Value::Null);
        assert_eq!(state["agentLinked"], false);
    }

    #[test]
    fn unknown_page_actions_are_rejected() {
        let host = BrowserHost::new();
        let err = host
            .action(None, "tab-1", 99, &json!({ "action": "reload" }), false)
            .unwrap_err();
        assert_eq!(err, "unknown page: 99");
    }

    #[test]
    fn selecting_a_page_that_does_not_exist_is_a_no_op() {
        let host = BrowserHost::new();
        let state = host.action(None, "tab-1", 7, &json!({ "action": "select" }), false).unwrap();
        assert_eq!(state["activePageId"], Value::Null);
        host.close_tab("tab-1");
        host.close_all();
    }
}
