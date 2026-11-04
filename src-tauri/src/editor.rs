//! External-editor round trip for the composer draft.
//!
//! Blueprint: `src/main/editor.ts`. Write the draft to a temp file, run `$VISUAL` / `$EDITOR`
//! (notepad on Windows) to completion, read the file back on a clean exit. A non-zero exit means
//! the user cancelled, so the caller keeps its own copy of the text.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// Resolution order: `$VISUAL`, `$EDITOR`, then notepad on Windows.
pub fn resolve_editor(visual: Option<&str>, editor: Option<&str>, windows: bool) -> Option<String> {
    non_empty(visual)
        .or_else(|| non_empty(editor))
        .or_else(|| windows.then(|| "notepad".to_owned()))
}

/// Split a configured command like `code -w` into program plus arguments.
pub fn split_command(command: &str) -> (String, Vec<String>) {
    let mut parts = command.split_whitespace();
    let program = parts.next().unwrap_or_default().to_owned();
    (program, parts.map(str::to_owned).collect())
}

/// Read-back contract: strip exactly one trailing newline.
pub fn strip_one_newline(text: &str) -> String {
    text.strip_suffix('\n').unwrap_or(text).to_owned()
}

fn temp_path() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("omp-editor-{stamp}-{}.md", std::process::id()))
}

/// Run the editor round trip. Always reports the same envelope shape the renderer expects.
pub fn open_external(content: &str) -> Value {
    let command = match resolve_editor(
        std::env::var("VISUAL").ok().as_deref(),
        std::env::var("EDITOR").ok().as_deref(),
        cfg!(windows),
    ) {
        Some(command) => command,
        None => {
            return json!({
                "ok": false,
                "unavailable": true,
                "text": Value::Null,
                "error": "Set $VISUAL or $EDITOR to use an external editor",
            });
        }
    };

    let path = temp_path();
    if let Err(err) = std::fs::write(&path, content) {
        return json!({ "ok": false, "unavailable": false, "text": Value::Null, "error": err.to_string() });
    }

    let (program, args) = split_command(&command);
    let status = Command::new(&program)
        .args(&args)
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    let result = match status {
        Ok(status) if status.success() => match std::fs::read_to_string(&path) {
            Ok(text) => json!({
                "ok": true,
                "unavailable": false,
                "text": strip_one_newline(&text),
            }),
            Err(err) => json!({
                "ok": false,
                "unavailable": false,
                "text": Value::Null,
                "error": err.to_string(),
            }),
        },
        // Cancelled: the draft is unchanged, which is not an error.
        Ok(_) => json!({ "ok": true, "unavailable": false, "text": Value::Null }),
        Err(err) => json!({
            "ok": false,
            "unavailable": false,
            "text": Value::Null,
            "error": err.to_string(),
        }),
    };

    let _ = std::fs::remove_file(&path);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_wins_over_editor_and_notepad_is_the_windows_fallback() {
        assert_eq!(
            resolve_editor(Some("code -w"), Some("vim"), true).as_deref(),
            Some("code -w")
        );
        assert_eq!(resolve_editor(None, Some("vim"), true).as_deref(), Some("vim"));
        assert_eq!(resolve_editor(None, None, true).as_deref(), Some("notepad"));
        assert_eq!(resolve_editor(None, None, false), None);
        assert_eq!(resolve_editor(Some("   "), None, false), None);
    }

    #[test]
    fn commands_split_into_program_and_args() {
        let (program, args) = split_command("code -w --new-window");
        assert_eq!(program, "code");
        assert_eq!(args, vec!["-w", "--new-window"]);
        assert_eq!(split_command("notepad").1.len(), 0);
    }

    #[test]
    fn only_one_trailing_newline_is_stripped() {
        assert_eq!(strip_one_newline("a\n"), "a");
        assert_eq!(strip_one_newline("a\n\n"), "a\n");
        assert_eq!(strip_one_newline("a"), "a");
    }
}
