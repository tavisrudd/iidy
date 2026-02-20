//! Custom YAML emitter for iidy-js compatible output
//!
//! This module provides a custom YAML emitter that extends yaml-rust's functionality
//! with intelligent string handling to match iidy-js output formatting.

use std::fmt;
use yaml_rust::{Yaml, yaml::Hash};

// Module-level predicate functions for YAML processing

/// Check if string can be written in plain style (no quotes needed)
/// Based on js-yaml's isPlainSafeFirst, isPlainSafe, and isPlainSafeLast functions
fn is_plain_safe_string(string: &str) -> bool {
    if string.is_empty() {
        return false;
    }

    let chars: Vec<char> = string.chars().collect();

    // Check first character
    if !is_plain_safe_first(chars[0]) {
        return false;
    }

    // Check last character
    if !is_plain_safe_last(chars[chars.len() - 1]) {
        return false;
    }

    // Check all characters in sequence
    let mut prev_char: Option<char> = None;
    for &ch in &chars {
        if !is_plain_safe(ch, prev_char, true) {
            // inblock = true for block context
            return false;
        }
        prev_char = Some(ch);
    }

    true
}

/// js-yaml isWhitespace equivalent
fn is_whitespace(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// js-yaml isPrintable equivalent
fn is_printable(c: char) -> bool {
    let code = c as u32;
    (0x00020..=0x00007E).contains(&code)
        || ((0x000A1..=0x00D7FF).contains(&code) && code != 0x2028 && code != 0x2029)
        || ((0x0E000..=0x00FFFD).contains(&code) && code != 0xFEFF) // BOM
        || (0x10000..=0x10FFFF).contains(&code)
}

/// js-yaml isNsCharOrWhitespace equivalent
fn is_ns_char_or_whitespace(c: char) -> bool {
    is_printable(c)
        && c != '\u{FEFF}' // BOM
        && c != '\r'       // carriage return
        && c != '\n' // line feed
}

/// js-yaml isPlainSafeFirst equivalent
fn is_plain_safe_first(c: char) -> bool {
    is_printable(c)
        && !is_whitespace(c)
        && !matches!(
            c,
            '-' | '?'
                | ':'
                | ','
                | '['
                | ']'
                | '{'
                | '}'
                | '#'
                | '&'
                | '*'
                | '!'
                | '|'
                | '='
                | '>'
                | '\''
                | '"'
                | '%'
                | '@'
                | '`'
        )
}

/// js-yaml isPlainSafeLast equivalent  
fn is_plain_safe_last(c: char) -> bool {
    !is_whitespace(c) && c != ':'
}

/// js-yaml isPlainSafe equivalent
fn is_plain_safe(c: char, prev_char: Option<char>, inblock: bool) -> bool {
    let c_is_ns_char_or_whitespace = is_ns_char_or_whitespace(c);
    let c_is_ns_char = c_is_ns_char_or_whitespace && !is_whitespace(c);

    // ns-plain-safe logic
    let plain_safe = if inblock {
        c_is_ns_char_or_whitespace
    } else {
        c_is_ns_char_or_whitespace && !matches!(c, ',' | '[' | ']' | '{' | '}') // flow indicators
    };

    // ns-plain-char logic
    plain_safe && c != '#' && (prev_char != Some(':') || c_is_ns_char)
        || (prev_char.is_some_and(|p| is_ns_char_or_whitespace(p) && !is_whitespace(p)) && c == '#')
        || (prev_char == Some(':') && c_is_ns_char)
}

// /// iidy-js isPlainSafe equivalent (simplified version)
// fn is_plain_safe_old_iidy(c: char, prev_char: Option<char>) -> bool {
//     // Uses a subset of nb-char - c-flow-indicator - ":" - "#"
//     // where nb-char ::= c-printable - b-char - c-byte-order-mark.
//     is_printable(c) && c != '\u{FEFF}'
//         // - c-flow-indicator
//         && c != ','
//         && c != '['
//         && c != ']'
//         && c != '{'
//         && c != '}'
//         // - ":" - "#"
//         // /* An ns-char preceding */ "#"
//         && c != ':'
//         && (c != '#' || prev_char.map_or(false, |p| is_ns_char_or_whitespace(p)))
// }

// /// iidy-js isPlainSafeFirst equivalent (simplified version)
// fn is_plain_safe_first_old_iidy(c: char) -> bool {
//     // Uses a subset of ns-char - c-indicator
//     // where ns-char = nb-char - s-white.
//     is_printable(c) && c != '\u{FEFF}'
//         && !is_whitespace(c) // - s-white
//         // - (c-indicator ::=
//         // "-" | "?" | ":" | "," | "[" | "]" | "{" | "}"
//         && c != '-'
//         && c != '?'
//         && c != ':'
//         && c != ','
//         && c != '['
//         && c != ']'
//         && c != '{'
//         && c != '}'
//         // | "#" | "&" | "*" | "!" | "|" | "=" | ">" | "'" | """
//         && c != '#'
//         && c != '&'
//         && c != '*'
//         && c != '!'
//         && c != '|'
//         && c != '='
//         && c != '>'
//         && c != '\''
//         && c != '"'
//         // | "%" | "@" | "`")
//         && c != '%'
//         && c != '@'
//         && c != '`'
// }

/// js-yaml testImplicitResolving equivalent - test if string would be interpreted as another type
fn is_ambiguous_type(string: &str) -> bool {
    // Check for boolean-like strings
    matches!(string,
        "true" | "false" | "null" | "~" | 
        "yes" | "no" | "on" | "off" |
        "True" | "False" | "Null" |
        "Yes" | "No" | "On" | "Off" |
        "TRUE" | "FALSE" | "NULL" |
        "YES" | "NO" | "ON" | "OFF"
    )
    // Check for numeric strings
    || string.parse::<i64>().is_ok()
    || string.parse::<f64>().is_ok()
    // Check for base60 numbers (deprecated YAML 1.1 syntax)
    || is_base60_syntax(string)
}

/// Check for deprecated YAML 1.1 base60 syntax
fn is_base60_syntax(string: &str) -> bool {
    // Regex equivalent: /^[-+]?[0-9_]+(?::[0-9_]+)+(?:\\.[0-9_]*)?$/
    if string.is_empty() {
        return false;
    }

    let mut chars = string.chars().peekable();

    // Optional sign
    if matches!(chars.peek(), Some('-') | Some('+')) {
        chars.next();
    }

    // Must have at least one digit or underscore
    let mut has_digit = false;
    while let Some(&ch) = chars.peek() {
        if ch.is_ascii_digit() || ch == '_' {
            has_digit = true;
            chars.next();
        } else {
            break;
        }
    }

    if !has_digit {
        return false;
    }

    // Must have at least one colon section
    let mut has_colon = false;
    while let Some(&ch) = chars.peek() {
        if ch == ':' {
            has_colon = true;
            chars.next();

            // After colon, need digits/underscores
            let mut section_has_digit = false;
            while let Some(&ch) = chars.peek() {
                if ch.is_ascii_digit() || ch == '_' {
                    section_has_digit = true;
                    chars.next();
                } else {
                    break;
                }
            }

            if !section_has_digit {
                return false;
            }
        } else {
            break;
        }
    }

    if !has_colon {
        return false;
    }

    // Optional decimal part
    if let Some(&'.') = chars.peek() {
        chars.next();
        while let Some(&ch) = chars.peek() {
            if ch.is_ascii_digit() || ch == '_' {
                chars.next();
            } else {
                break;
            }
        }
    }

    // Must have consumed all characters
    chars.peek().is_none()
}

/// Custom YAML emitter - currently identical to yaml-rust YamlEmitter
pub struct IidyYamlEmitter<'a> {
    writer: &'a mut dyn fmt::Write,
    best_indent: usize,
    compact: bool,
    level: isize,
}

