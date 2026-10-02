//! Custom tools, like P4V's: external programs run from TOME's menus with the selected files
//! or revision filled into their arguments.
//!
//! Arguments are a template split into words first (spaces separate words, `"…"` or `'…'`
//! keeps spaces), and only then are the variables replaced inside each word. The program gets
//! each word as one argument and no shell runs, so a file name can never change the command.
//!
//! | Variable | Value |
//! |---|---|
//! | `%f` | selected files, full local paths (one argument each) |
//! | `%F` | selected files, repository-relative paths (one argument each) |
//! | `%n` | selected files, file names only (one argument each) |
//! | `%r` | selected revision id |
//! | `%R` | selected revision number |
//! | `%a` | the answer to the tool's prompt |
//! | `$r` | working copy root |
//! | `$b` | current branch |
//! | `%%`, `$$` | a literal `%`, `$` |
//!
//! A word holding a per-file variable (`--file=%f`) becomes one argument per file.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

/// Where a tool shows up, and so which selection it gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Context {
    /// The toolbar's tool menu: the working copy, nothing selected.
    Repository,
    /// Files: changed files, locks.
    File,
    /// A revision in the Smartlog.
    Revision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RunMode {
    /// Wait for it and show what it printed.
    #[default]
    Capture,
    /// In a console window of its own that stays open.
    Terminal,
    /// Start it and do not wait (an editor, a viewer).
    Detached,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tool {
    pub id: String,
    pub name: String,
    /// The program: a path or a name found on PATH (variables allowed).
    pub program: String,
    /// The argument template.
    pub args: String,
    /// The working folder (variables allowed); empty: the working copy root.
    pub cwd: String,
    pub contexts: Vec<Context>,
    pub run: RunMode,
    /// Asked before running; the answer is `%a`. Empty: nothing is asked.
    pub prompt: String,
    /// Ask "run it?" first.
    pub confirm: bool,
    /// Rescan the working copy afterwards.
    pub refresh: bool,
}

impl Default for Tool {
    fn default() -> Self {
        Tool {
            id: String::new(),
            name: String::new(),
            program: String::new(),
            args: String::new(),
            cwd: String::new(),
            contexts: vec![Context::Repository],
            run: RunMode::Capture,
            prompt: String::new(),
            confirm: false,
            refresh: false,
        }
    }
}

/// What the tool is run on.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Selection {
    /// Repository-relative paths with `/`.
    pub files: Vec<String>,
    pub revision: String,
    pub revision_number: u64,
    pub branch: String,
    pub answer: String,
}

/// A command line ready to start: the program, its arguments, its folder.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Invocation {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

impl Invocation {
    /// The command line as text, for the status bar (quoted only where needed).
    pub fn display(&self) -> String {
        std::iter::once(&self.program).chain(&self.args).map(|a| quote(a)).collect::<Vec<_>>().join(" ")
    }
}

fn quote(text: &str) -> String {
    if !text.is_empty() && !text.chars().any(|c| c.is_whitespace() || c == '"' || c == '\'') {
        text.to_string()
    } else {
        format!("\"{}\"", text.replace('"', "\\\""))
    }
}

/// Splits a template into words: whitespace separates, quotes keep spaces (and are removed).
pub fn split_words(template: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    for c in template.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => word.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                in_word = true;
            }
            None if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            None => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if let Some(q) = quote {
        return Err(format!("닫히지 않은 따옴표 {q}"));
    }
    if in_word {
        words.push(word);
    }
    Ok(words)
}

/// The per-file values of a variable letter, or None for a single-valued one.
fn per_file(letter: char, root: &Path, selection: &Selection) -> Option<Vec<String>> {
    let local = |p: &String| root.join(p.replace('/', std::path::MAIN_SEPARATOR_STR)).to_string_lossy().into_owned();
    match letter {
        'f' => Some(selection.files.iter().map(local).collect()),
        'F' => Some(selection.files.clone()),
        'n' => Some(selection.files.iter().map(|p| p.rsplit('/').next().unwrap_or(p).to_string()).collect()),
        _ => None,
    }
}

