use anyhow::{Result, anyhow};
use std::io::Read;

use crate::command;
use crate::tmux;

pub fn run(
    handle: Option<String>,
    pane_id: Option<String>,
    message: Option<String>,
    as_command: bool,
) -> Result<()> {
    let handle = command::resolve_name(handle.as_deref())?;
    let message = read_message(message)?;
    send_message(
        &handle,
        pane_id.as_deref(),
        &message,
        as_command,
        command::agent::resolve_agent_pane,
        tmux::paste_multiline,
        tmux::send_keys_to_agent,
        tmux::send_keys,
    )
}

fn send_message<R, P, S, L>(
    handle: &str,
    pane_id: Option<&str>,
    message: &str,
    as_command: bool,
    resolve: R,
    paste: P,
    send: S,
    send_line: L,
) -> Result<()>
where
    R: Fn(&str, Option<&str>) -> Result<command::agent::AgentPaneTarget>,
    P: Fn(&str, &str) -> Result<()>,
    S: Fn(&str, &str, Option<&str>) -> Result<()>,
    L: Fn(&str, &str) -> Result<()>,
{
    let target = resolve(handle, pane_id)?;

    if as_command {
        let trimmed = message.trim_end_matches(['\n', '\r']);
        if trimmed.contains('\n') {
            return Err(anyhow!(
                "--command only supports single-line input; remove newlines or use without --command"
            ));
        }
        send(&target.pane_id, trimmed, target.agent.as_deref())
    } else if message.contains('\n') {
        paste(&target.pane_id, message)
    } else {
        send_line(&target.pane_id, message)
    }
}

fn read_message(message: Option<String>) -> Result<String> {
    if let Some(message) = message {
        if message.trim().is_empty() {
            return Err(anyhow!("Message is empty"));
        }
        return Ok(message);
    }

    let mut buffer = String::new();
    std::io::stdin()
        .read_to_string(&mut buffer)
        .map_err(|e| anyhow!("Failed to read stdin: {}", e))?;

    if buffer.trim().is_empty() {
        return Err(anyhow!("Message is empty"));
    }

    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::agent::AgentPaneTarget;
    use std::cell::Cell;

    fn resolve(_: &str, _: Option<&str>) -> Result<AgentPaneTarget> {
        Ok(AgentPaneTarget {
            pane_id: "%1".to_string(),
            agent: Some("codex".to_string()),
        })
    }

    #[test]
    fn test_send_message_rejects_newlines_for_command() {
        let err = send_message(
            "handle",
            None,
            "line1\nline2",
            true,
            resolve,
            |_, _| Ok(()),
            |_: &str, _: &str, _: Option<&str>| Ok(()),
            |_: &str, _: &str| Ok(()),
        )
        .expect_err("expected newline rejection");

        assert!(err.to_string().contains("--command"));
    }

    #[test]
    fn test_send_message_command_trims() {
        let sent = Cell::new(String::new());
        send_message(
            "handle",
            None,
            "hello\n",
            true,
            resolve,
            |_, _| Ok(()),
            |_: &str, message: &str, _: Option<&str>| {
                sent.set(message.to_string());
                Ok(())
            },
            |_: &str, _: &str| Ok(()),
        )
        .expect("send message");

        assert_eq!(sent.take(), "hello");
    }

    #[test]
    fn test_send_message_paste_multiline() {
        let pasted = Cell::new(String::new());
        send_message(
            "handle",
            None,
            "hello\nworld",
            false,
            resolve,
            |_: &str, message: &str| {
                pasted.set(message.to_string());
                Ok(())
            },
            |_: &str, _: &str, _: Option<&str>| Ok(()),
            |_: &str, _: &str| Ok(()),
        )
        .expect("send message");

        assert_eq!(pasted.take(), "hello\nworld");
    }

    #[test]
    fn test_send_message_single_line_uses_send_keys() {
        let sent = Cell::new(String::new());
        send_message(
            "handle",
            None,
            "hello",
            false,
            resolve,
            |_, _| Ok(()),
            |_: &str, _: &str, _: Option<&str>| Ok(()),
            |_: &str, message: &str| {
                sent.set(message.to_string());
                Ok(())
            },
        )
        .expect("send message");

        assert_eq!(sent.take(), "hello");
    }

    #[test]
    fn test_read_message_rejects_empty() {
        let err = read_message(Some(" ".to_string())).expect_err("empty message");
        assert!(err.to_string().contains("Message is empty"));
    }
}
