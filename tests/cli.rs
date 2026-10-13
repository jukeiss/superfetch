//! End-to-end tests: run the real binary against a throwaway home directory.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{env, fs};

/// A private config and cache directory, removed when the test ends.
struct Sandbox(PathBuf);

impl Sandbox {
    fn new() -> Sandbox {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let unique = format!(
            "superfetch-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let dir = env::temp_dir().join(unique);
        fs::create_dir_all(&dir).unwrap();
        Sandbox(dir)
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_superfetch"))
            .args(args)
            .env("XDG_CONFIG_HOME", self.0.join("config"))
            .env("XDG_CACHE_HOME", self.0.join("cache"))
            // On Windows these two take the place of the XDG fallbacks.
            .env("APPDATA", self.0.join("config"))
            .env("LOCALAPPDATA", self.0.join("cache"))
            .env_remove("NO_COLOR")
            .output()
            .unwrap()
    }

    fn stdout(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "{args:?} failed: {}",
            stderr(&output)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn config_file(&self) -> PathBuf {
        self.0.join("config").join("superfetch").join("config.toml")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

#[test]
fn prints_the_default_layout() {
    let sandbox = Sandbox::new();
    let output = sandbox.stdout(&["--logo", "none"]);
    for label in ["OS: ", "Kernel: ", "Memory: ", "Disk: "] {
        assert!(output.contains(label), "missing {label:?} in:\n{output}");
    }
    // Not a terminal, so no escape sequences.
    assert!(!output.contains('\x1b'));
}

#[test]
fn first_run_writes_the_default_config() {
    let sandbox = Sandbox::new();
    assert!(!sandbox.config_file().exists());
    sandbox.stdout(&[]);
    assert_eq!(
        read(&sandbox.config_file()),
        sandbox.stdout(&["--default-config"])
    );
}

#[test]
fn color_can_be_forced_and_disabled() {
    let sandbox = Sandbox::new();
    assert!(sandbox.stdout(&["--force-color"]).contains("\x1b["));
    assert!(!sandbox.stdout(&["--no-color"]).contains('\x1b'));
}

#[test]
fn modules_flag_selects_and_orders_lines() {
    let sandbox = Sandbox::new();
    let output = sandbox.stdout(&["--logo", "none", "--modules", "kernel,os"]);
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 2, "{output}");
    assert!(lines[0].starts_with("Kernel: "));
    assert!(lines[1].starts_with("OS: "));
}

#[test]
fn set_overrides_any_key() {
    let sandbox = Sandbox::new();
    let output = sandbox.stdout(&[
        "--logo=none",
        "--modules=os",
        "--set=module.os.label=System",
        "--set=module.os.format=fixed text",
        "--set=layout.key_separator= -> ",
    ]);
    assert_eq!(output, "System -> fixed text\n");
}

#[test]
fn save_persists_flags() {
    let sandbox = Sandbox::new();
    sandbox.stdout(&[
        "--color", "blue", "--hide", "gpu", "--logo", "none", "--save",
    ]);
    let saved = read(&sandbox.config_file());
    assert!(saved.contains("accent = \"blue\""));
    assert!(saved.contains("source = \"none\""));
    assert!(!saved.contains("    \"gpu\","));
    // The documentation in the file survives the rewrite.
    assert!(saved.starts_with("# superfetch configuration"));

    let output = sandbox.stdout(&[]);
    assert!(!output.contains("GPU: "));
    assert!(
        output.starts_with(|c: char| c.is_alphanumeric()),
        "logo still drawn:\n{output}"
    );
}

#[test]
fn mistakes_are_reported() {
    let sandbox = Sandbox::new();
    let cases: [(&[&str], &str); 5] = [
        (&["--toggle", "gnu"], "did you mean 'gpu'"),
        (&["--color", "blurple"], "not a color"),
        (&["--frobnicate"], "unknown option"),
        (&["--set", "bar.width=wide"], "--set"),
        (
            &["--config-file", "/nonexistent/superfetch.toml"],
            "cannot read",
        ),
    ];
    for (args, expected) in cases {
        let output = sandbox.run(args);
        assert!(!output.status.success(), "{args:?} should fail");
        assert!(
            stderr(&output).contains(expected),
            "{args:?}: {}",
            stderr(&output)
        );
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn broken_config_falls_back_to_defaults() {
    let sandbox = Sandbox::new();
    let path = sandbox.config_file();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "[theme]\nacent = \"red\"\n").unwrap();

    let output = sandbox.run(&["--logo", "none"]);
    assert!(output.status.success());
    assert!(stderr(&output).contains("acent"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("OS: "));
}

#[test]
fn custom_modules_and_logos() {
    let sandbox = Sandbox::new();
    let logo = sandbox.0.join("logo.txt");
    fs::write(&logo, "$1ab\n$2cd$$\n").unwrap();
    let config = sandbox.0.join("custom.toml");
    fs::write(
        &config,
        format!(
            "modules = [\"greeting\", \"shout\"]\n\
             [logo]\nsource = {logo:?}\ngap = 1\n\
             [module.greeting]\nlabel = \"\"\nformat = \"hello {{env:SUPERFETCH_TEST}}\"\n\
             [module.shout]\nlabel = \"Echo\"\ncommand = \"echo loud\"\n"
        ),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_superfetch"))
        .args(["--config-file", config.to_str().unwrap()])
        .env("SUPERFETCH_TEST", "world")
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "ab  hello world\ncd$ Echo: loud\n"
    );
}

#[test]
fn informational_flags() {
    let sandbox = Sandbox::new();
    assert!(sandbox.stdout(&["--help"]).contains("Usage: superfetch"));
    assert!(sandbox.stdout(&["--version"]).starts_with("superfetch "));
    let list = sandbox.stdout(&["--list"]);
    assert!(list.contains("cpu") && list.contains("Logos:"));
}
