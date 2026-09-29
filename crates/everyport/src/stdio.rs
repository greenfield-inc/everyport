//! `everyport stdio`: events as JSON lines on stdout, requests as JSON lines on stdin.

use crate::hub::{parse_request, Hub};
use std::io::{self, BufRead, Write};
use std::sync::mpsc;
use std::thread;

/// Serves until `input` ends and every answer is written.
pub fn run(
    hub: Hub,
    mut input: impl BufRead + Send + 'static,
    mut output: impl Write,
) -> io::Result<()> {
    let (events, rx) = mpsc::channel();
    hub.subscribe(events.clone());
    thread::spawn(move || {
        let mut line = Vec::new();
        while matches!(input.read_until(b'\n', &mut line), Ok(n) if n > 0) {
            let text = String::from_utf8_lossy(&line);
            if !text.trim().is_empty() {
                match parse_request(&text) {
                    Ok(request) => hub.call(request, events.clone()),
                    Err(error) => hub.reply(error, events.clone()),
                }
            }
            line.clear();
        }
        // Dropping `hub` stops the scanner once it has answered every call.
    });
    for event in rx {
        serde_json::to_writer(&mut output, &event)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub::fixture;
    use everyport::protocol::{Event, RequestResult};

    fn transcript(input: &str) -> Vec<Event> {
        let mut output = Vec::new();
        run(
            fixture::hub(),
            io::Cursor::new(input.to_string()),
            &mut output,
        )
        .unwrap();
        String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn result(event: &Event) -> &RequestResult {
        match event {
            Event::Result(result) => result,
            other => panic!("expected a result, got {other:?}"),
        }
    }

    #[test]
    fn answers_each_request_in_order_and_exits_at_eof() {
        let input = [
            r#"{"id":1,"method":"refresh"}"#,
            "",
            "not json",
            r#"{"id":9,"method":"launch"}"#,
            r#"{"id":2,"method":"stop","params":{"port":3000,"root":{"pid":48198,"started_at":1790183520000},"force":false}}"#,
        ]
        .join("\n");
        let events = transcript(&input);

        let Event::Hello(hello) = &events[0] else {
            panic!("hello first, got {:?}", events[0])
        };
        assert_eq!(hello.protocol, 1);
        assert_eq!(hello.host.hostname, "devbox");
        assert_eq!(events[1], Event::Snapshot(fixture::snapshot()));
        // refresh: its result, then a fresh snapshot, then the alert that scan raised.
        assert_eq!(result(&events[2]), &RequestResult { id: 1, error: None });
        assert!(matches!(events[3], Event::Snapshot(_)));
        assert!(matches!(&events[4], Event::Alert(alert) if alert.port == 6006));
        // Bad lines get an error result: id 0 when unreadable, else the request's id.
        assert_eq!(result(&events[5]).id, 0);
        assert!(result(&events[5]).error.is_some());
        assert_eq!(result(&events[6]).id, 9);
        assert!(result(&events[6])
            .error
            .as_deref()
            .unwrap()
            .contains("launch"));
        // A failed call reports the engine's error. Nothing changed, so no snapshot follows.
        assert_eq!(
            result(&events[7]),
            &RequestResult {
                id: 2,
                error: Some("pid 48198 was reused".into())
            }
        );
        assert_eq!(events.len(), 8);
    }

    #[test]
    fn finishes_a_restart_before_exiting() {
        let pending = std::sync::Arc::default();
        let input = r#"{"id":1,"method":"restart","params":{"port":3000,"root":{"pid":48198,"started_at":1790183520000}}}"#;
        run(
            fixture::tracking(std::sync::Arc::clone(&pending)),
            io::Cursor::new(input),
            io::sink(),
        )
        .unwrap();
        assert_eq!(pending.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn empty_input_still_says_hello() {
        let events = transcript("");
        assert!(matches!(events[..], [Event::Hello(_), Event::Snapshot(_)]));
    }
}