/// Replaces the variables in one word; a word with a per-file variable becomes one word per file.
fn expand_word(word: &str, root: &Path, selection: &Selection) -> Result<Vec<String>, String> {
    let mut parts: Vec<Result<String, char>> = Vec::new(); // Ok: text, Err: per-file letter
    let mut text = String::new();
    let mut chars = word.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' && c != '$' {
            text.push(c);
            continue;
        }
        let Some(&letter) = chars.peek() else {
            text.push(c);
            break;
        };
        chars.next();
        let value = match (c, letter) {
            ('%', '%') => "%".to_string(),
            ('$', '$') => "$".to_string(),
            ('%', 'f' | 'F' | 'n') => {
                parts.push(Ok(std::mem::take(&mut text)));
                parts.push(Err(letter));
                continue;
            }
            ('%', 'r') => required(&selection.revision, "%r", "리비전")?,
            ('%', 'R') => {
                if selection.revision.is_empty() {
                    return Err("%R: 선택된 리비전이 없습니다".into());
                }
                selection.revision_number.to_string()
            }
            ('%', 'a') => selection.answer.clone(),
            ('$', 'r') => root.to_string_lossy().into_owned(),
            ('$', 'b') => required(&selection.branch, "$b", "브랜치")?,
            _ => return Err(format!("알 수 없는 변수 {c}{letter}")),
        };
        text.push_str(&value);
    }
    parts.push(Ok(text));

    let letters: Vec<char> = parts.iter().filter_map(|p| p.as_ref().err().copied()).collect();
    if letters.len() > 1 {
        return Err(format!("한 단어에 파일 변수가 둘 이상 있습니다: {word}"));
    }
    let Some(&letter) = letters.first() else {
        return Ok(vec![parts.into_iter().map(|p| p.unwrap()).collect()]);
    };
    let values = per_file(letter, root, selection).unwrap();
    if values.is_empty() {
        return Err(format!("%{letter}: 선택된 파일이 없습니다"));
    }
    Ok(values
        .into_iter()
        .map(|value| parts.iter().map(|p| match p { Ok(t) => t.as_str(), Err(_) => value.as_str() }).collect())
        .collect())
}

fn required(value: &str, name: &str, what: &str) -> Result<String, String> {
    if value.is_empty() { Err(format!("{name}: 선택된 {what}이(가) 없습니다")) } else { Ok(value.to_string()) }
}

/// The command line for `tool` on `selection` in the working copy at `root`.
pub fn expand(tool: &Tool, root: &Path, selection: &Selection) -> Result<Invocation, String> {
    if tool.program.trim().is_empty() {
        return Err("실행할 프로그램이 비어 있습니다".into());
    }
    let program = single(&tool.program, root, selection, "프로그램")?;
    let mut args = Vec::new();
    for word in split_words(&tool.args)? {
        args.extend(expand_word(&word, root, selection)?);
    }
    let cwd = if tool.cwd.trim().is_empty() { root.to_path_buf() } else { PathBuf::from(single(&tool.cwd, root, selection, "작업 폴더")?) };
    Ok(Invocation { program, args, cwd })
}

/// A field that must stay one value (program, folder): variables allowed, per-file ones not.
fn single(template: &str, root: &Path, selection: &Selection, what: &str) -> Result<String, String> {
    let mut values = expand_word(template.trim(), root, selection)?;
    if values.len() != 1 {
        return Err(format!("{what}에는 파일 변수를 쓸 수 없습니다"));
    }
    Ok(values.remove(0))
}

