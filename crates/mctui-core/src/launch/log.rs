//! The game's output as log lines, each with a level to filter on.
//!
//! With Mojang's logging setup (see [`LoggingConfig`](crate::mojang::LoggingConfig)),
//! the game prints each log event to stdout as XML, for a launcher to read:
//!
//! ```text
//! <log4j:Event logger="net.minecraft.client.Minecraft" timestamp="1700000000000" level="INFO" thread="Render thread">
//!   <log4j:Message><![CDATA[Setting user: Steve]]></log4j:Message>
//! </log4j:Event>
//! ```
//!
//! [`Parser`] turns those back into the lines `logs/latest.log` has,
//! `[22:13:20] [Render thread/INFO]: Setting user: Steve`, with the event's
//! own level. Anything else (stderr, or a version without that setup) is
//! passed through, its level guessed by [`classify`].

use jiff::Timestamp;
use jiff::tz::TimeZone;

use crate::config::LogVerbosity;

/// How serious a log line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// Debug and trace output.
    Debug,
    /// Ordinary progress.
    Info,
    /// Something's off, but the game carries on.
    Warn,
    /// Errors, exceptions, and stack traces.
    Error,
}

impl Level {
    /// Whether a line at this level shows at `verbosity`: errors only,
    /// errors and warnings, or everything.
    pub fn shown_at(self, verbosity: LogVerbosity) -> bool {
        match verbosity {
            LogVerbosity::Error => self >= Level::Error,
            LogVerbosity::Warn => self >= Level::Warn,
            LogVerbosity::All => true,
        }
    }

    /// A log4j level name: `TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR`, `FATAL`.
    fn from_log4j(name: &str) -> Option<Self> {
        Some(match name {
            "TRACE" | "DEBUG" => Level::Debug,
            "INFO" => Level::Info,
            "WARN" => Level::Warn,
            "ERROR" | "FATAL" => Level::Error,
            _ => return None,
        })
    }
}

/// Which of the game's outputs a line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stream {
    /// Standard output: the game's log.
    Stdout,
    /// Standard error: mostly Java's own complaints and crashes.
    Stderr,
}

/// One line of the game's output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    /// The text, without its line ending.
    pub text: String,
    /// How serious it is.
    pub level: Level,
    /// Where it came from.
    pub stream: Stream,
}

/// Guesses a plain line's level from its text, as the Go app did: log
/// markers like `[main/WARN]` or `[ERROR]`, else exceptions and stack trace
/// lines are errors, and anything else is info.
pub fn classify(line: &str) -> Level {
    let lower = line.to_ascii_lowercase();
    let has = |markers: &[&str]| markers.iter().any(|m| lower.contains(m));
    if has(&[
        "[debug]", "[trace]", "/debug]", "/trace]", "[finer]", "[fine]",
    ]) {
        Level::Debug
    } else if has(&["[warn]", "[warning]", "/warn]", "/warning]"]) || lower.starts_with("warning:")
    {
        Level::Warn
    } else if has(&[
        "[error]",
        "[fatal]",
        "[severe]",
        "/error]",
        "/fatal]",
        "/severe]",
        "exception",
        "caused by:",
    ]) || is_stack_frame(line)
    {
        Level::Error
    } else {
        Level::Info
    }
}

/// `at net.minecraft.Foo.bar(Foo.java:1)`, as a stack trace prints them.
fn is_stack_frame(line: &str) -> bool {
    line.trim_start().starts_with("at ") && line.contains('(')
}

const EVENT_START: &str = "<log4j:Event ";
const EVENT_END: &str = "</log4j:Event>";
const MESSAGE_START: &str = "<log4j:Message><![CDATA[";
const THROWABLE_START: &str = "<log4j:Throwable><![CDATA[";
/// Ends a message or a throwable.
const CDATA_END: &str = "]]></log4j:";

/// Turns one output stream's lines into log lines, reading log4j's XML
/// events back into text.
#[derive(Debug)]
pub(super) struct Parser {
    time_zone: TimeZone,
    event: Option<Event>,
}

