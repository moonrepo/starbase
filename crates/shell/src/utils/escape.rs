use std::fmt::Write;

// referenced from 'https://github.com/solidsnack/shell-escape/blob/master/Data/ByteString/ShellEscape/Bash.hs'

#[derive(Debug, PartialEq)]
enum EscapingMode {
    ANSIHex,
    ANSIBackslash,
    Literal,
    Quoted,
}

fn classify(c: char) -> EscapingMode {
    match c {
        '\u{0}'..='\u{6}' => EscapingMode::ANSIHex,    // \x00-\x06: ANSIHex
        '\u{9}' => EscapingMode::ANSIBackslash,        // \t: ANSIBackslash
        '\u{a}' => EscapingMode::ANSIBackslash,        // \n: ANSIBackslash
        '\u{d}' => EscapingMode::ANSIBackslash,        // \r: ANSIBackslash
        '\u{1f}' => EscapingMode::ANSIHex,             // \x1f: ANSIHex
        '\u{20}'..='\u{26}' => EscapingMode::Quoted,   // ' ' - '&': Quoted
        '\'' => EscapingMode::ANSIBackslash,           // '\'': ANSIBackslash
        '\u{2b}' => EscapingMode::Quoted,              // '+': Quoted
        '\u{30}'..='\u{39}' => EscapingMode::Literal,  // '0' - '9': Literal
        '\u{3f}' => EscapingMode::Quoted,              // '?': Quoted
        '\u{41}'..='\u{5a}' => EscapingMode::Literal,  // 'A' - 'Z': Literal
        '[' => EscapingMode::Quoted,                   // '[': Quoted
        '\\' => EscapingMode::ANSIBackslash,           // '\\': ANSIBackslash
        '_' => EscapingMode::Literal,                  // '_': Literal
        '\u{5d}' => EscapingMode::Quoted,              // ']': Quoted
        '\u{60}' => EscapingMode::Quoted,              // '`': Quoted
        '\u{61}'..='\u{7a}' => EscapingMode::Literal,  // 'a' - 'z': Literal
        '\u{7e}' => EscapingMode::Quoted,              // '~': Quoted
        '\u{7f}' => EscapingMode::ANSIHex,             // \x7f: ANSIHex
        _ => EscapingMode::ANSIHex,                    // Default: ANSIHex
    }
}

fn hexify(c: char) -> String {
    format!("\\x{:02X}", c as u32)
}

fn backslashify(c: char) -> String {
    format!("\\{}", c)
}

pub fn escapeBash(input: &str) -> String {
    let mut output = String::new();
    let mut escape_needed = false;

    for c in input.chars() {
        match classify(c) {
            EscapingMode::ANSIHex => {
                write!(output, "{}", hexify(c)).unwrap();
                escape_needed = true;
            }
            EscapingMode::ANSIBackslash => {
                write!(output, "{}", backslashify(c)).unwrap();
                escape_needed = true;
            }
            EscapingMode::Quoted => {
                write!(output, "{}", c).unwrap();
                escape_needed = true;
            }
            EscapingMode::Literal => {
                write!(output, "{}", c).unwrap();
            }
        }
    }

    if escape_needed {
        format!("$'{}'", output)
    } else {
        output
    }
}
