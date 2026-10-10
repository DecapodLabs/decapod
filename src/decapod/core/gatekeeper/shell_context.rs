//! Bounded, fail-closed evidence for native shell and Dockerfile expansions.
//!
//! File names select a grammar; they never exempt a file. Rust string literals
//! are deliberately not interpreted as shell: quoting is not proof of their
//! eventual use. The caller must scan literal `include_str!` dependencies too.

use std::collections::BTreeSet;
use std::ops::Range;
use std::path::Path;

#[derive(Default)]
pub(super) struct ShellContext {
    safe: Vec<Range<usize>>,
    execution: Vec<Range<usize>>,
}

impl ShellContext {
    pub(super) fn parse(path: &Path, source: &str) -> Self {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
        let mut execution = Vec::new();
        let safe = if name == "Dockerfile" || name.starts_with("Dockerfile.") {
            dockerfile_ranges(source, &mut execution)
        } else if matches!(extension, "sh" | "bash") || shell_shebang(source) {
            shell_ranges(source, &mut execution).unwrap_or_default()
        } else {
            Vec::new()
        };
        Self { safe, execution }
    }

    pub(super) fn execution_boundaries(&self) -> &[Range<usize>] {
        &self.execution
    }

    pub(super) fn is_safe(&self, occurrence: Range<usize>) -> bool {
        occurrence.start < occurrence.end
            && self
                .safe
                .iter()
                .any(|safe| safe.start <= occurrence.start && occurrence.end <= safe.end)
    }
}

fn shell_shebang(source: &str) -> bool {
    matches!(
        source.lines().next(),
        Some("#!/bin/sh" | "#!/bin/bash" | "#!/usr/bin/env sh" | "#!/usr/bin/env bash")
    )
}

#[derive(Default)]
struct Word {
    source: Range<usize>,
    keyword: bool,
    assignment: bool,
    // None means the shell expands some part of this word.
    literal: Option<String>,
    safe: Vec<Range<usize>>,
}

#[derive(Default)]
struct Statement {
    words: Vec<Word>,
}

/// This deliberately recognizes a subset rather than guessing at shell grammar.
/// Unsupported quoting, here-documents, reparsing, and incomplete syntax retain
/// all findings. In particular a quote never hides a command substitution.
fn shell_ranges(source: &str, execution: &mut Vec<Range<usize>>) -> Option<Vec<Range<usize>>> {
    shell_ranges_nested(source, execution, 0)
}

fn shell_ranges_nested(
    source: &str,
    execution: &mut Vec<Range<usize>>,
    depth: usize,
) -> Option<Vec<Range<usize>>> {
    let bytes = source.as_bytes();
    let mut statements = Vec::<Statement>::new();
    let mut current = Statement::default();
    let mut defined = BTreeSet::new();
    let mut delimiters = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b' ' | b'\t' | b'\r' => index += 1,
            b'\\' if bytes.get(index + 1) == Some(&b'\n') => index += 2,
            b'#' => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'\n' | b';' | b'|' | b'&' | b'(' | b')' | b'{' | b'}' => {
                match bytes[index] {
                    b'(' | b'{' => {
                        if bytes[index] == b'('
                            && source[index + 1..].trim_start().starts_with(')')
                            && current.words.len() == 1
                        {
                            defined.insert(current.words[0].literal.clone()?);
                        }
                        if bytes[index] == b'{'
                            && current.words.len() == 2
                            && current.words[0].literal.as_deref() == Some("function")
                        {
                            defined.insert(current.words[1].literal.clone()?);
                        }
                        delimiters.push(bytes[index]);
                    }
                    b')' if delimiters.pop() != Some(b'(') => return None,
                    b'}' if delimiters.pop() != Some(b'{') => return None,
                    _ => {}
                }
                if !current.words.is_empty() {
                    statements.push(std::mem::take(&mut current));
                }
                index += 1;
            }
            b'<' if bytes.get(index + 1) == Some(&b'<') => return None,
            b'<' | b'>' => {
                // Redirection targets are not command arguments. End this
                // statement's evidence rather than misclassifying a target.
                if !current.words.is_empty() {
                    statements.push(std::mem::take(&mut current));
                }
                index += 1;
                if bytes
                    .get(index)
                    .is_some_and(|b| matches!(b, b'>' | b'&' | b'|'))
                {
                    index += 1;
                }
                if index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                    let _ = read_word(source, &mut index)?;
                }
            }
            _ => current.words.push(read_word(source, &mut index)?),
        }
    }
    if !current.words.is_empty() {
        statements.push(current);
    }

    if !delimiters.is_empty() || !valid_blocks(&statements) {
        return None;
    }
    for statement in &statements {
        if let Some(boundary) = execution_boundary(command_words(statement), depth) {
            execution.push(boundary);
        }
    }
    // Shell functions and aliases can replace even ordinary data commands.
    // A dynamic command or in-process evaluator can install such definitions;
    // without scope/dataflow proof no part of that script receives an exemption.
    for statement in &statements {
        if let Some(command) = command_words(statement).first() {
            let name = command.literal.as_deref()?;
            if name == "command" {
                let words = command_words(statement);
                if !matches!(
                    words.get(1).and_then(|word| word.literal.as_deref()),
                    Some("-v" | "-V")
                ) {
                    return None;
                }
            }
            if name == "function" {
                let words = command_words(statement);
                if words.len() != 2 {
                    return None;
                }
                defined.insert(words[1].literal.clone()?);
            }
            if matches!(
                name,
                "eval" | "source" | "." | "alias" | "unalias" | "enable" | "builtin" | "trap"
            ) {
                return None;
            }
        }
    }

    let mut safe = Vec::new();
    for statement in statements {
        let words = command_words(&statement);
        let Some(command) = words.first().and_then(|word| word.literal.as_deref()) else {
            continue;
        };
        if defined.contains(command) {
            continue;
        }
        if data_command(command, &words[1..]) {
            for word in &words[1..] {
                safe.extend(word.safe.iter().cloned());
            }
        } else if command == "git"
            && words.get(1).and_then(|word| word.literal.as_deref()) == Some("-c")
            && words.get(2).and_then(|word| word.literal.as_deref())
                == Some("safe.directory=<data>")
        {
            // Only this fixed key's value is data. Later git arguments could
            // configure a command interpreter and require independent evidence.
            safe.extend(words[2].safe.iter().cloned());
        }
    }
    Some(safe)
}