/// What a captured run printed.
#[derive(Debug, Clone, Serialize)]
pub struct Output {
    pub command: String,
    /// None when the tool was not waited for (terminal, detached).
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Console output as text: UTF-8, or on Windows the console's legacy code page (CP949 for Korean).
fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) if cfg!(windows) => encoding_rs::EUC_KR.decode(bytes).0.into_owned(),
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Starts `invocation` the way `mode` says.
pub fn run(invocation: &Invocation, mode: RunMode) -> Result<Output, String> {
    let command_text = invocation.display();
    let failed = |e: std::io::Error| format!("{} 실행 실패: {e}", invocation.program);
    match mode {
        RunMode::Capture => {
            let output = Command::new(&invocation.program)
                .args(&invocation.args)
                .current_dir(&invocation.cwd)
                .stdin(Stdio::null())
                .output()
                .map_err(failed)?;
            Ok(Output { command: command_text, exit_code: output.status.code(), stdout: decode(&output.stdout), stderr: decode(&output.stderr) })
        }
        RunMode::Detached => {
            Command::new(&invocation.program)
                .args(&invocation.args)
                .current_dir(&invocation.cwd)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(failed)?;
            Ok(Output { command: command_text, exit_code: None, stdout: String::new(), stderr: String::new() })
        }
        RunMode::Terminal => {
            terminal(invocation).map_err(failed)?;
            Ok(Output { command: command_text, exit_code: None, stdout: String::new(), stderr: String::new() })
        }
    }
}

/// A console window that stays open after the tool ends.
#[cfg(windows)]
fn terminal(invocation: &Invocation) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    Command::new("cmd")
        .arg("/k")
        .arg(&invocation.program)
        .args(&invocation.args)
        .current_dir(&invocation.cwd)
        .creation_flags(CREATE_NEW_CONSOLE)
        .spawn()
        .map(|_| ())
}

/// Terminal.app running the command in the tool's folder.
#[cfg(not(windows))]
fn terminal(invocation: &Invocation) -> std::io::Result<()> {
    let shell = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    let line = std::iter::once(invocation.program.as_str()).chain(invocation.args.iter().map(String::as_str)).map(shell).collect::<Vec<_>>().join(" ");
    let script = format!("cd {} && {line}", shell(&invocation.cwd.to_string_lossy()));
    let apple = format!("tell application \"Terminal\" to do script \"{}\"", script.replace('\\', "\\\\").replace('"', "\\\""));
    Command::new("osascript").args(["-e", &apple, "-e", "tell application \"Terminal\" to activate"]).spawn().map(|_| ())
}

/// The project's shared tools, `.tome/tools.json` in the working copy.
pub fn project_file(root: &Path) -> PathBuf {
    root.join(".tome").join("tools.json")
}

pub fn read_project(root: &Path) -> Result<Vec<Tool>, String> {
    match std::fs::read_to_string(project_file(root)) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!(".tome/tools.json을 읽을 수 없습니다: {e}")),
        Err(_) => Ok(Vec::new()),
    }
}

pub fn write_project(root: &Path, tools: &[Tool]) -> Result<(), String> {
    let file = project_file(root);
    std::fs::create_dir_all(file.parent().unwrap()).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(tools).map_err(|e| e.to_string())? + "\n";
    std::fs::write(file, text).map_err(|e| e.to_string())
}

/// A fingerprint of the project tools file; trust is given to this exact content. FNV-1a
/// twice over (two offsets), fixed so it stays the same across Rust versions.
pub fn fingerprint(root: &Path) -> String {
    let bytes = std::fs::read(project_file(root)).unwrap_or_default();
    if bytes.is_empty() {
        return String::new();
    }
    let fnv = |offset: u64| bytes.iter().fold(offset, |h, &b| (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3));
    format!("{:016x}{:016x}", fnv(0xcbf2_9ce4_8422_2325), fnv(0x6c62_272e_07bb_0142))
}

