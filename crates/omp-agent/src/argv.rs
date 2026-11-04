//! Sidecar argv construction gated by probed capabilities.
//!
//! Mirrors `src/main/sidecar.ts` argv assembly plus the capability probe table from the plan:
//! flags that a stock binary does not understand are never emitted.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Capabilities {
    /// `get_settings_schema` answers with `success: true`.
    pub settings_schema: bool,
    pub terminal: bool,
    pub browser_host: bool,
    pub jobs: bool,
    /// `--help` mentions `--chat`.
    pub chat_flag: bool,
    /// `--help` mentions `--no-auto-resume`.
    pub no_auto_resume: bool,
    /// `--help` mentions `--session`.
    pub session_flag: bool,
    /// `stats --help` mentions `--no-open`.
    pub stats_no_open: bool,
}

impl Capabilities {
    /// A fork binary: everything supported.
    pub fn fork() -> Self {
        Self {
            settings_schema: true,
            terminal: true,
            browser_host: true,
            jobs: true,
            chat_flag: true,
            no_auto_resume: true,
            session_flag: true,
            stats_no_open: true,
        }
    }

    /// Stock 18.15.0: none of the fork-only surfaces exist.
    pub fn stock() -> Self {
        Self::default()
    }

    /// Derive flags from `--help` text plus the RPC probe results.
    pub fn from_help(help: &str, stats_help: &str, rpc: RpcProbe) -> Self {
        Self {
            settings_schema: rpc.settings_schema,
            terminal: rpc.terminal,
            browser_host: rpc.browser_host,
            jobs: rpc.jobs,
            chat_flag: help.contains("--chat"),
            no_auto_resume: help.contains("--no-auto-resume"),
            session_flag: help.contains("--session"),
            stats_no_open: stats_help.contains("--no-open"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RpcProbe {
    pub settings_schema: bool,
    pub terminal: bool,
    pub browser_host: bool,
    pub jobs: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabKind {
    Agent,
    Chat,
}

#[derive(Debug, Clone, Default)]
pub struct LaunchOptions {
    pub cwd: PathBuf,
    pub kind: TabKind,
    /// Session to resume, when restoring a saved tab.
    pub resume_session: Option<PathBuf>,
    /// Start a brand-new session instead of auto-resuming the cwd's last one.
    pub fresh_launch: bool,
    /// Per-tab isolated session directory, used when `--no-auto-resume` is unavailable.
    pub session_dir: Option<PathBuf>,
    /// Flags from the launch profile, already denylist-stripped upstream.
    pub user_flags: Vec<String>,
}

impl Default for TabKind {
    fn default() -> Self {
        TabKind::Agent
    }
}

/// Flags the GUI controls itself; a launch profile may not supply them.
pub const DENYLISTED_FLAGS: &[&str] = &[
    "--session",
    "--mode",
    "--print",
    "--print-thoughts",
    "--export",
    "--cwd",
    "--resume",
    "--fork",
    "--help",
    "--version",
    "--no-pty",
    "--no-title",
    "--no-auto-resume",
    "--api-key",
    "--chat",
];

/// Denylisted flags that consume a separate value token. The rest are boolean switches;
/// `--resume`/`--session` take an optional value and are treated as value-taking so a
/// smuggled value never survives as a stray positional argument.
pub const DENYLISTED_WITH_VALUE: &[&str] = &[
    "--session",
    "--mode",
    "--export",
    "--cwd",
    "--resume",
    "--fork",
    "--api-key",
];

/// Non-denylisted flags that consume a separate value token. Their value is DATA (a prompt, a
/// path, a tool list) and must never be inspected as a smuggled flag.
pub const VALUE_FLAGS: &[&str] = &[
    "--system-prompt",
    "--append-system-prompt",
    "--add-dir",
    "--tools",
    "--profile",
    "--session-dir",
    "--config",
];

/// Flags a particular spawn denies on top of the tab denylist (scheduled runs).
#[derive(Debug, Clone, Default)]
pub struct FlagDenylist {
    pub names: Vec<String>,
    /// The subset of `names` that consume a separate value token.
    pub with_value: Vec<String>,
}

impl FlagDenylist {
    pub fn new(names: &[&str], with_value: &[&str]) -> Self {
        FlagDenylist {
            names: names.iter().map(|s| (*s).to_owned()).collect(),
            with_value: with_value.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    fn denies(&self, name: &str) -> bool {
        self.names.iter().any(|n| n == name)
    }

    fn takes_value(&self, name: &str) -> bool {
        self.with_value.iter().any(|n| n == name)
    }
}

fn has(list: &[&str], name: &str) -> bool {
    list.contains(&name)
}

/// Drop denylisted flags (and their separate value tokens) from a flag list.
///
/// Handles both `--flag value` and `--flag=value`. Pair-aware: the value of a non-denylisted
/// valued flag is pushed verbatim and never inspected, so a legitimate value that merely looks
/// like a protected flag survives.
pub fn strip_denylisted_flags(flags: &[String], extra: Option<&FlagDenylist>) -> Vec<String> {
    let mut out = Vec::with_capacity(flags.len());
    let mut i = 0;
    while i < flags.len() {
        let token = flags[i].as_str();
        if !token.starts_with("--") {
            out.push(flags[i].clone());
            i += 1;
            continue;
        }
        let eq = token.find('=');
        let name = match eq {
            Some(pos) => &token[..pos],
            None => token,
        };
        let denied = has(DENYLISTED_FLAGS, name) || extra.is_some_and(|e| e.denies(name));
        if denied {
            let takes_value = has(DENYLISTED_WITH_VALUE, name)
                || extra.is_some_and(|e| e.takes_value(name));
            if eq.is_none()
                && takes_value
                && let Some(next) = flags.get(i + 1)
                && !next.starts_with('-')
            {
                i += 1;
            }
            i += 1;
            continue;
        }
        out.push(flags[i].clone());
        if eq.is_none()
            && has(VALUE_FLAGS, name)
            && let Some(next) = flags.get(i + 1)
        {
            out.push(next.clone());
            i += 1;
        }
        i += 1;
    }
    out
}

/// Build the sidecar argv for one tab.
pub fn build(caps: &Capabilities, opts: &LaunchOptions) -> Vec<String> {
    let mut args: Vec<String> = vec!["--mode".into(), "rpc-ui".into()];

    match (&opts.resume_session, caps.session_flag) {
        (Some(path), true) => {
            args.push("--session".into());
            args.push(path.to_string_lossy().into_owned());
        }
        (Some(path), false) => {
            args.push("--resume".into());
            args.push(path.to_string_lossy().into_owned());
        }
        (None, _) => {
            if opts.fresh_launch && caps.no_auto_resume {
                args.push("--no-auto-resume".into());
            }
        }
    }

    if opts.kind == TabKind::Chat {
        if caps.chat_flag {
            args.push("--chat".into());
        } else {
            // Stock fallback: isolate chats by their own session directory.
            if let Some(dir) = &opts.session_dir {
                args.push("--session-dir".into());
                args.push(dir.to_string_lossy().into_owned());
            }
        }
    } else if opts.resume_session.is_none()
        && !caps.no_auto_resume
        && let Some(dir) = &opts.session_dir
    {
        args.push("--session-dir".into());
        args.push(dir.to_string_lossy().into_owned());
    }

    args.extend(strip_denylisted_flags(&opts.user_flags, None));
    args
}

/// `stats` subcommand argv for the dashboard server.
pub fn build_stats(caps: &Capabilities) -> Vec<String> {
    let mut args: Vec<String> = vec!["stats".into(), "--host".into(), "127.0.0.1".into(), "--port".into(), "0".into()];
    if caps.stats_no_open {
        args.push("--no-open".into());
    }
    args
}

/// Resolve the sidecar binary: env override, packaged resources, then the dev checkout.
pub fn resolve_sidecar(exe_dir: Option<&Path>, dev_root: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("OMP_SIDECAR") {
        let p = PathBuf::from(path);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(dir) = exe_dir {
        for candidate in [
            dir.join("resources").join("omp.exe"),
            dir.join("omp.exe"),
        ] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    if let Some(root) = dev_root {
        let candidate = root.join("resources").join("omp.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(kind: TabKind) -> LaunchOptions {
        LaunchOptions {
            cwd: PathBuf::from("D:/w"),
            kind,
            ..LaunchOptions::default()
        }
    }

    #[test]
    fn fork_agent_tab_uses_no_auto_resume() {
        let args = build(&Capabilities::fork(), &LaunchOptions { fresh_launch: true, ..opts(TabKind::Agent) });
        assert_eq!(args, vec!["--mode", "rpc-ui", "--no-auto-resume"]);
    }

    #[test]
    fn stock_agent_tab_falls_back_to_session_dir() {
        let args = build(
            &Capabilities::stock(),
            &LaunchOptions {
                fresh_launch: true,
                session_dir: Some(PathBuf::from("D:/data/tabs/t1")),
                ..opts(TabKind::Agent)
            },
        );
        assert_eq!(
            args,
            vec!["--mode", "rpc-ui", "--session-dir", "D:/data/tabs/t1"]
        );
        assert!(!args.iter().any(|a| a == "--no-auto-resume"));
    }

    #[test]
    fn fork_chat_tab_adds_chat_flag() {
        let args = build(&Capabilities::fork(), &opts(TabKind::Chat));
        assert!(args.contains(&"--chat".to_string()));
    }

    #[test]
    fn stock_chat_tab_uses_isolated_session_dir() {
        let args = build(
            &Capabilities::stock(),
            &LaunchOptions {
                session_dir: Some(PathBuf::from("D:/data/chats")),
                ..opts(TabKind::Chat)
            },
        );
        assert!(!args.iter().any(|a| a == "--chat"));
        assert!(args.windows(2).any(|w| w == ["--session-dir", "D:/data/chats"]));
    }

    #[test]
    fn resume_uses_session_flag_only_when_supported() {
        let resume = Some(PathBuf::from("C:/sessions/s.jsonl"));
        let fork = build(
            &Capabilities::fork(),
            &LaunchOptions { resume_session: resume.clone(), ..opts(TabKind::Agent) },
        );
        assert!(fork.windows(2).any(|w| w == ["--session", "C:/sessions/s.jsonl"]));

        let stock = build(
            &Capabilities::stock(),
            &LaunchOptions { resume_session: resume, ..opts(TabKind::Agent) },
        );
        assert!(stock.windows(2).any(|w| w == ["--resume", "C:/sessions/s.jsonl"]));
        assert!(!stock.iter().any(|a| a == "--session"));
    }

    #[test]
    fn strips_denylisted_pairs_but_keeps_lookalike_values() {
        let flags: Vec<String> = [
            "--session",
            "C:/other.jsonl",
            "--system-prompt",
            "--mode",
            "--tools",
            "read,grep",
            "--model",
            "x/y",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let out = strip_denylisted_flags(&flags, None);
        assert_eq!(
            out,
            vec!["--system-prompt", "--mode", "--tools", "read,grep", "--model", "x/y"]
        );
    }

    #[test]
    fn strips_equals_form_and_boolean_switch_leaves_value() {
        let flags: Vec<String> = [
            "--mode=print",
            "--cwd=/etc",
            "--api-key=sk-1",
            "--chat=1",
            "--no-pty",
            "keepme",
            "-x",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let out = strip_denylisted_flags(&flags, None);
        // `--no-pty` is a boolean switch: its following token is not consumed.
        // `-x` is flag-looking, so it is never swallowed either.
        assert_eq!(out, vec!["keepme", "-x"]);
    }

    #[test]
    fn extra_denylist_handles_valued_and_boolean_flags() {
        let extra = FlagDenylist::new(&["--no-tools", "--session-dir"], &["--session-dir"]);
        let flags: Vec<String> = ["--no-tools", "x", "--session-dir", "D:/s", "--keep", "y"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let out = strip_denylisted_flags(&flags, Some(&extra));
        assert_eq!(out, vec!["x", "--keep", "y"]);
    }

    #[test]
    fn stats_argv_respects_no_open_capability() {
        assert!(build_stats(&Capabilities::fork()).contains(&"--no-open".to_string()));
        assert!(!build_stats(&Capabilities::stock()).contains(&"--no-open".to_string()));
    }

    #[test]
    fn capabilities_from_help_text() {
        let caps = Capabilities::from_help(
            "--chat --no-auto-resume --session",
            "stats --no-open",
            RpcProbe { settings_schema: true, terminal: true, browser_host: false, jobs: false },
        );
        assert!(caps.chat_flag && caps.no_auto_resume && caps.session_flag && caps.stats_no_open);
        assert!(caps.settings_schema && caps.terminal);
        assert!(!caps.browser_host && !caps.jobs);
    }
}