fn command_words(statement: &Statement) -> &[Word] {
    let mut words = statement.words.as_slice();
    while let Some(word) = words.first() {
        if word.assignment
            || word.keyword
                && matches!(
                    word.literal.as_deref(),
                    Some("if" | "then" | "elif" | "else" | "while" | "until" | "do" | "!")
                )
        {
            words = &words[1..];
        } else {
            break;
        }
    }
    words
}

fn valid_blocks(statements: &[Statement]) -> bool {
    let mut blocks = Vec::new();
    for statement in statements {
        for word in &statement.words {
            if !word.keyword {
                break;
            }
            match word.literal.as_deref() {
                Some("if") => {
                    blocks.push("if");
                    break;
                }
                Some("while" | "until" | "for") => {
                    blocks.push("loop");
                    break;
                }
                Some("then" | "else" | "elif") => {
                    if blocks.last() != Some(&"if") {
                        return false;
                    }
                }
                Some("fi") => {
                    if blocks.pop() != Some("if") {
                        return false;
                    }
                }
                Some("do") => {
                    if blocks.last() != Some(&"loop") {
                        return false;
                    }
                }
                Some("done") => {
                    if blocks.pop() != Some("loop") {
                        return false;
                    }
                }
                Some("case" | "esac" | "select") => return false,
                _ => break,
            }
        }
    }
    blocks.is_empty()
}

fn execution_boundary(mut words: &[Word], depth: usize) -> Option<Range<usize>> {
    let beginning = words.first()?.source.start;
    loop {
        match words.first()?.literal.as_deref() {
            Some("builtin") => words = &words[1..],
            Some("command") => {
                words = &words[1..];
                if matches!(words.first()?.literal.as_deref(), Some("-v" | "-V")) {
                    return None;
                }
                if words.first()?.literal.as_deref() == Some("-p") {
                    words = &words[1..];
                }
                if words.first()?.literal.as_deref() == Some("--") {
                    words = &words[1..];
                }
            }
            _ => break,
        }
    }
    let command = words.first()?;
    let Some(name) = command.literal.as_deref() else {
        return Some(beginning..command.source.end);
    };
    let dynamic = |word: &&Word| word.literal.is_none() || !word.safe.is_empty();
    if matches!(name, "eval" | "source" | ".") {
        return words[1..]
            .iter()
            .find(dynamic)
            .map(|word| beginning..word.source.end);
    }
    let name = name.rsplit('/').next()?;
    if !matches!(name, "sh" | "bash" | "dash" | "ksh" | "zsh") {
        return None;
    }
    for (index, argument) in words[1..].iter().enumerate() {
        let Some(option) = argument.literal.as_deref() else {
            // An expanded program path/option is an execution boundary too.
            return Some(beginning..argument.source.end);
        };
        if option.starts_with('-') && !option.starts_with("--") && option.contains('c') {
            let program = words.get(index + 2)?;
            return shell_program_is_dynamic(program, depth)
                .then_some(beginning..program.source.end);
        }
        if !option.starts_with('-') {
            return None;
        }
    }
    None
}

