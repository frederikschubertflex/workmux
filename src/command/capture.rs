use anyhow::{Result, anyhow};

use crate::command;
use crate::tmux;

pub fn run(
    handle: Option<String>,
    pane_id: Option<String>,
    lines: u16,
    ansi: bool,
) -> Result<()> {
    let handle = command::resolve_name(handle.as_deref())?;
    let output = capture_output(
        &handle,
        pane_id.as_deref(),
        lines,
        ansi,
        command::agent::resolve_agent_pane,
        tmux::capture_pane,
        tmux::capture_pane_plain,
    )?;
    print!("{}", output);
    Ok(())
}

fn capture_output<R, CAnsi, CPlain>(
    handle: &str,
    pane_id: Option<&str>,
    lines: u16,
    ansi: bool,
    resolve: R,
    capture_ansi: CAnsi,
    capture_plain: CPlain,
) -> Result<String>
where
    R: Fn(&str, Option<&str>) -> Result<command::agent::AgentPaneTarget>,
    CAnsi: Fn(&str, u16) -> Option<String>,
    CPlain: Fn(&str, u16) -> Option<String>,
{
    let target = resolve(handle, pane_id)?;
    let output = if ansi {
        capture_ansi(&target.pane_id, lines)
    } else {
        capture_plain(&target.pane_id, lines)
    };

    let Some(output) = output else {
        return Err(anyhow!("Failed to capture pane {}", target.pane_id));
    };

    Ok(trim_output_lines(&output, lines))
}

fn trim_output_lines(output: &str, lines: u16) -> String {
    let max_lines = usize::from(lines);
    if max_lines == 0 {
        return String::new();
    }

    let segments: Vec<&str> = output.split_inclusive('\n').collect();
    if segments.len() <= max_lines {
        return output.to_string();
    }

    segments[segments.len() - max_lines..].concat()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::agent::AgentPaneTarget;
    use std::cell::Cell;

    fn resolve(_: &str, _: Option<&str>) -> Result<AgentPaneTarget> {
        Ok(AgentPaneTarget {
            pane_id: "%1".to_string(),
            agent: None,
        })
    }

    #[test]
    fn test_capture_output_ansi_selects_ansi() {
        let used = Cell::new(false);
        let output = capture_output(
            "handle",
            None,
            10,
            true,
            resolve,
            |_, _| {
                used.set(true);
                Some("ansi".to_string())
            },
            |_, _| Some("plain".to_string()),
        )
        .expect("capture output");

        assert!(used.get());
        assert_eq!(output, "ansi");
    }

    #[test]
    fn test_capture_output_plain_selects_plain() {
        let output = capture_output(
            "handle",
            None,
            10,
            false,
            resolve,
            |_, _| Some("ansi".to_string()),
            |_, _| Some("plain".to_string()),
        )
        .expect("capture output");

        assert_eq!(output, "plain");
    }

    #[test]
    fn test_capture_output_errors_on_missing() {
        let err = capture_output(
            "handle",
            None,
            10,
            false,
            resolve,
            |_, _| None,
            |_, _| None,
        )
        .expect_err("missing output");

        assert!(err.to_string().contains("Failed to capture pane"));
    }
}
