//! Rules that name compute programs to leave out of a run, and the ray-tracing chain list.
//! Parsing and matching only. The lists come from the user's machine (the title's `xcl.toml`,
//! an environment variable, or a file in the title's data folder), never from this repository.

use std::path::Path;

/// A kind of program a rule can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Trace,
    Build,
    Consumer,
    Loops,
}

/// One rule: a pipeline's record key, a name glob, a class, or the negation of one of those.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token {
    Record(u64),
    Name(String),
    Class(Class),
    Not(Box<Token>),
}

/// What a program does in the ray-tracing chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    None,
    Build,
    Trace,
    Producer,
    Consumer,
}

/// The parsed leave-out rules: each token with its text as written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rules {
    pub tokens: Vec<(Token, String)>,
}

impl Rules {
    pub fn new(tokens: Vec<(Token, String)>) -> Rules {
        Rules { tokens }
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// The text of the rule that leaves the program out, or None. A `!` rule whose token matches
    /// wins over every other rule.
    pub fn matches(&self, record: u64, name: Option<&str>, fx: Option<&crate::gpu::effects::Effects>, role: Role) -> Option<String> {
        let hit = |t: &Token| match t {
            Token::Record(k) => *k == record,
            Token::Name(g) => name.is_some_and(|n| crate::util::glob_match(&g.to_ascii_lowercase(), &n.to_ascii_lowercase())),
            Token::Class(Class::Trace) => role == Role::Trace || fx.is_some_and(|f| f.traces),
            Token::Class(Class::Build) => role == Role::Build,
            Token::Class(Class::Consumer) => role == Role::Producer || role == Role::Consumer,
            Token::Class(Class::Loops) => fx.is_some_and(|f| f.loops),
            Token::Not(_) => false,
        };
        if self.tokens.iter().any(|(t, _)| matches!(t, Token::Not(inner) if hit(inner.as_ref()))) {
            return None;
        }
        self.tokens.iter().find(|(t, _)| !matches!(t, Token::Not(_)) && hit(t)).map(|(_, s)| s.clone())
    }
}

/// The text without a leading BOM, as lines without their `#` comments.
fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.strip_prefix('\u{feff}').unwrap_or(text).lines().map(|l| l.split('#').next().unwrap_or(""))
}

/// The text of the file `@path` names (a relative path is taken from `base`).
fn read_at(path: &str, base: &Path) -> Result<String, String> {
    let p = base.join(path);
    std::fs::read_to_string(&p).map_err(|e| format!("@{path}: cannot read {}: {e}", p.display()))
}

/// A token without `!`: 16 hex digits, `name:<glob>` or `class:<kind>`.
fn parse_plain(w: &str) -> Option<Token> {
    if w.len() == 16 && w.bytes().all(|b| b.is_ascii_hexdigit()) {
        return u64::from_str_radix(w, 16).ok().map(Token::Record);
    }
    if let Some(glob) = w.get(..5).filter(|p| p.eq_ignore_ascii_case("name:")).map(|_| &w[5..]) {
        return (!glob.is_empty()).then(|| Token::Name(glob.to_string()));
    }
    let kind = w.get(..6).filter(|p| p.eq_ignore_ascii_case("class:")).map(|_| &w[6..])?;
    match kind.to_ascii_lowercase().as_str() {
        "trace" => Some(Token::Class(Class::Trace)),
        "build" => Some(Token::Class(Class::Build)),
        "consumer" => Some(Token::Class(Class::Consumer)),
        "loops" => Some(Token::Class(Class::Loops)),
        _ => None,
    }
}

fn parse_word(w: &str) -> Option<Token> {
    match w.strip_prefix('!') {
        Some(rest) => parse_plain(rest).map(|t| Token::Not(Box::new(t))),
        None => parse_plain(w),
    }
}

/// The rules in `text`, with their text as written, and warnings for what was dropped. Tokens are
/// separated by commas, spaces, tabs or newlines; `#` starts a comment; `@path` reads a file
/// (relative to `base`) in place, and `@` inside a file is not expanded.
pub fn parse(text: &str, base: &Path) -> (Vec<(Token, String)>, Vec<String>) {
    let mut tokens = Vec::new();
    let mut warnings = Vec::new();
    let add = |w: &str, tokens: &mut Vec<(Token, String)>, warnings: &mut Vec<String>| match parse_word(w) {
        Some(t) => tokens.push((t, w.to_string())),
        None => warnings.push(format!("leave-out: {w:?} is not a record key, name:<glob> or class:<trace|build|consumer|loops>: dropped")),
    };
    for line in lines(text) {
        for w in line.split(|c: char| c == ',' || c.is_whitespace()).filter(|w| !w.is_empty()) {
            if let Some(path) = w.strip_prefix('@') {
                match read_at(path, base) {
                    Ok(inner) => {
                        for l in lines(&inner) {
                            for w in l.split(|c: char| c == ',' || c.is_whitespace()).filter(|w| !w.is_empty()) {
                                if w.starts_with('@') {
                                    warnings.push(format!("leave-out: {w:?} inside a file is not expanded: dropped"));
                                } else {
                                    add(w, &mut tokens, &mut warnings);
                                }
                            }
                        }
                    }
                    Err(e) => warnings.push(format!("leave-out: {e}")),
                }
            } else {
                add(w, &mut tokens, &mut warnings);
            }
        }
    }
    (tokens, warnings)
}