fn shell_program_is_dynamic(program: &Word, depth: usize) -> bool {
    let Some(literal) = program.literal.as_deref() else {
        return true;
    };
    if !literal.contains(['$', '`']) {
        return false;
    }
    if depth >= 4 || literal.contains("$(") || literal.contains('`') {
        return true;
    }
    let mut nested_execution = Vec::new();
    let Some(safe) = shell_ranges_nested(literal, &mut nested_execution, depth + 1) else {
        return true;
    };
    if !nested_execution.is_empty() {
        return true;
    }
    literal.match_indices('$').any(|(start, _)| {
        variable_end(literal, start).is_some_and(|end| {
            !safe
                .iter()
                .any(|range| range.start <= start && end <= range.end)
        })
    })
}

fn data_command(command: &str, args: &[Word]) -> bool {
    match command {
        // These arguments are data, not another shell program. Unrecognized
        // commands deliberately receive no assumption about their semantics.
        "cd" | "mkdir" | "echo" | "cat" | "cp" | "mv" | "rm" | "touch" | "ls" | "head" | "tail"
        | "wc" | "cut" | "tr" | "basename" | "dirname" | "readlink" | "realpath" | "chmod"
        | "chown" => true,
        "printf" => args
            .first()
            .and_then(|word| word.literal.as_deref())
            .is_some_and(printf_format_is_data),
        "[" | "test" => test_arguments(args),
        "git" => {
            // General git options can configure command execution. Only the
            // fixed data-only identity config operations are proven here.
            let literals: Vec<_> = args.iter().map(|word| word.literal.as_deref()).collect();
            matches!(
                literals.as_slice(),
                [
                    Some("config"),
                    Some("--global"),
                    Some("user.name" | "user.email"),
                    _
                ]
            )
        }
        _ => false,
    }
}

fn printf_format_is_data(format: &str) -> bool {
    if format.starts_with('-') {
        return false;
    }
    let mut chars = format.chars();
    while let Some(character) = chars.next() {
        if character == '%' && !matches!(chars.next(), Some('s' | '%')) {
            // Bash %n (including width/flags) assigns through its argument;
            // other conversions are outside this bounded data-only proof.
            return false;
        }
    }
    true
}

fn test_arguments(args: &[Word]) -> bool {
    let args = if args.last().and_then(|word| word.literal.as_deref()) == Some("]") {
        &args[..args.len() - 1]
    } else {
        args
    };
    match args {
        [_] => true,
        [operator, _] => operator.literal.as_deref().is_some_and(|op| {
            matches!(
                op,
                "-n" | "-z" | "-e" | "-f" | "-d" | "-r" | "-w" | "-x" | "-s" | "-L"
            )
        }),
        [_, operator, _] => operator.literal.as_deref().is_some_and(|op| {
            matches!(
                op,
                "=" | "!=" | "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge"
            )
        }),
        _ => false,
    }
}