/// The event being read.
#[derive(Debug)]
struct Event {
    /// `[22:13:20] [Render thread/INFO]: `, for the message's first line.
    prefix: String,
    level: Level,
    part: Part,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    /// Between the event's elements.
    Between,
    /// In the message; `first` until its first line is out.
    Message { first: bool },
    /// In the stack trace.
    Throwable,
}

impl Parser {
    /// A parser giving event times in `time_zone`.
    pub(super) fn new(time_zone: TimeZone) -> Self {
        Self {
            time_zone,
            event: None,
        }
    }

    /// Reads one raw line, passing `emit` the log lines it makes (none for
    /// XML tags, one for most lines).
    pub(super) fn push(&mut self, line: &str, mut emit: impl FnMut(String, Level)) {
        let Some(event) = &mut self.event else {
            match line.trim_start().strip_prefix(EVENT_START) {
                Some(attributes) => self.event = Some(Event::start(attributes, &self.time_zone)),
                None => emit(line.to_owned(), classify(line)),
            }
            return;
        };
        match event.part {
            Part::Between => {
                let line = line.trim_start();
                if line.starts_with(EVENT_END) {
                    self.event = None;
                } else if let Some(text) = line.strip_prefix(MESSAGE_START) {
                    event.part = Part::Message { first: true };
                    event.content(text, emit);
                } else if let Some(text) = line.strip_prefix(THROWABLE_START) {
                    event.part = Part::Throwable;
                    event.content(text, emit);
                }
                // Anything else in an event is markup we don't show.
            }
            Part::Message { .. } | Part::Throwable => event.content(line, emit),
        }
    }
}

impl Event {
    fn start(attributes: &str, time_zone: &TimeZone) -> Self {
        let level_name = attribute(attributes, "level").unwrap_or_default();
        let thread = attribute(attributes, "thread").unwrap_or_default();
        let time = attribute(attributes, "timestamp")
            .and_then(|ms| ms.parse().ok())
            .and_then(|ms| Timestamp::from_millisecond(ms).ok())
            .map(|at| format!("[{}] ", at.to_zoned(time_zone.clone()).strftime("%H:%M:%S")))
            .unwrap_or_default();
        Self {
            prefix: format!("{time}[{thread}/{level_name}]: "),
            level: Level::from_log4j(&level_name).unwrap_or(Level::Info),
            part: Part::Between,
        }
    }

    /// A line of the message or stack trace, up to where it ends.
    fn content(&mut self, line: &str, mut emit: impl FnMut(String, Level)) {
        let (text, ended) = match line.find(CDATA_END) {
            Some(end) => (&line[..end], true),
            None => (line, false),
        };
        match self.part {
            Part::Message { first: true } => {
                emit(format!("{}{text}", self.prefix), self.level);
                self.part = Part::Message { first: false };
            }
            // The stack trace ends with a line break, so the closing tag
            // gets a line of its own.
            _ if text.is_empty() && ended => {}
            _ => emit(text.to_owned(), self.level),
        }
        if ended {
            self.part = Part::Between;
        }
    }
}