/// One chain entry: `<record or name> [producer|consumer]`.
fn chain_entry(entry: &str, out: &mut Vec<(Token, Role)>, warnings: &mut Vec<String>) {
    let words: Vec<&str> = entry.split_whitespace().collect();
    let Some(first) = words.first() else { return };
    let token = match parse_word(first) {
        Some(t @ (Token::Record(_) | Token::Name(_))) => t,
        _ => {
            warnings.push(format!("chain: {first:?} is not a record key or name:<glob>: entry dropped"));
            return;
        }
    };
    let role = match words.get(1).map(|r| r.to_ascii_lowercase()).as_deref() {
        None | Some("consumer") => Role::Consumer,
        Some("producer") => Role::Producer,
        Some(other) => {
            warnings.push(format!("chain: role {other:?} is not producer or consumer: entry {entry:?} dropped"));
            return;
        }
    };
    if words.len() > 2 {
        warnings.push(format!("chain: entry {entry:?} has more than a token and a role: dropped"));
        return;
    }
    out.push((token, role));
}

/// The chain list in `text`: entries separated by newlines or commas, each a token and an
/// optional role (`producer`, or `consumer` by default). `#` comments, a BOM and `@path` (read in
/// place, not nested) work as in `parse`.
pub fn parse_chain(text: &str, base: &Path) -> (Vec<(Token, Role)>, Vec<String>) {
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    for line in lines(text) {
        for entry in line.split(',') {
            let entry = entry.trim();
            if let Some(path) = entry.strip_prefix('@') {
                match read_at(path.trim(), base) {
                    Ok(inner) => {
                        for l in lines(&inner) {
                            for e in l.split(',') {
                                if e.trim().starts_with('@') {
                                    warnings.push(format!("chain: {:?} inside a file is not expanded: dropped", e.trim()));
                                } else {
                                    chain_entry(e, &mut out, &mut warnings);
                                }
                            }
                        }
                    }
                    Err(e) => warnings.push(format!("chain: {e}")),
                }
            } else {
                chain_entry(entry, &mut out, &mut warnings);
            }
        }
    }
    (out, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::effects::Effects;

    const KEY: u64 = 0x0123456789abcdef;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("xcl-leave-out-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn rules(text: &str) -> Rules {
        let (t, w) = parse(text, Path::new("."));
        assert!(w.is_empty(), "{w:?}");
        Rules::new(t)
    }

    #[test]
    fn leave_out_parses_keys_names_classes_and_negation() {
        let (t, w) = parse("0123456789ABCDEF, name:CS_Test*  Class:TRACE\nclass:build\tclass:consumer class:loops !name:CS_Keep !0123456789abcdef", Path::new("."));
        assert!(w.is_empty(), "{w:?}");
        let tokens: Vec<Token> = t.iter().map(|(t, _)| t.clone()).collect();
        assert_eq!(
            tokens,
            vec![
                Token::Record(KEY),
                Token::Name("CS_Test*".into()),
                Token::Class(Class::Trace),
                Token::Class(Class::Build),
                Token::Class(Class::Consumer),
                Token::Class(Class::Loops),
                Token::Not(Box::new(Token::Name("CS_Keep".into()))),
                Token::Not(Box::new(Token::Record(KEY))),
            ]
        );
        // the text is kept as written
        assert_eq!(t[0].1, "0123456789ABCDEF");
        assert_eq!(t[2].1, "Class:TRACE");
        let r = Rules::new(t);
        assert!(!r.is_empty() && Rules::default().is_empty());
        // a key and a name (the name match ignores case)
        let r = rules("name:cs_test*");
        assert_eq!(r.matches(1, Some("CS_TestResolve"), None, Role::None).as_deref(), Some("name:cs_test*"));
        assert_eq!(r.matches(1, Some("PS_Other"), None, Role::None), None);
        assert_eq!(r.matches(1, None, None, Role::None), None);
        let r = rules("0123456789abcdef");
        assert_eq!(r.matches(KEY, None, None, Role::None).as_deref(), Some("0123456789abcdef"));
        assert_eq!(r.matches(KEY + 1, None, None, Role::None), None);
    }

    #[test]
    fn leave_out_reads_at_files_relative_to_base_with_comments_and_bom() {
        let dir = temp_dir("at");
        std::fs::write(dir.join("list.txt"), "\u{feff}# a comment\n0123456789abcdef # trailing\nname:CS_TestResolve,class:loops\n").unwrap();
        let (t, w) = parse("\u{feff}class:trace # not this: name:CS_Hidden\n@list.txt\nname:CS_Last", &dir);
        assert!(w.is_empty(), "{w:?}");
        let texts: Vec<&str> = t.iter().map(|(_, s)| s.as_str()).collect();
        assert_eq!(texts, ["class:trace", "0123456789abcdef", "name:CS_TestResolve", "class:loops", "name:CS_Last"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn leave_out_negation_wins() {
        let r = rules("name:CS_Test* !name:CS_TestKeep class:loops");
        let fx = Effects { loops: true, ..Effects::default() };
        assert_eq!(r.matches(1, Some("CS_TestResolve"), None, Role::None).as_deref(), Some("name:CS_Test*"));
        assert_eq!(r.matches(1, Some("CS_TestKeep"), None, Role::None), None);
        // the negation wins over a rule of another kind that also matches
        assert_eq!(r.matches(1, Some("CS_TestKeep"), Some(&fx), Role::None), None);
        // order does not matter
        let r = rules("!0123456789abcdef 0123456789abcdef");
        assert_eq!(r.matches(KEY, None, None, Role::None), None);
        // the first matching rule is named
        let r = rules("class:loops name:CS_Test*");
        assert_eq!(r.matches(1, Some("CS_TestResolve"), Some(&fx), Role::None).as_deref(), Some("class:loops"));
    }

    #[test]
    fn leave_out_classes_follow_role_and_effects() {
        let none = Effects::default();
        let traces = Effects { traces: true, ..Effects::default() };
        let loops = Effects { loops: true, ..Effects::default() };
        let m = |class: &str, fx: Option<&Effects>, role: Role| rules(class).matches(1, None, fx, role).is_some();
        // trace: the role, or the program's own ray-intersection instruction
        assert!(m("class:trace", None, Role::Trace));
        assert!(m("class:trace", Some(&traces), Role::None));
        assert!(!m("class:trace", Some(&none), Role::Build));
        assert!(!m("class:trace", None, Role::None));
        // build: the role only
        assert!(m("class:build", None, Role::Build));
        assert!(!m("class:build", Some(&traces), Role::Trace));
        // consumer: producers and consumers of the chain
        assert!(m("class:consumer", None, Role::Producer));
        assert!(m("class:consumer", None, Role::Consumer));
        assert!(!m("class:consumer", Some(&loops), Role::Trace));
        assert!(!m("class:consumer", None, Role::None));
        // loops: the program's facts only
        assert!(m("class:loops", Some(&loops), Role::None));
        assert!(!m("class:loops", Some(&none), Role::Consumer));
        assert!(!m("class:loops", None, Role::Consumer));
    }

    #[test]
    fn chain_lines_default_to_consumer() {
        let dir = temp_dir("chain");
        std::fs::write(dir.join("chain.txt"), "# list\nname:CS_TestFiltered producer\n").unwrap();
        let text = "\u{feff}0123456789abcdef # default\nname:CS_TestResolve producer\nname:CS_TestA, name:CS_TestB\tconsumer\n\n@chain.txt\n";
        let (c, w) = parse_chain(text, &dir);
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(
            c,
            vec![
                (Token::Record(KEY), Role::Consumer),
                (Token::Name("CS_TestResolve".into()), Role::Producer),
                (Token::Name("CS_TestA".into()), Role::Consumer),
                (Token::Name("CS_TestB".into()), Role::Consumer),
                (Token::Name("CS_TestFiltered".into()), Role::Producer),
            ]
        );
        let (c, w) = parse_chain("name:CS_TestX PRODUCER", &dir);
        assert!(w.is_empty() && c == vec![(Token::Name("CS_TestX".into()), Role::Producer)], "{c:?} {w:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bad_tokens_warn_and_are_dropped() {
        let dir = temp_dir("bad");
        std::fs::write(dir.join("nested.txt"), "@other.txt\nname:CS_TestKept\n").unwrap();
        let text = "0123456789abcde name: class:unknown !!name:CS_TestX 0123456789abcdef0 @nested.txt @missing.txt name:CS_TestOk";
        let (t, w) = parse(text, &dir);
        let texts: Vec<&str> = t.iter().map(|(_, s)| s.as_str()).collect();
        assert_eq!(texts, ["name:CS_TestKept", "name:CS_TestOk"]);
        // one warning per dropped token: five bad words, the nested `@`, the missing file
        assert_eq!(w.len(), 7, "{w:?}");
        for needle in ["0123456789abcde\"", "\"name:\"", "class:unknown", "!!name:CS_TestX", "0123456789abcdef0", "@other.txt", "missing.txt"] {
            assert!(w.iter().any(|m| m.contains(needle)), "no warning names {needle}: {w:?}");
        }

        let text = "class:trace\nname:CS_TestA producer extra\nname:CS_TestB maybe\n!name:CS_TestC\nname:CS_TestOk";
        let (c, w) = parse_chain(text, &dir);
        assert_eq!(c, vec![(Token::Name("CS_TestOk".into()), Role::Consumer)]);
        assert_eq!(w.len(), 4, "{w:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