fn read_word(source: &str, index: &mut usize) -> Option<Word> {
    let bytes = source.as_bytes();
    let start = *index;
    let mut literal = String::new();
    let mut dynamic = false;
    let mut safe = Vec::new();
    let mut quote = None;
    let mut substitution = false;
    while *index < bytes.len() {
        let byte = bytes[*index];
        if quote.is_none() && (byte.is_ascii_whitespace() || b";|&(){}<>".contains(&byte)) {
            break;
        }
        match (quote, byte) {
            (Some(b'\''), b'\'') | (Some(b'"'), b'"') => {
                quote = None;
                *index += 1;
            }
            (None, b'\'' | b'"') => {
                quote = Some(byte);
                *index += 1;
            }
            (Some(b'\''), _) => {
                // Quoted text still may be another interpreter's program.
                // The command-level proof below is required for these ranges.
                if byte == b'$' {
                    let from = *index;
                    if bytes.get(from + 1) == Some(&b'(') {
                        substitution = true;
                    }
                    if let Some(end) = variable_end(source, from) {
                        safe.push(expansion_range(source, from, end, quote));
                    }
                }
                literal.push(byte as char);
                *index += 1;
            }
            (_, b'\\') => {
                let next = *bytes.get(*index + 1)?;
                if next == b'\n' {
                    *index += 2;
                } else if quote == Some(b'"') && !b"$`\"\\".contains(&next) {
                    literal.push('\\');
                    *index += 1;
                } else {
                    literal.push(next as char);
                    *index += 2;
                }
            }
            (_, b'$') if bytes.get(*index + 1) == Some(&b'(') => {
                dynamic = true;
                substitution = true;
                *index = substitution_end(source, *index + 2)?;
            }
            (_, b'`') => {
                dynamic = true;
                substitution = true;
                *index += 1;
                while *index < bytes.len() && bytes[*index] != b'`' {
                    if bytes[*index] == b'\\' {
                        *index += 1;
                    }
                    *index += 1;
                }
                if bytes.get(*index) != Some(&b'`') {
                    return None;
                }
                *index += 1;
            }
            (_, b'$') => {
                if bytes
                    .get(*index + 1)
                    .is_some_and(|next| matches!(next, b'\'' | b'"'))
                {
                    // ANSI-C and localized quoting have different decoding rules.
                    return None;
                }
                let from = *index;
                if let Some(end) = variable_end(source, from) {
                    dynamic = true;
                    if quote == Some(b'"') && simple_parameter(&source[from..end]) {
                        safe.push(expansion_range(source, from, end, quote));
                    }
                    *index = end;
                } else if bytes.get(from + 1) == Some(&b'{') {
                    return None;
                } else {
                    literal.push('$');
                    *index += 1;
                }
            }
            (_, _) => {
                literal.push(byte as char);
                *index += 1;
            }
        }
    }
    if quote.is_some() || *index == start {
        return None;
    }
    if substitution {
        safe.clear();
    }
    // Preserve only a proven configuration key, never an arbitrary dynamic
    // option. This marker is internal and is not a user/source wordlist.
    let literal = if dynamic && source[start..*index].starts_with("safe.directory=") {
        Some("safe.directory=<data>".to_owned())
    } else if dynamic {
        None
    } else {
        Some(literal)
    };
    let original = &source[start..*index];
    let keyword = literal.as_deref() == Some(original);
    let assignment = original.split_once('=').is_some_and(|(name, _)| {
        let mut chars = name.chars();
        chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    });
    Some(Word {
        source: start..*index,
        keyword,
        assignment,
        literal,
        safe,
    })
}

fn variable_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(start) != Some(&b'$') {
        return None;
    }
    if bytes.get(start + 1) == Some(&b'{') {
        let mut index = start + 2;
        let mut depth = 1usize;
        while index < bytes.len() {
            match bytes[index] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(index + 1);
                    }
                }
                b'\n' | b'\r' => return None,
                _ => {}
            }
            index += 1;
        }
        return None;
    }
    let first = *bytes.get(start + 1)?;
    if first.is_ascii_alphabetic() || first == b'_' {
        let mut end = start + 2;
        while bytes
            .get(end)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            end += 1;
        }
        Some(end)
    } else if first.is_ascii_digit() || b"@*#?$!-".contains(&first) {
        Some(start + 2)
    } else {
        None
    }
}

fn simple_parameter(value: &str) -> bool {
    let Some(body) = value
        .strip_prefix("${")
        .and_then(|body| body.strip_suffix('}'))
    else {
        return !value.contains("$(");
    };
    // Array subscripts/indirection and arithmetic are not plain parameter
    // defaults. Some shells evaluate them as code even inside double quotes.
    if body.contains(['[', ']', '(', ')', '`', '\\', '\'', '"']) {
        return false;
    }
    let name_end = body
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        .count();
    if name_end == 0 {
        return false;
    }
    let rest = &body[name_end..];
    if rest.is_empty() {
        return true;
    }
    let Some(default) = [":-", ":+", ":?", "-", "+", "?"]
        .iter()
        .find_map(|operator| rest.strip_prefix(operator))
    else {
        return false;
    };
    let mut index = 0;
    while let Some(found) = default[index..].find('$') {
        let start = index + found;
        let Some(end) = variable_end(default, start) else {
            return false;
        };
        if !simple_parameter(&default[start..end]) {
            return false;
        }
        index = end;
    }
    true
}

fn expansion_range(source: &str, start: usize, end: usize, quote: Option<u8>) -> Range<usize> {
    // The legacy bare-variable regex includes one trailing nonquote byte.
    // Accommodate a literal suffix only within the same protected quote.
    let suffix = source.as_bytes().get(end).copied();
    let extended = quote.is_some()
        && suffix.is_some_and(|byte| {
            !byte.is_ascii_whitespace() && !b"$`\\\"'".contains(&byte) && byte.is_ascii()
        });
    start..end + usize::from(extended)
}