/// The value of `name="…"` in an XML start tag's attributes, unescaped.
fn attribute(attributes: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let start = attributes
        .match_indices(&key)
        .find(|&(i, _)| i == 0 || attributes[..i].ends_with(' '))
        .map(|(i, _)| i + key.len())?;
    let len = attributes[start..].find('"')?;
    Some(unescape(&attributes[start..start + len]))
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_matches_go() {
        for (line, want) in [
            ("[01:00:00] [main/ERROR]: boom", Level::Error),
            ("[01:00:00] [main/WARN]: nah", Level::Warn),
            ("[01:00:00] [Render thread/INFO]: hi", Level::Info),
            ("[01:00:00] [main/DEBUG]: detail", Level::Debug),
            ("[main] [FINE] fine", Level::Debug),
            (
                "WARNING: An illegal reflective access operation",
                Level::Warn,
            ),
            ("[SEVERE] gone", Level::Error),
            ("java.lang.NullPointerException: x", Level::Error),
            ("Caused by: something", Level::Error),
            ("\tat net.minecraft.Foo.bar(Foo.java:1)", Level::Error),
            ("at the end of the day", Level::Info),
            ("Some info text without marker", Level::Info),
        ] {
            assert_eq!(classify(line), want, "{line:?}");
        }
    }

    #[test]
    fn verbosity_filters_by_level() {
        use LogVerbosity::{All, Error, Warn};
        let shown = |level: Level| [Error, Warn, All].map(|v| level.shown_at(v));
        assert_eq!(shown(Level::Error), [true, true, true]);
        assert_eq!(shown(Level::Warn), [false, true, true]);
        assert_eq!(shown(Level::Info), [false, false, true]);
        assert_eq!(shown(Level::Debug), [false, false, true]);
    }

    fn parse(lines: &str) -> Vec<(String, Level)> {
        let mut parser = Parser::new(TimeZone::UTC);
        let mut out = Vec::new();
        for line in lines.lines() {
            parser.push(line, |text, level| out.push((text, level)));
        }
        out
    }

    #[test]
    fn log4j_events_read_back_as_lines() {
        let output = r#"<log4j:Event logger="net.minecraft.client.Minecraft" timestamp="1700000000000" level="INFO" thread="Render thread">
  <log4j:Message><![CDATA[Setting user: Steve]]></log4j:Message>
</log4j:Event>
plain output from System.out
<log4j:Event logger="x" timestamp="1700000001000" level="ERROR" thread="Worker &amp; co">
  <log4j:Message><![CDATA[Two lines:
second line]]></log4j:Message>
  <log4j:Throwable><![CDATA[java.lang.IllegalStateException: boom
	at a.B.c(B.java:1)
]]></log4j:Throwable>
</log4j:Event>
<log4j:Event logger="x" timestamp="1700000002000" level="WARN" thread="main">
  <log4j:Message><![CDATA[Careful]]></log4j:Message>
</log4j:Event>"#;
        let info = |s: &str| (s.to_owned(), Level::Info);
        let error = |s: &str| (s.to_owned(), Level::Error);
        assert_eq!(
            parse(output),
            [
                info("[22:13:20] [Render thread/INFO]: Setting user: Steve"),
                info("plain output from System.out"),
                error("[22:13:21] [Worker & co/ERROR]: Two lines:"),
                error("second line"),
                error("java.lang.IllegalStateException: boom"),
                error("\tat a.B.c(B.java:1)"),
                ("[22:13:22] [main/WARN]: Careful".to_owned(), Level::Warn),
            ]
        );
    }

    #[test]
    fn events_missing_parts_still_read() {
        let output = "<log4j:Event level=\"DEBUG\" thread=\"t\">\n\
                      <log4j:Message><![CDATA[no time]]></log4j:Message>\n\
                      </log4j:Event>\n\
                      <log4j:Event logger=\"x\">\n\
                      <log4j:Message><![CDATA[]]></log4j:Message>\n\
                      </log4j:Event>";
        assert_eq!(
            parse(output),
            [
                ("[t/DEBUG]: no time".to_owned(), Level::Debug),
                ("[/]: ".to_owned(), Level::Info),
            ]
        );
    }

    #[test]
    fn attributes() {
        let tag = r#"logger="a" level="WARN" thread="x &quot;y&quot;">"#;
        assert_eq!(attribute(tag, "logger").as_deref(), Some("a"));
        assert_eq!(attribute(tag, "level").as_deref(), Some("WARN"));
        assert_eq!(attribute(tag, "thread").as_deref(), Some("x \"y\""));
        assert_eq!(attribute(tag, "timestamp"), None);
        assert_eq!(attribute(r#"xlevel="no""#, "level"), None);
    }
}
