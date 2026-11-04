//! Contract gate: the argv handed to the sidecar may only contain flags the probed capabilities
//! allow, and no user-supplied flag may override a GUI-controlled one.
//!
//! This mirrors the plan's `argv_matches_capabilities` test: three capability matrices (fork,
//! stock, partially-missing) crossed with the launch shapes the GUI actually produces.

use std::path::PathBuf;

use omp_agent::argv::{
    Capabilities, DENYLISTED_FLAGS, FlagDenylist, LaunchOptions, TabKind, VALUE_FLAGS, build,
    build_stats, strip_denylisted_flags,
};

fn options(kind: TabKind) -> LaunchOptions {
    LaunchOptions {
        cwd: PathBuf::from("D:/w"),
        kind,
        ..LaunchOptions::default()
    }
}

fn contains_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn contains_pair(args: &[String], flag: &str, value: &str) -> bool {
    args.windows(2).any(|pair| pair[0] == flag && pair[1] == value)
}

#[test]
fn fork_matrix_uses_the_modern_flags_and_never_the_fallbacks() {
    let caps = Capabilities::fork();

    let fresh = build(
        &caps,
        &LaunchOptions {
            fresh_launch: true,
            ..options(TabKind::Agent)
        },
    );
    assert!(contains_flag(&fresh, "--no-auto-resume"));
    assert!(!contains_flag(&fresh, "--session-dir"));

    let resumed = build(
        &caps,
        &LaunchOptions {
            resume_session: Some(PathBuf::from("C:/s/a.jsonl")),
            ..options(TabKind::Agent)
        },
    );
    assert!(contains_pair(&resumed, "--session", "C:/s/a.jsonl"));
    assert!(!contains_flag(&resumed, "--resume"));

    let chat = build(&caps, &options(TabKind::Chat));
    assert!(contains_flag(&chat, "--chat"));
    assert!(!contains_flag(&chat, "--session-dir"));

    assert!(contains_flag(&build_stats(&caps), "--no-open"));
}

#[test]
fn stock_matrix_falls_back_without_emitting_unknown_flags() {
    let caps = Capabilities::stock();

    let fresh = build(
        &caps,
        &LaunchOptions {
            fresh_launch: true,
            session_dir: Some(PathBuf::from("D:/data/tabs/t1")),
            ..options(TabKind::Agent)
        },
    );
    assert!(!contains_flag(&fresh, "--no-auto-resume"));
    assert!(contains_pair(&fresh, "--session-dir", "D:/data/tabs/t1"));

    let resumed = build(
        &caps,
        &LaunchOptions {
            resume_session: Some(PathBuf::from("C:/s/a.jsonl")),
            ..options(TabKind::Agent)
        },
    );
    assert!(contains_pair(&resumed, "--resume", "C:/s/a.jsonl"));
    assert!(!contains_flag(&resumed, "--session"));

    let chat = build(
        &caps,
        &LaunchOptions {
            session_dir: Some(PathBuf::from("D:/data/chats")),
            ..options(TabKind::Chat)
        },
    );
    assert!(!contains_flag(&chat, "--chat"));
    assert!(contains_pair(&chat, "--session-dir", "D:/data/chats"));

    assert!(!contains_flag(&build_stats(&caps), "--no-open"));
}

#[test]
fn each_capability_gates_only_its_own_flag() {
    // A binary that understands `--chat` but not `--session`: the chat tab may use the flag,
    // the resume path must still fall back.
    let caps = Capabilities {
        chat_flag: true,
        session_flag: false,
        no_auto_resume: false,
        ..Capabilities::stock()
    };
    let chat = build(&caps, &options(TabKind::Chat));
    assert!(contains_flag(&chat, "--chat"));

    let resumed = build(
        &caps,
        &LaunchOptions {
            resume_session: Some(PathBuf::from("C:/s/a.jsonl")),
            ..options(TabKind::Agent)
        },
    );
    assert!(contains_pair(&resumed, "--resume", "C:/s/a.jsonl"));
    assert!(!contains_flag(&resumed, "--session"));
    assert!(!contains_flag(&resumed, "--no-auto-resume"));
}

#[test]
fn no_user_flag_can_override_a_gui_controlled_one() {
    // A value for the flags that consume a separate token, and both spellings for all of them.
    let valued = ["--session", "--mode", "--export", "--cwd", "--resume", "--fork", "--api-key"];
    let mut user_flags: Vec<String> = Vec::new();
    for flag in DENYLISTED_FLAGS {
        user_flags.push((*flag).to_string());
        if valued.contains(flag) {
            user_flags.push("smuggled".to_string());
        }
        user_flags.push(format!("{flag}=smuggled"));
    }
    // Data that merely looks like a protected flag must survive untouched.
    user_flags.extend(
        ["--system-prompt", "--mode", "--tools", "read,grep", "--model", "x/y"]
            .iter()
            .map(|flag| (*flag).to_string()),
    );

    let args = build(
        &Capabilities::fork(),
        &LaunchOptions {
            fresh_launch: true,
            user_flags,
            ..options(TabKind::Agent)
        },
    );

    // The GUI prefix is exactly `--mode rpc-ui --no-auto-resume`.
    assert_eq!(&args[..2], ["--mode", "rpc-ui"]);
    assert_eq!(args[2], "--no-auto-resume");

    // Walk the user-supplied tail the way the sidecar's parser does: skip the value of every
    // value-taking flag, then treat whatever is left as a real flag. None may be denied — the
    // GUI's own `--mode rpc-ui --no-auto-resume` prefix is above this point and stays intact.
    let tail = &args[3..];
    let mut real_flags: Vec<&str> = Vec::new();
    let mut index = 0;
    while index < tail.len() {
        let token = tail[index].as_str();
        if token.starts_with('-') {
            real_flags.push(token);
            if VALUE_FLAGS.contains(&token) {
                index += 1;
            }
        }
        index += 1;
    }
    for flag in &real_flags {
        assert!(
            !DENYLISTED_FLAGS.contains(flag),
            "{flag} reached the sidecar as a flag: {real_flags:?}"
        );
    }
    // And no denied flag appears in the `--flag=value` spelling anywhere.
    for flag in DENYLISTED_FLAGS {
        assert!(
            !args.iter().any(|arg| arg.starts_with(&format!("{flag}="))),
            "{flag}=… leaked into {args:?}"
        );
    }

    // No smuggled value survived as a stray argument.
    assert!(!args.iter().any(|arg| arg == "smuggled"), "{args:?}");
    // Legitimate lookalike data is preserved.
    assert!(contains_pair(&args, "--system-prompt", "--mode"));
    assert!(contains_pair(&args, "--tools", "read,grep"));
    assert!(contains_pair(&args, "--model", "x/y"));
}

#[test]
fn scheduled_run_denials_strip_only_the_listed_pairs() {
    let deny = FlagDenylist::new(&["--no-tools", "--session-dir"], &["--session-dir"]);
    let flags: Vec<String> = ["--no-tools", "x", "--session-dir", "D:/s", "--keep", "y"]
        .iter()
        .map(|flag| (*flag).to_string())
        .collect();
    let out = strip_denylisted_flags(&flags, Some(&deny));
    assert_eq!(out, vec!["x", "--keep", "y"]);
}