fn substitution_end(source: &str, mut index: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 1usize;
    let mut quote = None;
    while index < bytes.len() {
        let byte = bytes[index];
        match (quote, byte) {
            (_, b'\\') if quote != Some(b'\'') => index += 2,
            (Some(b'\''), b'\'') | (Some(b'"'), b'"') => {
                quote = None;
                index += 1;
            }
            (None, b'\'' | b'"') => {
                quote = Some(byte);
                index += 1;
            }
            (None, b'(') => {
                depth += 1;
                index += 1;
            }
            (None, b')') => {
                depth -= 1;
                index += 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => index += 1,
        }
    }
    None
}

fn dockerfile_ranges(source: &str, execution: &mut Vec<Range<usize>>) -> Vec<Range<usize>> {
    // Alternative escaping and here-documents change instruction boundaries.
    // Decline the complete input rather than interpreting their bodies as new
    // Dockerfile instructions or ordinary shell text.
    if source.contains("<<")
        || source.lines().any(|line| {
            let line = line.trim_start_matches('\u{feff}').trim_start();
            let Some(directive) = line.strip_prefix('#').or_else(|| line.strip_prefix("//")) else {
                // A JSON/custom frontend is not the native Dockerfile grammar.
                return line.starts_with('{');
            };
            let Some((name, _)) = directive.trim_start().split_once('=') else {
                return false;
            };
            matches!(
                name.trim().to_ascii_lowercase().as_str(),
                "escape" | "syntax"
            )
        })
    {
        return Vec::new();
    }
    let mut safe = Vec::new();
    let mut from_seen = false;
    let mut shell_changed = false;
    let mut logical = String::new();
    let mut mapping = Vec::new();
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        // Docker ignores blank/comment lines inside a continued instruction;
        // they cannot terminate a RUN and hide its later shell arguments.
        if !logical.is_empty() && (line.trim_start().starts_with('#') || line.trim().is_empty()) {
            offset += line.len();
            continue;
        }
        let trimmed_end = line.trim_end_matches(['\r', '\n']);
        let continued = trimmed_end.ends_with('\\');
        let length = trimmed_end.len() - usize::from(continued);
        logical.push_str(&line[..length]);
        mapping.extend(offset..offset + length);
        offset += line.len();
        if continued {
            logical.push(' ');
            mapping.push(offset - line.len() + length);
            continue;
        }
        let trimmed = logical.trim_start();
        if !trimmed.starts_with('#') && !trimmed.is_empty() {
            let leading = logical.len() - trimmed.len();
            let instruction_end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
            let instruction = trimmed[..instruction_end].to_ascii_uppercase();
            let body = &trimmed[instruction_end..];
            let body_start = leading + instruction_end;
            let mut local = Vec::new();
            match instruction.as_str() {
                "FROM" => {
                    from_seen = true;
                    docker_variables(body, body_start, &mut local);
                }
                "ARG" | "LABEL" | "ENV" | "WORKDIR" | "USER" | "EXPOSE" | "STOPSIGNAL" | "COPY"
                | "ADD" | "VOLUME" => docker_variables(body, body_start, &mut local),
                "SHELL" => {
                    shell_changed = true;
                }
                "RUN" if !shell_changed && !body.trim_start().starts_with('[') => {
                    let mut boundaries = Vec::new();
                    if let Some(ranges) = shell_ranges(body, &mut boundaries) {
                        local.extend(
                            ranges
                                .into_iter()
                                .map(|range| body_start + range.start..body_start + range.end),
                        );
                    }
                    for range in boundaries {
                        if let (Some(&start), Some(&end)) = (
                            mapping.get(body_start + range.start),
                            mapping.get(body_start + range.end - 1),
                        ) {
                            execution.push(start..end + 1);
                        }
                    }
                }
                _ => {}
            }
            for range in local {
                if let (Some(&start), Some(&end)) =
                    (mapping.get(range.start), mapping.get(range.end - 1))
                {
                    // Only contiguous original bytes are proof. A variable
                    // assembled across continuation boundaries stays flagged.
                    if end + 1 - start == range.len() {
                        safe.push(start..end + 1);
                    }
                }
            }
        }
        logical.clear();
        mapping.clear();
    }
    if from_seen && logical.is_empty() {
        safe
    } else {
        Vec::new()
    }
}