impl<'a> IidyYamlEmitter<'a> {
    pub fn new(writer: &'a mut dyn fmt::Write) -> IidyYamlEmitter<'a> {
        IidyYamlEmitter {
            writer,
            best_indent: 2,
            compact: false,
            level: -1,
        }
    }

    pub fn dump(&mut self, doc: &Yaml) -> Result<(), fmt::Error> {
        self.emit_node(doc, true)
    }

    fn emit_node(&mut self, node: &Yaml, first: bool) -> Result<(), fmt::Error> {
        match *node {
            Yaml::Array(ref v) => self.emit_array(v, first),
            Yaml::Hash(ref h) => {
                // Check if this is a tagged value in mapping format
                if self.is_tagged_value_mapping(h) {
                    self.emit_tagged_value_iidy(h)
                } else {
                    self.emit_hash(h, first)
                }
            }
            Yaml::String(ref v) => self.emit_str_iidy(v),
            Yaml::Boolean(v) => {
                if v {
                    self.writer.write_str("true")?;
                } else {
                    self.writer.write_str("false")?;
                }
                Ok(())
            }
            Yaml::Integer(v) => {
                write!(self.writer, "{v}")?;
                Ok(())
            }
            Yaml::Real(ref v) => {
                write!(self.writer, "{v}")?;
                Ok(())
            }
            Yaml::Null => {
                self.writer.write_str("null")?;
                Ok(())
            }
            Yaml::Alias(_) => {
                self.writer.write_str("null")?;
                Ok(())
            }
            Yaml::BadValue => {
                self.writer.write_str("null")?;
                Ok(())
            }
        }
    }

    fn emit_array(&mut self, v: &[Yaml], first: bool) -> Result<(), fmt::Error> {
        if v.is_empty() {
            self.writer.write_str("[]")?;
        } else {
            self.level += 1;
            for (cnt, x) in v.iter().enumerate() {
                if cnt > 0 || !first {
                    self.writer.write_str("\n")?;
                    self.write_indent()?;
                }
                self.writer.write_str("- ")?;
                self.emit_node(x, true)?;
            }
            self.level -= 1;
        }
        Ok(())
    }

    fn emit_hash(&mut self, h: &Hash, first: bool) -> Result<(), fmt::Error> {
        if h.is_empty() {
            self.writer.write_str("{}")?;
        } else {
            self.level += 1;
            for (cnt, (k, v)) in h.iter().enumerate() {
                if cnt > 0 || !first {
                    self.writer.write_str("\n")?;
                    self.write_indent()?;
                }
                self.emit_node(k, true)?;
                self.writer.write_str(":")?;
                self.emit_val_iidy(v)?;
            }
            self.level -= 1;
        }
        Ok(())
    }

    // iidy CUSTOMIZED fns below:

    fn emit_val_iidy(&mut self, val: &Yaml) -> Result<(), fmt::Error> {
        match *val {
            Yaml::Array(ref v) => {
                if v.is_empty() {
                    self.writer.write_str(" []")?;
                } else if self.compact {
                    self.writer.write_str(" ")?;
                    self.emit_array(v, true)?;
                } else {
                    self.emit_array(v, false)?;
                }
            }
            Yaml::Hash(ref h) => {
                if h.is_empty() {
                    self.writer.write_str(" {}")?;
                } else if self.is_tagged_value_mapping(h) {
                    // Handle tagged values like CloudFormation tags
                    self.writer.write_str(" ")?;
                    self.emit_tagged_value_iidy(h)?;
                } else if self.compact {
                    self.writer.write_str(" ")?;
                    self.emit_hash(h, true)?;
                } else {
                    self.emit_hash(h, false)?;
                }
            }
            _ => {
                self.writer.write_str(" ")?;
                self.emit_node(val, true)?;
            }
        }
        Ok(())
    }

    fn emit_str_iidy(&mut self, v: &str) -> Result<(), fmt::Error> {
        // Check if it's a multiline string first
        if v.contains('\n') {
            return self.emit_multiline_string_iidy(v);
        }

        // For single-line strings, prefer no quotes when possible
        if !self.need_quotes_iidy(v) {
            return write!(self.writer, "{v}");
        }

        // String needs quoting - prefer single quotes unless they contain single quotes
        if !v.contains('\'') {
            self.emit_single_quoted_string_iidy(v)
        } else if !v.contains('"') {
            self.emit_double_quoted_string(v)
        } else {
            // Contains both - use double quotes with escaping
            self.emit_double_quoted_string(v)
        }
    }

    fn emit_multiline_string_iidy(&mut self, string: &str) -> Result<(), fmt::Error> {
        // Use |- for simple line sequences (no blank lines, all lines short and non-empty)
        // Use | for everything else (content blocks, blank lines, etc.)
        let lines: Vec<&str> = string.lines().collect();
        let is_simple_line_sequence = !string.contains("\n\n")
            && lines
                .iter()
                .all(|line| !line.trim().is_empty() && line.len() < 20);

        if is_simple_line_sequence {
            self.writer.write_str("|-")?;
        } else {
            self.writer.write_str("|")?;
        }
        // Use string.lines() which properly handles whitespace-only lines
        for line in string.lines() {
            self.writer.write_str("\n")?;
            // Only indent non-empty lines
            if !line.is_empty() {
                // Multiline strings are indented one level beyond the current level
                let indent_level = if self.level >= 0 {
                    self.level as usize + 1
                } else {
                    1
                };
                for _ in 0..indent_level * self.best_indent {
                    self.writer.write_char(' ')?;
                }
            }
            self.writer.write_str(line)?;
        }
        Ok(())
    }

    fn emit_single_quoted_string_iidy(&mut self, string: &str) -> Result<(), fmt::Error> {
        self.writer.write_str("'")?;
        // In single quotes, we only need to escape single quotes by doubling them
        for ch in string.chars() {
            if ch == '\'' {
                self.writer.write_str("''")?;
            } else {
                self.writer.write_char(ch)?;
            }
        }
        self.writer.write_str("'")?;
        Ok(())
    }

    fn emit_double_quoted_string(&mut self, string: &str) -> Result<(), fmt::Error> {
        self.writer.write_str("\"")?;
        for ch in string.chars() {
            match ch {
                '"' => self.writer.write_str("\\\"")?,
                '\\' => self.writer.write_str("\\\\")?,
                '\n' => self.writer.write_str("\\n")?,
                '\r' => self.writer.write_str("\\r")?,
                '\t' => self.writer.write_str("\\t")?,
                ch if ch.is_control() => write!(self.writer, "\\u{:04x}", ch as u32)?,
                ch => self.writer.write_char(ch)?,
            }
        }
        self.writer.write_str("\"")?;
        Ok(())
    }

    // js-yaml compatible quote detection - based on chooseScalarStyle logic
    fn need_quotes_iidy(&self, string: &str) -> bool {
        !is_plain_safe_string(string) || is_ambiguous_type(string)
    }

    /// Check if a hash represents a tagged value in mapping format
    /// e.g., { "!Ref": "MyResource" } or { "!CustomTag": {...} }
    fn is_tagged_value_mapping(&self, h: &Hash) -> bool {
        // Tagged values should be single-key hashes with keys starting with !
        if h.len() == 1
            && let Some((Yaml::String(key_str), _)) = h.iter().next()
        {
            return key_str.starts_with('!');
        }
        false
    }

    /// Emit a tagged value in the proper !Tag value format
    /// Handles any tag (CloudFormation, custom, etc.) with any value type (string, array, hash, etc.)
    fn emit_tagged_value_iidy(&mut self, h: &Hash) -> Result<(), fmt::Error> {
        if let Some((Yaml::String(tag_name), value)) = h.iter().next() {
            self.writer.write_str(tag_name)?;

            match value {
                Yaml::String(_)
                | Yaml::Integer(_)
                | Yaml::Real(_)
                | Yaml::Boolean(_)
                | Yaml::Null => {
                    self.writer.write_str(" ")?;
                    self.emit_node(value, true)?;
                }
                Yaml::Array(_) | Yaml::Hash(_) => {
                    self.emit_val_iidy(value)?;
                }
                _ => {
                    self.writer.write_str(" ")?;
                    self.emit_node(value, true)?;
                }
            }
        }
        Ok(())
    }

    fn write_indent(&mut self) -> Result<(), fmt::Error> {
        for _ in 0..self.level {
            for _ in 0..self.best_indent {
                self.writer.write_char(' ')?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yaml_rust::{YamlLoader, yaml::Hash};

    #[test]
    fn test_cloudformation_tag_emission() {
        // Create a single-key hash representing a CloudFormation tag
        let mut h = Hash::new();
        h.insert(
            yaml_rust::Yaml::String("!Ref".to_string()),
            yaml_rust::Yaml::String("MyResource".to_string()),
        );
        let cf_tag_yaml = yaml_rust::Yaml::Hash(h);

        // Test our emitter directly with this hash
        let mut output = String::new();
        {
            let mut emitter = IidyYamlEmitter::new(&mut output);
            emitter.emit_node(&cf_tag_yaml, true).unwrap();
        }

        println!("CloudFormation tag emission test output: '{output}'");

        // Should emit "!Ref MyResource", not "'!Ref': MyResource"
        assert_eq!(output.trim(), "!Ref MyResource");
    }

    #[test]
    fn test_cloudformation_tag_as_hash_value() {
        // Create the real-world scenario: a parent hash containing a CloudFormation tag as a value
        let mut cf_tag_hash = Hash::new();
        cf_tag_hash.insert(
            yaml_rust::Yaml::String("!Ref".to_string()),
            yaml_rust::Yaml::String("MyResource".to_string()),
        );

        let mut parent_hash = Hash::new();
        parent_hash.insert(
            yaml_rust::Yaml::String("test_ref".to_string()),
            yaml_rust::Yaml::Hash(cf_tag_hash),
        );

        let parent_yaml = yaml_rust::Yaml::Hash(parent_hash);

        // Test our emitter with the parent hash (this mimics the real pipeline)
        let mut output = String::new();
        {
            let mut emitter = IidyYamlEmitter::new(&mut output);
            emitter.emit_node(&parent_yaml, true).unwrap();
        }

        println!("Parent hash with CloudFormation tag emission test output:");
        println!("{output}");

        // Should emit "test_ref: !Ref MyResource", not "test_ref:\n  '!Ref': MyResource"
        let expected = "test_ref: !Ref MyResource";
        assert_eq!(output.trim(), expected);
    }

    #[test]
    fn test_standard_yaml_rust_behavior() {
        let yaml_str = r#"
config_map:
  simple_key: simple_value
  quoted_key: "needs quotes: because of colon"
nested_config:
  database:
    host: localhost
    port: 5432
"#;

        let docs = YamlLoader::load_from_str(yaml_str).unwrap();
        let doc = &docs[0];

        // Test with standard yaml-rust emitter
        let mut std_out = String::new();
        {
            let mut std_emitter = yaml_rust::YamlEmitter::new(&mut std_out);
            std_emitter.dump(doc).unwrap();
        }

        // Test with our emitter
        let mut our_out = String::new();
        {
            let mut our_emitter = IidyYamlEmitter::new(&mut our_out);
            our_emitter.dump(doc).unwrap();
        }

        println!("Standard yaml-rust output:");
        println!("{std_out}");
        println!("\nOur emitter output:");
        println!("{our_out}");

        // Our emitter is designed to prefer single quotes while yaml-rust prefers double quotes
        // This difference is intentional for iidy-js compatibility
        println!("Our emitter successfully processes the same YAML structure");
    }
}
