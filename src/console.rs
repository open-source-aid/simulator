use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use core::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Command {
    Battery(u8),
    Reservoir(i32),
    Alarms(i32),
    Reboot,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParseError(String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown command `{}`: expected battery <n>, reservoir <n>, alarms <n> or reboot",
            self.0
        )
    }
}

impl core::error::Error for ParseError {}

impl FromStr for Command {
    type Err = ParseError;

    fn from_str(line: &str) -> Result<Self, Self::Err> {
        let mut words = line.split_whitespace();
        let command = match (words.next(), words.next(), words.next()) {
            (Some("battery"), Some(value), None) => value.parse().ok().map(Self::Battery),
            (Some("reservoir"), Some(value), None) => value.parse().ok().map(Self::Reservoir),
            (Some("alarms"), Some(value), None) => value.parse().ok().map(Self::Alarms),
            (Some("reboot"), None, None) => Some(Self::Reboot),
            _ => None,
        };
        command.ok_or_else(|| ParseError(line.trim().to_string()))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lines(Vec<u8>);

impl Lines {
    pub fn push(&mut self, byte: u8) -> Option<String> {
        if byte != b'\r' && byte != b'\n' {
            self.0.push(byte);
            return None;
        }
        let line = String::from_utf8_lossy(&core::mem::take(&mut self.0)).into_owned();
        (!line.is_empty()).then_some(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;

    #[test]
    fn parses_scenario_knobs() {
        assert_eq!("battery 1".parse(), Ok(Command::Battery(1)));
        assert_eq!(" reservoir  1500 ".parse(), Ok(Command::Reservoir(1500)));
        assert_eq!("alarms 3".parse(), Ok(Command::Alarms(3)));
        assert_eq!("reboot".parse(), Ok(Command::Reboot));
    }

    #[test]
    fn rejects_malformed_lines() {
        for line in [
            "",
            "battery",
            "battery x",
            "battery 1 2",
            "reboot now",
            "fly 1",
        ] {
            assert_eq!(
                line.parse::<Command>(),
                Err(ParseError(line.trim().to_string()))
            );
        }
        assert_eq!(
            ParseError("fly".into()).to_string(),
            "unknown command `fly`: expected battery <n>, reservoir <n>, alarms <n> or reboot"
        );
    }

    #[test]
    fn splits_bytes_into_lines() {
        let mut lines = Lines::default();
        let pushed: Vec<_> = b"ab\r\ncd\n".iter().map(|&b| lines.push(b)).collect();
        assert_eq!(
            pushed,
            vec![
                None,
                None,
                Some("ab".into()),
                None,
                None,
                None,
                Some("cd".into())
            ]
        );
    }
}