fn docker_variables(body: &str, offset: usize, safe: &mut Vec<Range<usize>>) {
    // Escaped/JSON-form instruction values are outside this bounded grammar.
    if body.contains('\\') || body.trim_start().starts_with('[') {
        return;
    }
    for (start, _) in body.match_indices('$') {
        let Some(end) = variable_end(body, start) else {
            continue;
        };
        let value = &body[start..end];
        if !value.contains("$(") && !value.contains(['`', '\'', '"']) {
            let range = expansion_range(body, start, end, Some(b'"'));
            safe.push(offset + range.start..offset + range.end);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn safe(path: &str, source: &str, needle: &str) -> bool {
        let start = source.find(needle).unwrap();
        ShellContext::parse(Path::new(path), source).is_safe(start..start + needle.len())
    }

    #[test]
    fn native_quoted_data_words_are_proven_individually() {
        let script =
            "cd \"${WORKSPACE:-$PWD}\"\nmkdir -p \"${HOME:-/tmp/home}\"\nprintf '%s' \"$NAME\"\n";
        assert!(safe("start.sh", script, "${WORKSPACE:-$PWD}"));
        assert!(safe("start.sh", script, "$PWD}"));
        assert!(safe("start.sh", script, "${HOME:-/tmp/home}"));
        assert!(safe("start.sh", script, "$NAME"));
        assert!(!safe("start.rs", script, "$NAME"));
    }

    #[test]
    fn command_substitutions_and_same_token_attacks_remain() {
        for source in [
            "echo \"$(id -u)\"",
            "echo \"${HOME:-$(id)}\"",
            "echo \"${HOME}$(id)\"",
            "echo \"${HOME:-`id`}\"",
            "echo '${HOME}$(id)'",
        ] {
            let needle = if source.contains("${") {
                &source[source.find("${").unwrap()..source.rfind('}').unwrap() + 1]
            } else {
                "$(id -u)"
            };
            assert!(!safe("test.sh", source, needle), "{source}");
        }
        let source = "echo \"${SAFE}\"; echo $(id)";
        assert!(safe("test.sh", source, "${SAFE}"));
        assert!(!safe("test.sh", source, "$(id)"));
    }

    #[test]
    fn quoting_does_not_prove_reparsed_or_unknown_programs_safe() {
        for source in [
            "eval \"${CMD}\"",
            "sh -c \"${CMD}\"",
            "/bin/bash -c '${CMD}'",
            "source \"${CMD}\"",
            ". \"${CMD}\"",
            "\"${CMD}\" argument",
            "unknown \"${CMD}\"",
            "python -c \"${CMD}\"",
            "awk '${CMD}'",
            "sh -c 'echo ${CMD}'",
            "echo(){ eval \"$1\"; }; echo \"${CMD}\"",
            "alias echo=eval; echo \"${CMD}\"",
            "eval something; echo \"${CMD}\"",
        ] {
            assert!(!safe("test.sh", source, "${CMD}"), "{source}");
        }
    }

    #[test]
    fn malformed_and_unsupported_shell_syntax_fails_closed() {
        for source in [
            "echo \"${HOME}",
            "echo \"${HOME}\"; echo '",
            "echo ${HOME}",
            "cat <<EOF\n${HOME}\nEOF",
            "echo $'${HOME}'",
            "echo \"${HOME\"",
            "printf -v name \"${HOME}\"",
            "test -v \"${HOME}\"",
        ] {
            assert!(!safe("test.sh", source, "${HOME"), "{source}");
        }
    }

    #[test]
    fn escaped_quotes_and_continuations_preserve_context() {
        assert!(safe("test.sh", "echo \\\n \"${HOME}\"", "${HOME}"));
        assert!(safe("test.sh", "echo \"a \\\" ${HOME}\"", "${HOME}"));
        assert!(!safe("test.sh", "echo \\\"${HOME}\\\"", "${HOME}"));
        assert!(!safe("test.sh", "echo \"${HOME}\"; eval \\\n x", "${HOME}"));
    }

    #[test]
    fn dockerfile_variables_are_not_blanket_exemptions() {
        let source = "ARG IMAGE=base\nFROM $IMAGE\nLABEL name=\"${NAME}\"\nRUN echo \"${HOME}\"\nRUN echo $(id)\nRUN eval \"${CMD}\"\n";
        assert!(safe("Dockerfile.template", source, "$IMAGE"));
        assert!(safe("Dockerfile.template", source, "${NAME}"));
        assert!(safe("Dockerfile.template", source, "${HOME}"));
        assert!(!safe("Dockerfile.template", source, "$(id)"));
        assert!(!safe("Dockerfile.template", source, "${CMD}"));
        assert!(!safe("not_shell.rs", source, "$IMAGE"));
        assert!(!safe(
            "Dockerfile",
            "FROM ${IMAGE:-$(id)}",
            "${IMAGE:-$(id)}"
        ));
    }

    #[test]
    fn native_shell_command_overrides_and_dynamic_modes_retain_findings() {
        for source in [
            "echo() { sh -c \"$1\"; }; echo \"${DATA}\"",
            "function echo { sh -c \"$1\"; }; echo \"${DATA}\"",
            "git -c alias.x=\"${DATA}\" x",
            "git config --global core.sshCommand \"${DATA}\"",
            "printf \"${DATA}\"",
            "\"$COMMAND\"; echo \"${DATA}\"",
        ] {
            assert!(!safe("test.sh", source, "${DATA}"), "{source}");
        }
        assert!(safe(
            "test.sh",
            "git config --global user.name \"${NAME}\"",
            "${NAME}"
        ));
        assert!(safe(
            "test.sh",
            "git -c safe.directory=\"${WORKSPACE}\" status",
            "${WORKSPACE}"
        ));
    }
    #[test]
    fn extracted_templates_have_language_evidence_and_keep_execution_findings() {
        let startup = include_str!("../../plugins/container/startup.sh");
        for (start, _) in startup.match_indices("${") {
            let end = start + startup[start..].find('}').unwrap() + 1;
            assert!(
                ShellContext::parse(Path::new("startup.sh"), startup).is_safe(start..end),
                "{}",
                &startup[start..end]
            );
        }
        assert!(!safe("startup.sh", startup, "$(id -u)"));
        assert!(!safe("startup.sh", startup, "$(id -g)"));
        let docker = include_str!("../../plugins/container/Dockerfile.template");
        for needle in [
            "$DECAPOD_IMAGE",
            "$DECAPOD_WORKSPACE_PATH",
            "$DECAPOD_VERSION",
            "$DECAPOD_USE_LOCAL_BINARY",
        ] {
            assert!(safe("Dockerfile.template", docker, needle), "{needle}");
        }
    }

    #[test]
    fn dockerfile_reinterpretation_controls_remain_findings() {
        for source in [
            "FROM base\nSHELL [\"bash\", \"-c\"]\nRUN echo \"${DATA}\"\n",
            "# escape=`\nFROM base\nLABEL name=${DATA}\n",
            "FROM base\nRUN <<EOF\nLABEL name=${DATA}\nEOF\n",
            "FROM base\nONBUILD RUN sh -c '${DATA}'\n",
            "FROM base\nRUN [\"sh\", \"-c\", \"echo ${DATA}\"]\n",
        ] {
            assert!(!safe("Dockerfile", source, "${DATA}"), "{source}");
        }
        let source = "FROM base\nRUN echo ready && \\\n echo \"${DATA}\"\n";
        assert!(safe("Dockerfile", source, "${DATA}"));
    }
    #[test]
    fn quoted_parameter_arrays_and_indirection_are_not_proven_data() {
        for source in [
            "echo \"${ARRAY[$INPUT]}\"",
            "echo \"${!NAME}\"",
            "echo \"${HOME:-${ARRAY[$INPUT]}}\"",
            "echo \"${NAME:=value}\"",
        ] {
            let start = source.find("${").unwrap();
            let end = source.rfind('}').unwrap() + 1;
            assert!(
                !ShellContext::parse(Path::new("test.sh"), source).is_safe(start..end),
                "{source}"
            );
        }
    }
    #[test]
    fn escaped_function_names_and_unclosed_delimiters_fail_closed() {
        for source in [
            "function 'echo' { sh -c \"$1\"; }; echo \"${DATA}\"",
            "function \"echo\"() { sh -c \"$1\"; }; echo \"${DATA}\"",
            "function 'echo'\n{ sh -c \"$1\"; }; echo \"${DATA}\"",
            "e\\cho() { sh -c \"$1\"; }; echo \"${DATA}\"",
            "function e\\cho { sh -c \"$1\"; }; echo \"${DATA}\"",
            "echo \"${DATA}\"; (",
            "echo \"${DATA}\"; {",
        ] {
            assert!(!safe("test.sh", source, "${DATA}"), "{source}");
        }
    }
    #[test]
    fn wrapper_evaluators_disable_quoted_data_exemptions() {
        for source in [
            "VAR=value eval \"$1\"; echo \"${DATA}\"",
            "VAR=value command eval \"$1\"; echo \"${DATA}\"",
            "command eval \"$1\"; echo \"${DATA}\"",
            "command -p eval \"$1\"; echo \"${DATA}\"",
            "builtin source \"$1\"; echo \"${DATA}\"",
            "builtin eval \"$1\"; echo \"${DATA}\"",
            "builtin . \"$1\"; echo \"${DATA}\"",
            "trap 'echo(){ sh -c \"$1\"; }' DEBUG; echo \"${DATA}\"",
        ] {
            assert!(!safe("test.sh", source, "${DATA}"), "{source}");
        }
        assert!(safe(
            "test.sh",
            "command -v git; echo \"${DATA}\"",
            "${DATA}"
        ));
    }

    #[test]
    fn positional_execution_boundaries_do_not_need_a_regex_candidate() {
        for source in [
            "sh -c \"$1\"",
            "/bin/bash -ec \"$1\"",
            "eval \"$1\"",
            "source \"$1\"",
            ". \"$1\"",
            "command eval \"$1\"",
            "builtin source \"$1\"",
            "command -p bash -c \"$1\"",
            "\"$1\" arg",
        ] {
            let context = ShellContext::parse(Path::new("test.sh"), source);
            assert!(!context.execution_boundaries().is_empty(), "{source}");
        }
        assert!(
            ShellContext::parse(Path::new("test.sh"), "sh -c 'echo \"$1\"'")
                .execution_boundaries()
                .is_empty()
        );
        assert!(
            !ShellContext::parse(Path::new("test.sh"), "sh -c 'echo $1'")
                .execution_boundaries()
                .is_empty()
        );
        let source = "FROM base\nRUN sh -c \"$1\"\n";
        assert!(
            !ShellContext::parse(Path::new("Dockerfile"), source)
                .execution_boundaries()
                .is_empty()
        );
        assert!(
            ShellContext::parse(Path::new("test.sh"), "command -v git; echo \"$1\"")
                .execution_boundaries()
                .is_empty()
        );
    }
    #[test]
    fn quoted_reserved_words_and_incomplete_blocks_are_not_control_flow() {
        for source in [
            "'if' echo \"${DATA}\"",
            "\"then\" echo \"${DATA}\"",
            "X\\=1 echo \"${DATA}\"",
            "'X=1' echo \"${DATA}\"",
            "if echo \"${DATA}\"; then",
            "while true; do echo \"${DATA}\"",
        ] {
            assert!(!safe("test.sh", source, "${DATA}"), "{source}");
        }
        assert!(safe(
            "test.sh",
            "if true; then echo \"${DATA}\"; fi",
            "${DATA}"
        ));
        assert!(
            !ShellContext::parse(Path::new("test.sh"), "X=\"$1\" eval \"$1\"")
                .execution_boundaries()
                .is_empty()
        );
    }
    #[test]
    fn dockerfile_continuation_comments_cannot_hide_reinterpretation() {
        let source = "FROM base\nRUN echo \"${DATA}\"; \\\n# comment between continued lines\n sh -c \"$1\"\n";
        let context = ShellContext::parse(Path::new("Dockerfile"), source);
        assert!(!context.execution_boundaries().is_empty());
        let boundary = &context.execution_boundaries()[0];
        assert!(source[boundary.clone()].contains("sh -c"));
    }
    #[test]
    fn docker_parser_directives_cannot_relabel_executable_continuations() {
        for directive in [
            "#escape=`",
            "#\tescape=`",
            "#  EsCaPe = `",
            "\u{feff}#escape=`",
            "#syntax=custom/frontend",
            "#\tSYNTAX = custom/frontend",
            "//syntax=custom/frontend",
            "//\tSyNtAx=custom/frontend",
            "\u{feff}//syntax=custom/frontend",
            "{\"syntax\":\"custom/frontend\"}",
        ] {
            let source =
                format!("{directive}\nFROM base\nRUN eval `\n LABEL name=\"${{INPUT}}\"\n");
            assert!(!safe("Dockerfile", &source, "${INPUT}"), "{directive}");
        }
        assert!(safe(
            "Dockerfile",
            "FROM base\nLABEL name=\"${INPUT}\"\n",
            "${INPUT}"
        ));
    }

    #[test]
    fn command_options_and_printf_assignment_modes_are_not_data_only() {
        for source in [
            "sort --compress-program=\"${PAYLOAD}\" -S 1 input.txt",
            "sort --compress-program \"${PAYLOAD}\" -S 1 input.txt",
            "printf '%n' \"${PAYLOAD}\"",
            "printf '%5n' \"${PAYLOAD}\"",
            "printf '%-5n' \"${PAYLOAD}\"",
        ] {
            assert!(!safe("test.sh", source, "${PAYLOAD}"), "{source}");
        }
        assert!(safe(
            "test.sh",
            "printf '%s\\n' \"${PAYLOAD}\"",
            "${PAYLOAD}"
        ));
        assert!(safe("test.sh", "echo \"${PAYLOAD}\"", "${PAYLOAD}"));
    }
}
