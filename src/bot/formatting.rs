use teloxide::prelude::*;
use teloxide::types::ParseMode;

use crate::memory::{Finding, RunSummary, ScheduledTask};

/// Maximum Telegram message length (leave margin from 4096 limit)
const MAX_MESSAGE_LENGTH: usize = 4000;

/// Escape HTML special characters for safe embedding in Telegram HTML messages
pub fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Split a message at newline boundaries near 4000 chars
pub fn chunk_message(text: &str) -> Vec<String> {
    if text.len() <= MAX_MESSAGE_LENGTH {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut remaining = text;
    while !remaining.is_empty() {
        if remaining.len() <= MAX_MESSAGE_LENGTH {
            chunks.push(remaining.to_string());
            break;
        }
        // Find a good split point (newline near boundary)
        let split_at = remaining[..MAX_MESSAGE_LENGTH]
            .rfind('\n')
            .unwrap_or(MAX_MESSAGE_LENGTH);
        let split_at = if split_at == 0 {
            MAX_MESSAGE_LENGTH
        } else {
            split_at
        };
        chunks.push(remaining[..split_at].to_string());
        remaining = remaining[split_at..].trim_start();
    }
    chunks
}

/// Chunk and send each piece with HTML parse mode
pub async fn send_chunked(
    bot: &Bot,
    chat_id: ChatId,
    text: &str,
) -> Result<(), teloxide::RequestError> {
    let chunks = chunk_message(text);
    for chunk in chunks {
        bot.send_message(chat_id, chunk)
            .parse_mode(ParseMode::Html)
            .await?;
    }
    Ok(())
}

/// Format a RunSummary as HTML for Telegram
pub fn format_status(summary: &RunSummary) -> String {
    let score_line = if summary.total_score != 0 || summary.detection_count != 0 {
        format!(
            "\n<b>Score:</b> {} | <b>Detections:</b> {}",
            summary.total_score, summary.detection_count
        )
    } else {
        String::new()
    };

    let last_event = summary
        .last_score_event
        .as_deref()
        .unwrap_or("none");

    format!(
        "<b>Run Status</b>\n\n\
         <b>Tasks:</b> {} total, {} completed, {} failed\n\
         <b>Findings:</b> {}{}\n\
         <b>Last event:</b> {}",
        summary.task_count,
        summary.completed_task_count,
        summary.failed_task_count,
        summary.finding_count,
        score_line,
        escape_html(last_event),
    )
}

/// Format a list of findings as HTML
pub fn format_findings(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "No findings found.".to_string();
    }

    let mut out = String::from("<b>Findings</b>\n\n");
    for f in findings {
        let host = f.host.as_deref().unwrap_or("unknown");
        out.push_str(&format!(
            "<b>{}</b> [{}]\n<code>{}</code>\n\n",
            escape_html(host),
            escape_html(&f.finding_type),
            escape_html(&f.data),
        ));
    }
    out
}

/// Format a list of scheduled tasks as HTML
pub fn format_schedule_list(schedules: &[ScheduledTask]) -> String {
    if schedules.is_empty() {
        return "No scheduled tasks.".to_string();
    }

    let mut out = String::from("<b>Scheduled Tasks</b>\n\n");
    for s in schedules {
        let last_result_preview = s
            .last_result
            .as_deref()
            .map(|r| {
                let truncated = if r.len() > 80 { &r[..80] } else { r };
                format!("\n  Last: {}", escape_html(truncated))
            })
            .unwrap_or_default();

        out.push_str(&format!(
            "<code>{}</code> [{}]\n  Cron: <code>{}</code>\n  Prompt: {}\n  Next: {}{}\n\n",
            escape_html(&s.id[..8.min(s.id.len())]),
            escape_html(&s.status),
            escape_html(&s.schedule),
            escape_html(&s.prompt),
            s.next_run,
            last_result_preview,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_html() {
        assert_eq!(escape_html("<script>"), "&lt;script&gt;");
        assert_eq!(escape_html("a & b"), "a &amp; b");
        assert_eq!(escape_html("normal text"), "normal text");
    }

    #[test]
    fn test_chunk_message_short() {
        let text = "Hello, world!";
        let chunks = chunk_message(text);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], text);
    }

    #[test]
    fn test_chunk_message_long() {
        // Create a message that's over 4000 chars with newlines
        let line = "A".repeat(100);
        let text = (0..50).map(|_| line.as_str()).collect::<Vec<_>>().join("\n");
        assert!(text.len() > MAX_MESSAGE_LENGTH);
        let chunks = chunk_message(&text);
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(chunk.len() <= MAX_MESSAGE_LENGTH);
        }
    }

    #[test]
    fn test_format_status() {
        let summary = RunSummary {
            task_count: 5,
            finding_count: 3,
            completed_task_count: 4,
            failed_task_count: 1,
            total_score: 42,
            detection_count: 0,
            last_score_event: Some("host_discovered".to_string()),
        };
        let html = format_status(&summary);
        assert!(html.contains("<b>Run Status</b>"));
        assert!(html.contains("5 total"));
        assert!(html.contains("4 completed"));
        assert!(html.contains("1 failed"));
        assert!(html.contains("Score:</b> 42"));
    }

    #[test]
    fn test_format_findings_empty() {
        assert_eq!(format_findings(&[]), "No findings found.");
    }

    #[test]
    fn test_format_findings_with_data() {
        let findings = vec![Finding {
            id: 1,
            run_id: Some(1),
            host: Some("192.168.1.1".to_string()),
            finding_type: "open_port".to_string(),
            data: "port 22 SSH".to_string(),
            timestamp: "2026-01-01".to_string(),
        }];
        let html = format_findings(&findings);
        assert!(html.contains("192.168.1.1"));
        assert!(html.contains("open_port"));
        assert!(html.contains("port 22 SSH"));
    }

    #[test]
    fn test_format_schedule_list_empty() {
        assert_eq!(format_schedule_list(&[]), "No scheduled tasks.");
    }

    #[test]
    fn test_format_schedule_list_with_data() {
        let schedules = vec![ScheduledTask {
            id: "abcdef12-3456-7890-abcd-ef1234567890".to_string(),
            chat_id: "123".to_string(),
            prompt: "scan network".to_string(),
            schedule: "0 */6 * * *".to_string(),
            next_run: 1700000000,
            last_run: None,
            last_result: None,
            status: "active".to_string(),
        }];
        let html = format_schedule_list(&schedules);
        assert!(html.contains("abcdef12"));
        assert!(html.contains("active"));
        assert!(html.contains("scan network"));
    }
}