/// The tool to run, looked up where it lives (never taken from the caller): one of `personal`,
/// or one in the project file at `root`, which runs only while the file's fingerprint is
/// `trusted` (what the user trusted, if anything).
pub fn find(personal: &[Tool], root: &Path, trusted: Option<&str>, project: bool, id: &str) -> Result<Tool, String> {
    let list = if project {
        let print = fingerprint(root);
        if print.is_empty() || trusted != Some(print.as_str()) {
            return Err("프로젝트 도구(.tome/tools.json)가 신뢰되지 않았거나 바뀌었습니다. 도구 관리에서 내용을 확인하고 신뢰하세요.".into());
        }
        read_project(root)?
    } else {
        personal.to_vec()
    };
    list.into_iter().find(|t| t.id == id).ok_or_else(|| format!("도구를 찾을 수 없습니다: {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        if cfg!(windows) { PathBuf::from(r"C:\Project\Game") } else { PathBuf::from("/work/Game") }
    }

    fn sep(path: &str) -> String {
        path.replace('/', std::path::MAIN_SEPARATOR_STR)
    }

    fn tool(program: &str, args: &str) -> Tool {
        Tool { id: "t".into(), name: "t".into(), program: program.into(), args: args.into(), ..Default::default() }
    }

    fn selection(files: &[&str]) -> Selection {
        Selection {
            files: files.iter().map(|f| f.to_string()).collect(),
            revision: "abc123".into(),
            revision_number: 42,
            branch: "main".into(),
            answer: "hello world".into(),
        }
    }

    #[test]
    fn words_split_on_spaces_and_keep_quoted_text() {
        assert_eq!(split_words(r#"a  "b c" 'd "e"' f"g h"i"#).unwrap(), ["a", "b c", r#"d "e""#, "fg hi"]);
        assert_eq!(split_words("").unwrap(), Vec::<String>::new());
        assert_eq!(split_words(r#""""#).unwrap(), [""], "an empty quoted word is still a word");
        assert!(split_words(r#"a "b"#).is_err());
    }

    #[test]
    fn file_variables_give_one_argument_per_file() {
        let inv = expand(&tool("diff", "-u %f"), &root(), &selection(&["Content/A.uasset", "Source/B c.cpp"])).unwrap();
        let r = root();
        assert_eq!(inv.args, ["-u".to_string(), r.join(sep("Content/A.uasset")).to_string_lossy().into(), r.join(sep("Source/B c.cpp")).to_string_lossy().into()]);
        let inv = expand(&tool("x", "--file=%F %n"), &root(), &selection(&["Content/A.uasset", "Source/B c.cpp"])).unwrap();
        assert_eq!(inv.args, ["--file=Content/A.uasset", "--file=Source/B c.cpp", "A.uasset", "B c.cpp"]);
    }

    #[test]
    fn single_values_and_literals() {
        let inv = expand(&tool("lore", "show %r r%R $b \"%a\" 100%% $$HOME"), &root(), &selection(&[])).unwrap();
        assert_eq!(inv.args, ["show", "abc123", "r42", "main", "hello world", "100%", "$HOME"]);
        let inv = expand(&tool("$r/Tools/bake.bat", ""), &root(), &selection(&[])).unwrap();
        assert_eq!(inv.program, format!("{}/Tools/bake.bat", root().to_string_lossy()));
        assert_eq!(inv.cwd, root(), "the folder defaults to the working copy root");
    }

    #[test]
    fn a_file_name_cannot_inject_arguments() {
        // Quotes and spaces inside a path stay inside its one argument.
        let inv = expand(&tool("echo", "%F"), &root(), &selection(&[r#"a" & del *.* & "b.txt"#])).unwrap();
        assert_eq!(inv.args, [r#"a" & del *.* & "b.txt"#]);
    }

    #[test]
    fn missing_selection_and_bad_templates_are_errors() {
        let empty = Selection::default();
        assert!(expand(&tool("x", "%f"), &root(), &empty).unwrap_err().contains("파일"));
        assert!(expand(&tool("x", "%r"), &root(), &empty).unwrap_err().contains("리비전"));
        assert!(expand(&tool("x", "%R"), &root(), &empty).is_err());
        assert!(expand(&tool("x", "$b"), &root(), &empty).is_err());
        assert!(expand(&tool("x", "%q"), &root(), &selection(&[])).unwrap_err().contains("%q"));
        assert!(expand(&tool("x", "%f=%F"), &root(), &selection(&["a"])).is_err());
        assert!(expand(&tool("", "a"), &root(), &empty).is_err());
        assert!(expand(&tool("%f", ""), &root(), &selection(&["a", "b"])).is_err(), "program must be one value");
        // A trailing % or $ is kept as text.
        assert_eq!(expand(&tool("x", "50% $"), &root(), &empty).unwrap().args, ["50%", "$"]);
    }

    #[test]
    fn display_quotes_where_needed() {
        let inv = Invocation { program: "p".into(), args: vec!["a b".into(), "c".into(), "".into()], cwd: root() };
        assert_eq!(inv.display(), r#"p "a b" c """#);
    }

    #[test]
    fn captured_run_returns_output_and_exit_code() {
        let dir = std::env::temp_dir();
        let (program, args): (&str, Vec<String>) = if cfg!(windows) {
            ("cmd", vec!["/c".into(), "echo tome& echo oops 1>&2& exit /b 3".into()])
        } else {
            ("sh", vec!["-c".into(), "echo tome; echo oops >&2; exit 3".into()])
        };
        let out = run(&Invocation { program: program.into(), args, cwd: dir }, RunMode::Capture).unwrap();
        assert_eq!(out.stdout.trim(), "tome");
        assert_eq!(out.stderr.trim(), "oops");
        assert_eq!(out.exit_code, Some(3));
    }

    #[test]
    fn a_missing_program_is_an_error() {
        let inv = Invocation { program: "tome-no-such-program".into(), args: vec![], cwd: std::env::temp_dir() };
        assert!(run(&inv, RunMode::Capture).unwrap_err().contains("tome-no-such-program"));
    }

    #[test]
    fn project_tools_round_trip_and_fingerprint_changes() {
        let dir = std::env::temp_dir().join(format!("tome-tools-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(fingerprint(&dir), "", "no file: nothing to trust");
        let tools = vec![Tool { contexts: vec![Context::File], run: RunMode::Detached, ..tool("code", "%f") }];
        write_project(&dir, &tools).unwrap();
        assert_eq!(read_project(&dir).unwrap(), tools);
        let first = fingerprint(&dir);
        write_project(&dir, &[tool("code", "%F")]).unwrap();
        assert_ne!(fingerprint(&dir), first, "any change asks for trust again");
        std::fs::write(project_file(&dir), "{").unwrap();
        assert!(read_project(&dir).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn project_tools_run_only_while_trusted() {
        let dir = std::env::temp_dir().join(format!("tome-trust-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mine = [tool("code", "%f")];
        assert!(find(&mine, &dir, None, true, "t").is_err(), "no project file");
        write_project(&dir, &[tool("bake", "")]).unwrap();
        assert!(find(&mine, &dir, None, true, "t").is_err(), "never trusted");
        let print = fingerprint(&dir);
        assert_eq!(find(&mine, &dir, Some(&print), true, "t").unwrap().program, "bake");
        // Someone changes the shared file: trust no longer holds.
        write_project(&dir, &[tool("evil", "")]).unwrap();
        assert!(find(&mine, &dir, Some(&print), true, "t").unwrap_err().contains("신뢰"));
        // My own tools need no trust; an unknown id is an error.
        assert_eq!(find(&mine, &dir, None, false, "t").unwrap().program, "code");
        assert!(find(&mine, &dir, None, false, "nope").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_fields_take_defaults() {
        let tools: Vec<Tool> = serde_json::from_str(r#"[{"name": "열기", "program": "code"}]"#).unwrap();
        assert_eq!(tools[0].contexts, [Context::Repository]);
        assert_eq!(tools[0].run, RunMode::Capture);
        assert!(tools[0].args.is_empty());
    }
}
